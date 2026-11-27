// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Detail objects (docs/behavior/detail-objects.md): the cache of 49 × 49
//! cells around the camera, each cell's instances placed from its slot of
//! `level.details` (docs/formats/level-details.md) on the collision
//! triangles, and the per-frame selection of what is drawn, with the drawn
//! size and sway of each instance.

use std::io;

use volya_engine_formats::level_details::{LevelDetails, NO_MODEL, Slot};
use volya_engine_formats::ltx::{Config, value};

use crate::collision::CollisionWorld;
use crate::collision::geometry::box_intersects;
use crate::occlusion::Cached;
use crate::particles::random::Generator;

pub type Vec3 = [f32; 3];
pub type Mat4 = [[f32; 4]; 4];

/// The cache's side in cells (2.2).
pub const CACHE: usize = 49;
/// The groups' side in entries, and the groups per side (2.4).
const GROUP: usize = 4;
const GROUPS: usize = 12;
/// Entries built per update unless every entry is pending (section 3).
const BUILDS_PER_UPDATE: usize = 7;
/// 47.5²: the selection's distance (5.3).
const FAR_SQUARED: f32 = 2_256.25;
/// The fade's slope, `f32` 0x39E87975 (5.3).
const FADE: f32 = f32::from_bits(0x39E8_7975);
/// The jitter per unit of density, `f32` 0x3F169696 (4.1).
const JITTER: f32 = f32::from_bits(0x3F16_9696);
/// 2π / 32,767 (4.2).
const TURN: f32 = 0.000_191_753_45;
/// The placement's seed constant (4.1).
const SEED: u32 = 0x1207_1980;

/// The threshold matrix's source values (4.3).
const Q: [u32; 16] = [0, 14, 3, 13, 11, 5, 8, 6, 12, 2, 15, 1, 7, 9, 4, 10];

/// The threshold matrix T (4.3): first index x, second z.
pub fn threshold_matrix() -> [[u8; 16]; 16] {
    let mut t = [[0u8; 16]; 16];
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let v =
                        Q[4 * c + d] as f32 * 15.875 * 0.0625 + Q[4 * a + b] as f32 * 15.875 + 0.5;
                    t[4 * c + a][4 * d + b] = v as u8;
                }
            }
        }
    }
    t
}

/// The count n of positions per axis for `r__detail_density` `rho` (4.1).
pub fn count(rho: f32) -> u32 {
    (2.0f32 / rho).ceil() as u32
}

/// The five sway values (WA 5.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sway {
    pub amp1: f32,
    pub amp2: f32,
    pub rot1: f32,
    pub rot2: f32,
    pub speed: f32,
}

impl Sway {
    /// Each value `normal` · (1 − *W*) + `fast` · *W*.
    pub fn blend(normal: &Sway, fast: &Sway, w: f32) -> Sway {
        let f = |a: f32, b: f32| crate::weather::wind::sway(a, b, w);
        Sway {
            amp1: f(normal.amp1, fast.amp1),
            amp2: f(normal.amp2, fast.amp2),
            rot1: f(normal.rot1, fast.rot1),
            rot2: f(normal.rot2, fast.rot2),
            speed: f(normal.speed, fast.speed),
        }
    }

    /// The calm (`swing_normal_*`) and windy (`swing_fast_*`) sets of
    /// section `[details]`; a missing key fails, as the game's read does.
    pub fn read(system: &Config) -> io::Result<(Sway, Sway)> {
        let set = |prefix: &str| -> io::Result<Sway> {
            let v = |key: &str| -> io::Result<f32> {
                Ok(value::float(
                    system.read("details", &format!("{prefix}{key}"))?,
                ))
            };
            Ok(Sway {
                amp1: v("amp1")?,
                amp2: v("amp2")?,
                rot1: v("rot1")?,
                rot2: v("rot2")?,
                speed: v("speed")?,
            })
        };
        Ok((set("swing_normal_")?, set("swing_fast_")?))
    }
}

/// What the renderer needs of a model (LD 3.1, DO 4.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelInfo {
    pub still: bool,
    pub min_scale: f32,
    pub max_scale: f32,
    pub bounds: (Vec3, Vec3),
    /// Half the diagonal of the vertex bounds.
    pub radius: f32,
}

/// A placed instance (4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub position: Vec3,
    pub scale: f32,
    /// The rotation's cos θ and sin θ.
    pub rotation: [f32; 2],
    /// 0 still, 1 or 2 a sway.
    pub variant: u8,
    pub sun: f32,
    pub hemi: f32,
}

/// One drawn instance as the renderer reads it: position, drawing scale
/// σ′, cos θ, sin θ, sun factor, hemisphere factor.
pub type DrawnInstance = [f32; 8];

