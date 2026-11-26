// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The camera pass's sectors and portals in the first renderer: the camera
//! sector (docs/formats/level.md 9.1, docs/behavior/visibility-scope.md 6),
//! the traversal through portals with its options
//! (level.md 9.2, docs/behavior/traversal-options.md), and the static
//! visuals of each visited sector (level.md 9.3, docs/formats/ogf.md 8).
//!
//! Frusta are sets of planes `(a, b, c, d)`, inside where
//! `a x + b y + c z + d ≥ 0`. The occlusion tests the camera pass makes
//! (visibility-scope.md 3.3) are not specified yet: [`portal_hidden`] and
//! [`visual_hidden`] stand for them and hide nothing, so what is kept is a
//! superset of what the original keeps (visibility-scope.md 8).

use std::io;

use crate::camera::{Mat4, Vec3};
use crate::collision::CollisionWorld;
use crate::collision::geometry::{box_intersects, cross, dot, ray_triangle_distance, sub};
use volya_engine_formats::level::{Level, Thresholds};
use volya_engine_formats::ogf::{VisualType, coverage_factor};

/// The downward ray of level.md 9.1, metres.
pub const SECTOR_RAY: f32 = 500.0;
/// A portal is taken over a collision triangle unless the triangle is
/// nearer by more than this (level.md 9.1).
const PORTAL_PREFERENCE: f32 = 1e-5;
/// The camera sector is re-determined only after a move by more than this
/// on an axis (level.md 9.1).
const MOVED: f32 = 1e-7;
/// Half-size of the box around the camera that marks portals as touched
/// (level.md 9.2 rule 2).
const TOUCH: f32 = 0.002;

/// A plane `(a, b, c, d)`, inside where `a x + b y + c z + d ≥ 0`.
pub type Plane = [f32; 4];

fn distance(p: &Plane, v: Vec3) -> f32 {
    p[0] * v[0] + p[1] * v[1] + p[2] * v[2] + p[3]
}

/// A convex volume bounded by planes.
#[derive(Clone, Debug, PartialEq)]
pub struct Frustum {
    pub planes: Vec<Plane>,
}

impl Frustum {
    /// The camera pass's view frustum from the view-projection `m`
    /// (row-vector clip = (p, 1) · M, −w ≤ x, y ≤ w, z ≤ w): the left,
    /// right, bottom, top and far planes and no near plane
    /// (visibility-scope.md 4).
    pub fn view(m: &Mat4) -> Self {
        let col = |c: usize| [m[0][c], m[1][c], m[2][c], m[3][c]];
        let (x, y, z, w) = (col(0), col(1), col(2), col(3));
        let plane = |p: Plane| {
            let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-20);
            p.map(|v| v / l)
        };
        let add = |a: Plane, b: Plane, s: f32| std::array::from_fn(|i| a[i] + s * b[i]);
        Self {
            planes: vec![
                plane(add(w, x, 1.0)),
                plane(add(w, x, -1.0)),
                plane(add(w, y, 1.0)),
                plane(add(w, y, -1.0)),
                plane(add(w, z, -1.0)),
            ],
        }
    }

    /// Whether the sphere is not outside: for no plane does its centre lie
    /// outside by more than its radius.
    pub fn meets(&self, centre: Vec3, radius: f32) -> bool {
        self.planes.iter().all(|p| distance(p, centre) >= -radius)
    }

    /// The convex `polygon` clipped against every plane; fewer than three
    /// vertices left is empty (our choice: such a remainder has no area).
    pub fn clip(&self, polygon: &[Vec3]) -> Vec<Vec3> {
        let mut poly = polygon.to_vec();
        let mut next = Vec::with_capacity(poly.len() + 4);
        for p in &self.planes {
            next.clear();
            for (i, &a) in poly.iter().enumerate() {
                let b = poly[(i + 1) % poly.len()];
                let (da, db) = (distance(p, a), distance(p, b));
                if da >= 0.0 {
                    next.push(a);
                }
                if (da >= 0.0) != (db >= 0.0) {
                    let t = da / (da - db);
                    next.push(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t));
                }
            }
            std::mem::swap(&mut poly, &mut next);
            if poly.len() < 3 {
                return Vec::new();
            }
        }
        poly
    }

    /// The frustum narrowed to the clipped `polygon` seen from `eye`: the
    /// planes through the eye and the polygon's edges (level.md 9.2), with
    /// `far`, the start frustum's last plane (our choice, #481: the spec names
    /// only the edge planes; for the view frustum it is the far plane, which
    /// keeps the narrowed volume within the view's). Edges too short to
    /// give a plane are left out.
    fn through(eye: Vec3, polygon: &[Vec3], far: Option<Plane>) -> Self {
        let n = polygon.len() as f32;
        let centre: Vec3 = std::array::from_fn(|k| polygon.iter().map(|v| v[k]).sum::<f32>() / n);
        let mut planes = Vec::with_capacity(polygon.len() + 1);
        for (i, &a) in polygon.iter().enumerate() {
            let b = polygon[(i + 1) % polygon.len()];
            let c = cross(sub(a, eye), sub(b, eye));
            let len = dot(c, c).sqrt();
            if len.is_nan() || len <= 1e-12 {
                continue;
            }
            let mut p = [c[0] / len, c[1] / len, c[2] / len, 0.0];
            p[3] = -(p[0] * eye[0] + p[1] * eye[1] + p[2] * eye[2]);
            if distance(&p, centre) < 0.0 {
                p = p.map(|v| -v);
            }
            planes.push(p);
        }
        planes.extend(far);
        Self { planes }
    }
}

