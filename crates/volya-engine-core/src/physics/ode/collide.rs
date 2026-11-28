// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The contact phase of a step (docs/behavior/physics-contacts.md, cited by
//! section; `world` 3, item 3) over an [`OdeSolver`]: every active object's
//! collision pass (1.1) with the other objects of the index (the filter,
//! 1.3; a pair, 1.2) and with the level (section 3), each contact handled
//! (2.3) and made a contact joint of the bodies it joins (6), the groups of
//! a pair merged and a resting object woken (1.2), and the liquid's damping
//! recorded for the bodies' pre-step (2.4).
//!
//! Box and sphere pairs are the library's (`dCollide`, 4.2); cylinders get
//! no contacts yet (TODO(#1010)).
//! Motion rays (1.4) are not cast: only thrown missiles get the ray mark,
//! and none is in the world.

use std::collections::HashSet;
use std::ffi::c_int;

use volya_engine_formats::gamemtl::{GameMaterial, LIQUID, PASS_THROUGH};

use super::{
    BodyId, Contact, ContactGeom, ContactKind, DxBody, DxGeom, Grouped, OdeSolver, Real, Surface,
};
use crate::collision::geometry::{Vec3, dot, length, scale, sub};
use crate::physics::contacts::level::{self, LevelContact};
use crate::physics::contacts::{self, Bounds, Properties, RawContact, overlap, properties};
use crate::physics::shell::Shape;
use crate::physics::{
    Active, MAX_GROUP_BODIES, MAX_GROUP_JOINTS, MAX_PAIR_CONTACTS, Object, Params,
};

unsafe extern "C" {
    fn dCreateSphere(space: *mut std::ffi::c_void, radius: Real) -> *mut DxGeom;
    fn dCreateBox(space: *mut std::ffi::c_void, lx: Real, ly: Real, lz: Real) -> *mut DxGeom;
    fn dGeomSphereSetRadius(g: *mut DxGeom, radius: Real);
    fn dGeomBoxSetLengths(g: *mut DxGeom, lx: Real, ly: Real, lz: Real);
    fn dGeomSetPosition(g: *mut DxGeom, x: Real, y: Real, z: Real);
    fn dGeomSetRotation(g: *mut DxGeom, r: *const Real);
    pub(super) fn dGeomDestroy(g: *mut DxGeom);
    fn dCollide(
        o1: *mut DxGeom,
        o2: *mut DxGeom,
        flags: c_int,
        contact: *mut ContactGeom,
        skip: c_int,
    ) -> c_int;
    fn dBodyGetLinearVel(b: *mut DxBody) -> *const Real;
    fn dBodyAddForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyIsEnabled(b: *mut DxBody) -> c_int;
    fn dBodyGetMass(b: *mut DxBody, m: *mut super::DMass);
}

/// The contacts the library is asked for in a pair (1.2).
const ASKED_CONTACTS: usize = 810;
/// The liquid force's factor and its least `k` (2.4).
const LIQUID_FACTOR: f32 = 10_000.0;
const LIQUID_LEAST: f32 = 1e-7;

/// A geometry of an object at the contact phase (`shells` 2.2): its shape
/// in the world, the body it lies on (none for static geometry), its
/// material and whether it meets the level (3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geom {
    pub shape: Shape,
    pub body: Option<BodyId>,
    pub material: u32,
    pub meets_level: bool,
}

/// A contact being handled (2.3): the contact the joint would carry and
/// whether it is still made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draft {
    pub contact: Contact,
    pub made: bool,
    /// The two materials (first, second).
    pub materials: (u32, u32),
    /// A contact with the level.
    pub level: bool,
}

/// What a body's liquid contacts left for its pre-step (2.4): the damping
/// value, the contact's normal and whether its material is pass-through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Liquid {
    pub value: f32,
    pub normal: Vec3,
    pub pass_through: bool,
}