/// The lists drawn: sway 1, sway 2, still (6.1).
pub const SWAY_1: usize = 0;
pub const SWAY_2: usize = 1;
pub const STILL: usize = 2;

/// One draw of a frame: a model's instances of one list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetailDraw {
    pub model: u8,
    /// [`SWAY_1`], [`SWAY_2`] or [`STILL`].
    pub list: u8,
    pub first: u32,
    pub count: u32,
}

/// What a frame draws (5.4, 6.1): the instances, the draws in their order
/// (sway 1 of every model in model order, then sway 2, then still).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailFrame {
    pub instances: std::sync::Arc<Vec<DrawnInstance>>,
    pub draws: Vec<DetailDraw>,
}

/// An axis-aligned box; min > max is empty.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    const EMPTY: Self = Self {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };

    fn is_empty(&self) -> bool {
        (0..3).any(|k| self.min[k] > self.max[k])
    }

    fn add(&mut self, other: &Aabb) {
        for k in 0..3 {
            self.min[k] = self.min[k].min(other.min[k]);
            self.max[k] = self.max[k].max(other.max[k]);
        }
    }

    fn centre(&self) -> Vec3 {
        std::array::from_fn(|k| (self.min[k] + self.max[k]) * 0.5)
    }

    /// Centre and half the diagonal.
    fn sphere(&self) -> [f32; 4] {
        let c = self.centre();
        let d: f32 = (0..3).map(|k| (self.max[k] - self.min[k]).powi(2)).sum();
        [c[0], c[1], c[2], d.sqrt() * 0.5]
    }
}

/// An entry of the cache (2.3).
#[derive(Clone, Debug)]
struct Entry {
    cell: [i32; 2],
    bounds: Aabb,
    sphere: [f32; 4],
    empty: bool,
    pending: bool,
    slot: Slot,
    /// The parts' instances, part after part, each in placement order.
    instances: Vec<Instance>,
    /// Where each part's instances end in `instances`.
    part_ends: [u16; 4],
    /// The frame after which the next refresh is due.
    due: u64,
    /// The lists of the last refresh, part after part, each part's sway 1,
    /// sway 2 and still.
    lists: Vec<DrawnInstance>,
    /// Where each part's lists end in `lists`, by `3 * part + list`.
    list_ends: [u16; 12],
    /// The cached occlusion test (occlusion-tests.md 5, 6): *N* 0 when
    /// made.
    occlusion: Cached,
}

impl Default for Entry {
    /// Fresh: empty, not pending, due at once (2.3).
    fn default() -> Self {
        Self {
            cell: [0; 2],
            bounds: Aabb::EMPTY,
            sphere: [0.0; 4],
            empty: true,
            pending: false,
            slot: Slot::NONE,
            instances: Vec::new(),
            part_ends: [0; 4],
            due: 0,
            lists: Vec::new(),
            list_ends: [0; 12],
            occlusion: Cached::default(),
        }
    }
}

impl Entry {
    /// Part `part`'s range of `instances`.
    fn part(&self, part: usize) -> std::ops::Range<usize> {
        let start = if part == 0 {
            0
        } else {
            self.part_ends[part - 1]
        };
        usize::from(start)..usize::from(self.part_ends[part])
    }

    /// The range of `lists` of part `part`'s list `list`.
    fn list(&self, part: usize, list: usize) -> std::ops::Range<usize> {
        let k = 3 * part + list;
        let start = if k == 0 { 0 } else { self.list_ends[k - 1] };
        usize::from(start)..usize::from(self.list_ends[k])
    }
}

/// An index or a range's end in an entry's instances or lists: at most (n + 1)²
/// instances an entry, n at most 10 at the densities allowed (4.1).
fn at(i: usize) -> u16 {
    u16::try_from(i).expect("an entry's instances fit 16 bits")
}

/// A group of 4 × 4 entries (2.4).
#[derive(Clone, Copy, Debug)]
struct Group {
    bounds: Aabb,
    sphere: [f32; 4],
    empty: bool,
}

/// One pass's inputs (5.1 – 5.3).
#[derive(Clone, Copy, Debug)]
pub struct Pass {
    /// The engine's frame number.
    pub frame: u64,
    pub eye: Vec3,
    /// Row-vector view-projection (clip z from 0 to w).
    pub view_projection: Mat4,
    /// T_s = ssa_min of static-shading.md 10.2 for this frame.
    pub ssa_min: f32,
    /// `r__detail_density` now, read when an entry is built.
    pub density: f32,
}

