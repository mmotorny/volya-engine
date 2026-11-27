// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's dynamic lights (docs/behavior/dynamic-lights.md, cited as
//! DL): the light records every source creates and updates (section 2),
//! the level's chunk-6 records and the sun's two directional records
//! (3.1), the lights chosen each frame (4) and each light pass's frustum,
//! fade and constants (5.2, 5.3, 6.3). Drawing is the renderer's.
//!
//! A record's sector is the one the downward ray of `level.md` 9.1 finds
//! from its position ([`Lights::update_sectors`]); rules 1 and 2 of
//! section 4 take the camera pass's sectors and frusta and the cached
//! occlusion test from the caller ([`Lights::choose`]).
//!
//! Sources keep their records for as long as they exist and change them
//! each frame through the setters, which apply the update thresholds of
//! section 2.2 ([`Lights::create`], [`Lights::destroy`]).

use crate::camera::{Mat4, Vec3};
use crate::occlusion::Cached;

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add_scaled(a: Vec3, b: Vec3, k: f32) -> Vec3 {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

/// `v / |v|`; a zero vector stays zero (TODO(#347): DL 2.2 does not say
/// what a zero direction does).
fn normalize(v: Vec3) -> Vec3 {
    let l = length(v);
    if l > 0.0 { v.map(|c| c / l) } else { v }
}

/// `a · b`, row-vector convention.
pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

/// A record's type (DL 2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    Point,
    Spot,
    Directional,
}

/// A light record of the renderer (DL 2.1).
#[derive(Clone, Debug, PartialEq)]
pub struct LightRecord {
    pub kind: LightKind,
    pub position: Vec3,
    /// Stored normalised.
    pub direction: Vec3,
    /// The second ("right") vector, stored normalised; not used by the
    /// passes.
    pub right: Vec3,
    pub range: f32,
    /// Full apex angle, radians (spot lights).
    pub cone: f32,
    /// (R, G, B, A).
    pub colour: [f32; 4],
    pub active: bool,
    /// Stored; no effect on the passes (DL 2.5).
    pub shadow: bool,
    /// Never listed (the level's chunk-6 records, DL 3.1).
    pub excluded: bool,
    /// The projected-texture name (no effect in the first renderer, DL
    /// 2.2, 2.5).
    pub texture: String,
    /// Bounding sphere centre and radius (DL 2.3).
    pub centre: Vec3,
    pub radius: f32,
    /// Its sector (DL 2.4), none until the ray of `level.md` 9.1 first
    /// finds one.
    pub sector: Option<u16>,
    /// Its cached occlusion test (occlusion-tests.md 5; *N* when made is
    /// unestablished: 0, tested when first reached).
    pub occlusion: Cached,
    /// Its position changed since its sector was last determined.
    sector_due: bool,
}

impl Default for LightRecord {
    /// A new record (DL 2.1): game objects' defaults.
    fn default() -> Self {
        let mut r = Self {
            kind: LightKind::Point,
            position: [0.0, -1000.0, 0.0],
            direction: [0.0, -1.0, 0.0],
            right: [0.0; 3],
            range: 8.0,
            // 1.0471976 rad, 60°.
            cone: std::f32::consts::FRAC_PI_3,
            colour: [1.0; 4],
            active: false,
            shadow: true,
            excluded: false,
            texture: String::new(),
            centre: [0.0; 3],
            radius: 0.0,
            sector: None,
            occlusion: Cached::default(),
            sector_due: true,
        };
        r.update_sphere();
        r
    }
}

impl LightRecord {
    /// The bounding sphere of the record's type (DL 2.3).
    fn update_sphere(&mut self) {
        let (p, d, r, theta) = (self.position, self.direction, self.range, self.cone);
        (self.centre, self.radius) = match self.kind {
            LightKind::Spot if theta < std::f32::consts::FRAC_PI_2 => {
                let c = (theta / 2.0).cos();
                let radius = r / (2.0 * c * c);
                (add_scaled(p, d, radius), radius)
            }
            LightKind::Spot => (add_scaled(p, d, r), r * (theta / 2.0).tan()),
            // Directional records are never active (DL 3.1): their sphere
            // is unused; a point's is kept (our choice).
            LightKind::Point | LightKind::Directional => (p, r),
        };
    }
}

/// A record of [`Lights`]: valid until [`Lights::destroy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LightId(u32);

/// The renderer's settings of DL section 8.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSettings {
    /// `r1_dlights`.
    pub dlights: bool,
    /// `r1_dlights_clip`, 10 … 150.
    pub clip: f32,
    /// `r2_sun_lumscale`, −1 … 3.
    pub sun_lumscale: f32,
}

