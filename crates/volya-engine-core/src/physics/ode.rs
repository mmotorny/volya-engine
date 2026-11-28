// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The solver behind [`Solver`]: the Open Dynamics Engine 0.5.0, the shipped
//! library's base, its core built in single precision by build.rs
//! (docs/architecture/simulation.md), through the few of its functions the
//! world uses.
//!
//! The groups of docs/behavior/physics-world.md 4.2 are the library's
//! worlds, which the shipped library makes plain lists of bodies and joints
//! (physics-library.md 3.1; that the game uses them so is believed there):
//! each group lists its object's bodies; a contact between two objects moves
//! the bodies of one group into the other's for the rest of the step, and
//! every group still listing bodies is stepped as one. After the solving
//! every group lists its own bodies again and the contacts are taken out of
//! the lists.
//!
//! A [`OdeSolver`] owns its groups and the step's contacts, and is used from
//! the thread that made it (it is neither `Send` nor `Sync`). The library
//! keeps global state: the world parameters are library-wide (the shipped
//! library's, physics-library.md 3.1; creating a world resets them) and the
//! row shuffle draws from the C run time's generator, so making a group,
//! and the parameters and steps of a solving, take [`STEPPING`].

#![allow(unsafe_code)]

pub mod harness;
#[cfg(test)]
mod harness_rules;
#[cfg(test)]
mod harness_rules_2;
pub mod msvc_rand;
pub mod x87;

use std::collections::HashMap;
use std::ffi::c_int;
use std::sync::Mutex;

/// Held by each world's parameters and step (above).
static STEPPING: Mutex<()> = Mutex::new(());

use super::{Active, MAX_GROUP_BODIES, MAX_GROUP_JOINTS, MAX_PAIR_CONTACTS, Params, Solver};
use crate::collision::geometry::Vec3;

/// ODE's `dReal`: single precision (`dSINGLE`, csrc/ode/include/ode/config.h).
pub type Real = f32;

#[repr(C)]
struct DxWorld {
    _private: [u8; 0],
}

#[repr(C)]
struct DxBody {
    _private: [u8; 0],
}

#[repr(C)]
struct DxJoint {
    _private: [u8; 0],
}

#[repr(C)]
struct DxJointGroup {
    _private: [u8; 0],
}

#[repr(C)]
struct DxGeom {
    _private: [u8; 0],
}

/// ODE's `dMass`: the mass, its centre (`dVector3`) and inertia
/// (`dMatrix3`, rows of four).
#[repr(C)]
#[derive(Default)]
struct DMass {
    mass: Real,
    c: [Real; 4],
    i: [Real; 12],
}

/// A contact's surface: ODE's `dSurfaceParameters` (the mode's bits and the
/// values they enable).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surface {
    pub mode: c_int,
    pub mu: Real,
    pub mu2: Real,
    pub bounce: Real,
    pub bounce_vel: Real,
    pub soft_erp: Real,
    pub soft_cfm: Real,
    pub motion1: Real,
    pub motion2: Real,
    pub slip1: Real,
    pub slip2: Real,
}

/// ODE's `dContactGeom`.
#[repr(C)]
#[derive(Clone, Copy)]
struct ContactGeom {
    pos: [Real; 4],
    normal: [Real; 4],
    depth: Real,
    g1: *mut DxGeom,
    g2: *mut DxGeom,
}

impl Default for ContactGeom {
    fn default() -> Self {
        Self {
            pos: [0.0; 4],
            normal: [0.0; 4],
            depth: 0.0,
            g1: std::ptr::null_mut(),
            g2: std::ptr::null_mut(),
        }
    }
}

/// ODE's `dContact`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DContact {
    surface: Surface,
    geom: ContactGeom,
    fdir1: [Real; 4],
}

/// The two kinds of contact joint (physics-scope.md 1.1): the library's,
/// and the shipped library's added one (physics-library.md 5.2, item 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContactKind {
    #[default]
    Ordinary,
    Special,
}

/// A contact of a step, as contact generation gives it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Contact {
    pub kind: ContactKind,
    pub surface: Surface,
    pub position: Vec3,
    /// From the second body towards the first.
    pub normal: Vec3,
    pub depth: Real,
    /// The first friction direction, where the surface's mode asks for one.
    pub fdir1: Vec3,
}

impl Contact {
    fn raw(&self) -> DContact {
        let [px, py, pz] = self.position;
        let [nx, ny, nz] = self.normal;
        let [fx, fy, fz] = self.fdir1;
        DContact {
            surface: self.surface,
            geom: ContactGeom {
                pos: [px, py, pz, 0.0],
                normal: [nx, ny, nz, 0.0],
                depth: self.depth,
                ..ContactGeom::default()
            },
            fdir1: [fx, fy, fz, 0.0],
        }
    }
}