/// The level's detail objects.
pub struct Details {
    grid: LevelDetails,
    models: Vec<ModelInfo>,
    threshold: [[u8; 16]; 16],
    /// The cache's centre cell.
    centre: [i32; 2],
    /// Physical column and row of logical column 0 and row 0.
    column0: usize,
    row0: usize,
    entries: Vec<Entry>,
    /// Physical indices, in list order.
    pending: Vec<u16>,
    groups: Vec<Group>,
    /// The frame of the last pass, and of the last selection with its
    /// result.
    last_pass: Option<u64>,
    selected: Option<(u64, std::sync::Arc<DetailFrame>)>,
    scratch: Vec<u32>,
    /// Drawing lists per list and model, reused from frame to frame.
    collect: Vec<Vec<DrawnInstance>>,
    /// The entries a selection chose, reused from frame to frame.
    chosen: Vec<usize>,
}

/// Physical index of a cache position.
fn index(column: usize, row: usize) -> usize {
    row * CACHE + column
}

impl Details {
    /// The cache set up for `grid` (2.1, 2.2): centred on cell (0, 0)
    /// (as disputed in review; the first update moves it to the camera),
    /// every entry initialised and pending.
    pub fn new(grid: LevelDetails) -> Self {
        let models = grid
            .models
            .iter()
            .map(|m| {
                let bounds = m.bounds();
                let d: f32 = (0..3).map(|k| (bounds.1[k] - bounds.0[k]).powi(2)).sum();
                ModelInfo {
                    still: m.still(),
                    min_scale: m.min_scale,
                    max_scale: m.max_scale,
                    bounds,
                    radius: d.sqrt() * 0.5,
                }
            })
            .collect::<Vec<_>>();
        let collect = vec![Vec::new(); 3 * models.len()];
        let mut details = Self {
            grid,
            models,
            threshold: threshold_matrix(),
            centre: [0, 0],
            column0: 0,
            row0: 0,
            entries: vec![Entry::default(); CACHE * CACHE],
            pending: Vec::with_capacity(CACHE * CACHE),
            groups: vec![
                Group {
                    bounds: Aabb::EMPTY,
                    sphere: [0.0; 4],
                    empty: true,
                };
                GROUPS * GROUPS
            ],
            last_pass: None,
            selected: None,
            scratch: Vec::new(),
            collect,
            chosen: Vec::new(),
        };
        for m in 0..CACHE {
            for k in 0..CACHE {
                details.init(k, m);
            }
        }
        details
    }

    pub fn grid(&self) -> &LevelDetails {
        &self.grid
    }

    pub fn models(&self) -> &[ModelInfo] {
        &self.models
    }

    /// The cell of logical column `k` and row `m` (2.2).
    fn cell_at(&self, k: usize, m: usize) -> [i32; 2] {
        [
            self.centre[0] - 24 + k as i32,
            self.centre[1] + 24 - m as i32,
        ]
    }

    fn physical(&self, k: usize, m: usize) -> usize {
        index((k + self.column0) % CACHE, (m + self.row0) % CACHE)
    }

    /// Initialises the entry at logical (`k`, `m`) for its cell (2.3).
    fn init(&mut self, k: usize, m: usize) {
        let cell = self.cell_at(k, m);
        let i = self.physical(k, m);
        let slot = self.grid.slot(cell[0], cell[1]);
        let e = &mut self.entries[i];
        e.cell = cell;
        e.slot = slot;
        e.bounds = cell_box(cell, &slot);
        e.sphere = e.bounds.sphere();
        e.empty = slot.is_empty();
        e.instances.clear();
        e.part_ends = [0; 4];
        // Nothing is handed out for an entry built after its last refresh
        // (5.3's recommendation).
        e.lists.clear();
        e.list_ends = [0; 12];
        if !e.pending {
            e.pending = true;
            self.pending.push(i as u16);
        }
    }

    /// The camera's cell (2.2).
    pub fn camera_cell(eye: Vec3) -> [i32; 2] {
        [
            ((eye[0] + 1.0) * 0.5).floor() as i32,
            ((eye[2] + 1.0) * 0.5).floor() as i32,
        ]
    }

    /// The cache's centre cell.
    pub fn centre(&self) -> [i32; 2] {
        self.centre
    }

    /// Entries waiting to be built.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// The cache's update (2.2, 3): the centre steps to the camera's cell,
    /// entries are built, and the groups recomputed if the cell changed.
    pub fn update(
        &mut self,
        eye: Vec3,
        density: f32,
        collision: &CollisionWorld,
        random: &mut Generator,
    ) {
        let cell = Self::camera_cell(eye);
        let mut changed = false;
        while self.centre != cell {
            changed = true;
            if self.centre[0] != cell[0] {
                if cell[0] > self.centre[0] {
                    self.centre[0] += 1;
                    self.column0 = (self.column0 + 1) % CACHE;
                    for m in 0..CACHE {
                        self.init(CACHE - 1, m);
                    }
                } else {
                    self.centre[0] -= 1;
                    self.column0 = (self.column0 + CACHE - 1) % CACHE;
                    for m in 0..CACHE {
                        self.init(0, m);
                    }
                }
            } else if cell[1] > self.centre[1] {
                self.centre[1] += 1;
                self.row0 = (self.row0 + CACHE - 1) % CACHE;
                for k in 0..CACHE {
                    self.init(k, 0);
                }
            } else {
                self.centre[1] -= 1;
                self.row0 = (self.row0 + 1) % CACHE;
                for k in 0..CACHE {
                    self.init(k, CACHE - 1);
                }
            }
        }
        if self.pending.len() == CACHE * CACHE {
            for i in std::mem::take(&mut self.pending) {
                self.build(usize::from(i), density, collision, random);
            }
        } else {
            for _ in 0..BUILDS_PER_UPDATE {
                let Some(at) = self.nearest_pending(eye) else {
                    break;
                };
                let i = self.pending.remove(at);
                self.build(usize::from(i), density, collision, random);
            }
        }
        // The groups after this update's builds (review gap).
        if changed {
            self.recompute_groups();
        }
    }

