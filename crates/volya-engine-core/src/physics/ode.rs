// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The solver behind [`Solver`]: the Open Dynamics Engine 0.5.0, the shipped
//! library's base, its core built in single precision by build.rs
//! (docs/architecture/simulation.md), through the few of its functions the
//! world uses.
//!
//! A [`OdeSolver`] owns an ODE world and its contact group, and is used from
//! the thread that made it (it is neither `Send` nor `Sync`). The library
//! keeps global state: the world parameters are library-wide (the shipped
//! library's, docs/behavior/physics-library.md 3.1; creating a world resets
//! them) and the row shuffle draws from the C run time's generator, so a
//! world's creation, and its parameters and step together, take
//! [`STEPPING`].
//! Contacts are not generated yet: [`Solver::collide`] adds none (#751).

#![allow(unsafe_code)]

pub mod harness;
#[cfg(test)]
mod harness_rules;
#[cfg(test)]
mod harness_rules_2;
pub mod msvc_rand;
pub mod x87;

use std::ffi::c_int;
use std::sync::Mutex;

/// Held by each world's parameters and step (above).
static STEPPING: Mutex<()> = Mutex::new(());

use super::{Active, Params, Solver};
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
struct DxJointGroup {
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

    fn dJointGroupCreate(max_size: c_int) -> *mut DxJointGroup;
    fn dJointGroupDestroy(g: *mut DxJointGroup);
    fn dJointGroupEmpty(g: *mut DxJointGroup);

    fn dBodyCreate(w: *mut DxWorld) -> *mut DxBody;
    fn dBodyDestroy(b: *mut DxBody);
    fn dBodySetPosition(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetPosition(b: *mut DxBody) -> *const Real;
    fn dBodySetMass(b: *mut DxBody, mass: *const DMass);
    fn dBodySetGravityMode(b: *mut DxBody, mode: c_int);
    fn dMassSetSphereTotal(m: *mut DMass, total: Real, radius: Real);
}

/// A body of an [`OdeSolver`]'s world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(*mut DxBody);

/// The ODE world of a level and its contacts.
pub struct OdeSolver {
    world: *mut DxWorld,
    contacts: *mut DxJointGroup,
    bodies: Vec<BodyId>,
}

impl OdeSolver {
    pub fn new() -> Self {
        // Creating a world resets the library-wide parameters.
        let _turn = STEPPING.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: ODE 0.5.0 needs no initialisation; it fails fatally
        // rather than return null.
        let (world, contacts) = unsafe { (dWorldCreate(), dJointGroupCreate(0)) };
        Self {
            world,
            contacts,
            bodies: Vec::new(),
        }
    }

    /// Adds a body at `position` with the mass `mass` (kg) of a uniform
    /// sphere of `radius` (m).
    pub fn add_sphere(&mut self, position: Vec3, mass: f32, radius: f32) -> BodyId {
        let mut m = DMass::default();
        // SAFETY: the world is live; `m` is a valid dMass.
        let body = unsafe {
            let body = dBodyCreate(self.world);
            dMassSetSphereTotal(&mut m, mass, radius);
            dBodySetMass(body, &m);
            dBodySetPosition(body, position[0], position[1], position[2]);
            body
        };
        let id = BodyId(body);
        self.bodies.push(id);
        id
    }

    /// Removes a body of this solver; another is ignored.
    pub fn remove(&mut self, body: BodyId) {
        if let Some(i) = self.bodies.iter().position(|&b| b == body) {
            self.bodies.swap_remove(i);
            // SAFETY: the body is this world's and not yet destroyed.
            unsafe { dBodyDestroy(body.0) };
        }
    }

    /// A body's gravity switch (1.4).
    pub fn set_gravity(&mut self, body: BodyId, on: bool) {
        if self.bodies.contains(&body) {
            // SAFETY: the body is this world's.
            unsafe { dBodySetGravityMode(body.0, c_int::from(on)) };
        }
    }

    /// A body's position, if it is this solver's.
    pub fn position(&self, body: BodyId) -> Option<Vec3> {
        if !self.bodies.contains(&body) {
            return None;
        }
        // SAFETY: the body is this world's; ODE returns its dVector3.
        let p = unsafe { std::slice::from_raw_parts(dBodyGetPosition(body.0), 3) };
        Some([p[0], p[1], p[2]])
    }