unsafe extern "C" {
    fn dWorldCreate() -> *mut DxWorld;
    fn dWorldDestroy(w: *mut DxWorld);
    fn dWorldSetGravity(w: *mut DxWorld, x: Real, y: Real, z: Real);
    fn dWorldSetERP(w: *mut DxWorld, erp: Real);
    fn dWorldSetCFM(w: *mut DxWorld, cfm: Real);
    fn dWorldSetQuickStepNumIterations(w: *mut DxWorld, num: c_int);
    fn dWorldSetQuickStepW(w: *mut DxWorld, over_relaxation: Real);
    fn dWorldSetContactSurfaceLayer(w: *mut DxWorld, depth: Real);
    fn dWorldSetContactMaxCorrectingVel(w: *mut DxWorld, vel: Real);
    fn dWorldQuickStep(w: *mut DxWorld, stepsize: Real);
    fn dWorldAddBody(w: *mut DxWorld, b: *mut DxBody);
    fn dWorldRemoveBody(w: *mut DxWorld, b: *mut DxBody);
    fn dWorldAddJoint(w: *mut DxWorld, j: *mut DxJoint);
    fn dWorldRemoveJoint(w: *mut DxWorld, j: *mut DxJoint);

    fn dJointGroupCreate(max_size: c_int) -> *mut DxJointGroup;
    fn dJointGroupDestroy(g: *mut DxJointGroup);
    fn dJointGroupEmpty(g: *mut DxJointGroup);
    fn dJointCreateContact(
        w: *mut DxWorld,
        g: *mut DxJointGroup,
        c: *const DContact,
    ) -> *mut DxJoint;
    fn dJointCreateContactSpecial(
        w: *mut DxWorld,
        g: *mut DxJointGroup,
        c: *const DContact,
    ) -> *mut DxJoint;
    fn dJointAttach(j: *mut DxJoint, b1: *mut DxBody, b2: *mut DxBody);

    fn dBodyCreate(w: *mut DxWorld) -> *mut DxBody;
    fn dBodyDestroy(b: *mut DxBody);
    fn dBodySetPosition(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetPosition(b: *mut DxBody) -> *const Real;
    fn dBodySetMass(b: *mut DxBody, mass: *const DMass);
    fn dBodySetGravityMode(b: *mut DxBody, mode: c_int);
    fn dMassSetSphereTotal(m: *mut DMass, total: Real, radius: Real);
}

/// A body of an [`OdeSolver`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(*mut DxBody);

/// A group of an [`OdeSolver`]: one object's (4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GroupId(u32);

/// What an object of a world over an [`OdeSolver`] has besides
/// [`super::Object`]: its group.
pub trait Grouped {
    fn group(&self) -> GroupId;
}

/// A group: its list, its own bodies (in the order made) and, while the
/// step lasts, what is merged into it.
struct Group {
    world: *mut DxWorld,
    own: Vec<*mut DxBody>,
    /// The group this one is merged into this step (itself if none).
    parent: u32,
    /// The groups merged into this one this step, itself first (while it
    /// is its own parent).
    members: Vec<u32>,
    /// The bodies and joints (contacts counted) of the merged group.
    bodies: usize,
    joints: usize,
    /// The contacts in its list.
    contacts: Vec<*mut DxJoint>,
}

/// The groups of a level's world and the contacts of its step.
pub struct OdeSolver {
    groups: Vec<Option<Group>>,
    contacts: *mut DxJointGroup,
    /// The step's contacts.
    made: Vec<*mut DxJoint>,
    bodies: HashMap<BodyId, GroupId>,
}

impl OdeSolver {
    pub fn new() -> Self {
        // SAFETY: ODE 0.5.0 needs no initialisation; it fails fatally
        // rather than return null.
        let contacts = unsafe { dJointGroupCreate(0) };
        Self {
            groups: Vec::new(),
            contacts,
            made: Vec::new(),
            bodies: HashMap::new(),
        }
    }

    /// A new, empty group for an object.
    pub fn add_group(&mut self) -> GroupId {
        // Creating a world resets the library-wide parameters; a step sets
        // them again.
        let world = {
            let _turn = STEPPING.lock().unwrap_or_else(|e| e.into_inner());
            // SAFETY: as in `new`.
            unsafe { dWorldCreate() }
        };
        let index = self
            .groups
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.groups.len());
        let group = Group {
            world,
            own: Vec::new(),
            parent: index as u32,
            members: vec![index as u32],
            bodies: 0,
            joints: 0,
            contacts: Vec::new(),
        };
        if index == self.groups.len() {
            self.groups.push(Some(group));
        } else {
            self.groups[index] = Some(group);
        }
        GroupId(index as u32)
    }

    /// Removes a group with its bodies; another solver's is ignored. Not
    /// while a step's contacts are made.
    pub fn remove_group(&mut self, id: GroupId) {
        let Some(group) = self.groups.get_mut(id.0 as usize).and_then(Option::take) else {
            return;
        };
        for &b in &group.own {
            self.bodies.remove(&BodyId(b));
        }
        // SAFETY: the world is this solver's and lists the group's own
        // bodies only (no step is under way); destroying it destroys them.
        unsafe { dWorldDestroy(group.world) };
    }

    fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(id.0 as usize)?.as_ref()
    }

    fn group_mut(&mut self, index: u32) -> &mut Group {
        self.groups[index as usize]
            .as_mut()
            .unwrap_or_else(|| unreachable!("a live group's index"))
    }

    /// Adds to `group` a body at `position` with the mass `mass` (kg) of a
    /// uniform sphere of `radius` (m); `None` without the group.
    pub fn add_sphere(
        &mut self,
        group: GroupId,
        position: Vec3,
        mass: f32,
        radius: f32,
    ) -> Option<BodyId> {
        let g = self.groups.get_mut(group.0 as usize)?.as_mut()?;
        let mut m = DMass::default();
        // SAFETY: the world is live; `m` is a valid dMass.
        let body = unsafe {
            let body = dBodyCreate(g.world);
            dMassSetSphereTotal(&mut m, mass, radius);
            dBodySetMass(body, &m);
            dBodySetPosition(body, position[0], position[1], position[2]);
            body
        };
        g.own.push(body);
        g.bodies += 1;
        let id = BodyId(body);
        self.bodies.insert(id, group);
        Some(id)
    }

    /// Removes a body of this solver; another is ignored.
    pub fn remove(&mut self, body: BodyId) {
        let Some(group) = self.bodies.remove(&body) else {
            return;
        };
        let g = self.group_mut(group.0);
        g.own.retain(|&b| b != body.0);
        g.bodies -= 1;
        // SAFETY: the body is this solver's and not yet destroyed.
        unsafe { dBodyDestroy(body.0) };
    }

    /// A body's gravity switch (1.4).
    pub fn set_gravity(&mut self, body: BodyId, on: bool) {
        if self.bodies.contains_key(&body) {
            // SAFETY: the body is this solver's.
            unsafe { dBodySetGravityMode(body.0, c_int::from(on)) };
        }
    }

    /// A body's position, if it is this solver's.
    pub fn position(&self, body: BodyId) -> Option<Vec3> {
        if !self.bodies.contains_key(&body) {
            return None;
        }
        // SAFETY: the body is this solver's; ODE returns its dVector3.
        let p = unsafe { std::slice::from_raw_parts(dBodyGetPosition(body.0), 3) };
        Some([p[0], p[1], p[2]])
    }

    fn root(&self, mut index: u32) -> u32 {
        loop {
            let parent = self.groups[index as usize]
                .as_ref()
                .map_or(index, |g| g.parent);
            if parent == index {
                return index;
            }
            index = parent;
        }
    }

    /// The merged group of a body's group, its bodies and its joints.
    fn merged(&self, body: BodyId) -> Option<(u32, usize, usize)> {
        let root = self.root(self.bodies.get(&body)?.0);
        let g = self.groups[root as usize].as_ref()?;
        Some((root, g.bodies, g.joints))
    }

    /// Merges group `from` (merged groups both) into `into` for the rest of
    /// the step: its bodies and contacts move to `into`'s list.
    fn merge(&mut self, into: u32, from: u32) {
        let (members, contacts, bodies, joints) = {
            let g = self.group_mut(from);
            g.parent = into;
            (
                std::mem::take(&mut g.members),
                std::mem::take(&mut g.contacts),
                g.bodies,
                g.joints,
            )
        };
        let (from_world, into_world) = (self.group_mut(from).world, self.group_mut(into).world);
        for &m in &members {
            for &b in &self.group_mut(m).own.clone() {
                // SAFETY: the body is listed in `from`'s world; both live.
                unsafe {
                    dWorldRemoveBody(from_world, b);
                    dWorldAddBody(into_world, b);
                }
            }
        }
        for &j in &contacts {
            // SAFETY: as above, for the contact.
            unsafe {
                dWorldRemoveJoint(from_world, j);
                dWorldAddJoint(into_world, j);
            }
        }
        let g = self.group_mut(into);
        g.members.extend(members);
        g.contacts.extend(contacts);
        g.bodies += bodies;
        g.joints += joints;
    }

    /// Step item 3 (4.2): up to [`MAX_PAIR_CONTACTS`] of `contacts`, the
    /// contacts of one pair of geometries, between body `a` and body `b`
    /// of another object, or the level's geometry when `b` is `None`.
    /// Between two objects a contact is made, and their groups merged, only
    /// while the two groups together have fewer than [`MAX_GROUP_JOINTS`]
    /// joints and fewer than [`MAX_GROUP_BODIES`] bodies; with the level only
    /// while the group has fewer than [`MAX_GROUP_JOINTS`] joints. Returns
    /// the contacts made; none for a body not of this solver.
    pub fn add_contacts(&mut self, a: BodyId, b: Option<BodyId>, contacts: &[Contact]) -> usize {
        let mut made = 0;
        for c in contacts.iter().take(MAX_PAIR_CONTACTS) {
            let Some((ra, bodies_a, joints_a)) = self.merged(a) else {
                return made;
            };
            let root = match b {
                None => {
                    if joints_a >= MAX_GROUP_JOINTS {
                        break;
                    }
                    ra
                }
                Some(b) => {
                    let Some((rb, bodies_b, joints_b)) = self.merged(b) else {
                        return made;
                    };
                    // Two objects already merged are one group (our
                    // reading of 4.2's "the two groups together", #884).
                    let (bodies, joints) = if ra == rb {
                        (bodies_a, joints_a)
                    } else {
                        (bodies_a + bodies_b, joints_a + joints_b)
                    };
                    if joints >= MAX_GROUP_JOINTS || bodies >= MAX_GROUP_BODIES {
                        break;
                    }
                    if ra != rb {
                        self.merge(ra, rb);
                    }
                    ra
                }
            };
            let raw = c.raw();
            let world = self.group_mut(root).world;
            // SAFETY: the joint group and the world are live, the bodies
            // this solver's and now listed in `world`.
            let joint = unsafe {
                let make = match c.kind {
                    ContactKind::Ordinary => dJointCreateContact,
                    ContactKind::Special => dJointCreateContactSpecial,
                };
                let j = make(std::ptr::null_mut(), self.contacts, &raw);
                dWorldAddJoint(world, j);
                dJointAttach(j, a.0, b.map_or(std::ptr::null_mut(), |b| b.0));
                j
            };
            let g = self.group_mut(root);
            g.contacts.push(joint);
            g.joints += 1;
            self.made.push(joint);
            made += 1;
        }
        made
    }

    /// The world's parameters for the coming steps (1.3), library-wide.
    fn apply(params: &Params) {
        let [x, y, z] = params.gravity;
        let w = std::ptr::null_mut();
        // SAFETY: the shipped library's setters ignore the world (3.1).
        unsafe {
            dWorldSetGravity(w, x, y, z);
            dWorldSetERP(w, params.erp);
            dWorldSetCFM(w, params.cfm);
            dWorldSetQuickStepNumIterations(w, params.iterations as c_int);
            dWorldSetQuickStepW(w, params.relaxation);
            dWorldSetContactSurfaceLayer(w, params.surface_layer);
            dWorldSetContactMaxCorrectingVel(w, params.max_correcting_speed);
        }
    }

    /// Step item 7 (4.2): every merged group of `groups` (each the group of
    /// an active object, in the objects' order) solved once for a step of
    /// `params`. A group is solved by the iterative solver (2.4): the exact
    /// one only for a group marked for it, which no code marks. Then every
    /// group lists its own bodies again (item 8).
    fn solve_groups(&mut self, params: &Params, groups: impl Iterator<Item = GroupId>) {
        let _turn = STEPPING.lock().unwrap_or_else(|e| e.into_inner());
        Self::apply(params);
        let mut solved = Vec::new();
        for id in groups {
            if self.group(id).is_none() {
                continue;
            }
            let root = self.root(id.0);
            if solved.contains(&root) {
                continue;
            }
            solved.push(root);
            let g = self.group_mut(root);
            if g.bodies > 0 {
                // SAFETY: the world is live; ODE steps what it lists.
                unsafe { dWorldQuickStep(g.world, params.step) };
            }
        }
        self.reset();
    }

    /// Every group lists its own bodies again and no contact (4.2).
    fn reset(&mut self) {
        for index in 0..self.groups.len() as u32 {
            let Some(g) = self.groups[index as usize].as_mut() else {
                continue;
            };
            if g.parent != index {
                continue;
            }
            let world = g.world;
            for j in std::mem::take(&mut g.contacts) {
                // SAFETY: the contact is listed in `world`.
                unsafe { dWorldRemoveJoint(world, j) };
            }
            for m in std::mem::take(&mut g.members) {
                let member = self.group_mut(m);
                member.parent = m;
                member.members = vec![m];
                member.bodies = member.own.len();
                member.joints = 0;
                if m == index {
                    continue;
                }
                let own_world = member.world;
                for &b in &member.own {
                    // SAFETY: the body is listed in `world`; both live.
                    unsafe {
                        dWorldRemoveBody(world, b);
                        dWorldAddBody(own_world, b);
                    }
                }
            }
        }
    }
}