    /// The pending entry whose box centre is nearest `eye`, first in list
    /// order on a tie.
    fn nearest_pending(&self, eye: Vec3) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (at, &i) in self.pending.iter().enumerate() {
            let c = self.entries[usize::from(i)].bounds.centre();
            let d: f32 = (0..3).map(|k| (c[k] - eye[k]).powi(2)).sum();
            if best.is_none_or(|(_, b)| d < b) {
                best = Some((at, d));
            }
        }
        best.map(|(at, _)| at)
    }

    fn recompute_groups(&mut self) {
        for gm in 0..GROUPS {
            for gk in 0..GROUPS {
                let mut bounds = Aabb::EMPTY;
                let mut empty = true;
                for m in gm * GROUP..(gm + 1) * GROUP {
                    for k in gk * GROUP..(gk + 1) * GROUP {
                        let e = &self.entries[self.physical(k, m)];
                        bounds.add(&e.bounds);
                        empty &= e.empty;
                    }
                }
                self.groups[gm * GROUPS + gk] = Group {
                    bounds,
                    sphere: bounds.sphere(),
                    empty,
                };
            }
        }
    }

    /// Builds the entry at physical index `i` (section 4).
    fn build(
        &mut self,
        i: usize,
        density: f32,
        collision: &CollisionWorld,
        random: &mut Generator,
    ) {
        let e = &mut self.entries[i];
        e.pending = false;
        if e.empty {
            return;
        }
        let (cell, slot, bounds) = (e.cell, e.slot, e.bounds);
        let Some(placed) = self.placement(cell, &slot, bounds, density, collision, random) else {
            return;
        };
        let e = &mut self.entries[i];
        let mut union = Aabb::EMPTY;
        e.instances.clear();
        for (part, instances) in placed.into_iter().enumerate() {
            if let Some(model) = self.models.get(usize::from(slot.ids()[part])) {
                for inst in &instances {
                    union.add(&instance_box(model, inst));
                }
            }
            e.instances.extend_from_slice(&instances);
            e.part_ends[part] = at(e.instances.len());
        }
        e.bounds = union;
        e.sphere = union.sphere();
    }

    /// The instances of `cell` with `slot` and box `bounds` (4.1, 4.2);
    /// `None` when no collision triangle meets the box.
    fn placement(
        &mut self,
        cell: [i32; 2],
        slot: &Slot,
        bounds: Aabb,
        density: f32,
        collision: &CollisionWorld,
        random: &mut Generator,
    ) -> Option<[Vec<Instance>; 4]> {
        let centre = bounds.centre();
        let half: Vec3 = std::array::from_fn(|k| (bounds.max[k] - bounds.min[k]) * 0.5);
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut found = std::mem::take(&mut self.scratch);
        found.clear();
        collision.query(bounds.min, bounds.max, &mut found);
        found.retain(|&t| box_intersects(centre, axes, half, &collision.corners(t)));
        let triangles: Vec<[Vec3; 3]> = found.iter().map(|&t| collision.corners(t)).collect();
        self.scratch = found;
        if triangles.is_empty() {
            return None;
        }
        Some(place(PlaceInput {
            cell,
            slot,
            bounds: (bounds.min, bounds.max),
            triangles: &triangles,
            models: &self.models,
            threshold: &self.threshold,
            density,
            random,
        }))
    }

    /// The instances `cell` places at `density`, per part, outside the
    /// cache (tests; the variants draw from the shared generator).
    pub fn place_cell(
        &mut self,
        cell: [i32; 2],
        density: f32,
        collision: &CollisionWorld,
        random: &mut Generator,
    ) -> [Vec<Instance>; 4] {
        let slot = self.grid.slot(cell[0], cell[1]);
        if slot.is_empty() {
            return Default::default();
        }
        let bounds = cell_box(cell, &slot);
        self.placement(cell, &slot, bounds, density, collision, random)
            .unwrap_or_default()
    }

    /// One pass of frame `pass.frame` (5.1): the cache's update and the
    /// selection when the pass ran in the previous frame; otherwise nothing
    /// is drawn. A second pass in the same frame draws what the first did.
    /// `visible` is the cached occlusion test of an entry's box with its
    /// cache (5.2 step 3).
    // TODO(#407): whether the first frame after loading draws detail
    // objects is unestablished (5.1); here it does not.
    pub fn pass(
        &mut self,
        pass: &Pass,
        collision: &CollisionWorld,
        random: &mut Generator,
        mut visible: impl FnMut(&mut Cached, Vec3, Vec3, &mut Generator) -> bool,
    ) -> std::sync::Arc<DetailFrame> {
        if let Some((frame, selected)) = &self.selected
            && *frame == pass.frame
        {
            return selected.clone();
        }
        let previous = self.last_pass.is_some_and(|f| f + 1 == pass.frame);
        self.last_pass = Some(pass.frame);
        // The latest frame's lists, refilled in place once the frame drawn
        // with them has let them go (docs/architecture/memory.md rule 5).
        let mut frame = match self.selected.take() {
            Some((_, f)) if std::sync::Arc::strong_count(&f) == 1 => f,
            _ => Default::default(),
        };
        let out = std::sync::Arc::get_mut(&mut frame).expect("held once");
        if std::sync::Arc::get_mut(&mut out.instances).is_none() {
            out.instances = Default::default();
        }
        let instances = std::sync::Arc::get_mut(&mut out.instances).expect("held once");
        instances.clear();
        out.draws.clear();
        if previous {
            self.update(pass.eye, pass.density, collision, random);
            self.select(pass, random, &mut visible, instances, &mut out.draws);
        }
        self.selected = Some((pass.frame, frame.clone()));
        frame
    }

    /// The selection (5.2 – 5.4), into the frame's empty `instances` and
    /// `draws`.
    fn select(
        &mut self,
        pass: &Pass,
        random: &mut Generator,
        visible: &mut impl FnMut(&mut Cached, Vec3, Vec3, &mut Generator) -> bool,
        instances: &mut Vec<DrawnInstance>,
        draws: &mut Vec<DetailDraw>,
    ) {
        let planes = frustum(&pass.view_projection);
        let mut chosen = std::mem::take(&mut self.chosen);
        chosen.clear();
        for gm in 0..GROUPS {
            for gk in 0..GROUPS {
                let g = self.groups[gm * GROUPS + gk];
                if g.empty {
                    continue;
                }
                let sphere = test_sphere(&planes, g.sphere);
                if sphere == Side::Outside || !box_in(&planes, &g.bounds) {
                    continue;
                }
                let partial = sphere != Side::Inside;
                for m in gm * GROUP..(gm + 1) * GROUP {
                    for k in gk * GROUP..(gk + 1) * GROUP {
                        let i = self.physical(k, m);
                        let e = &mut self.entries[i];
                        if e.empty || e.bounds.is_empty() {
                            continue;
                        }
                        if partial
                            && (test_sphere(&planes, e.sphere) == Side::Outside
                                || !box_in(&planes, &e.bounds))
                        {
                            continue;
                        }
                        let (min, max) = (e.bounds.min, e.bounds.max);
                        if !visible(&mut e.occlusion, min, max, random) {
                            continue;
                        }
                        chosen.push(i);
                    }
                }
            }
        }
        let models = self.models.len();
        for list in &mut self.collect {
            list.clear();
        }
        for &i in &chosen {
            if pass.frame > self.entries[i].due {
                let c = self.entries[i].sphere;
                let d2: f32 = (0..3).map(|k| (pass.eye[k] - c[k]).powi(2)).sum();
                if d2 > FAR_SQUARED {
                    continue;
                }
                let delay = 15 + u64::from(random.next_k() % 15);
                self.refresh(i, d2, pass.ssa_min);
                self.entries[i].due = pass.frame + delay;
            }
            let e = &self.entries[i];
            for (part, &id) in e.slot.ids().iter().enumerate() {
                if id == NO_MODEL || usize::from(id) >= models {
                    continue;
                }
                for list in 0..3 {
                    self.collect[list * models + usize::from(id)]
                        .extend_from_slice(&e.lists[e.list(part, list)]);
                }
            }
        }
        self.chosen = chosen;
        for list in [SWAY_1, SWAY_2, STILL] {
            for model in 0..models {
                let drawn = &self.collect[list * models + model];
                if drawn.is_empty() {
                    continue;
                }
                draws.push(DetailDraw {
                    model: model as u8,
                    list: list as u8,
                    first: instances.len() as u32,
                    count: drawn.len() as u32,
                });
                instances.extend_from_slice(drawn);
            }
        }
    }

    /// A refresh of entry `i` at squared distance `d2` (5.3 steps 2, 4).
    fn refresh(&mut self, i: usize, d2: f32, ssa_min: f32) {
        let fade = if d2 >= 1.0 {
            1.0 - (d2 - 1.0) * FADE
        } else {
            1.0
        };
        let e = &mut self.entries[i];
        let ids = e.slot.ids();
        e.lists.clear();
        for part in 0..4 {
            let model = self.models.get(usize::from(ids[part]));
            // Each list in turn, its instances in placement order.
            for list in [SWAY_1, SWAY_2, STILL] {
                if let Some(model) = model {
                    let r2 = model.radius * model.radius;
                    for k in e.part(part) {
                        let inst = &e.instances[k];
                        let scale = inst.scale * fade;
                        let q = scale * scale * r2 / d2;
                        if q < ssa_min {
                            continue;
                        }
                        let drawn = if q <= 16.0 * ssa_min {
                            STILL
                        } else {
                            match inst.variant {
                                1 => SWAY_1,
                                2 => SWAY_2,
                                _ => STILL,
                            }
                        };
                        if drawn != list {
                            continue;
                        }
                        let [x, y, z] = inst.position;
                        e.lists.push([
                            x,
                            y,
                            z,
                            scale,
                            inst.rotation[0],
                            inst.rotation[1],
                            inst.sun,
                            inst.hemi,
                        ]);
                    }
                }
                e.list_ends[3 * part + list] = at(e.lists.len());
            }
        }
    }

    /// Every instance placed in the cache now (tests).
    pub fn instances(&self) -> impl Iterator<Item = ([i32; 2], u8, &Instance)> + '_ {
        self.entries.iter().flat_map(|e| {
            let ids = e.slot.ids();
            (0..4).flat_map(move |p| {
                e.instances[e.part(p)]
                    .iter()
                    .map(move |inst| (e.cell, ids[p], inst))
            })
        })
    }

    /// Heap bytes held by the cache's instances and lists (estimate).
    pub fn memory_bytes(&self) -> usize {
        let entries: usize = self
            .entries
            .iter()
            .map(|e| {
                e.instances.capacity() * size_of::<Instance>()
                    + e.lists.capacity() * size_of::<DrawnInstance>()
            })
            .sum();
        self.grid.memory_bytes()
            + entries
            + self.entries.capacity() * size_of::<Entry>()
            + self
                .collect
                .iter()
                .map(|v| v.capacity() * 32)
                .sum::<usize>()
    }
}