/// A portal with the values the traversal derives (level.md 5).
#[derive(Clone, Debug, PartialEq)]
pub struct Portal {
    pub sector_a: u16,
    pub sector_b: u16,
    pub polygon: Vec<Vec3>,
    /// Plane normal and offset: sector A lies where `n·p + d > 0`.
    pub normal: Vec3,
    pub d: f32,
    pub centre: Vec3,
    pub radius: f32,
}

impl Portal {
    /// The sector other than `sector`.
    fn other(&self, sector: u16) -> u16 {
        if sector == self.sector_a {
            self.sector_b
        } else {
            self.sector_a
        }
    }

    fn side(&self, p: Vec3) -> f32 {
        dot(self.normal, p) + self.d
    }

    /// The triangles of the polygon (a fan).
    fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        let v = &self.polygon;
        (1..v.len().saturating_sub(1)).map(move |i| [v[0], v[i], v[i + 1]])
    }
}

/// What the traversal and the visuals' descent need of one static visual.
#[derive(Clone, Debug, PartialEq)]
struct Node {
    kind: VisualType,
    centre: Vec3,
    radius: f32,
    box_min: Vec3,
    box_max: Vec3,
    /// Billboard groups' coverage factor (ogf.md 7), else 1.
    coverage: f32,
    children: Vec<u32>,
}

/// A level's sectors, portals and static visual hierarchy, as the camera
/// pass reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sectors {
    pub portals: Vec<Portal>,
    /// Each sector's portal list.
    pub sector_portals: Vec<Vec<u16>>,
    /// Each sector's root visual.
    pub roots: Vec<u32>,
    nodes: Vec<Node>,
}

/// Which of the traversal's options are on (traversal-options.md 1); the
/// screen-rectangle mode is used by no pass (4) and is not offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Bit 1: rule 6, the occlusion test of the clipped portal.
    pub occlusion: bool,
    /// Bit 2: rule 4's *T*\_min test.
    pub size: bool,
    /// Bit 8: the fade list and the *T*\_B test (only with `size`).
    pub fade: bool,
}

/// The camera pass's options (traversal-options.md 4).
pub const CAMERA_PASS: Options = Options {
    occlusion: true,
    size: true,
    fade: true,
};

/// The result of one traversal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Traversal {
    /// The sectors entered, in the order of their first entry.
    pub visited: Vec<u16>,
    /// By sector: every frustum it was entered with (empty when not
    /// visited).
    pub frusta: Vec<Vec<Frustum>>,
    /// The fade list: portals collected for the fade with their measure
    /// (traversal-options.md 2.2); empty without the fade option.
    pub fade: Vec<(u16, f32)>,
}

/// The camera pass's result for one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraFrame {
    pub traversal: Traversal,
    /// By visited sector (aligned with `traversal.visited`): the static
    /// visuals drawn from it, each visual once a frame. A mesh or tree is
    /// drawn; a billboard group listed here has its billboard drawn
    /// (ogf.md 8.2).
    pub visuals: Vec<Vec<u32>>,
}