impl Default for OdeSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OdeSolver {
    fn drop(&mut self) {
        self.reset();
        self.drop_step_contacts();
        // SAFETY: all are live; destroying a world destroys its bodies.
        unsafe {
            dJointGroupDestroy(self.contacts);
            for g in self.groups.drain(..).flatten() {
                dWorldDestroy(g.world);
            }
        }
    }
}

impl OdeSolver {
    /// Step item 10: the contacts, out of every list already, are detached
    /// from their bodies (emptying the joint group leaves joints without a
    /// world attached, physics-library.md 6, item 2) and dropped.
    fn drop_step_contacts(&mut self) {
        for j in self.made.drain(..) {
            // SAFETY: a live contact of the joint group, in no list.
            unsafe { dJointAttach(j, std::ptr::null_mut(), std::ptr::null_mut()) };
        }
        // SAFETY: the joint group is live.
        unsafe { dJointGroupEmpty(self.contacts) };
    }
}

impl<O: Grouped> Solver<O> for OdeSolver {
    // TODO(#883): the contacts are gathered as PH-4 specifies, through
    // `OdeSolver::add_contacts`.
    fn collide(&mut self, _: &Params, _: &mut Active<'_, O>) {}

    fn solve(&mut self, params: &Params, objects: &mut Active<'_, O>) {
        let groups: Vec<GroupId> = objects.iter_mut().map(|(_, o)| o.group()).collect();
        self.solve_groups(params, groups.into_iter());
    }

    fn drop_contacts(&mut self) {
        self.drop_step_contacts();
    }
}

/// The smoke check, the same natively and in the engine worker (the
/// `physics_selftest` op): a sphere falls under the world's gravity for
/// [`FALL_STEPS`] steps of [`super::INITIAL_STEP`], from (0, 10, 0). Each
/// step, ODE's integration gives v += g·h, then y += v·h, so after k steps
/// y = 10 + g·h²·k(k+1)/2 (g < 0); single precision keeps each within
/// [`FALL_TOLERANCE`] of it. Returns the last y.
pub fn fall_check() -> Result<f32, String> {
    use super::{Kind, LIBRARY_ITERATIONS, Object, World};

    struct Ball(GroupId);
    impl Grouped for Ball {
        fn group(&self) -> GroupId {
            self.0
        }
    }
    impl Object for Ball {
        fn kind(&self) -> Kind {
            Kind::Shell
        }
        fn has_geometry(&self) -> bool {
            true
        }
        fn height(&self) -> f32 {
            0.0
        }
        fn set_gravity(&mut self, _: bool) {}
        fn add_force(&mut self, _: Vec3) {}
        fn pre_step(&mut self, _: &Params) {}
        fn post_step(&mut self, _: &Params) -> bool {
            false
        }
    }

    let h = super::INITIAL_STEP;
    let mut world = World::new(OdeSolver::new(), h, LIBRARY_ITERATIONS, -1000.0);
    let group = world.solver_mut().add_group();
    let body = (world.solver_mut())
        .add_sphere(group, [0.0, 10.0, 0.0], 1.0, 0.1)
        .ok_or("no group")?;
    let id = world.add(Ball(group));
    world.activate(id).map_err(|e| e.to_string())?;
    let g = f64::from(world.params().gravity[1]);
    let mut y = 10.0;
    for k in 1..=FALL_STEPS {
        world.step(0, 0);
        let p = world
            .solver_mut()
            .position(body)
            .ok_or("the body is gone")?;
        let k = f64::from(k);
        let expected = 10.0 + g * f64::from(h) * f64::from(h) * k * (k + 1.0) / 2.0;
        if p[0] != 0.0 || p[2] != 0.0 || (f64::from(p[1]) - expected).abs() > FALL_TOLERANCE {
            return Err(format!("step {k}: {p:?}, expected y {expected}"));
        }
        y = p[1];
    }
    Ok(y)
}

pub const FALL_STEPS: u32 = 100;
/// [`fall_check`]'s last y, natively and in the engine worker alike (the
/// platform suite's check holds the worker to it).
pub const FALL_LAST_Y: f32 = 0.091_897_085;
/// Metres.
pub const FALL_TOLERANCE: f64 = 1e-4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls() {
        let y = fall_check().unwrap();
        // 10 − 19.62 · 0.01² · 100 · 101 / 2.
        assert!((y - 0.0919).abs() < 1e-4, "{y}");
        assert_eq!(y.to_bits(), FALL_LAST_Y.to_bits(), "{y:e}");
    }