/// A cell's box (2.3).
fn cell_box(cell: [i32; 2], slot: &Slot) -> Aabb {
    let (low, top) = (slot.y_low(), slot.y_top());
    Aabb {
        min: [
            2.0 * cell[0] as f32 - 0.001,
            low - 0.001,
            2.0 * cell[1] as f32 - 0.001,
        ],
        max: [
            2.0 * cell[0] as f32 + 2.001,
            top + 0.001,
            2.0 * cell[1] as f32 + 2.001,
        ],
    }
}

/// The inputs of one cell's placement.
pub struct PlaceInput<'a> {
    pub cell: [i32; 2],
    pub slot: &'a Slot,
    /// The entry's box (2.3).
    pub bounds: (Vec3, Vec3),
    /// The collision triangles meeting the box (4.1).
    pub triangles: &'a [[Vec3; 3]],
    pub models: &'a [ModelInfo],
    pub threshold: &'a [[u8; 16]; 16],
    pub density: f32,
    /// The shared generator (the variants).
    pub random: &'a mut Generator,
}

/// The ground height under (`x`, `z`) from `top` (4.2 step 7): the highest
/// hit at or below the start on a triangle whose front faces up, else
/// `bottom` − 5.
fn ground(triangles: &[[Vec3; 3]], x: f32, z: f32, top: f32, bottom: f32) -> f32 {
    let mut y = bottom - 5.0;
    for t in triangles {
        let e1 = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
        let e2 = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
        let cross_y = e1[2] * e2[0] - e1[0] * e2[2];
        if cross_y < 0.000_01 {
            continue;
        }
        // Barycentric coordinates in x and z (edges included).
        let det = e1[0] * e2[2] - e1[2] * e2[0];
        let (dx, dz) = (x - t[0][0], z - t[0][2]);
        let u = (dx * e2[2] - dz * e2[0]) / det;
        let v = (e1[0] * dz - e1[2] * dx) / det;
        if u < 0.0 || v < 0.0 || u + v > 1.0 {
            continue;
        }
        let hit = t[0][1] + u * e1[1] + v * e2[1];
        if hit <= top && hit > y {
            y = hit;
        }
    }
    y
}

