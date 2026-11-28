// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's physics world (docs/behavior/physics-world.md, cited by
//! section): made under `st_client_spawning` (1.1) over the ODE solver,
//! run after the frame's object updates (2.1), following `ph_frequency`
//! and `ph_iterations` (2.3, 2.4), removed with the level.

use crate::core::collision::geometry::Vec3;
use crate::core::console::{Console, Value};
use crate::core::physics::ode::{GroupId, Grouped, OdeSolver};
use crate::core::physics::{INITIAL_STEP, Kind, LIBRARY_ITERATIONS, Object, Params, Timers, World};

/// What the level's world steps. Nothing enters it yet: shells come with
/// PH-3 (#882), the characters' bodies with their specs (PH-8, PH-22).
pub enum LevelBody {}

impl Object for LevelBody {
    fn kind(&self) -> Kind {
        match *self {}
    }
    fn has_geometry(&self) -> bool {
        match *self {}
    }
    fn height(&self) -> f32 {
        match *self {}
    }
    fn set_gravity(&mut self, _: bool) {
        match *self {}
    }
    fn add_force(&mut self, _: Vec3) {
        match *self {}
    }
    fn pre_step(&mut self, _: &Params) {
        match *self {}
    }
    fn post_step(&mut self, _: &Params) -> bool {
        match *self {}
    }
}

impl Grouped for LevelBody {
    fn group(&self) -> GroupId {
        match *self {}
    }
}

/// The console's physics values in force.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsSettings {
    /// `ph_frequency`'s step `h`, s (2.3).
    pub step: f32,
    /// How often `ph_iterations` has run, and its value (2.4).
    pub iterations_runs: u64,
    pub iterations: u32,
}

impl Default for PhysicsSettings {
    fn default() -> Self {
        Self {
            step: INITIAL_STEP,
            iterations_runs: 0,
            iterations: LIBRARY_ITERATIONS,
        }
    }
}

impl PhysicsSettings {
    pub fn from_console(console: &Console) -> Self {
        let step = match console.value("ph_frequency") {
            Some(Value::Frequency(step)) => step,
            _ => INITIAL_STEP,
        };
        Self {
            step,
            iterations_runs: console.executions("ph_iterations"),
            iterations: console.number("ph_iterations") as u32,
        }
    }

    /// The iteration count the library has: the last one `ph_iterations`
    /// passed, or its own before the command first runs (2.4).
    fn library_iterations(&self) -> u32 {
        if self.iterations_runs > 0 {
            self.iterations
        } else {
            LIBRARY_ITERATIONS
        }
    }
}

/// The world as the engine interface reports it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct PhysicsReport {
    /// The steps run since it was made (3, item 2).
    pub steps: u64,
    /// The step `h`, s, and the iterative solver's iterations.
    pub step: f32,
    pub iterations: u32,
    /// The floor's height, m (4.5).
    pub floor: f32,
}

/// The world of one level.
pub struct LevelPhysics {
    world: World<LevelBody, OdeSolver>,
    /// The `ph_iterations` runs the world has followed.
    iterations_runs: u64,
}

impl LevelPhysics {
    /// 1.2: made with the settings in force, over a level whose bounding
    /// box's lowest y is `lowest_y`. The step hook the level installs does
    /// nothing (3, item 11): it has none.
    pub fn new(settings: PhysicsSettings, lowest_y: f32) -> Self {
        Self {
            world: World::new(
                OdeSolver::new(),
                settings.step,
                settings.library_iterations(),
                lowest_y,
            ),
            iterations_runs: settings.iterations_runs,
        }
    }

    pub fn world(&self) -> &World<LevelBody, OdeSolver> {
        &self.world
    }

    /// The console's values before a frame, `dt_f` the display frame time
    /// as it stands: a new step resets the accumulator (2.3); each run of
    /// `ph_iterations` passes its value (2.4).
    pub fn follow(&mut self, settings: PhysicsSettings, dt_f: f32) {
        if settings.step != self.world.params().step {
            self.world.set_step(settings.step, dt_f);
        }
        if settings.iterations_runs != self.iterations_runs {
            self.iterations_runs = settings.iterations_runs;
            self.world.set_iterations(settings.iterations);
        }
    }

    pub fn report(&self) -> PhysicsReport {
        let w = &self.world;
        PhysicsReport {
            steps: w.counter(),
            step: w.params().step,
            iterations: w.params().iterations,
            floor: w.floor(),
        }
    }

    /// 2.2: the frame's steps, `dt_f` the display frame time and `now_ms`
    /// the millisecond clock at the frame's start; returns the steps run.
    pub fn frame(&mut self, dt_f: f32, now_ms: u64) -> u32 {
        self.world.frame(dt_f, now_ms)
    }

    /// The statistics overlay's physics timers since last taken (6).
    pub fn take_timers(&mut self) -> Timers {
        self.world.take_timers()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_with_the_settings_in_force() {
        let p = LevelPhysics::new(PhysicsSettings::default(), -5.0);
        let w = p.world();
        assert_eq!((w.params().step, w.params().iterations), (0.01, 20));
        assert_eq!(w.floor(), -35.0);
        // ph_iterations ran before: its value; E and C from the step.
        let settings = PhysicsSettings {
            step: 0.005,
            iterations_runs: 1,
            iterations: 18,
        };
        let p = LevelPhysics::new(settings, 0.0);
        let params = p.world().params();
        assert_eq!(params.iterations, 18);
        assert_eq!(*params, Params::new(0.005, 18));
    }

    #[test]
    fn follows_the_console() {
        let mut p = LevelPhysics::new(PhysicsSettings::default(), 0.0);
        let (erp, cfm) = (p.world().params().erp, p.world().params().cfm);
        let mut s = PhysicsSettings {
            step: 0.005,
            ..PhysicsSettings::default()
        };
        p.follow(s, 0.017);
        let w = p.world();
        assert_eq!(w.params().step, 0.005);
        assert!((w.accumulator() - 0.002).abs() < 1e-6);
        assert_eq!((w.params().erp, w.params().cfm), (erp, cfm));
        // Each run passes the value, the same or not.
        s.iterations_runs = 1;
        s.iterations = 20;
        p.follow(s, 0.0);
        assert_eq!(p.world().params().iterations, 20);
        s.iterations = 30;
        p.follow(s, 0.0);
        assert_eq!(p.world().params().iterations, 20);
        s.iterations_runs = 2;
        p.follow(s, 0.0);
        assert_eq!(p.world().params().iterations, 30);
    }

    #[test]
    fn steps_by_the_frame_time() {
        let mut p = LevelPhysics::new(PhysicsSettings::default(), 0.0);
        assert_eq!(p.frame(0.035, 0), 3);
        assert_eq!(p.frame(0.0, 0), 0);
        assert_eq!(p.world().counter(), 3);
        assert_eq!(p.take_timers().steps, 3);
    }
}