impl Default for LightSettings {
    /// The built-in initial values (DL 8).
    fn default() -> Self {
        Self {
            dlights: true,
            clip: 30.0,
            sun_lumscale: 1.0,
        }
    }
}

impl LightSettings {
    /// The settings from console values as their statuses read (`None`:
    /// unset, the built-in value). A value outside its range is clamped
    /// and one that does not parse is ignored (our choice: the console's
    /// handling of these variables is not specified).
    pub fn from_statuses(
        dlights: Option<&str>,
        clip: Option<&str>,
        sun_lumscale: Option<&str>,
    ) -> Self {
        let d = Self::default();
        let float = |s: Option<&str>, default: f32, lo: f32, hi: f32| {
            s.and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .map_or(default, |v| v.clamp(lo, hi))
        };
        Self {
            dlights: match dlights.map(str::trim) {
                Some("on") | Some("1") => true,
                Some("off") | Some("0") => false,
                _ => d.dlights,
            },
            clip: float(clip, d.clip, 10.0, 150.0),
            sun_lumscale: float(sun_lumscale, d.sun_lumscale, -1.0, 3.0),
        }
    }
}

/// One light pass (DL 5, 6.3): its frustum and constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightPass {
    /// Element 3 (spot) or 2 (point).
    pub spot: bool,
    /// `L_dynamic_pos`: (P, 0.5 / r) point, (P, 1 / r) spot.
    pub position: [f32; 4],
    /// `L_dynamic_color`: (f R, f G, f B, 1).
    pub colour: [f32; 4],
    /// `L_dynamic_xform` = M · T.
    pub xform: Mat4,
    /// M = V · Q_L: the light's frustum is its clip volume.
    pub frustum: Mat4,
    /// The eye O of V (5.3), its traversal's eye (5.4).
    pub eye: Vec3,
}

/// A frustum's six planes (a, b, c, d) with `a x + b y + c z + d ≥ 0`
/// inside, normalised, from a row-vector clip matrix (clip = [p 1] · M,
/// −w ≤ x, y ≤ w, 0 ≤ z ≤ w).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum(pub [[f32; 4]; 6]);

impl Frustum {
    pub fn of(m: &Mat4) -> Self {
        let col = |c: usize| [m[0][c], m[1][c], m[2][c], m[3][c]];
        let (x, y, z, w) = (col(0), col(1), col(2), col(3));
        let plane = |p: [f32; 4]| {
            let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-20);
            p.map(|v| v / l)
        };
        let add = |a: [f32; 4], b: [f32; 4], s: f32| std::array::from_fn(|i| a[i] + s * b[i]);
        Self([
            plane(add(w, x, 1.0)),
            plane(add(w, x, -1.0)),
            plane(add(w, y, 1.0)),
            plane(add(w, y, -1.0)),
            plane(z),
            plane(add(w, z, -1.0)),
        ])
    }

    /// Whether the sphere is not outside: for no plane does its centre lie
    /// outside by more than its radius.
    pub fn meets(&self, centre: Vec3, radius: f32) -> bool {
        self.0.iter().all(|p| {
            let d = p[0] * centre[0] + p[1] * centre[1] + p[2] * centre[2] + p[3];
            d >= -radius
        })
    }
}

/// The D3D left-handed view of DL 5.3: eye `o`, forward `f`, up hint `u`.
pub fn light_view(o: Vec3, f: Vec3, u: Vec3) -> Mat4 {
    let x = normalize(cross(u, f));
    let y = cross(f, x);
    [
        [x[0], y[0], f[0], 0.0],
        [x[1], y[1], f[1], 0.0],
        [x[2], y[2], f[2], 0.0],
        [-dot(o, x), -dot(o, y), -dot(o, f), 1.0],
    ]
}

/// Q_L of DL 5.3: scale `h`, near `n`, far `z_f`.
pub fn light_projection(h: f32, n: f32, z_f: f32) -> Mat4 {
    let k = z_f / (z_f - n);
    [
        [h, 0.0, 0.0, 0.0],
        [0.0, h, 0.0, 0.0],
        [0.0, 0.0, k, 1.0],
        [0.0, 0.0, -k * n, 0.0],
    ]
}

/// The pass's fade f = sat(g), g = 1 − (|E − C| − R) / clip; `None` when
/// the light is skipped: g < 0.00001, or a point light with r < 0.01
/// (DL 5.2).
pub fn fade(record: &LightRecord, eye: Vec3, clip: f32) -> Option<f32> {
    let g = 1.0 - (length(sub(eye, record.centre)) - record.radius) / clip;
    if g < 0.000_01 || (record.kind == LightKind::Point && record.range < 0.01) {
        return None;
    }
    Some(g.clamp(0.0, 1.0))
}