    fn params() -> Params {
        Params::new(0.01, crate::physics::LIBRARY_ITERATIONS)
    }

    #[test]
    fn gravity_switch_and_removal() {
        let mut s = OdeSolver::new();
        let g = s.add_group();
        let a = s.add_sphere(g, [0.0, 0.0, 0.0], 1.0, 0.1).unwrap();
        let b = s.add_sphere(g, [1.0, 0.0, 0.0], 1.0, 0.1).unwrap();
        s.set_gravity(a, false);
        s.solve_groups(&params(), [g].into_iter());
        assert_eq!(s.position(a), Some([0.0, 0.0, 0.0]));
        assert!(s.position(b).unwrap()[1] < 0.0);
        s.remove(b);
        assert_eq!(s.position(b), None);
        s.remove_group(g);
        assert_eq!(s.position(a), None);
        assert_eq!(s.add_sphere(g, [0.0; 3], 1.0, 0.1), None);
    }

    /// Only the groups of active objects are stepped (4.2).
    #[test]
    fn groups_stepped_alone() {
        let mut s = OdeSolver::new();
        let (g, h) = (s.add_group(), s.add_group());
        let a = s.add_sphere(g, [0.0; 3], 1.0, 0.1).unwrap();
        let b = s.add_sphere(h, [0.0; 3], 1.0, 0.1).unwrap();
        s.solve_groups(&params(), [g, g].into_iter());
        let y = s.position(a).unwrap()[1];
        // Solved once: v = g·h, y = g·h².
        assert!((y + 19.62e-4).abs() < 1e-7, "{y}");
        assert_eq!(s.position(b), Some([0.0; 3]));
    }

