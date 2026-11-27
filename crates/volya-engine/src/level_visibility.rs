// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's camera pass (docs/behavior/visibility-scope.md): its
//! sectors, portals and static visuals (`volya-engine-core::visibility`),
//! its occlusion (`volya-engine-core::occlusion`), the camera's sector, and
//! the latest pass's result, which the dynamic items are tested against
//! (docs/behavior/object-visibility.md 4 – 6).

use std::io;
use std::sync::Arc;

use crate::core::camera::Vec3;
use crate::core::collision::CollisionWorld;
use crate::core::occlusion::{Cached, Occlusion, OcclusionMesh, View};
use crate::core::particles::random::Generator;
use crate::core::visibility::{CameraFrame, CameraSector, PassBuffers, Sectors, triangle_below};
use crate::render::billboards::{BillboardDraw, BillboardFrame, BillboardGroup, QUAD_VERTICES};
use crate::render::particles::ParticleVertex;
use volya_engine_formats::level::{Level, Thresholds};

/// The camera pass's state for a level's run.
#[derive(Debug)]
pub struct LevelVisibility {
    /// Shared with the level's world ([`crate::level::LevelWorld::sectors`]).
    sectors: Arc<Sectors>,
    occlusion: Occlusion,
    camera: CameraSector,
    frame: CameraFrame,
    /// The pass's working lists, kept with `frame` between frames.
    buffers: PassBuffers,
    /// The latest pass's view and thresholds.
    view: Option<View>,
    thresholds: Option<Thresholds>,
    /// Each visual's static draws (indices of the level's draw items):
    /// those of visual `v` are `draws[starts[v] .. starts[v + 1]]`.
    starts: Vec<u32>,
    draws: Vec<u32>,
    /// The billboard groups (ogf.md 7), each with its texture set (none:
    /// not drawn), and each visual's group (`u32::MAX`: none).
    billboards: Vec<(BillboardGroup, Option<u32>)>,
    billboard_of: Vec<u32>,
    /// The dynamic items' state (object-visibility.md 4, occlusion-tests.md
    /// 5): the level's object instances, and its effect objects by slot.
    pub objects: Vec<Item>,
    pub effects: Vec<Item>,
}

/// A dynamic item's sector and occlusion cache.
#[derive(Clone, Copy, Debug, Default)]
pub struct Item {
    /// Its sector, none until the ray first finds one.
    pub sector: Option<u16>,
    /// The sphere its sector was last found for: a placement since marks it
    /// for a refresh (object-visibility.md 2.3, 4).
    pub at: Option<[f32; 4]>,
    pub cache: Cached,
}

impl Default for LevelVisibility {
    /// A level without sectors: nothing is ever visited.
    fn default() -> Self {
        Self {
            sectors: Arc::default(),
            occlusion: Occlusion::new(None, 0),
            camera: CameraSector::default(),
            frame: CameraFrame::default(),
            buffers: PassBuffers::default(),
            view: None,
            thresholds: None,
            starts: vec![0],
            draws: Vec::new(),
            billboards: Vec::new(),
            billboard_of: Vec::new(),
            objects: Vec::new(),
            effects: Vec::new(),
        }
    }
}

impl LevelVisibility {
    /// The sectors of `level`, its occlusion mesh (none without
    /// `level.hom`), the visual of each of its static draws and its
    /// billboard groups by visual.
    pub fn new(
        level: &Level,
        mesh: Option<OcclusionMesh>,
        draw_visuals: impl Iterator<Item = u32>,
        billboards: Vec<(u32, BillboardGroup)>,
    ) -> io::Result<Self> {
        let sectors = Sectors::new(level)?;
        let occlusion = Occlusion::new(mesh, sectors.visual_count());
        let mut by_visual = vec![Vec::new(); sectors.visual_count()];
        for (i, v) in draw_visuals.enumerate() {
            if let Some(d) = by_visual.get_mut(v as usize) {
                d.push(i as u32);
            }
        }
        let mut starts = Vec::with_capacity(by_visual.len() + 1);
        starts.push(0);
        let mut draws = Vec::new();
        for d in by_visual {
            draws.extend(d);
            starts.push(draws.len() as u32);
        }
        let mut billboard_of = vec![u32::MAX; sectors.visual_count()];
        let billboards = (billboards.into_iter().enumerate())
            .map(|(k, (v, g))| {
                if let Some(b) = billboard_of.get_mut(v as usize) {
                    *b = k as u32;
                }
                (g, None)
            })
            .collect();
        Ok(Self {
            sectors: Arc::new(sectors),
            occlusion,
            starts,
            draws,
            billboards,
            billboard_of,
            ..Self::default()
        })
    }

