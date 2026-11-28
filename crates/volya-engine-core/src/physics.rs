// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The physics world of a level and its step (docs/behavior/physics-world.md,
//! cited by section).
//!
//! The world holds what the spec states independently of the solver: the
//! parameters fixed when it is made (1.2, 1.3), the frame's steps from an
//! accumulator (2.2) and `ph_frequency` (2.3), the order of one step (3), the
//! per-step actions (4.3), the rest pass (4.4), the floor (4.5) and freezing
//! (5). Contacts and solving go through [`Solver`]; what it steps through
//! [`Object`] and [`Updater`].

pub mod contacts;
pub mod ode;
pub mod shell;
#[cfg(test)]
mod tests;

use crate::collision::body::{WORLD_DAMPING, WORLD_SPRING};
use crate::collision::geometry::Vec3;
use crate::movement::GRAVITY;

/// The step before `ph_frequency` runs, s (2.3).
pub const INITIAL_STEP: f32 = 0.01;
/// The iterative solver's iteration count until `ph_iterations` first runs:
/// the library's own (2.4).
pub const LIBRARY_ITERATIONS: u32 = 20;
/// The iterative solver's relaxation factor (1.3).
pub const RELAXATION: f32 = 1.1;
/// The contact surface layer, m (1.3).
pub const SURFACE_LAYER: f32 = 0.001;
/// The floor lies this far below the level's lowest y, m (1.2).
pub const FLOOR_DEPTH: f32 = 30.0;
/// A rest pass runs every this many steps, the first at the first step (3).
pub const REST_PASS_PERIOD: u32 = 64;
/// A resting shell's count when listed (4.4).
pub const RESTING_COUNT: u32 = 10;
/// Group limits (4.2): contacts between two objects only while the merged
/// group has fewer joints (contacts counted) and bodies than these; contacts
/// with the level only up to [`MAX_GROUP_JOINTS`]; contacts per pair of
/// geometries.
pub const MAX_GROUP_JOINTS: usize = 1500;
pub const MAX_GROUP_BODIES: usize = 500;
pub const MAX_PAIR_CONTACTS: usize = 809;
/// A group marked for the exact solver is solved by it only with fewer
/// joints than this, contacts included (4.2).
pub const EXACT_SOLVER_JOINTS: usize = 30;

/// A fatal error (`console-log` 3.8): the program shows it and stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fatal(pub &'static str);

impl std::fmt::Display for Fatal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Fatal {}

/// The solver's parameters (1.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// The step `h`, s: `ph_frequency`'s (2.3).
    pub step: f32,
    /// (0, −19.62, 0) m/s² (1.2).
    pub gravity: Vec3,
    /// Error reduction `E` and constraint-force mixing `C`, from the step at
    /// the world's creation (1.2); `ph_frequency` leaves them (1.3).
    pub erp: f32,
    pub cfm: f32,
    /// The iterative solver's iterations (2.4) and relaxation factor (1.3).
    pub iterations: u32,
    pub relaxation: f32,
    /// The contact surface layer, m, and the limit on a contact's correcting
    /// speed: none (1.3).
    pub surface_layer: f32,
    pub max_correcting_speed: f32,
}

impl Params {
    /// 1.2: the parameters a world made at step `h` sets, with the
    /// iteration count in force.
    pub fn new(step: f32, iterations: u32) -> Self {
        let sh = WORLD_SPRING * step;
        Self {
            step,
            gravity: [0.0, -GRAVITY, 0.0],
            erp: sh / (sh + WORLD_DAMPING),
            cfm: 1.0 / (sh + WORLD_DAMPING),
            iterations,
            relaxation: RELAXATION,
            surface_layer: SURFACE_LAYER,
            max_correcting_speed: f32::INFINITY,
        }
    }

    /// 1.4: the acceleration of gravity on a body whose switch is `on`.
    pub fn gravity_on(&self, on: bool) -> Vec3 {
        if on { self.gravity } else { [0.0; 3] }
    }