/// Whether the occlusion grid hides the clipped portal `polygon`
/// (traversal-options.md 2.3; visibility-scope.md 3.3).
// TODO(#460): the polygon test is not specified yet (V3, V4); it hides
// nothing.
pub fn portal_hidden(_polygon: &[Vec3]) -> bool {
    false
}

/// Whether the occlusion grid hides a static visual's box `min` … `max`
/// (ogf.md 8.1; visibility-scope.md 3.3).
// TODO(#460): the box test and its caching are not specified yet (V3, V4);
// it hides nothing.
pub fn visual_hidden(_min: Vec3, _max: Vec3) -> bool {
    false
}

impl Sectors {
    /// The sectors, portals and visual hierarchy of `level`; fails on an
    /// invalid portal (level.md 5) or a reference out of range.
    pub fn new(level: &Level) -> io::Result<Self> {
        let invalid = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
        let sectors = level.sectors.len();
        let portals = level
            .portals
            .iter()
            .map(|p| {
                if usize::from(p.sector_a) >= sectors || usize::from(p.sector_b) >= sectors {
                    return Err(invalid("portal's sector out of range"));
                }
                let (normal, d) = p.plane()?;
                let (centre, radius) = p.sphere();
                Ok(Portal {
                    sector_a: p.sector_a,
                    sector_b: p.sector_b,
                    polygon: p.vertices.clone(),
                    normal,
                    d,
                    centre,
                    radius,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let visuals = level.visuals.len();
        for s in &level.sectors {
            if s.portals.iter().any(|&p| usize::from(p) >= portals.len()) {
                return Err(invalid("sector's portal out of range"));
            }
            if s.root as usize >= visuals {
                return Err(invalid("sector's root visual out of range"));
            }
        }
        let nodes = level
            .visuals
            .iter()
            .map(|v| {
                let h = &v.header;
                if v.children.iter().any(|&c| c as usize >= visuals) {
                    return Err(invalid("visual's child out of range"));
                }
                Ok(Node {
                    kind: h.kind,
                    centre: h.sphere_centre,
                    radius: h.sphere_radius,
                    box_min: h.box_min,
                    box_max: h.box_max,
                    coverage: if h.kind == VisualType::BillboardGroup {
                        coverage_factor(h)
                    } else {
                        1.0
                    },
                    children: v.children.clone(),
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            portals,
            sector_portals: level.sectors.iter().map(|s| s.portals.clone()).collect(),
            roots: level.sectors.iter().map(|s| s.root).collect(),
            nodes,
        })
    }

    /// The nearest portal the downward ray from `origin` meets within
    /// [`SECTOR_RAY`], with the sector it puts `origin` in (level.md 9.1).
    pub fn portal_below(&self, origin: Vec3) -> Option<(f32, u16)> {
        let down = [0.0, -1.0, 0.0];
        let mut best: Option<(f32, u16)> = None;
        for p in &self.portals {
            let hit = p
                .triangles()
                .filter_map(|t| ray_triangle_distance(origin, down, &t))
                .filter(|t| (0.0..=SECTOR_RAY).contains(t))
                .min_by(f32::total_cmp);
            if let Some(t) = hit
                && best.is_none_or(|(b, _)| t < b)
            {
                let sector = if p.side(origin) > 0.0 {
                    p.sector_a
                } else {
                    p.sector_b
                };
                best = Some((t, sector));
            }
        }
        best
    }

    /// The sector the downward ray from `origin` finds, from the nearest
    /// portal and the nearest collision triangle `triangle` gives (distance
    /// and sector), if any (level.md 9.1).
    pub fn sector_below(&self, origin: Vec3, triangle: Option<(f32, u16)>) -> Option<u16> {
        match (self.portal_below(origin), triangle) {
            (Some((p, s)), Some((t, _))) if t >= p - PORTAL_PREFERENCE => Some(s),
            (Some((_, s)), None) => Some(s),
            (_, Some((_, s))) => Some(s),
            (None, None) => None,
        }
    }

    /// Traverses from sector `start` with `frustum`, seen from `eye`, with
    /// `options` and the frame's `thresholds` (level.md 9.2,
    /// traversal-options.md 1 – 2). The start frustum's last plane is kept
    /// in every narrowed one (its far plane for [`Frustum::view`]). `hidden` is the occlusion test of a
    /// clipped portal (rule 6), asked only with the occlusion option.
    pub fn traverse(
        &self,
        start: u16,
        frustum: Frustum,
        eye: Vec3,
        options: Options,
        thresholds: &Thresholds,
        mut hidden: impl FnMut(&[Vec3]) -> bool,
    ) -> Traversal {
        // Portals the box around the eye meets are touched (rule 2).
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let touched = (self.portals.iter())
            .map(|p| {
                p.triangles()
                    .any(|t| box_intersects(eye, axes, [TOUCH; 3], &t))
            })
            .collect();
        let mut walk = Walk {
            sectors: self,
            start,
            eye,
            options,
            thresholds,
            far: frustum.planes.last().copied(),
            passed: vec![false; self.portals.len()],
            touched,
            hidden: &mut hidden,
            out: Traversal {
                visited: Vec::new(),
                frusta: vec![Vec::new(); self.sector_portals.len()],
                fade: Vec::new(),
            },
        };
        walk.enter(start, frustum);
        walk.out
    }

    /// The static visuals of the visited sectors (level.md 9.3, ogf.md 8)
    /// seen from `eye`: each root descended once per frustum its sector
    /// was entered with, culled by bounding sphere and by `hidden` (the
    /// occlusion test of a visual's box), and each visual drawn once.
    pub fn static_visuals(
        &self,
        traversal: &Traversal,
        eye: Vec3,
        thresholds: &Thresholds,
        mut hidden: impl FnMut(Vec3, Vec3) -> bool,
    ) -> Vec<Vec<u32>> {
        let mut drawn = vec![false; self.nodes.len()];
        let mut stack = Vec::new();
        traversal
            .visited
            .iter()
            .map(|&s| {
                let mut out = Vec::new();
                for f in &traversal.frusta[usize::from(s)] {
                    stack.push(self.roots[usize::from(s)]);
                    while let Some(i) = stack.pop() {
                        let n = &self.nodes[i as usize];
                        if !f.meets(n.centre, n.radius) || hidden(n.box_min, n.box_max) {
                            continue;
                        }
                        let d = sub(n.centre, eye);
                        let sigma = n.radius / (dot(d, d) + 1e-5) * n.coverage;
                        let mut draw = |out: &mut Vec<u32>| {
                            if !std::mem::replace(&mut drawn[i as usize], true) {
                                out.push(i);
                            }
                        };
                        match n.kind {
                            VisualType::Group => stack.extend(n.children.iter().rev()),
                            VisualType::BillboardGroup => {
                                if sigma < thresholds.t_min {
                                    continue;
                                }
                                if sigma < thresholds.t_a {
                                    draw(&mut out);
                                }
                                if sigma > thresholds.t_b {
                                    stack.extend(n.children.iter().rev());
                                }
                            }
                            _ => {
                                if sigma > thresholds.t_min {
                                    draw(&mut out);
                                }
                            }
                        }
                    }
                }
                out
            })
            .collect()
    }

    /// The camera pass for one frame (visibility-scope.md 1): from the
    /// camera sector `sector` with the view frustum of `view_projection`
    /// and eye `eye`, with the camera pass's options. Without a camera
    /// sector nothing is visited (6).
    pub fn camera_pass(
        &self,
        sector: Option<u16>,
        view_projection: &Mat4,
        eye: Vec3,
        thresholds: &Thresholds,
    ) -> CameraFrame {
        let Some(sector) = sector.filter(|&s| usize::from(s) < self.roots.len()) else {
            return CameraFrame::default();
        };
        let traversal = self.traverse(
            sector,
            Frustum::view(view_projection),
            eye,
            CAMERA_PASS,
            thresholds,
            portal_hidden,
        );
        let visuals = self.static_visuals(&traversal, eye, thresholds, visual_hidden);
        CameraFrame { traversal, visuals }
    }
}

/// One traversal in progress.
struct Walk<'a, H> {
    sectors: &'a Sectors,
    start: u16,
    eye: Vec3,
    options: Options,
    thresholds: &'a Thresholds,
    far: Option<Plane>,
    passed: Vec<bool>,
    touched: Vec<bool>,
    hidden: &'a mut H,
    out: Traversal,
}

impl<H: FnMut(&[Vec3]) -> bool> Walk<'_, H> {
    /// Enters `sector` with `frustum` and goes on through its portals by
    /// rules 1 – 6 of level.md 9.2.
    fn enter(&mut self, sector: u16, frustum: Frustum) {
        let s = usize::from(sector);
        // The lists are emptied at a sector's first entry in a traversal
        // (traversal-options.md 3, as disputed in review).
        if self.out.frusta[s].is_empty() {
            self.out.visited.push(sector);
        }
        self.out.frusta[s].push(frustum.clone());
        for &p in &self.sectors.sector_portals[s] {
            let portal = &self.sectors.portals[usize::from(p)];
            let p = usize::from(p);
            // Rule 1.
            if self.passed[p] {
                continue;
            }
            // Rule 2, its skip against the start sector
            // (traversal-options.md 2.4).
            let touched = self.touched[p];
            let target = if touched {
                portal.other(sector)
            } else if portal.side(self.eye) <= 0.0 {
                portal.sector_a
            } else {
                portal.sector_b
            };
            if !touched && (target == sector || target == self.start) {
                continue;
            }
            // Rule 3.
            if !frustum.meets(portal.centre, portal.radius) {
                continue;
            }
            // Rule 4 (traversal-options.md 2.1, 2.2).
            if self.options.size {
                let m = self.measure(portal);
                if m < self.thresholds.t_min {
                    continue;
                }
                if self.options.fade {
                    if m < self.thresholds.t_a {
                        self.out.fade.push((p as u16, m));
                    }
                    if m < self.thresholds.t_b {
                        continue;
                    }
                }
            }
            // Rule 5.
            let clipped = frustum.clip(&portal.polygon);
            if clipped.is_empty() {
                continue;
            }
            // Rule 6 (traversal-options.md 2.3).
            if self.options.occlusion && (self.hidden)(&clipped) {
                continue;
            }
            self.passed[p] = true;
            self.touched[p] = false;
            // Our choice (#481): through a touched portal the eye lies (almost) in
            // the portal's plane, where the planes through it and the edges
            // close to that plane and leave almost nothing; the current
            // frustum is kept (the spec does not say how such a portal
            // narrows it).
            let narrowed = if touched {
                frustum.clone()
            } else {
                Frustum::through(self.eye, &clipped, self.far)
            };
            self.enter(target, narrowed);
        }
    }

    /// The portal's screen measure `m = r² / D² × |cos|` (rule 4); a portal
    /// centred on the eye is as large as can be (our choice).
    fn measure(&self, portal: &Portal) -> f32 {
        let v = sub(portal.centre, self.eye);
        let d2 = dot(v, v);
        if d2.is_nan() || d2 <= 0.0 {
            return f32::INFINITY;
        }
        let cos = (dot(portal.normal, v) / d2.sqrt()).abs();
        portal.radius * portal.radius / d2 * cos
    }
}

/// The camera sector (level.md 9.1): none after a level loads, kept when
/// the ray finds nothing, re-determined only when the camera moved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraSector {
    sector: Option<u16>,
    at: Option<Vec3>,
}

impl CameraSector {
    /// The sector, none until a ray first finds one.
    pub fn get(&self) -> Option<u16> {
        self.sector
    }

    /// The sector with the camera at `eye`: re-determined from `sectors`
    /// and `collision` when it moved by more than 10⁻⁷ on an axis.
    pub fn update(
        &mut self,
        eye: Vec3,
        sectors: &Sectors,
        collision: &CollisionWorld,
    ) -> Option<u16> {
        let moved = self
            .at
            .is_none_or(|c| (0..3).any(|k| (c[k] - eye[k]).abs() > MOVED));
        if moved {
            self.at = Some(eye);
            if let Some(s) = sectors.sector_below(eye, triangle_below(collision, eye)) {
                self.sector = Some(s);
            }
        }
        self.sector
    }
}

/// The nearest collision triangle the downward ray from `origin` meets
/// within [`SECTOR_RAY`] (either side), its distance and sector.
pub fn triangle_below(collision: &CollisionWorld, origin: Vec3) -> Option<(f32, u16)> {
    let down = [0.0, -1.0, 0.0];
    collision
        .ray_nearest(origin, down, SECTOR_RAY, |t| {
            ray_triangle_distance(origin, down, &collision.corners(t))
                .filter(|d| (0.0..=SECTOR_RAY).contains(d))
        })
        .map(|(d, t)| (d, collision.sector(t)))
}

#[cfg(test)]
mod tests;