    /// The texture set of each billboard group, in the order they were
    /// given ([`LevelVisibility::new`]).
    pub fn set_billboard_sets(&mut self, sets: impl IntoIterator<Item = Option<u32>>) {
        for (b, s) in self.billboards.iter_mut().zip(sets) {
            b.1 = s;
        }
    }

    /// The billboard groups.
    pub fn billboard_count(&self) -> usize {
        self.billboards.len()
    }

    /// The billboards of the latest pass's visuals into `out` (ogf.md 7,
    /// 8.2: each listed billboard group's quad for the pass's eye), drawn
    /// set by set, in the pass's order within a set.
    pub fn billboards_into(&self, out: &mut BillboardFrame) {
        let vertices = std::sync::Arc::make_mut(&mut out.vertices);
        vertices.clear();
        out.draws.clear();
        let (Some(view), Some(t)) = (self.view.as_ref(), self.thresholds.as_ref()) else {
            return;
        };
        let mut listed: Vec<(u32, &BillboardGroup)> = (self.frame.visuals.iter().flatten())
            .filter_map(|&v| {
                let &k = self.billboard_of.get(v as usize)?;
                let (g, set) = self.billboards.get(k as usize)?;
                Some(((*set)?, g))
            })
            .collect();
        listed.sort_by_key(|&(set, _)| set);
        for (set, g) in listed {
            let first = vertices.len() as u32;
            vertices.extend(
                g.quad(view.eye, g.measure(view.eye), t.t_a, t.t_b)
                    .map(|v| v.packed()),
            );
            match out.draws.last_mut() {
                Some(d) if d.set == set => d.count += QUAD_VERTICES as u32,
                _ => out.draws.push(BillboardDraw {
                    set,
                    first,
                    count: QUAD_VERTICES as u32,
                }),
            }
        }
    }

    /// The static draws of the latest pass's visuals, by draw item (of
    /// `count`).
    pub fn static_draws(&self, count: usize) -> Vec<bool> {
        let mut drawn = Vec::new();
        self.static_draws_into(count, &mut drawn);
        drawn
    }

    /// [`LevelVisibility::static_draws`] into `drawn`, its list kept.
    pub fn static_draws_into(&self, count: usize, drawn: &mut Vec<bool>) {
        drawn.clear();
        drawn.resize(count, false);
        for &v in self.frame.visuals.iter().flatten() {
            let v = v as usize;
            if v + 1 < self.starts.len() {
                for &d in &self.draws[self.starts[v] as usize..self.starts[v + 1] as usize] {
                    if let Some(x) = drawn.get_mut(d as usize) {
                        *x = true;
                    }
                }
            }
        }
    }

    /// The static draws a projected-light cell draws for a receiver in
    /// `sector` with query box `min` … `max` (object-projected-light.md
    /// 4.3), by draw item, into `out`.
    pub fn cell_draws(&self, sector: u16, min: Vec3, max: Vec3, out: &mut Vec<u32>) {
        let mut visuals = Vec::new();
        self.sectors.cell_visuals(sector, min, max, &mut visuals);
        out.clear();
        for v in visuals {
            let v = v as usize;
            if v + 1 < self.starts.len() {
                out.extend(&self.draws[self.starts[v] as usize..self.starts[v + 1] as usize]);
            }
        }
    }