    /// 1.3: a contact's correcting speed at `depth` with error reduction `e`.
    pub fn correcting_speed(&self, depth: f32, e: f32) -> f32 {
        ((depth - self.surface_layer).max(0.0) * (1.0 / self.step) * e)
            .min(self.max_correcting_speed)
    }
}

/// What an object of the world is (conventions; 4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Shell,
    Character,
}

/// An object the world steps as a unit: a shell or a character's body.
pub trait Object {
    fn kind(&self) -> Kind;
    /// Whether it has collision geometry; activating one without is fatal (5).
    fn has_geometry(&self) -> bool;
    /// The y of a shell's first element, or of a character's body (4.5).
    fn height(&self) -> f32;
    /// Its bodies' gravity switches (1.4).
    fn set_gravity(&mut self, on: bool);
    /// Adds a force to the shell for the coming step (4.3).
    fn add_force(&mut self, force: Vec3);
    /// Step item 4.
    fn pre_step(&mut self, params: &Params);
    /// Step item 8, after its group is reset: reads the solved state and
    /// moves its bounds; returns whether all its bodies are switched off
    /// (resting, 4.4).
    fn post_step(&mut self, params: &Params) -> bool;
    /// Freezing set it aside, or unfreezing made it active again (5).
    fn set_aside(&mut self) {}
    fn restore(&mut self) {}
    /// The rest pass empties a resting shell's cached level-contact data (4.4).
    fn clear_level_contacts(&mut self) {}
    /// A character's body fell below the floor (4.5: the handling is
    /// unestablished).
    fn below_floor(&mut self) {}
}

/// A game part that hooks into each step (3, items 5 and 9).
pub trait Updater {
    fn pre_step(&mut self, params: &Params);
    fn post_step(&mut self, params: &Params);
    fn set_aside(&mut self) {}
    fn restore(&mut self) {}
}

/// The contacts and the solving of a step (3, items 3, 7, 10; 4.2).
pub trait Solver<O> {
    /// Item 3: every active object gathers its contacts into its group.
    fn collide(&mut self, params: &Params, objects: &mut Active<'_, O>);
    /// Item 7: every group is solved for one step.
    fn solve(&mut self, params: &Params, objects: &mut Active<'_, O>);
    /// Item 10: all contacts of the step are dropped.
    fn drop_contacts(&mut self);
}

/// An object's handle in its world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Active,
    /// Added and not stepped: in the collision index, so an active
    /// object's contact can wake it (physics-contacts.md 1.2).
    Idle,
    SetAside,
    /// Taken out of the simulation.
    Inactive,
}

struct Entry<O> {
    id: ObjectId,
    state: State,
    object: O,
}

/// The world's active objects, as a step sees them, and the objects of
/// the collision index (physics-contacts.md 1.1) by position.
pub struct Active<'a, O> {
    entries: &'a mut Vec<Entry<O>>,
    resting: &'a mut Vec<(ObjectId, u32)>,
}