    /// A body resting on the level by a contact, `objects` others hanging
    /// from it by contacts; returns the solver and the bodies.
    fn stack(objects: usize) -> (OdeSolver, Vec<BodyId>) {
        let mut s = OdeSolver::new();
        let bodies: Vec<BodyId> = (0..=objects)
            .map(|k| {
                let g = s.add_group();
                s.add_sphere(g, [0.0, k as f32, 0.0], 1.0, 0.5).unwrap()
            })
            .collect();
        (s, bodies)
    }

    fn up() -> Contact {
        Contact {
            normal: [0.0, 1.0, 0.0],
            surface: Surface {
                mu: 1.0,
                ..Surface::default()
            },
            ..Contact::default()
        }
    }

    #[test]
    fn a_contact_merges_two_groups_for_the_step() {
        let (mut s, b) = stack(2);
        let groups: Vec<GroupId> = b.iter().map(|b| s.bodies[b]).collect();
        // The ground holds b0; b1 rests on b0; b2 falls freely.
        assert_eq!(s.add_contacts(b[0], None, &[up()]), 1);
        assert_eq!(s.add_contacts(b[1], Some(b[0]), &[up()]), 1);
        // One group: b1's, its list now b0's body and contact too.
        assert_eq!(s.merged(b[0]), Some((groups[1].0, 2, 2)));
        assert_eq!(s.merged(b[1]), s.merged(b[0]));
        assert_eq!(s.merged(b[2]), Some((groups[2].0, 1, 0)));
        s.solve_groups(&params(), groups.iter().copied());
        // Held: the contacts' softness leaves little motion.
        for (k, &body) in b.iter().enumerate().take(2) {
            let y = s.position(body).unwrap()[1] - k as f32;
            assert!(y.abs() < 1e-3, "{k}: {y}");
        }
        assert!((s.position(b[2]).unwrap()[1] - 2.0 + 19.62e-4).abs() < 1e-6);
        // Reset: each group its own again.
        for (k, g) in groups.iter().enumerate() {
            assert_eq!(s.merged(b[k]), Some((g.0, 1, 0)));
        }
        Solver::<Ball>::drop_contacts(&mut s);
        assert!(s.made.is_empty());
        // Without contacts b1 falls.
        s.solve_groups(&params(), groups.iter().copied());
        assert!(s.position(b[1]).unwrap()[1] < 1.0 - 1e-3);
    }