/// One draw `r` scaled: lo + r · (hi − lo) / 32,767.
fn between(lo: f32, hi: f32, r: u32) -> f32 {
    lo + r as f32 * (hi - lo) / 32_767.0
}

/// The placement of one cell (4.1, 4.2): per part, its instances in
/// placement order.
pub fn place(input: PlaceInput) -> [Vec<Instance>; 4] {
    let PlaceInput {
        cell,
        slot,
        bounds: (min, max),
        triangles,
        models,
        threshold,
        density,
        random,
    } = input;
    let mut out: [Vec<Instance>; 4] = Default::default();
    let n = count(density);
    let jitter = density * JITTER;
    let seed = (cell[0].wrapping_mul(cell[1]) as u32) ^ SEED;
    let (mut gp, mut gc, mut gs, mut gr) = (
        Generator::new(seed),
        Generator::new(seed),
        Generator::new(seed),
        Generator::new(seed),
    );
    let ids = slot.ids();
    // TODO(#407): an id neither 63 nor below the model count is
    // unestablished; its part is left out.
    let parts: Vec<(usize, [f32; 4])> = (0..4)
        .filter(|&p| ids[p] != NO_MODEL && usize::from(ids[p]) < models.len())
        .map(|p| (p, slot.densities(p).map(|d| f32::from(d) * 17.0)))
        .collect();
    let (sun, hemi) = (slot.sun() as f32 / 15.0, slot.hemi() as f32 / 15.0);
    let nf = n as f32;
    let mut qualifying = Vec::with_capacity(4);
    for j in 0..=n {
        for i in 0..=n {
            let r1 = gp.next_k() % 16;
            let r2 = gp.next_k() % 16;
            let (ic, jc) = (i.min(n - 1), j.min(n - 1));
            let (a, b) = (ic as f32 / nf, jc as f32 / nf);
            let t = threshold[((ic + r1) % 16) as usize][((jc + r2) % 16) as usize];
            qualifying.clear();
            for &(p, [d0, d1, d2, d3]) in &parts {
                let d = d0 * (1.0 - a) * (1.0 - b)
                    + d1 * a * (1.0 - b)
                    + d2 * (1.0 - a) * b
                    + d3 * a * b;
                let v = (d + 0.5).floor().clamp(0.0, 255.0) as u32;
                if v > u32::from(t) {
                    qualifying.push(p);
                }
            }
            let part = match qualifying.len() {
                0 => continue,
                1 => qualifying[0],
                k => qualifying[(gc.next_k() % k as u32) as usize],
            };
            let tz = between(-jitter, jitter, gp.next_k());
            let tx = between(-jitter, jitter, gp.next_k());
            let x = min[0] + 2.0 * i as f32 / nf + tx;
            let z = min[2] + 2.0 * j as f32 / nf + tz;
            let y = ground(triangles, x, z, max[1], min[1]);
            if y < min[1] {
                continue;
            }
            let model = &models[usize::from(ids[part])];
            let scale = between(0.5 * model.min_scale, 0.9 * model.max_scale, gs.next_k());
            let theta = gr.next_k() as f32 * TURN;
            let variant = if model.still {
                0
            } else if random.next_k() % 3 != 0 {
                1
            } else {
                2
            };
            out[part].push(Instance {
                position: [x, y, z],
                scale,
                rotation: [theta.cos(), theta.sin()],
                variant,
                sun,
                hemi,
            });
        }
    }
    out
}