impl<O> Active<'_, O> {
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (ObjectId, &mut O)> {
        self.entries
            .iter_mut()
            .filter(|e| e.state == State::Active)
            .map(|e| (e.id, &mut e.object))
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut O> {
        self.entries
            .iter_mut()
            .find(|e| e.id == id && e.state == State::Active)
            .map(|e| &mut e.object)
    }

    /// The objects' positions run to `len()`, the active ones in their
    /// order (an object woken is appended at the end).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the object at `i` is active.
    pub fn is_active(&self, i: usize) -> bool {
        self.entries
            .get(i)
            .is_some_and(|e| e.state == State::Active)
    }

    /// Whether the object at `i` is in the collision index: active, or
    /// added and not stepped (not set aside or taken out).
    pub fn indexed(&self, i: usize) -> bool {
        (self.entries.get(i)).is_some_and(|e| matches!(e.state, State::Active | State::Idle))
    }

    pub fn id(&self, i: usize) -> ObjectId {
        self.entries[i].id
    }

    /// The position of the object `id`.
    pub fn position(&self, id: ObjectId) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    pub fn at(&self, i: usize) -> &O {
        &self.entries[i].object
    }

    pub fn at_mut(&mut self, i: usize) -> &mut O {
        &mut self.entries[i].object
    }

    /// The objects at `i` and `j`, two positions.
    pub fn pair_mut(&mut self, i: usize, j: usize) -> (&mut O, &mut O) {
        assert_ne!(i, j);
        if i < j {
            let (a, b) = self.entries.split_at_mut(j);
            (&mut a[i].object, &mut b[0].object)
        } else {
            let (a, b) = self.entries.split_at_mut(i);
            (&mut b[0].object, &mut a[j].object)
        }
    }

    /// Makes the object at `i` active (physics-contacts.md 1.2;
    /// physics-shell-shapeless-root.md 4.2): off the resting list (4.4) and
    /// appended to the end of the active objects. Returns its position.
    pub fn wake(&mut self, i: usize) -> usize {
        let id = self.entries[i].id;
        self.resting.retain(|&(r, _)| r != id);
        let mut e = self.entries.remove(i);
        e.state = State::Active;
        self.entries.push(e);
        self.entries.len() - 1
    }
}

/// Whether a per-step action's condition holds (4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Holds,
    Waits,
    Done,
}

/// A (condition, action) pair of the per-step list (4.3).
pub trait Action<O> {
    /// The object whose destruction removes the entry, if any.
    fn object(&self) -> Option<ObjectId>;
    fn condition(&mut self, counter: u64) -> Condition;
    /// Runs it; returns whether it is done.
    fn act(&mut self, counter: u64, objects: &mut Active<'_, O>) -> bool;
}

/// A script's constant force on a shell (4.3): acts at every step while the
/// step counter is within the time's whole steps.
struct ConstantForce {
    object: ObjectId,
    force: Vec3,
    last: u64,
}

impl<O: Object> Action<O> for ConstantForce {
    fn object(&self) -> Option<ObjectId> {
        Some(self.object)
    }

    fn condition(&mut self, counter: u64) -> Condition {
        if counter > self.last {
            Condition::Done
        } else {
            Condition::Holds
        }
    }

    fn act(&mut self, _: u64, objects: &mut Active<'_, O>) -> bool {
        if let Some(o) = objects.get_mut(self.object) {
            o.add_force(self.force);
        }
        false
    }
}

/// The statistics overlay's physics timers (6; `display-mode` B.4.1), ms,
/// with the number of steps the `solver` timer measured.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timers {
    /// The world's whole work of each frame (2.2).
    pub physics: f64,
    /// The contact gathering of each step (3, item 3).
    pub collider: f64,
    /// The per-step actions and the solving of each step (3, items 6, 7).
    pub solver: f64,
    pub steps: u32,
}

/// The hook called after each step with its start and end time, ms (3,
/// item 11).
pub type StepHook = Box<dyn FnMut(u64, u64)>;

/// The physics world of one level (1.1): made under `st_client_spawning`,
/// dropped with the level, which drops its contacts with the solver.
pub struct World<O, S> {
    params: Params,
    solver: S,
    floor: f32,
    accumulator: f32,
    /// The time factor `k`: always 1 (2.2).
    time_factor: f32,
    counter: u64,
    rest_pass: u32,
    frozen: bool,
    stepping: bool,
    next_id: u32,
    objects: Vec<Entry<O>>,
    updaters: Vec<(bool, Box<dyn Updater>)>,
    actions: Vec<Box<dyn Action<O>>>,
    /// Resting shells and their counts (4.4).
    resting: Vec<(ObjectId, u32)>,
    hook: Option<StepHook>,
    /// The performance counter, ms.
    perf: fn() -> f64,
    timers: Timers,
}