    struct Ball;
    impl Grouped for Ball {
        fn group(&self) -> GroupId {
            GroupId(0)
        }
    }

    #[test]
    fn group_limits() {
        // A pair of geometries yields at most 809 contacts.
        let (mut s, b) = stack(1);
        assert_eq!(s.add_contacts(b[0], None, &[up(); 900]), MAX_PAIR_CONTACTS);
        // With the level: up to 1,500 joints in the group.
        assert_eq!(s.add_contacts(b[0], None, &[up(); 800]), 1500 - 809);
        assert_eq!(s.add_contacts(b[0], None, &[up()]), 0);
        // Between objects: fewer than 1,500 together.
        assert_eq!(s.add_contacts(b[1], Some(b[0]), &[up()]), 0);
        Solver::<Ball>::drop_contacts(&mut s);
        let groups: Vec<GroupId> = b.iter().map(|b| s.bodies[b]).collect();
        s.reset();
        assert_eq!(s.merged(b[0]), Some((groups[0].0, 1, 0)));
        // Fewer than 500 bodies together: a chain of objects merges until
        // the group has 499; the next link starts a group of its own.
        let (mut s, b) = stack(500);
        for k in 1..=500 {
            let made = s.add_contacts(b[k], Some(b[k - 1]), &[up()]);
            assert_eq!(made, usize::from(k != 499), "{k}");
        }
        assert_eq!(s.merged(b[500]).unwrap().1, 2);
        assert_eq!(s.merged(b[0]).unwrap().1, 499);
        s.reset();
        Solver::<Ball>::drop_contacts(&mut s);
    }