/// The axis-aligned box of `model`'s vertex bounds transformed by the
/// instance (4.4).
fn instance_box(model: &ModelInfo, inst: &Instance) -> Aabb {
    let (lo, hi) = model.bounds;
    let [c, s] = inst.rotation;
    let mut b = Aabb::EMPTY;
    for corner in 0..8 {
        let p = [
            if corner & 1 == 0 { lo[0] } else { hi[0] },
            if corner & 2 == 0 { lo[1] } else { hi[1] },
            if corner & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let q = [
            (p[0] * c + p[2] * s) * inst.scale + inst.position[0],
            p[1] * inst.scale + inst.position[1],
            (-p[0] * s + p[2] * c) * inst.scale + inst.position[2],
        ];
        b.add(&Aabb { min: q, max: q });
    }
    b
}

/// The frustum's left, right, bottom, top and far planes, inside where
/// a x + b y + c z + d ≥ 0, normalised (5.2 step 1: no near plane).
fn frustum(m: &Mat4) -> [[f32; 4]; 5] {
    let col = |c: usize| [m[0][c], m[1][c], m[2][c], m[3][c]];
    let (x, y, z, w) = (col(0), col(1), col(2), col(3));
    let plane = |p: [f32; 4]| {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-20);
        p.map(|v| v / l)
    };
    let add = |a: [f32; 4], b: [f32; 4], s: f32| std::array::from_fn(|i| a[i] + s * b[i]);
    [
        plane(add(w, x, 1.0)),
        plane(add(w, x, -1.0)),
        plane(add(w, y, 1.0)),
        plane(add(w, y, -1.0)),
        plane(add(w, z, -1.0)),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Outside,
    Partial,
    Inside,
}

fn test_sphere(planes: &[[f32; 4]; 5], s: [f32; 4]) -> Side {
    let mut side = Side::Inside;
    for p in planes {
        let d = p[0] * s[0] + p[1] * s[1] + p[2] * s[2] + p[3];
        if d < -s[3] {
            return Side::Outside;
        }
        if d < s[3] {
            side = Side::Partial;
        }
    }
    side
}

/// Whether a box is not outside any plane.
fn box_in(planes: &[[f32; 4]; 5], b: &Aabb) -> bool {
    planes.iter().all(|p| {
        let v: Vec3 = std::array::from_fn(|k| if p[k] >= 0.0 { b.max[k] } else { b.min[k] });
        p[0] * v[0] + p[1] * v[1] + p[2] * v[2] + p[3] >= 0.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_threshold_matrix() {
        // 4.3: values 0 … 253, row 0's start, the sum.
        let t = threshold_matrix();
        assert_eq!(t[0][..8], [0, 222, 48, 206, 14, 236, 62, 220]);
        let all: Vec<u32> = t.iter().flatten().map(|&v| u32::from(v)).collect();
        assert_eq!(all.iter().sum::<u32>(), 32_386);
        assert_eq!((all.iter().min(), all.iter().max()), (Some(&0), Some(&253)));
    }

    #[test]
    fn counts_for_the_densities() {
        // 4.1 and 7.
        let n: Vec<u32> = [0.2, 0.3, 0.4, 0.5, 0.6].map(count).to_vec();
        assert_eq!(n, [10, 7, 5, 4, 4]);
        assert_eq!(JITTER, 0.588_235_26);
    }

    #[test]
    fn the_sway_values_at_half_wind() {
        // WA 5.2 with W = 0.5.
        let normal = Sway {
            amp1: 0.1,
            amp2: 0.05,
            rot1: 30.0,
            rot2: 1.0,
            speed: 2.0,
        };
        let fast = Sway {
            amp1: 0.35,
            amp2: 0.2,
            rot1: 5.0,
            rot2: 0.5,
            speed: 0.5,
        };
        let s = Sway::blend(&normal, &fast, 0.5);
        let got = [s.amp1, s.amp2, s.rot1, s.rot2, s.speed];
        for (g, w) in got.iter().zip([0.225, 0.125, 17.5, 0.75, 1.25]) {
            assert!((g - w).abs() < 1e-6, "{got:?}");
        }
    }

    #[test]
    fn the_ground_is_the_highest_upward_face() {
        // Two floors over a cell, one facing down: the higher upward one.
        let up = |y: f32| [[0.0, y, 0.0], [0.0, y, 2.0], [2.0, y, 0.0]];
        let down = |y: f32| [[0.0, y, 0.0], [2.0, y, 0.0], [0.0, y, 2.0]];
        let tris = [up(1.0), up(3.0), down(2.0)];
        assert_eq!(ground(&tris, 0.5, 0.5, 5.0, 0.0), 3.0);
        // From below the upper floor only the lower is at or below.
        assert_eq!(ground(&tris, 0.5, 0.5, 2.5, 0.0), 1.0);
        // On an edge it counts; outside, the default.
        assert_eq!(ground(&tris, 1.0, 1.0, 5.0, 0.0), 3.0);
        assert_eq!(ground(&tris, 1.5, 1.5, 5.0, 0.0), -5.0);
    }

    #[test]
    fn the_camera_cell() {
        // 2.2: cell c covers x 2c − 1 … 2c + 1.
        assert_eq!(Details::camera_cell([0.0, 0.0, 0.0]), [0, 0]);
        assert_eq!(Details::camera_cell([0.99, 0.0, -1.01]), [0, -1]);
        assert_eq!(Details::camera_cell([1.0, 0.0, -1.0]), [1, 0]);
    }

    #[test]
    fn frustum_tests() {
        // Identity clip matrix: x, y in −w … w, z ≤ w.
        let m: Mat4 = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let p = frustum(&m);
        assert_eq!(test_sphere(&p, [0.0, 0.0, 0.0, 0.5]), Side::Inside);
        assert_eq!(test_sphere(&p, [0.9, 0.0, 0.0, 0.5]), Side::Partial);
        assert_eq!(test_sphere(&p, [3.0, 0.0, 0.0, 0.5]), Side::Outside);
        // No near plane: behind (z < 0) is not cut.
        assert_eq!(test_sphere(&p, [0.0, 0.0, -50.0, 0.5]), Side::Inside);
        let b = Aabb {
            min: [1.2, -0.1, 0.0],
            max: [1.5, 0.1, 0.1],
        };
        assert!(!box_in(&p, &b));
    }
}