impl<O: Object, S: Solver<O>> World<O, S> {
    /// 1.2: a world at step `step` with the library's iteration count
    /// `iterations`, over a level whose bounding box's lowest y is `lowest_y`.
    pub fn new(solver: S, step: f32, iterations: u32, lowest_y: f32) -> Self {
        Self {
            params: Params::new(step, iterations),
            solver,
            floor: lowest_y - FLOOR_DEPTH,
            accumulator: 0.0,
            time_factor: 1.0,
            counter: 0,
            rest_pass: 0,
            frozen: false,
            stepping: false,
            next_id: 0,
            objects: Vec::new(),
            updaters: Vec::new(),
            actions: Vec::new(),
            resting: Vec::new(),
            hook: None,
            perf: crate::perf_counter::now_ms,
            timers: Timers::default(),
        }
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    pub fn solver_mut(&mut self) -> &mut S {
        &mut self.solver
    }

    pub fn floor(&self) -> f32 {
        self.floor
    }

    pub fn accumulator(&self) -> f32 {
        self.accumulator
    }

    /// The step counter (3, item 2).
    pub fn counter(&self) -> u64 {
        self.counter
    }

    pub fn frozen(&self) -> bool {
        self.frozen
    }

    /// Whether a frame's or a procedure's steps are running (2.2).
    pub fn stepping(&self) -> bool {
        self.stepping
    }

    /// 2.3: `ph_frequency` set the step `h`; `dt_f` is the current frame's.
    /// `E` and `C` keep their values (1.3).
    pub fn set_step(&mut self, step: f32, dt_f: f32) {
        self.params.step = step;
        self.accumulator = dt_f - (dt_f / step).floor() * step;
    }

    /// 2.4: `ph_iterations` ran.
    pub fn set_iterations(&mut self, iterations: u32) {
        self.params.iterations = iterations;
    }

    pub fn set_hook(&mut self, hook: Option<StepHook>) {
        self.hook = hook;
    }

    /// The performance counter the timers read, ms
    /// ([`crate::perf_counter::now_ms`] unless set).
    pub fn set_perf_counter(&mut self, counter: fn() -> f64) {
        self.perf = counter;
    }

    /// The timers' totals since they were last taken (the statistics
    /// overlay takes them every frame); they restart from 0.
    pub fn take_timers(&mut self) -> Timers {
        std::mem::take(&mut self.timers)
    }

    /// Adds an object, not stepped until [`World::activate`]; it is in the
    /// collision index (physics-shell-shapeless-root.md 3.1).
    pub fn add(&mut self, object: O) -> ObjectId {
        let id = ObjectId(self.next_id);
        self.next_id += 1;
        self.objects.push(Entry {
            id,
            state: State::Idle,
            object,
        });
        id
    }

    /// Removes a destroyed object with its per-step actions (4.3).
    pub fn remove(&mut self, id: ObjectId) -> Option<O> {
        let i = self.objects.iter().position(|e| e.id == id)?;
        self.actions.retain(|a| a.object() != Some(id));
        self.resting.retain(|&(r, _)| r != id);
        Some(self.objects.remove(i).object)
    }

    /// An object and the solver together.
    pub fn with_solver(&mut self, id: ObjectId) -> (Option<&mut O>, &mut S) {
        let o = (self.objects.iter_mut())
            .find(|e| e.id == id)
            .map(|e| &mut e.object);
        (o, &mut self.solver)
    }

    pub fn solver(&self) -> &S {
        &self.solver
    }

    pub fn get(&self, id: ObjectId) -> Option<&O> {
        self.entry(id).map(|e| &e.object)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut O> {
        self.objects
            .iter_mut()
            .find(|e| e.id == id)
            .map(|e| &mut e.object)
    }

    /// Whether the object is stepped.
    pub fn is_active(&self, id: ObjectId) -> bool {
        self.entry(id).is_some_and(|e| e.state == State::Active)
    }

    fn entry(&self, id: ObjectId) -> Option<&Entry<O>> {
        self.objects.iter().find(|e| e.id == id)
    }

    /// Makes an object active, also one set aside while frozen (5); it
    /// leaves the resting list (4.4). One not active is appended to the end
    /// of the active objects, as a contact's wake is
    /// (physics-shell-shapeless-root.md 4.2; our reading for every
    /// activation).
    pub fn activate(&mut self, id: ObjectId) -> Result<(), Fatal> {
        let Some(i) = self.objects.iter().position(|e| e.id == id) else {
            return Ok(());
        };
        if !self.objects[i].object.has_geometry() {
            return Err(Fatal("trying to activate destroyed or not created object!"));
        }
        if self.objects[i].state != State::Active {
            let mut e = self.objects.remove(i);
            e.state = State::Active;
            self.objects.push(e);
        }
        self.resting.retain(|&(r, _)| r != id);
        Ok(())
    }

    /// Takes a shell out of the simulation (5).
    pub fn deactivate(&mut self, id: ObjectId) -> Result<(), Fatal> {
        if self.stepping {
            return Err(Fatal(
                "can not deactivate physics shell during physics processing!!!",
            ));
        }
        if self.frozen {
            return Err(Fatal(
                "can not deactivate physics shell when ph world is freezed!!!",
            ));
        }
        let Some(e) = self.objects.iter_mut().find(|e| e.id == id) else {
            return Ok(());
        };
        if e.state == State::SetAside {
            return Err(Fatal("can not deactivate freezed !!!"));
        }
        e.state = State::Inactive;
        self.resting.retain(|&(r, _)| r != id);
        Ok(())
    }

    /// 1.4: switches the gravity of an object's bodies.
    pub fn set_gravity(&mut self, id: ObjectId, on: bool) {
        if let Some(o) = self.get_mut(id) {
            o.set_gravity(on);
        }
    }

    /// Adds a per-step updater (3, items 5 and 9).
    pub fn add_updater(&mut self, updater: Box<dyn Updater>) {
        self.updaters.push((true, updater));
    }

    /// Adds a per-step action (4.3).
    pub fn add_action(&mut self, action: Box<dyn Action<O>>) {
        self.actions.push(action);
    }

    /// 4.3: a script's constant force `direction × size` on a shell for `ms`
    /// milliseconds; nothing without the shell.
    pub fn constant_force(&mut self, id: ObjectId, direction: Vec3, size: f32, ms: u32) {
        if self.entry(id).is_none() {
            return;
        }
        let steps = (0.001 * ms as f32 / self.params.step).floor() as u64;
        let force = direction.map(|d| d * size);
        self.actions.push(Box::new(ConstantForce {
            object: id,
            force,
            last: self.counter + steps,
        }));
    }

    /// 5: sets every active object and updater aside.
    pub fn freeze(&mut self) -> Result<(), Fatal> {
        if self.frozen {
            return Err(Fatal("already freezed!!!"));
        }
        self.frozen = true;
        for e in self.objects.iter_mut().filter(|e| e.state == State::Active) {
            e.state = State::SetAside;
            e.object.set_aside();
        }
        for (active, u) in self.updaters.iter_mut().filter(|(a, _)| *a) {
            *active = false;
            u.set_aside();
        }
        Ok(())
    }

    /// 5: makes everything set aside active again.
    pub fn unfreeze(&mut self) -> Result<(), Fatal> {
        if !self.frozen {
            return Err(Fatal("is not freezed!!!"));
        }
        self.frozen = false;
        for e in self
            .objects
            .iter_mut()
            .filter(|e| e.state == State::SetAside)
        {
            e.state = State::Active;
            e.object.restore();
        }
        for (active, u) in self.updaters.iter_mut().filter(|(a, _)| !*a) {
            *active = true;
            u.restore();
        }
        Ok(())
    }

    /// 2.2: the frame's steps, `dt_f` the display frame time and `now_ms` the
    /// millisecond clock at the frame's start. Returns the steps run.
    pub fn frame(&mut self, dt_f: f32, now_ms: u64) -> u32 {
        let n = self.frame_count(dt_f);
        for i in 0..n {
            self.frame_step(now_ms, i);
        }
        n
    }

    /// 2.2: the number of steps a frame of display frame time `dt_f` runs,
    /// taken from the accumulator; [`World::frame_step`] runs each (the
    /// level runs them with the player's movement steps).
    pub fn frame_count(&mut self, dt_f: f32) -> u32 {
        if self.frozen {
            return 0;
        }
        self.accumulator += self.time_factor * dt_f;
        if self.accumulator < self.params.step {
            return 0;
        }
        let n = (self.accumulator / self.params.step).floor();
        self.accumulator -= n * self.params.step;
        n as u32
    }

    /// The step `i` (from 0) of a frame starting at `now_ms`, timed by the
    /// `Physics` timer.
    pub fn frame_step(&mut self, now_ms: u64, i: u32) {
        let start = (self.perf)();
        let ms = (1000.0 * self.params.step).round() as u64;
        let at = now_ms + u64::from(i) * ms;
        self.step(at, at + ms);
        self.timers.physics += (self.perf)() - start;
    }

    /// 3: one step, from `start_ms` to `end_ms`; a procedure runs it on a
    /// frozen world for the objects it put back (5).
    pub fn step(&mut self, start_ms: u64, end_ms: u64) {
        self.stepping = true;
        let params = self.params;
        // 1. The rest pass (4.4, as the review reads it: a count of 0 is
        // handled, a higher one drops).
        if self.rest_pass == 0 {
            let objects = &mut self.objects;
            self.resting.retain_mut(|(id, count)| {
                if *count > 0 {
                    *count -= 1;
                    return true;
                }
                if let Some(e) = objects.iter_mut().find(|e| e.id == *id) {
                    e.object.clear_level_contacts();
                }
                false
            });
        }
        self.rest_pass = (self.rest_pass + 1) % REST_PASS_PERIOD;
        // 2.
        self.counter += 1;
        // 3.
        let collider = (self.perf)();
        self.solver.collide(
            &params,
            &mut Active {
                entries: &mut self.objects,
                resting: &mut self.resting,
            },
        );
        self.timers.collider += (self.perf)() - collider;
        // 4, 5.
        for e in self.objects.iter_mut().filter(|e| e.state == State::Active) {
            e.object.pre_step(&params);
        }
        for (_, u) in self.updaters.iter_mut().filter(|(a, _)| *a) {
            u.pre_step(&params);
        }
        // 6.
        let solver = (self.perf)();
        let counter = self.counter;
        let (objects, resting) = (&mut self.objects, &mut self.resting);
        self.actions.retain_mut(|a| match a.condition(counter) {
            Condition::Done => false,
            Condition::Waits => true,
            Condition::Holds => !a.act(
                counter,
                &mut Active {
                    entries: objects,
                    resting,
                },
            ),
        });
        // 7.
        self.solver.solve(
            &params,
            &mut Active {
                entries: &mut self.objects,
                resting: &mut self.resting,
            },
        );
        self.timers.solver += (self.perf)() - solver;
        self.timers.steps += 1;
        // 8, with the floor (4.5) and resting (4.4).
        for e in self.objects.iter_mut().filter(|e| e.state == State::Active) {
            let resting = e.object.post_step(&params);
            let kind = e.object.kind();
            if kind == Kind::Shell && resting && !self.resting.iter().any(|&(r, _)| r == e.id) {
                self.resting.push((e.id, RESTING_COUNT));
            }
            if e.object.height() < self.floor {
                match kind {
                    Kind::Shell => e.state = State::Inactive,
                    Kind::Character => e.object.below_floor(),
                }
            }
        }
        // 9.
        for (_, u) in self.updaters.iter_mut().filter(|(a, _)| *a) {
            u.post_step(&params);
        }
        // 10.
        self.solver.drop_contacts();
        // 11.
        if let Some(hook) = &mut self.hook {
            hook(start_ms, end_ms);
        }
        self.stepping = false;
    }

    /// Whether the shell is on the resting list, with its count (4.4).
    pub fn resting_count(&self, id: ObjectId) -> Option<u32> {
        self.resting
            .iter()
            .find(|&&(r, _)| r == id)
            .map(|&(_, c)| c)
    }
}