    /// The shipped library's changes (docs/behavior/physics-library.md 3 –
    /// 6, third_party/ode), through the library's own functions.
    mod shipped {
        use super::super::*;
        use std::ffi::c_void;

        unsafe extern "C" {
            fn dWorldGetERP(w: *mut DxWorld) -> Real;
            fn dWorldGetCFM(w: *mut DxWorld) -> Real;
            fn dWorldGetQuickStepW(w: *mut DxWorld) -> Real;
            fn dWorldGetContactSurfaceLayer(w: *mut DxWorld) -> Real;
            fn dBodyDisable(b: *mut DxBody);
            fn dBodyIsEnabled(b: *mut DxBody) -> c_int;
            fn dBodyGetLinearVel(b: *mut DxBody) -> *const Real;
            fn dBodySetAutoDisableSteps(b: *mut DxBody, steps: c_int);
            fn dBodyGetAutoDisableSteps(b: *mut DxBody) -> c_int;
            fn dJointGetType(j: *mut DxJoint) -> c_int;
            fn dJointDestroy(j: *mut DxJoint);
            fn dNormalize3(a: *mut Real);
            fn dNormalize3_slow(a: *mut Real);
            fn dCreateBox(space: *mut c_void, lx: Real, ly: Real, lz: Real) -> *mut DxGeom;
            fn dCreateRay(space: *mut c_void, length: Real) -> *mut DxGeom;
            fn dGeomDestroy(g: *mut DxGeom);
            fn dGeomSetPosition(g: *mut DxGeom, x: Real, y: Real, z: Real);
            fn dGeomSetRotation(g: *mut DxGeom, r: *const Real);
            fn dGeomGetRotation(g: *mut DxGeom) -> *const Real;
            fn dGeomGetPosition(g: *mut DxGeom) -> *const Real;
            fn dGeomRaySet(
                g: *mut DxGeom,
                px: Real,
                py: Real,
                pz: Real,
                dx: Real,
                dy: Real,
                dz: Real,
            );
            fn dRFromAxisAndAngle(r: *mut Real, ax: Real, ay: Real, az: Real, angle: Real);
            fn dCollide(
                o1: *mut DxGeom,
                o2: *mut DxGeom,
                flags: c_int,
                contact: *mut ContactGeom,
                skip: c_int,
            ) -> c_int;
        }

        /// The library-wide parameters a fresh world leaves (3.1).
        fn lock() -> std::sync::MutexGuard<'static, ()> {
            STEPPING.lock().unwrap_or_else(|e| e.into_inner())
        }

        #[test]
        fn world_creation_resets_the_library_wide_values() {
            let _turn = lock();
            // SAFETY: the library's functions on live objects, under the lock.
            unsafe {
                let a = dWorldCreate();
                dWorldSetERP(a, 0.7);
                let b = dWorldCreate();
                assert_eq!(dWorldGetERP(a), 0.2, "creating b resets them");
                dWorldSetCFM(b, 0.5);
                assert_eq!(dWorldGetCFM(a), 0.5, "one value for every world");
                assert_eq!(dWorldGetQuickStepW(a), 1.1);
                assert_eq!(dWorldGetContactSurfaceLayer(a), 0.001);
                dWorldDestroy(a);
                dWorldDestroy(b);
            }
        }

        #[test]
        fn bodies_outside_worlds_and_no_auto_disabling() {
            let _turn = lock();
            // SAFETY: as above.
            unsafe {
                let w = dWorldCreate();
                dWorldSetGravity(w, 0.0, -10.0, 0.0);
                let b = dBodyCreate(std::ptr::null_mut());
                dBodySetAutoDisableSteps(b, 5);
                assert_eq!(dBodyGetAutoDisableSteps(b), 0, "no per-body values");
                dWorldQuickStep(w, 0.01);
                assert_eq!(*dBodyGetLinearVel(b).add(1), 0.0, "not in the world");
                dWorldAddBody(w, b);
                dBodyDisable(b);
                dWorldQuickStep(w, 0.01);
                let v = *dBodyGetLinearVel(b).add(1);
                assert!((v + 0.1).abs() < 1e-6, "stepped though disabled: {v}");
                assert_eq!(dBodyIsEnabled(b), 1);
                dWorldRemoveBody(w, b);
                dWorldQuickStep(w, 0.01);
                assert_eq!(*dBodyGetLinearVel(b).add(1), v, "not stepped once removed");
                dBodyDestroy(b);
                dWorldDestroy(w);
            }
        }