    /// The world's parameters for the coming step (1.3).
    fn apply(&mut self, params: &Params) {
        let [x, y, z] = params.gravity;
        let w = self.world;
        // SAFETY: the world is live.
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

    /// One step of the iterative solver (2.4) with `params`.
    fn step(&mut self, params: &Params) {
        let _turn = STEPPING.lock().unwrap_or_else(|e| e.into_inner());
        self.apply(params);
        // SAFETY: the world is live.
        unsafe { dWorldQuickStep(self.world, params.step) };
    }
}

impl Default for OdeSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OdeSolver {
    fn drop(&mut self) {
        // SAFETY: both are live; destroying the world destroys its bodies.
        unsafe {
            dJointGroupDestroy(self.contacts);
            dWorldDestroy(self.world);
        }
    }
}

impl<O> Solver<O> for OdeSolver {
    fn collide(&mut self, _: &Params, _: &mut Active<'_, O>) {}

    fn solve(&mut self, params: &Params, _: &mut Active<'_, O>) {
        self.step(params);
    }

    fn drop_contacts(&mut self) {
        // SAFETY: the group is live.
        unsafe { dJointGroupEmpty(self.contacts) };
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

    struct Ball;
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
    let id = world.add(Ball);
    world.activate(id).map_err(|e| e.to_string())?;
    let body = world.solver_mut().add_sphere([0.0, 10.0, 0.0], 1.0, 0.1);
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

    #[test]
    fn gravity_switch_and_removal() {
        let mut s = OdeSolver::new();
        let a = s.add_sphere([0.0, 0.0, 0.0], 1.0, 0.1);
        let b = s.add_sphere([1.0, 0.0, 0.0], 1.0, 0.1);
        s.set_gravity(a, false);
        let params = Params::new(0.01, crate::physics::LIBRARY_ITERATIONS);
        s.step(&params);
        assert_eq!(s.position(a), Some([0.0, 0.0, 0.0]));
        assert!(s.position(b).unwrap()[1] < 0.0);
        s.remove(b);
        assert_eq!(s.position(b), None);
    }

    /// The shipped library's changes (docs/behavior/physics-library.md 3 –
    /// 6, third_party/ode), through the library's own functions.
    mod shipped {
        use super::super::*;
        use std::ffi::c_void;

        #[repr(C)]
        struct DxGeom {
            _private: [u8; 0],
        }

        #[repr(C)]
        struct DxJoint {
            _private: [u8; 0],
        }

        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct SurfaceParameters {
            mode: c_int,
            mu: Real,
            mu2: Real,
            bounce: Real,
            bounce_vel: Real,
            soft_erp: Real,
            soft_cfm: Real,
            motion1: Real,
            motion2: Real,
            slip1: Real,
            slip2: Real,
        }

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

        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct Contact {
            surface: SurfaceParameters,
            geom: ContactGeom,
            fdir1: [Real; 4],
        }

        unsafe extern "C" {
            fn dWorldGetERP(w: *mut DxWorld) -> Real;
            fn dWorldGetCFM(w: *mut DxWorld) -> Real;
            fn dWorldGetQuickStepW(w: *mut DxWorld) -> Real;
            fn dWorldGetContactSurfaceLayer(w: *mut DxWorld) -> Real;
            fn dWorldAddBody(w: *mut DxWorld, b: *mut DxBody);
            fn dWorldRemoveBody(w: *mut DxWorld, b: *mut DxBody);
            fn dWorldAddJoint(w: *mut DxWorld, j: *mut DxJoint);
            fn dBodyDisable(b: *mut DxBody);
            fn dBodyIsEnabled(b: *mut DxBody) -> c_int;
            fn dBodyGetLinearVel(b: *mut DxBody) -> *const Real;
            fn dBodySetAutoDisableSteps(b: *mut DxBody, steps: c_int);
            fn dBodyGetAutoDisableSteps(b: *mut DxBody) -> c_int;
            fn dJointCreateContact(
                w: *mut DxWorld,
                g: *mut DxJointGroup,
                c: *const Contact,
            ) -> *mut DxJoint;
            fn dJointCreateContactSpecial(
                w: *mut DxWorld,
                g: *mut DxJointGroup,
                c: *const Contact,
            ) -> *mut DxJoint;
            fn dJointGetType(j: *mut DxJoint) -> c_int;
            fn dJointAttach(j: *mut DxJoint, b1: *mut DxBody, b2: *mut DxBody);
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
                let mut c = Contact::default();
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