/// The library's sphere and box geometries the pairs are collided with,
/// two of each (made once, posed for each test).
pub(super) struct Probes {
    spheres: [*mut DxGeom; 2],
    boxes: [*mut DxGeom; 2],
    buffer: Vec<ContactGeom>,
}

impl Probes {
    pub(super) fn new() -> Self {
        // SAFETY: geometries in no space; destroyed in `drop`.
        unsafe {
            Self {
                spheres: [
                    dCreateSphere(std::ptr::null_mut(), 1.0),
                    dCreateSphere(std::ptr::null_mut(), 1.0),
                ],
                boxes: [
                    dCreateBox(std::ptr::null_mut(), 1.0, 1.0, 1.0),
                    dCreateBox(std::ptr::null_mut(), 1.0, 1.0, 1.0),
                ],
                buffer: vec![ContactGeom::default(); ASKED_CONTACTS],
            }
        }
    }

    /// The library's geometry `slot` posed as `shape` (a sphere or box).
    fn pose(&mut self, slot: usize, shape: &Shape) -> *mut DxGeom {
        // SAFETY: the probes are live; the rotation is a dMatrix3.
        unsafe {
            match *shape {
                Shape::Sphere { centre, radius } => {
                    let g = self.spheres[slot];
                    dGeomSphereSetRadius(g, radius);
                    dGeomSetPosition(g, centre[0], centre[1], centre[2]);
                    g
                }
                Shape::Box {
                    axes,
                    centre,
                    half_size,
                } => {
                    let g = self.boxes[slot];
                    let [x, y, z] = half_size.map(|h| 2.0 * h);
                    dGeomBoxSetLengths(g, x, y, z);
                    dGeomSetPosition(g, centre[0], centre[1], centre[2]);
                    let mut r = [0.0; 12];
                    for (k, a) in axes.iter().enumerate() {
                        for i in 0..3 {
                            r[i * 4 + k] = a[i];
                        }
                    }
                    dGeomSetRotation(g, r.as_ptr());
                    g
                }
                Shape::Cylinder { .. } => unreachable!("not the library's"),
            }
        }
    }
}

impl Drop for Probes {
    fn drop(&mut self) {
        for g in self.spheres.into_iter().chain(self.boxes) {
            // SAFETY: made in `new`, in no space.
            unsafe { dGeomDestroy(g) };
        }
    }
}

impl OdeSolver {
    /// The contacts of geometry `a` (first) with `b` (second), their
    /// normals from `b` towards `a` (4.2, 4.3), at most `max`.
    pub fn pair_contacts(&mut self, a: &Shape, b: &Shape, max: usize, out: &mut Vec<RawContact>) {
        out.clear();
        match (a, b) {
            (Shape::Cylinder { .. }, _) | (_, Shape::Cylinder { .. }) => {
                // Cylinders' colliders wait (TODO(#1010)): none.
            }
            _ => {
                let probes = &mut self.probes;
                let (ga, gb) = (probes.pose(0, a), probes.pose(1, b));
                let asked = ASKED_CONTACTS as c_int;
                // SAFETY: posed probes; the buffer holds `asked` contacts.
                let n = unsafe {
                    dCollide(
                        ga,
                        gb,
                        asked,
                        probes.buffer.as_mut_ptr(),
                        size_of::<ContactGeom>() as c_int,
                    )
                };
                out.extend(
                    probes.buffer[..n.max(0) as usize]
                        .iter()
                        .take(max)
                        .map(|g| RawContact {
                            position: [g.pos[0], g.pos[1], g.pos[2]],
                            normal: [g.normal[0], g.normal[1], g.normal[2]],
                            depth: g.depth,
                        }),
                );
            }
        }
    }

    /// The game materials contacts are made with (`cform` 3).
    pub fn set_materials(&mut self, materials: Vec<GameMaterial>) {
        self.materials = materials;
    }