/// The light pass of `record` seen from `eye` (DL 5.2, 5.3, 6.3); `None`
/// when it is skipped.
pub fn light_pass(record: &LightRecord, eye: Vec3, clip: f32) -> Option<LightPass> {
    let f = fade(record, eye, clip)?;
    let (p, r) = (record.position, record.range);
    let spot = record.kind != LightKind::Point;
    let (eye, frustum) = if spot {
        let d = record.direction;
        let up = if d[1].abs() > 0.99 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        (
            p,
            multiply(
                &light_view(p, d, up),
                &light_projection(1.0 / (record.cone / 2.0).tan(), 0.1, r + 1e-7),
            ),
        )
    } else {
        let o = [p[0], p[1] + 300.0, p[2]];
        (
            o,
            multiply(
                &light_view(o, [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
                &light_projection(1.0 / 1f32.to_radians().tan(), 300.0 - r, 300.0 + r),
            ),
        )
    };
    let t: Mat4 = [
        [0.5, 0.0, 0.0, 0.0],
        [0.0, -0.5, 0.0, 0.0],
        [0.0, 0.0, 1.0 / r, 0.0],
        [0.515_625, 0.515_625, 0.0, 1.0],
    ];
    let c = record.colour;
    Some(LightPass {
        spot,
        position: [p[0], p[1], p[2], if spot { 1.0 / r } else { 0.5 / r }],
        colour: [f * c[0], f * c[1], f * c[2], 1.0],
        xform: multiply(&frustum, &t),
        frustum,
        eye,
    })
}

/// The direction of the sun's record B (DL 3.1): normalize(v) with
/// v = L_sun_dir − (0, 0.75, 0), L_sun_dir added to v again (at most 10
/// times) while |v| < 0.001. The objects' sun-visibility ray goes along
/// its negation (DL 11).
pub fn sun_b_direction(sun_dir: Vec3) -> Vec3 {
    let mut v = sub(sun_dir, [0.0, 0.75, 0.0]);
    for _ in 0..10 {
        if length(v) >= 0.001 {
            break;
        }
        v = add_scaled(v, sun_dir, 1.0);
    }
    normalize(v)
}

/// The level's light records (DL 2): every source's, chunk 6's and the
/// sun's.
#[derive(Clone, Debug, Default)]
pub struct Lights {
    records: Vec<Option<LightRecord>>,
    free: Vec<u32>,
    /// The sun's records A and B (DL 3.1), once chunk 6 is read.
    sun: Option<(LightId, LightId)>,
}

impl Lights {
    /// A new record with the defaults of DL 2.1 (inactive).
    pub fn create(&mut self) -> LightId {
        let record = Some(LightRecord::default());
        match self.free.pop() {
            Some(i) => {
                self.records[i as usize] = record;
                LightId(i)
            }
            None => {
                self.records.push(record);
                LightId(self.records.len() as u32 - 1)
            }
        }
    }

    /// Removes a record; its id is then invalid (ids are reused).
    pub fn destroy(&mut self, id: LightId) {
        if let Some(slot) = self.records.get_mut(id.0 as usize)
            && slot.take().is_some()
        {
            self.free.push(id.0);
        }
    }

    /// The record `id`, if it exists.
    pub fn get(&self, id: LightId) -> Option<&LightRecord> {
        self.records.get(id.0 as usize)?.as_ref()
    }

    fn record(&mut self, id: LightId) -> Option<&mut LightRecord> {
        self.records.get_mut(id.0 as usize)?.as_mut()
    }

    /// Every record with its id.
    pub fn iter(&self) -> impl Iterator<Item = (LightId, &LightRecord)> {
        (self.records.iter().enumerate())
            .filter_map(|(i, r)| Some((LightId(i as u32), r.as_ref()?)))
    }

    /// Records that exist.
    pub fn len(&self) -> usize {
        self.records.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Ignored when it differs from the stored position by less than
    /// 0.001 on every axis (DL 2.2).
    pub fn set_position(&mut self, id: LightId, p: Vec3) {
        let Some(r) = self.record(id) else { return };
        if (0..3).all(|k| (p[k] - r.position[k]).abs() < 0.001) {
            return;
        }
        r.position = p;
        r.sector_due = true;
        r.update_sphere();
    }

    /// Stored normalised; the sphere is recomputed only when
    /// 1 − D_new · D_old ≥ 0.00001 (DL 2.3).
    pub fn set_direction(&mut self, id: LightId, d: Vec3) {
        let Some(r) = self.record(id) else { return };
        let d = normalize(d);
        let changed = 1.0 - dot(d, r.direction) >= 0.000_01;
        r.direction = d;
        if changed {
            r.update_sphere();
        }
    }

    /// The second ("right") vector, stored normalised.
    pub fn set_right(&mut self, id: LightId, v: Vec3) {
        if let Some(r) = self.record(id) {
            r.right = normalize(v);
        }
    }

    /// Ignored when |r′ − r| < max(0.1 r, 0.001) (DL 2.2).
    pub fn set_range(&mut self, id: LightId, range: f32) {
        let Some(r) = self.record(id) else { return };
        if (range - r.range).abs() < (0.1 * r.range).max(0.001) {
            return;
        }
        r.range = range;
        r.update_sphere();
    }

    /// Ignored when it differs by less than 0.00001 (DL 2.2).
    pub fn set_cone(&mut self, id: LightId, cone: f32) {
        let Some(r) = self.record(id) else { return };
        if (cone - r.cone).abs() < 0.000_01 {
            return;
        }
        r.cone = cone;
        r.update_sphere();
    }

    /// (R, G, B, A); always stored.
    pub fn set_colour(&mut self, id: LightId, colour: [f32; 4]) {
        if let Some(r) = self.record(id) {
            r.colour = colour;
        }
    }

    /// (R, G, B) with A = 1.
    pub fn set_rgb(&mut self, id: LightId, rgb: Vec3) {
        self.set_colour(id, [rgb[0], rgb[1], rgb[2], 1.0]);
    }

    /// Always stored; the sphere follows the new type at its next
    /// recomputation (DL 2.3 lists no recomputation for a type change).
    pub fn set_kind(&mut self, id: LightId, kind: LightKind) {
        if let Some(r) = self.record(id) {
            r.kind = kind;
        }
    }

    /// An active record takes part in the frame's choice (DL 2.4).
    pub fn set_active(&mut self, id: LightId, active: bool) {
        if let Some(r) = self.record(id) {
            r.active = active;
        }
    }

    /// Stored; no effect (DL 2.5).
    pub fn set_shadow(&mut self, id: LightId, shadow: bool) {
        if let Some(r) = self.record(id) {
            r.shadow = shadow;
        }
    }

    /// The projected-texture name: stored, no effect (DL 2.2, 2.5).
    pub fn set_texture(&mut self, id: LightId, name: &str) {
        if let Some(r) = self.record(id) {
            r.texture = name.to_owned();
        }
    }

    /// Marks a record excluded: never listed (DL 3.1).
    pub fn set_excluded(&mut self, id: LightId, excluded: bool) {
        if let Some(r) = self.record(id) {
            r.excluded = excluded;
        }
    }

    /// The level's chunk-6 records (DL 3.1): each point record an excluded,
    /// active point light (shadow off; colour, position, range as stored;
    /// direction (0, 0, −1)); the directional record the sun's records A
    /// and B (excluded, never active), aimed along its direction. Fails
    /// without a directional record ("Where is sun?").
    pub fn add_level_lights(&mut self, records: &[LevelLight]) -> Result<(), String> {
        for l in records {
            match l {
                LevelLight::Point {
                    colour,
                    position,
                    range,
                } => {
                    let id = self.create();
                    self.set_excluded(id, true);
                    self.set_shadow(id, false);
                    self.set_kind(id, LightKind::Point);
                    self.set_position(id, *position);
                    self.set_range(id, *range);
                    self.set_colour(id, *colour);
                    self.set_direction(id, [0.0, 0.0, -1.0]);
                    self.set_active(id, true);
                }
                LevelLight::Directional { direction } if self.sun.is_none() => {
                    let mut sun = || {
                        let id = self.create();
                        self.set_excluded(id, true);
                        self.set_shadow(id, false);
                        self.set_kind(id, LightKind::Directional);
                        self.set_direction(id, *direction);
                        id
                    };
                    self.sun = Some((sun(), sun()));
                }
                // A second directional record: TODO(#347) (every shipped
                // level has one; which one the game keeps is not stated).
                LevelLight::Directional { .. } => {}
            }
        }
        if self.sun.is_none() {
            return Err("Where is sun?".into());
        }
        Ok(())
    }

    /// The sun's records A and B, once chunk 6 is read.
    pub fn sun(&self) -> Option<(LightId, LightId)> {
        self.sun
    }

    /// Sets the sun's records from the weather (DL 3.1), each frame before
    /// the objects' light inputs: A along `sun_dir` with `sun_color`, B
    /// along [`sun_b_direction`] with `sun_color` × `r2_sun_lumscale`, both
    /// placed 500 m back from `eye` with range 600.
    pub fn update_sun(&mut self, eye: Vec3, sun_dir: Vec3, sun_color: Vec3, lumscale: f32) {
        let Some((a, b)) = self.sun else { return };
        let d_b = sun_b_direction(sun_dir);
        for (id, d, k) in [(a, sun_dir, 1.0), (b, d_b, lumscale)] {
            self.set_direction(id, d);
            self.set_position(id, add_scaled(eye, d, -500.0));
            self.set_rgb(id, sun_color.map(|c| c * k));
            self.set_range(id, 600.0);
        }
    }

    /// Determines the sector of every active record whose position changed
    /// since (DL 2.4): `found` gives the sector the downward ray of
    /// `level.md` 9.1 finds from a point, if any. A record keeps its sector
    /// when the ray finds none.
    pub fn update_sectors(&mut self, mut found: impl FnMut(Vec3) -> Option<u16>) {
        for r in self.records.iter_mut().flatten() {
            if r.active && r.sector_due && !r.excluded {
                r.sector_due = false;
                if let Some(s) = found(r.position) {
                    r.sector = Some(s);
                }
            }
        }
    }

    /// The records the camera pass's spatial query holds (the active
    /// ones, object-visibility.md 2.1) with their sphere centres.
    pub fn candidates(&self) -> impl Iterator<Item = (LightId, Vec3)> + '_ {
        (self.iter())
            .filter(|(_, r)| r.active)
            .map(|(id, r)| (id, r.centre))
    }

    /// Handles record `id` in the camera pass's loop (object-visibility.md
    /// 5, 6.3; DL 4): skipped without a sector; `seen` makes rules 1 and 2
    /// (its sector visited with a frustum the sphere meets, then the cached
    /// occlusion test of the box C ± R); then whether it is listed: not
    /// excluded, `r1_dlights` on and a range of at least 0.1 (rule 4).
    pub fn handle(
        &mut self,
        id: LightId,
        settings: &LightSettings,
        seen: impl FnOnce(Seen<'_>) -> bool,
    ) -> bool {
        let Some(Some(r)) = self.records.get_mut(id.0 as usize) else {
            return false;
        };
        let Some(sector) = r.sector else { return false };
        let (centre, radius) = (r.centre, r.radius);
        let seen = seen(Seen {
            sector,
            centre,
            radius,
            min: centre.map(|v| v - radius),
            max: centre.map(|v| v + radius),
            cache: &mut r.occlusion,
        });
        seen && !r.excluded && settings.dlights && r.range >= 0.1
    }

    /// The listed records `listed`, in the order listed, as the spot list
    /// then the point list (DL 4).
    pub fn lists(&self, listed: &[LightId]) -> Vec<LightId> {
        let mut out = Vec::new();
        self.lists_into(listed, &mut out);
        out
    }

    /// [`Lights::lists`] into `out`, cleared first.
    pub fn lists_into(&self, listed: &[LightId], out: &mut Vec<LightId>) {
        let point = |id: &&LightId| self.get(**id).is_some_and(|r| r.kind == LightKind::Point);
        let spots = listed.iter().filter(|id| !point(id));
        let points = listed.iter().filter(point);
        out.clear();
        out.extend(spots.chain(points).copied());
    }

    /// The lights listed this frame when they alone are handled, nearest
    /// first by their sphere centre (equal distances in record order,
    /// unestablished): [`Lights::handle`] of each, then [`Lights::lists`].
    pub fn choose(
        &mut self,
        settings: &LightSettings,
        eye: Vec3,
        mut seen: impl FnMut(Seen<'_>) -> bool,
    ) -> Vec<LightId> {
        let mut order: Vec<(LightId, f32)> = (self.candidates())
            .map(|(id, c)| {
                let d = sub(c, eye);
                (id, dot(d, d))
            })
            .collect();
        order.sort_by(|a, b| a.1.total_cmp(&b.1));
        let listed: Vec<LightId> = (order.into_iter())
            .filter(|&(id, _)| self.handle(id, settings, &mut seen))
            .map(|(id, _)| id)
            .collect();
        self.lists(&listed)
    }

    /// The frame's light passes (DL 5) of the lights `choose` lists, seen
    /// from `eye`, spot lights first; skipped ones left out.
    pub fn passes(
        &mut self,
        settings: &LightSettings,
        eye: Vec3,
        seen: impl FnMut(Seen<'_>) -> bool,
    ) -> Vec<LightPass> {
        self.choose(settings, eye, seen)
            .into_iter()
            .filter_map(|id| light_pass(self.get(id)?, eye, settings.clip))
            .collect()
    }
}

/// What rules 1 and 2 of DL 4 test of a record: its sector and sphere, its
/// box C ± R and its occlusion cache.
pub struct Seen<'a> {
    pub sector: u16,
    pub centre: Vec3,
    pub radius: f32,
    pub min: Vec3,
    pub max: Vec3,
    pub cache: &'a mut Cached,
}

/// A record of the level's chunk 6 as DL 3.1 reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LevelLight {
    Point {
        colour: [f32; 4],
        position: Vec3,
        range: f32,
    },
    Directional {
        direction: Vec3,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    fn apply(m: &Mat4, p: Vec3) -> [f32; 4] {
        std::array::from_fn(|c| p[0] * m[0][c] + p[1] * m[1][c] + p[2] * m[2][c] + m[3][c])
    }

    #[test]
    fn new_records_take_the_defaults() {
        let mut lights = Lights::default();
        let id = lights.create();
        let r = lights.get(id).unwrap();
        assert_eq!(r.kind, LightKind::Point);
        assert_eq!(r.position, [0.0, -1000.0, 0.0]);
        assert_eq!(r.direction, [0.0, -1.0, 0.0]);
        assert_eq!((r.range, r.colour), (8.0, [1.0; 4]));
        assert!(!r.active && r.shadow && !r.excluded);
        assert!(close(r.cone, 60f32.to_radians(), 1e-6));
        assert_eq!((r.centre, r.radius), ([0.0, -1000.0, 0.0], 8.0));
    }

    #[test]
    fn small_updates_are_ignored() {
        let mut lights = Lights::default();
        let id = lights.create();
        lights.set_position(id, [0.0009, -1000.0005, 0.0]);
        assert_eq!(lights.get(id).unwrap().position, [0.0, -1000.0, 0.0]);
        lights.set_position(id, [0.001, -1000.0, 0.0]);
        assert_eq!(lights.get(id).unwrap().position, [0.001, -1000.0, 0.0]);
        // max(0.1 · 8, 0.001) = 0.8.
        lights.set_range(id, 8.79);
        assert_eq!(lights.get(id).unwrap().range, 8.0);
        lights.set_range(id, 8.8);
        assert_eq!(lights.get(id).unwrap().range, 8.8);
        let cone = lights.get(id).unwrap().cone;
        lights.set_cone(id, cone + 0.000_005);
        assert_eq!(lights.get(id).unwrap().cone, cone);
        lights.set_rgb(id, [0.5, 0.25, 0.0]);
        assert_eq!(lights.get(id).unwrap().colour, [0.5, 0.25, 0.0, 1.0]);
        lights.set_direction(id, [0.0, 0.0, -2.0]);
        assert_eq!(lights.get(id).unwrap().direction, [0.0, 0.0, -1.0]);
    }

    #[test]
    fn spot_spheres_follow_the_cone() {
        // DL 2.3: θ = 60°, r = 8: R = 5.3333, C 5.3333 ahead; θ = 120°:
        // C 8 ahead, R = 13.8564 (DL-N3).
        let mut lights = Lights::default();
        let id = lights.create();
        lights.set_kind(id, LightKind::Spot);
        lights.set_position(id, [1.0, 2.0, 3.0]);
        lights.set_direction(id, [1.0, 0.0, 0.0]);
        let r = lights.get(id).unwrap();
        assert!(close(r.radius, 5.3333, 1e-4));
        assert!(close(r.centre[0], 1.0 + 5.3333, 1e-4));
        lights.set_cone(id, 120f32.to_radians());
        let r = lights.get(id).unwrap();
        assert!(close(r.radius, 13.8564, 1e-3));
        assert!(close(r.centre[0], 9.0, 1e-5));
        // A direction change below the threshold keeps the sphere.
        let before = r.centre;
        lights.set_direction(id, [1.0, 0.002, 0.0]);
        assert_eq!(lights.get(id).unwrap().centre, before);
    }

    #[test]
    fn the_sun_b_direction_at_the_first_level_start() {
        // DL 3.1, DL-N1: (0.6670, −0.6997, 0.2560), altitude 44.4°.
        let d = sun_b_direction([0.9132, -0.2079, 0.3505]);
        for (a, b) in d.iter().zip([0.6670, -0.6997, 0.2560]) {
            assert!(close(*a, b, 1e-4), "{d:?}");
        }
        assert!(close((-d[1]).asin().to_degrees(), 44.4, 0.05));
        // |v| < 0.001: the sun direction added again.
        let d = sun_b_direction([0.0, 0.75, 0.0]);
        assert_eq!(d, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn level_lights_and_the_sun() {
        let mut lights = Lights::default();
        assert_eq!(
            lights.add_level_lights(&[LevelLight::Point {
                colour: [1.0; 4],
                position: [0.0; 3],
                range: 7.0
            }]),
            Err("Where is sun?".into())
        );
        let mut lights = Lights::default();
        lights
            .add_level_lights(&[
                LevelLight::Directional {
                    direction: [0.840, -0.423, 0.340],
                },
                LevelLight::Point {
                    colour: [0.5, 0.5, 0.5, 1.0],
                    position: [1.0, 2.0, 3.0],
                    range: 10.0,
                },
            ])
            .unwrap();
        let (a, b) = lights.sun().unwrap();
        let point = lights
            .iter()
            .find(|(_, r)| r.kind == LightKind::Point)
            .unwrap()
            .1;
        assert!(point.active && point.excluded && !point.shadow);
        assert_eq!((point.position, point.range), ([1.0, 2.0, 3.0], 10.0));
        lights.update_sun([10.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.8, 0.4, 0.2], 2.0);
        let (a, b) = (lights.get(a).unwrap(), lights.get(b).unwrap());
        assert!(!a.active && a.excluded && a.kind == LightKind::Directional);
        assert_eq!(a.position, [10.0, 500.0, 0.0]);
        assert_eq!((a.range, a.colour), (600.0, [0.8, 0.4, 0.2, 1.0]));
        assert_eq!(b.direction, [0.0, -1.0, 0.0]);
        assert_eq!(b.colour, [1.6, 0.8, 0.4, 1.0]);
        // Excluded records are never listed.
        lights.update_sectors(|_| Some(0));
        assert!(chosen(&mut lights, &LightSettings::default()).is_empty());
    }

    /// The lights `lights` lists with every sector visited and nothing
    /// occluded.
    fn chosen(lights: &mut Lights, s: &LightSettings) -> Vec<LightId> {
        lights.choose(s, [0.0; 3], |_| true)
    }

    #[test]
    fn the_frame_lists_spots_then_points() {
        let mut lights = Lights::default();
        let point = lights.create();
        let spot = lights.create();
        lights.set_kind(spot, LightKind::Spot);
        let tiny = lights.create();
        lights.set_range(tiny, 0.05);
        let lost = lights.create();
        for id in [point, spot, tiny, lost] {
            lights.set_active(id, true);
            lights.set_position(id, [0.0, 0.0, if id == lost { 99.0 } else { 0.0 }]);
        }
        lights.update_sectors(|p| (p[2] < 50.0).then_some(0));
        let s = LightSettings::default();
        assert_eq!(chosen(&mut lights, &s), vec![spot, point]);
        let off = LightSettings {
            dlights: false,
            ..s
        };
        assert!(chosen(&mut lights, &off).is_empty());
        // A sector, once found, is kept.
        lights.set_position(point, [0.0, 0.0, 60.0]);
        lights.update_sectors(|p| (p[2] < 50.0).then_some(0));
        assert_eq!(chosen(&mut lights, &s), vec![spot, point]);
        lights.destroy(spot);
        assert_eq!(chosen(&mut lights, &s), vec![point]);
        assert_eq!(lights.create(), spot);
    }

    #[test]
    fn lights_are_tested_by_sector_then_occlusion_nearest_first() {
        let mut lights = Lights::default();
        let (near, far, unseen) = (lights.create(), lights.create(), lights.create());
        for (id, z) in [(far, 5.0), (unseen, 3.0), (near, 1.0)] {
            lights.set_active(id, true);
            lights.set_position(id, [0.0, 0.0, z]);
        }
        // Each in the sector of its z; sector 3 not reached.
        lights.update_sectors(|p| Some(p[2] as u16));
        let mut tested = Vec::new();
        let s = LightSettings::default();
        let got = lights.choose(&s, [0.0; 3], |t| {
            if t.sector == 3 {
                return false;
            }
            tested.push((t.min[2], t.max[2]));
            t.min[2] < -5.0
        });
        // Boxes C ± R (R = 8), the nearest first; the far one hidden.
        assert_eq!(tested, [(-7.0, 9.0), (-3.0, 13.0)]);
        assert_eq!(got, [near]);
        // Tested with r1_dlights off too, and then not listed.
        let off = LightSettings {
            dlights: false,
            ..s
        };
        let mut n = 0;
        let got = lights.choose(&off, [0.0; 3], |_| {
            n += 1;
            true
        });
        assert_eq!((n, got.len()), (3, 0));
    }

    #[test]
    fn the_fade_example() {
        // DL 5.2, DL-N4: R = 3, clip 75: f = 0.3733 at |E − C| = 50,
        // skipped at 80.
        let mut lights = Lights::default();
        let id = lights.create();
        lights.set_position(id, [0.0; 3]);
        lights.set_range(id, 3.0);
        let r = lights.get(id).unwrap();
        assert!(close(
            fade(r, [50.0, 0.0, 0.0], 75.0).unwrap(),
            0.3733,
            1e-4
        ));
        assert!(fade(r, [80.0, 0.0, 0.0], 75.0).is_none());
        assert_eq!(fade(r, [1.0, 0.0, 0.0], 75.0), Some(1.0));
    }

    #[test]
    fn the_point_light_column() {
        // DL 5.3, DL-N2: half-width 300 tan 1° = 5.2365 at the light's
        // height; for r = 10, 5.0620 at its top and 5.4110 at its bottom.
        let mut lights = Lights::default();
        let id = lights.create();
        lights.set_position(id, [0.0, 0.0, 0.0]);
        lights.set_range(id, 10.0);
        let pass = light_pass(lights.get(id).unwrap(), [0.0; 3], 75.0).unwrap();
        assert!(!pass.spot);
        assert_eq!(pass.position, [0.0, 0.0, 0.0, 0.05]);
        assert_eq!(pass.eye, [0.0, 300.0, 0.0]);
        let edge = |y: f32| {
            // The x at which a point at height y leaves the frustum.
            let c = apply(&pass.frustum, [1.0, y, 0.0]);
            1.0 / (c[0] / c[3])
        };
        assert!(close(edge(0.0), 5.2365, 1e-3));
        assert!(close(edge(10.0), 5.0620, 1e-3));
        assert!(close(edge(-10.0), 5.4110, 1e-3));
        let f = Frustum::of(&pass.frustum);
        assert!(f.meets([5.0, 0.0, 0.0], 0.0));
        assert!(!f.meets([5.5, 0.0, 0.0], 0.0));
        assert!(!f.meets([0.0, 10.5, 0.0], 0.0));
        assert!(!f.meets([0.0, -10.5, 0.0], 0.0));
    }

    #[test]
    fn the_spot_depth_coordinate() {
        // DL 6.3, DL-N5: r = 8: t₂ = 0.4937 at depth 4, 0 at 0.1, 1 at r.
        let mut lights = Lights::default();
        let id = lights.create();
        lights.set_kind(id, LightKind::Spot);
        lights.set_position(id, [0.0; 3]);
        lights.set_direction(id, [0.0, 0.0, 1.0]);
        let pass = light_pass(lights.get(id).unwrap(), [0.0; 3], 75.0).unwrap();
        assert!(pass.spot);
        assert_eq!(pass.position[3], 0.125);
        assert_eq!(pass.eye, [0.0; 3]);
        let z = |d: f32| apply(&pass.xform, [0.0, 0.0, d])[2];
        assert!(close(z(4.0), 0.4937, 1e-4));
        assert!(close(z(0.1), 0.0, 1e-6));
        assert!(close(z(8.0), 1.0, 1e-5));
        // On the axis the projected coordinate is the texture's centre,
        // (0.515625, 0.515625) after the divide.
        let s = apply(&pass.xform, [0.0, 0.0, 4.0]);
        assert!(close(s[0] / s[3], 0.515_625, 1e-6));
        assert!(close(s[1] / s[3], 0.515_625, 1e-6));
        // A direction near the vertical takes the other up hint.
        lights.set_direction(id, [0.0, -1.0, 0.0]);
        let pass = light_pass(lights.get(id).unwrap(), [0.0; 3], 75.0).unwrap();
        assert!(pass.frustum.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn settings_from_the_console() {
        let s = LightSettings::from_statuses(None, None, None);
        assert_eq!(s, LightSettings::default());
        let s = LightSettings::from_statuses(Some("off"), Some("75"), Some("9"));
        assert_eq!((s.dlights, s.clip, s.sun_lumscale), (false, 75.0, 3.0));
        let s = LightSettings::from_statuses(Some("on"), Some("5"), Some("x"));
        assert_eq!((s.dlights, s.clip, s.sun_lumscale), (true, 10.0, 1.0));
    }
}