        /// A body of `mass` resting on a contact under gravity −10: the
        /// special contact's normal force stops at 100,000 (5.2, item 5).
        fn rest_on(special: bool, mass: f32) -> f32 {
            let _turn = lock();
            // SAFETY: as above.
            unsafe {
                let w = dWorldCreate();
                dWorldSetGravity(w, 0.0, -10.0, 0.0);
                // No mixing: an ordinary contact then holds the body.
                dWorldSetCFM(w, 0.0);
                let b = dBodyCreate(w);
                let mut m = DMass::default();
                dMassSetSphereTotal(&mut m, mass, 0.5);
                dBodySetMass(b, &m);
                let mut c = DContact::default();
                c.geom.normal = [0.0, 1.0, 0.0, 0.0];
                let make = if special {
                    dJointCreateContactSpecial
                } else {
                    dJointCreateContact
                };
                let j = make(std::ptr::null_mut(), std::ptr::null_mut(), &c);
                assert_eq!(dJointGetType(j), 4);
                dWorldAddJoint(w, j);
                dJointAttach(j, b, std::ptr::null_mut());
                dWorldQuickStep(w, 0.01);
                let v = *dBodyGetLinearVel(b).add(1);
                dJointDestroy(j);
                dWorldDestroy(w);
                v
            }
        }

        #[test]
        fn special_contact_caps_the_normal_force() {
            assert!(rest_on(false, 1e6).abs() < 1e-4);
            let v = rest_on(true, 1e6);
            // v = h (g + 100,000 / m) = 0.01 (−10 + 0.1).
            assert!((v + 0.099).abs() < 1e-5, "{v}");
            assert!(rest_on(true, 1e3).abs() < 1e-4, "below the cap");
        }

        #[test]
        fn normalisation() {
            // SAFETY: plain functions on 3-vectors.
            unsafe {
                let mut a = [3.0, 0.0, 4.0, 0.0];
                dNormalize3(a.as_mut_ptr());
                assert_eq!(a[..3], [0.6, 0.0, 0.8]);
                // Below the threshold: the slow path's result exactly.
                let mut small = [1e-3, -2e-3, 0.5e-3, 0.0];
                let mut slow = small;
                dNormalize3(small.as_mut_ptr());
                dNormalize3_slow(slow.as_mut_ptr());
                assert_eq!(small, slow);
                let mut zero = [0.0; 4];
                dNormalize3(zero.as_mut_ptr());
                assert_eq!(zero[..3], [1.0, 0.0, 0.0]);
            }
        }

        #[test]
        fn box_edges_crossing_give_more_contacts() {
            // SAFETY: geometries made, collided and destroyed here.
            unsafe {
                let a = dCreateBox(std::ptr::null_mut(), 1.0, 1.0, 1.0);
                let b = dCreateBox(std::ptr::null_mut(), 1.0, 1.0, 1.0);
                // An edge-edge case found by a sweep over random poses.
                let mut r = [0.0; 12];
                dRFromAxisAndAngle(
                    r.as_mut_ptr(),
                    0.381_027,
                    0.318_387_5,
                    0.214_326_14,
                    0.115_076_6,
                );
                dGeomSetRotation(a, r.as_ptr());
                dRFromAxisAndAngle(
                    r.as_mut_ptr(),
                    0.096_875_61,
                    0.127_960_15,
                    0.103_413_045,
                    1.061_063_6,
                );
                dGeomSetRotation(b, r.as_ptr());
                dGeomSetPosition(b, 0.213_990_27, 0.628_570_1, 0.631_959_26);
                let mut c = [ContactGeom::default(); 8];
                // At most 1 asked for: public 0.5.0's one, and the further
                // ones regardless.
                let n = dCollide(a, b, 1, c.as_mut_ptr(), size_of::<ContactGeom>() as c_int);
                assert_eq!(n, 4);
                for k in 0..n as usize {
                    assert!(c[k].depth > 0.0, "{k}: {}", c[k].depth);
                    assert_eq!(c[k].normal, c[0].normal);
                }
                dGeomDestroy(a);
                dGeomDestroy(b);
            }
        }

        #[test]
        fn ray_set_builds_the_whole_rotation() {
            // SAFETY: as above.
            unsafe {
                let r = dCreateRay(std::ptr::null_mut(), 1.0);
                dGeomRaySet(r, 1.0, 2.0, 3.0, 0.0, 0.0, 2.0);
                let p = std::slice::from_raw_parts(dGeomGetPosition(r), 3);
                assert_eq!(p, [1.0, 2.0, 3.0]);
                let m = std::slice::from_raw_parts(dGeomGetRotation(r), 12);
                // Third column the normalised direction; an orthonormal frame.
                assert_eq!([m[2], m[6], m[10]], [0.0, 0.0, 1.0]);
                let col = |j: usize| [m[j], m[4 + j], m[8 + j]];
                let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                assert!((dot(col(0), col(0)) - 1.0).abs() < 1e-6);
                assert!(dot(col(0), col(1)).abs() < 1e-6);
                dGeomDestroy(r);
            }
        }
    }
}