    /// The latest pass's thresholds.
    pub fn thresholds(&self) -> Option<&Thresholds> {
        self.thresholds.as_ref()
    }

    /// The sectors, portals and static visual hierarchy.
    pub fn sectors(&self) -> &Arc<Sectors> {
        &self.sectors
    }

    /// The portal's polygon and bounding-sphere centre.
    pub fn portal(&self, portal: u16) -> Option<(&[Vec3], Vec3)> {
        let p = self.sectors.portals.get(usize::from(portal))?;
        Some((self.sectors.polygon(p), p.centre))
    }

    /// The sector and frusta test and the cached occlusion test of a
    /// dynamic item (object-visibility.md 4 – 6.1) with sphere `sphere`
    /// (centre, radius) and box `min` … `max`: its sector found again from
    /// the sphere's centre when it moved (kept when none is found), then
    /// skipped without one, when its sector was not visited or the sphere
    /// is outside every frustum it was reached with; then the cached box
    /// test, the values from `random`.
    #[allow(clippy::too_many_arguments)]
    pub fn item_seen(
        &mut self,
        item: &mut Item,
        sphere: [f32; 4],
        min: Vec3,
        max: Vec3,
        collision: &CollisionWorld,
        random: &mut Generator,
    ) -> bool {
        let centre = [sphere[0], sphere[1], sphere[2]];
        if item.at != Some(sphere) {
            item.at = Some(sphere);
            if let Some(s) = self.sector_of(collision, centre) {
                item.sector = Some(s);
            }
        }
        let Some(sector) = item.sector else {
            return false;
        };
        self.reached(sector, centre, sphere[3]).is_some()
            && self.cached_visible(&mut item.cache, min, max, random)
    }

    /// The camera pass of `view` (visibility-scope.md 1): the camera's
    /// sector found again when it moved (level.md 9.1), then the grid, the
    /// traversal and the static visuals, the tests' values from `random`.
    pub fn camera_pass(
        &mut self,
        view: View,
        thresholds: &Thresholds,
        collision: &CollisionWorld,
        random: &mut Generator,
    ) {
        let sector = self.camera.update(view.eye, &self.sectors, collision);
        (self.sectors).camera_pass_into(
            (sector, &view, thresholds),
            &mut self.occlusion,
            random,
            &mut self.frame,
            &mut self.buffers,
        );
        self.view = Some(view);
        self.thresholds = Some(*thresholds);
    }

    /// The camera's sector, none until one is found (visibility-scope.md 6).
    pub fn camera_sector(&self) -> Option<u16> {
        self.camera.get()
    }

    /// The latest camera pass's result.
    pub fn frame(&self) -> &CameraFrame {
        &self.frame
    }

    /// The sector the downward ray of level.md 9.1 finds from `p`, if any.
    pub fn sector_of(&self, collision: &CollisionWorld, p: Vec3) -> Option<u16> {
        (self.sectors).sector_below(p, triangle_below(collision, p))
    }

    /// The sector and frusta test (object-visibility.md 5): the first
    /// frustum `sector` was reached with in the latest pass that the
    /// sphere is not outside, none when the sector was not visited.
    pub fn reached(&self, sector: u16, centre: Vec3, radius: f32) -> Option<usize> {
        (self.frame.traversal.frusta.get(usize::from(sector))?)
            .iter()
            .position(|f| f.meets(centre, radius))
    }

    /// The cached occlusion test (occlusion-tests.md 5) of the box `min` …
    /// `max` with the item's cache, in the latest pass's view; visible
    /// before any pass.
    pub fn cached_visible(
        &mut self,
        cache: &mut Cached,
        min: Vec3,
        max: Vec3,
        random: &mut Generator,
    ) -> bool {
        let Some(view) = self.view else {
            return true;
        };
        (self.occlusion).cached_visible(cache, min, max, &view, random)
    }