    /// The level's mesh its shapes collide with (section 3), and its
    /// materials.
    pub fn set_level(&mut self, level: std::sync::Arc<crate::collision::CollisionWorld>) {
        self.materials = level.materials().to_vec();
        self.level = Some(level);
    }

    /// A body's linear velocity, if it is this solver's.
    pub fn linear_velocity(&self, body: BodyId) -> Option<Vec3> {
        if !self.bodies.contains_key(&body) {
            return None;
        }
        // SAFETY: the body is this solver's.
        let v = unsafe { std::slice::from_raw_parts(dBodyGetLinearVel(body.0), 3) };
        Some([v[0], v[1], v[2]])
    }

    fn material(&self, index: u32) -> Option<&GameMaterial> {
        self.materials.get(index as usize)
    }

    /// 2.2 – 2.3, item 1: a contact's draft with materials `a` (first) and
    /// `b` (second) at step `h`.
    fn draft(&self, c: &RawContact, materials: (u32, u32), level: bool, h: f32) -> Option<Draft> {
        let (a, b) = (self.material(materials.0)?, self.material(materials.1)?);
        let p: Properties = properties(a, b, h);
        Some(Draft {
            contact: Contact {
                kind: ContactKind::Ordinary,
                surface: Surface {
                    mode: p.mode,
                    mu: p.mu,
                    mu2: 0.0,
                    bounce: p.bounce,
                    bounce_vel: p.bounce_speed,
                    soft_erp: p.erp,
                    soft_cfm: p.cfm,
                    ..Surface::default()
                },
                position: c.position,
                normal: c.normal,
                depth: c.depth,
                fdir1: [0.0; 3],
            },
            made: !(level && a.has(PASS_THROUGH)),
            materials,
            level,
        })
    }

    /// 2.3, item 7: a made contact as a joint of the merged group `root`
    /// attached to `b1` and `b2` (none for the fixed world).
    fn join(&mut self, root: u32, c: &Contact, b1: Option<BodyId>, b2: Option<BodyId>) {
        let raw = c.raw();
        let world = self.group_mut(root).world;
        let body = |b: Option<BodyId>| b.map_or(std::ptr::null_mut(), |b| b.0);
        // SAFETY: the joint group and world are live; the bodies this
        // solver's and listed in `world`.
        let joint = unsafe {
            let make = match c.kind {
                ContactKind::Ordinary => super::dJointCreateContact,
                ContactKind::Special => super::dJointCreateContactSpecial,
            };
            let j = make(std::ptr::null_mut(), self.contacts, &raw);
            super::dWorldAddJoint(world, j);
            super::dJointAttach(j, body(b1), body(b2));
            j
        };
        let g = self.group_mut(root);
        g.contacts.push(joint);
        g.joints += 1;
        self.made.push(joint);
    }

    /// The merged group of an object's group, if live.
    fn merged_group(&self, group: super::GroupId) -> Option<u32> {
        self.group(group)?;
        Some(self.root(group.0))
    }

    /// Whether a geometry's records hold a pass-through triangle (dropped,
    /// 2.3 item 4) and whether any is left (item 6).
    fn drop_passing(&self, state: Option<&mut level::LevelState>) -> bool {
        let Some(state) = state else {
            return false;
        };
        if let Some(world) = &self.level {
            state
                .records
                .retain(|r| !world.material(r.triangle).has(PASS_THROUGH));
        }
        state.inside()
    }

