// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The solver behind [`Solver`]: the Open Dynamics Engine 0.5.0, the shipped
//! library's base, its core built in single precision by build.rs
//! (docs/architecture/simulation.md), through the few of its functions the
//! world uses.
//!
//! A [`OdeSolver`] owns an ODE world and its contact group, and is used from
//! the thread that made it (it is neither `Send` nor `Sync`). The library
//! keeps global state its steps use (the random seed of the row shuffle; the
//! shipped library's world parameters, docs/behavior/physics-library.md 3.1),
//! so a world's parameters and step take [`STEPPING`] together.
//! Contacts are not generated yet: [`Solver::collide`] adds none (#751).

#![allow(unsafe_code)]

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
}