    /// The cached occlusion test (occlusion-tests.md 5) of the box `min` …
    /// `max` with the item's cache, made at `view`: before the frame's
    /// camera pass it projects with the tests' matrix of the previous
    /// drawing and its scan draws the grid early (2.1 step 3).
    pub fn cached_visible_at(
        &mut self,
        view: &View,
        cache: &mut Cached,
        min: Vec3,
        max: Vec3,
        random: &mut Generator,
    ) -> bool {
        (self.occlusion).cached_visible(cache, min, max, view, random)
    }

    /// Runs `f`, the light passes (occlusion-tests.md 4), with occlusion
    /// off.
    pub fn light_passes<R>(&mut self, f: impl FnOnce() -> R) -> R {
        self.occlusion.light_passes(|_| f())
    }
}

/// What the latest camera pass keeps of the level's draws
/// (visibility-scope.md 5, 6): nothing without a camera sector.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// Which pass filled the lists: changes with each pass.
    pub pass: u64,
    /// By draw item: the static draws of the visuals the pass reached.
    pub items: Vec<bool>,
    /// By object instance: those drawn.
    pub objects: Vec<bool>,
    /// The effect objects drawn, in the pass's order.
    pub effects: Vec<usize>,
    /// The portal fade's triangles, each as a quad of its three corners and
    /// the last again (docs/behavior/portal-fade.md 2).
    pub fade: Vec<ParticleVertex>,
    /// The billboards of the billboard groups drawn (ogf.md 7).
    pub billboards: BillboardFrame,
    /// The projected light texture's cells, every slot's as last given
    /// out, and the receivers with their slots (object-projected-light.md
    /// 3, 4).
    pub cells: Vec<std::sync::Arc<crate::projected_light::CellDraw>>,
    pub receivers: Vec<crate::projected_light::ReceiverSlot>,
}

/// The portal fade of the latest pass of `visibility` (portal-fade.md 1 –
/// 3): each entry of the fade list, nearest first from `eye` (equal
/// distances in list order, unestablished), its polygon's fan in the
/// weather's `ambient` colour with the alpha of its measure.
/// Into `out` (cleared first), with `entries` the list's working copy.
pub fn portal_fade_into(
    visibility: &LevelVisibility,
    eye: Vec3,
    ambient: Vec3,
    out: &mut Vec<ParticleVertex>,
    entries: &mut Vec<(f32, u32, u16, f32)>,
) {
    out.clear();
    entries.clear();
    let Some(t) = visibility.thresholds() else {
        return;
    };
    // The binary's conversion: truncation toward zero, +∞ giving 1.
    let byte = |x: f32| -> u32 {
        if x == f32::INFINITY {
            1
        } else {
            (x as i64).clamp(0, 255) as u32
        }
    };
    let [r, g, b] = ambient.map(|a| byte(255.0 * a));
    entries.extend(
        (visibility.frame.traversal.fade.iter().enumerate()).filter_map(|(i, &(p, m))| {
            let (_, c) = visibility.portal(p)?;
            let d = [c[0] - eye[0], c[1] - eye[1], c[2] - eye[2]];
            Some(((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2], i as u32, p, m))
        }),
    );
    // Equal distances in list order, as a stable sort keeps them, without
    // the stable sort's buffer.
    entries.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for &(_, _, p, m) in entries.iter() {
        let Some((polygon, _)) = visibility.portal(p) else {
            continue;
        };
        let a = byte((1.0 - (m - t.t_b) / (t.t_a - t.t_b)) * 255.5);
        let colour = a << 24 | r << 16 | g << 8 | b;
        let corner = |position: Vec3| ParticleVertex {
            position,
            colour,
            uv: [0.0; 2],
        };
        for i in 1..polygon.len().saturating_sub(1) {
            let (v0, v1, v2) = (polygon[0], polygon[i], polygon[i + 1]);
            out.extend([corner(v0), corner(v1), corner(v2), corner(v2)]);
        }
    }
}