    /// 1.2: the pair of the active object at `i` and the object at `j`;
    /// returns the contact joints made.
    fn pair<O: Grouped>(
        &mut self,
        params: &Params,
        objects: &mut Active<'_, O>,
        i: usize,
        j: usize,
    ) -> usize {
        let (Some(ra), Some(rb)) = (
            self.merged_group(objects.at(i).group()),
            self.merged_group(objects.at(j).group()),
        ) else {
            return 0;
        };
        let (ga, gb) = (
            self.groups[ra as usize].as_ref(),
            self.groups[rb as usize].as_ref(),
        );
        let (Some(ga), Some(gb)) = (ga, gb) else {
            return 0;
        };
        // Two objects already merged are one group (#884's reading).
        let (joints, bodies) = if ra == rb {
            (ga.joints, ga.bodies)
        } else {
            (ga.joints + gb.joints, ga.bodies + gb.bodies)
        };
        if joints >= MAX_GROUP_JOINTS || bodies >= MAX_GROUP_BODIES {
            return 0;
        }
        let limit = MAX_GROUP_JOINTS - joints;
        let mut geoms_a = std::mem::take(&mut self.scratch_a);
        let mut geoms_b = std::mem::take(&mut self.scratch_b);
        geoms_a.clear();
        geoms_b.clear();
        objects.at(i).geometries(self, &mut geoms_a);
        objects.at(j).geometries(self, &mut geoms_b);
        let mut raw = std::mem::take(&mut self.scratch_raw);
        let mut made = 0;
        let mut asked = 0;
        let (a, b) = objects.pair_mut(i, j);
        'pairs: for (ka, ea) in geoms_a.iter().enumerate() {
            let bounds_a = contacts::bounds(&ea.shape);
            for (kb, eb) in geoms_b.iter().enumerate() {
                if asked >= MAX_PAIR_CONTACTS {
                    break 'pairs;
                }
                if !overlap(&bounds_a, &contacts::bounds(&eb.shape)) {
                    continue;
                }
                self.pair_contacts(&ea.shape, &eb.shape, MAX_PAIR_CONTACTS - asked, &mut raw);
                asked += raw.len();
                for c in &raw {
                    let Some(mut d) = self.draft(c, (ea.material, eb.material), false, params.step)
                    else {
                        continue;
                    };
                    b.geometry_contact(kb, false, &mut d);
                    a.geometry_contact(ka, true, &mut d);
                    let inside_b = self.drop_passing(b.level_state(kb));
                    let inside_a = self.drop_passing(a.level_state(ka));
                    b.object_contact(kb, false, &mut d);
                    a.object_contact(ka, true, &mut d);
                    if inside_a || inside_b {
                        d.contact.surface.mu = f32::INFINITY;
                    }
                    if d.made && made < limit {
                        if made == 0 && ra != rb {
                            self.merge(ra, rb);
                        }
                        self.join(ra, &d.contact, ea.body, eb.body);
                        made += 1;
                    }
                }
            }
        }
        self.scratch_a = geoms_a;
        self.scratch_b = geoms_b;
        self.scratch_raw = raw;
        made
    }

    /// Section 3: the level pass of the active object at `i`.
    fn level_pass<O: Grouped>(&mut self, params: &Params, objects: &mut Active<'_, O>, i: usize) {
        let Some(world) = self.level.clone() else {
            return;
        };
        let Some(root) = self.merged_group(objects.at(i).group()) else {
            return;
        };
        let mut geoms = std::mem::take(&mut self.scratch_a);
        geoms.clear();
        objects.at(i).geometries(self, &mut geoms);
        let mut found: Vec<LevelContact> = Vec::new();
        let o = objects.at_mut(i);
        for (k, g) in geoms.iter().enumerate() {
            if !g.meets_level {
                continue;
            }
            let velocity = g.body.and_then(|b| self.linear_velocity(b));
            let Some(state) = o.level_state(k) else {
                continue;
            };
            found.clear();
            level::collide(
                &world,
                &g.shape,
                velocity,
                state,
                MAX_PAIR_CONTACTS,
                &mut found,
            );
            for lc in &found {
                let material = world.material_index(lc.triangle);
                let Some(mut d) =
                    self.draft(&lc.contact, (material, g.material), true, params.step)
                else {
                    continue;
                };
                let m = world.material(lc.triangle);
                // 2.4: the liquid's damping, without a crossed record.
                // Without a body the game stops ("static - static collision
                // !!!"): every geometry here has one.
                if m.has(LIQUID)
                    && !o.level_state(k).is_some_and(|s| s.inside())
                    && let Some(body) = g.body
                {
                    let value = 1.0 - m.liquid_factor;
                    let e = self.liquid.entry(body).or_insert(Liquid {
                        value: f32::NEG_INFINITY,
                        normal: [0.0; 3],
                        pass_through: false,
                    });
                    if value > e.value {
                        *e = Liquid {
                            value,
                            normal: d.contact.normal,
                            pass_through: m.has(PASS_THROUGH),
                        };
                    }
                }
                o.geometry_contact(k, false, &mut d);
                let inside = self.drop_passing(o.level_state(k));
                o.object_contact(k, false, &mut d);
                if inside {
                    d.contact.surface.mu = f32::INFINITY;
                }
                let joints = self.groups[root as usize]
                    .as_ref()
                    .map_or(usize::MAX, |g| g.joints);
                if d.made && joints < MAX_GROUP_JOINTS {
                    self.join(root, &d.contact, None, g.body);
                }
            }
        }
        self.scratch_a = geoms;
    }

    /// Item 3 (1.1): every active object's collision pass, in their order.
    pub(super) fn contact_phase<O: Grouped + Object>(
        &mut self,
        params: &Params,
        objects: &mut Active<'_, O>,
    ) {
        let mut passed = HashSet::new();
        let mut i = 0;
        while i < objects.len() {
            if !objects.is_active(i) {
                i += 1;
                continue;
            }
            let id = objects.id(i);
            let class = objects.at(i).class();
            if let Some(bounds) = objects.at(i).bounds() {
                let others: Vec<_> = (0..objects.len())
                    .filter(|&j| j != i && objects.indexed(j) && !passed.contains(&objects.id(j)))
                    .filter(|&j| {
                        let o = objects.at(j);
                        o.bounds().is_some_and(|b| overlap(&bounds, &b))
                            && !class.excludes(o.class())
                    })
                    .map(|j| objects.id(j))
                    .collect();
                for other in others {
                    let Some(j) = objects.position(other) else {
                        continue;
                    };
                    i = objects.position(id).unwrap_or(i);
                    if self.pair(params, objects, i, j) > 0 && !objects.is_active(j) {
                        let at = objects.wake(j);
                        objects.at_mut(at).wake(self);
                    }
                }
                i = objects.position(id).unwrap_or(i);
                if !class.ignores_static() {
                    self.level_pass(params, objects, i);
                }
            }
            passed.insert(id);
            i += 1;
        }
    }

    /// 2.4: the liquid's force on the bodies its contacts recorded, at step
    /// `h`, before the solving (the objects' pre-step); the values go.
    pub(super) fn liquid_forces(&mut self, h: f32) {
        for (body, l) in std::mem::take(&mut self.liquid) {
            if !self.bodies.contains_key(&body) {
                continue;
            }
            // SAFETY: the body is this solver's.
            unsafe {
                if dBodyIsEnabled(body.0) == 0 {
                    continue;
                }
                let v = std::slice::from_raw_parts(dBodyGetLinearVel(body.0), 3);
                let v = [v[0], v[1], v[2]];
                let mut m = super::DMass::default();
                dBodyGetMass(body.0, &mut m);
                let k = (l.value * l.value * LIQUID_FACTOR * length(v)).min(m.mass / h);
                if k < LIQUID_LEAST {
                    continue;
                }
                let mut f = scale(v, -k);
                if !l.pass_through {
                    f = sub(f, scale(l.normal, dot(f, l.normal)));
                }
                dBodyAddForce(body.0, f[0], f[1], f[2]);
            }
        }
    }
}

/// The union of geometries' bounds: an object's box (1.1).
pub fn bounds_of(geoms: &[Geom]) -> Option<Bounds> {
    geoms
        .iter()
        .map(|g| contacts::bounds(&g.shape))
        .reduce(|a, b| contacts::union(&a, &b))
}
