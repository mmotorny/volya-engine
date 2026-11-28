// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's physics world (docs/behavior/physics-world.md, cited by
//! section): made under `st_client_spawning` (1.1) over the ODE solver,
//! its steps run with the player's movement steps (our order: the
//! player's body is one of its objects, docs/behavior/physics-contacts.md
//! 5, and the movement steps it in the player's update), following
//! `ph_frequency` and `ph_iterations` (2.3, 2.4), removed with the level;
//! the contacts of its objects with each other and the level's mesh
//! (physics-contacts.md); and its shells
//! (docs/behavior/physics-shells.md, cited as `shells`): those of the
//! physics objects, hanging lamps and skeleton objects online at the start
//! (TODO(#949): those coming online later), built as the client creates
//! its objects (`shells` 1, 7).

use std::collections::HashMap;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use crate::core::collision::CollisionWorld;
use crate::core::collision::body::BodyBox;
use crate::core::collision::body::PlayerBody;
use crate::core::collision::geometry::{Vec3, normalize};
use crate::core::console::{Console, Value};
use crate::core::movement::StepHook;
use crate::core::physics::contacts::level::LevelState;
use crate::core::physics::contacts::{Bounds, Class};
use crate::core::physics::ode::character::OdeCharacter;
use crate::core::physics::ode::collide::{Draft, Geom};
use crate::core::physics::ode::shell::OdeShell;
use crate::core::physics::ode::{GroupId, Grouped, JointId, OdeSolver};
use crate::core::physics::shell::{
    self, BoneState, LampRecord, PhysicsRecord, Shape, Shell, ShellModel, turn,
};
use crate::core::physics::{
    INITIAL_STEP, Kind, LIBRARY_ITERATIONS, Object, ObjectId, Params, Timers, World,
};
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use crate::objects::{LevelObject, locate_model};
use volya_engine_formats::gamemtl::GameMaterial;
use volya_engine_formats::ltx;
use volya_engine_formats::model::{transform, transform_point};
use volya_engine_formats::object::{Object as Record, Skeleton};

/// What the level's world steps: shells and the player's body
/// (docs/behavior/physics-contacts.md 5); the other characters' bodies
/// come with their specs (PH-8, PH-22).
pub enum LevelBody {
    Shell(OdeShell),
    Character(OdeCharacter),
}

macro_rules! each {
    ($self:expr, $b:ident => $e:expr) => {
        match $self {
            LevelBody::Shell($b) => $e,
            LevelBody::Character($b) => $e,
        }
    };
}

impl Object for LevelBody {
    fn kind(&self) -> Kind {
        each!(self, b => b.kind())
    }
    fn has_geometry(&self) -> bool {
        each!(self, b => b.has_geometry())
    }
    fn height(&self) -> f32 {
        each!(self, b => b.height())
    }
    fn set_gravity(&mut self, on: bool) {
        each!(self, b => b.set_gravity(on))
    }
    fn add_force(&mut self, force: Vec3) {
        each!(self, b => b.add_force(force))
    }
    fn pre_step(&mut self, params: &Params) {
        each!(self, b => b.pre_step(params))
    }
    fn post_step(&mut self, params: &Params) -> bool {
        each!(self, b => b.post_step(params))
    }
    fn clear_level_contacts(&mut self) {
        each!(self, b => b.clear_level_contacts())
    }
}

impl Grouped for LevelBody {
    fn group(&self) -> GroupId {
        each!(self, b => Grouped::group(b))
    }
    fn before_solve(&mut self, solver: &mut OdeSolver, params: &Params) {
        each!(self, b => b.before_solve(solver, params))
    }
    fn after_solve(&mut self, solver: &OdeSolver) {
        each!(self, b => b.after_solve(solver))
    }
    fn class(&self) -> Class {
        each!(self, b => Grouped::class(b))
    }
    fn bounds(&self) -> Option<Bounds> {
        each!(self, b => b.bounds())
    }
    fn geometries(&self, solver: &OdeSolver, out: &mut Vec<Geom>) {
        each!(self, b => b.geometries(solver, out))
    }
    fn level_state(&mut self, geom: usize) -> Option<&mut LevelState> {
        each!(self, b => b.level_state(geom))
    }
    fn geometry_contact(&mut self, geom: usize, first: bool, d: &mut Draft) {
        each!(self, b => b.geometry_contact(geom, first, d))
    }
    fn object_contact(&mut self, geom: usize, first: bool, d: &mut Draft) {
        each!(self, b => b.object_contact(geom, first, d))
    }
    fn wake(&mut self, solver: &OdeSolver) {
        each!(self, b => Grouped::wake(b, solver))
    }
}

/// A level object's shell in the world.
struct LevelShell {
    /// The object's id.
    object: u16,
    id: ObjectId,
    /// The joints made, by the name of the bone each was made from.
    joints: Vec<(String, JointId)>,
    /// Its boxes in their elements' frames, with their materials.
    boxes: Vec<(usize, Shape, u32)>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
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
    /// The shells in the world, and those active (stepped).
    pub shells: usize,
    pub active: usize,
}

/// The world of one level.
pub struct LevelPhysics {
    world: World<LevelBody, OdeSolver>,
    /// The `ph_iterations` runs the world has followed.
    iterations_runs: u64,
    shells: Vec<LevelShell>,
    /// The next collision-group number (`shells` 5.2, `not_collide_parts`).
    collision_groups: u32,
    /// The player's body in the world (physics-contacts.md 5).
    player: Option<ObjectId>,
    /// The frame's steps still to run, the next one's index, and the
    /// frame's start (2.2).
    pending: u32,
    next: u32,
    frame_ms: u64,
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
            shells: Vec::new(),
            collision_groups: 0,
            player: None,
            pending: 0,
            next: 0,
            frame_ms: 0,
        }
    }

    /// The level's mesh and materials the contacts are made with
    /// (physics-contacts.md 2.1, 3).
    pub fn set_level(&mut self, level: Arc<CollisionWorld>) {
        self.world.solver_mut().set_level(level);
    }

    /// The player's body of mass `mass` enters the world, active
    /// (physics-contacts.md 5).
    pub fn add_player(&mut self, mass: f32) -> io::Result<()> {
        let c = self.world.solver_mut().add_character(mass);
        let id = self.world.add(LevelBody::Character(c));
        self.world.activate(id).map_err(|e| invalid(e.0))?;
        self.player = Some(id);
        Ok(())
    }

    /// 2.2: a frame of display frame time `dt_f` starting at `now_ms`
    /// begins: its steps run with the movement's ([`Self::steps`]), any
    /// left at [`Self::end_frame`].
    pub fn begin_frame(&mut self, dt_f: f32, now_ms: u64) {
        self.pending = self.world.frame_count(dt_f);
        self.next = 0;
        self.frame_ms = now_ms;
    }

    /// The frame's next step, if one is left, meeting the movement step
    /// `body` just ran: the player's body placed where that step began, and
    /// the reaction of its contacts with movable bodies handed back
    /// (physics-contacts.md 5, 7).
    pub fn player_step(&mut self, body: &mut PlayerBody) {
        if self.pending == 0 {
            return;
        }
        if let Some(id) = self.player {
            let (o, solver) = self.world.with_solver(id);
            if let Some(LevelBody::Character(c)) = o {
                c.set_step(solver, body.world_step());
            }
        }
        self.run_step();
        if let Some(id) = self.player
            && let Some(LevelBody::Character(c)) = self.world.get_mut(id)
        {
            let (dv, blocked) = c.take_reaction();
            body.react(dv, blocked);
        }
    }

    fn run_step(&mut self) {
        self.world.frame_step(self.frame_ms, self.next);
        self.next += 1;
        self.pending -= 1;
    }

    /// The contacts of the player's body with movable bodies its rules
    /// kept since it entered the world (physics-contacts.md 5).
    pub fn player_contacts(&self) -> u64 {
        match self.player.and_then(|id| self.world.get(id)) {
            Some(LevelBody::Character(c)) => c.kept(),
            _ => 0,
        }
    }

    /// The frame's steps the movement's did not run (2.2).
    pub fn end_frame(&mut self) {
        while self.pending > 0 {
            self.run_step();
        }
    }

    /// The movement's step hook running the frame's steps (2.2) from
    /// `now_ms`.
    pub fn steps(&mut self, now_ms: u64) -> PhysicsSteps<'_> {
        PhysicsSteps {
            physics: self,
            now_ms,
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
            shells: self.shells.len(),
            active: (self.shells.iter()).filter(|s| w.is_active(s.id)).count(),
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

    /// The shells of `objects` online at the start (`shells` 1): physics
    /// objects by their physics type (4), hanging lamps with a shell (5.3)
    /// and skeleton objects, each model read once; `materials` the game
    /// materials (the collision world's). An object without a model has
    /// none (our choice: none in the data).
    pub fn load_shells(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        dir: &str,
        objects: &[LevelObject],
        materials: &[GameMaterial],
    ) -> io::Result<()> {
        let mut models: HashMap<String, Rc<ShellModel>> = HashMap::new();
        for o in objects.iter().filter(|o| o.online) {
            let (visual, custom, skeleton) = match &o.record.object {
                Record::PhysicsObject(p) => (&p.visual, &p.object.custom_data, &p.skeleton),
                Record::HangingLamp(l) => (&l.visual, &l.object.custom_data, &l.skeleton),
                Record::SkeletonObject(s) => (&s.visual, &s.object.custom_data, &s.skeleton),
                _ => continue,
            };
            if visual.model.is_empty() {
                continue;
            }
            let h = &o.record.header;
            let named = |e: io::Error| invalid(format!("{}: {e}", h.name.as_str()));
            let path = locate_model(tree, data, dir, &visual.model)?;
            let model = match models.get(&path) {
                Some(m) => m.clone(),
                None => {
                    let m = Rc::new(
                        ShellModel::read(&vfs::read(tree, &path)?)
                            .map_err(|e| invalid(format!("{path}: {e}")))?,
                    );
                    models.insert(path, m.clone());
                    m
                }
            };
            // Custom data that is not LTX text has no keys (our choice).
            let custom = ltx::parse(custom.as_bytes()).ok();
            let x = transform(h.position, h.angles);
            let state = bone_state(skeleton);
            let shell = match &o.record.object {
                Record::PhysicsObject(p) => {
                    let r = PhysicsRecord {
                        physics_type: p.physics_type,
                        mass: p.mass,
                        fixed_bones: &p.fixed_bones,
                        skeleton_flags: p.skeleton.flags,
                        startup_animation: &p.skeleton.startup_animation,
                        custom_data: custom.as_ref(),
                        transform: x,
                        bone_state: state,
                    };
                    let Some(mut s) =
                        shell::physics_object(&model, &r, materials).map_err(named)?
                    else {
                        continue;
                    };
                    shell::apply_parts_keys(&mut s, &model, &mut self.collision_groups);
                    s
                }
                Record::HangingLamp(l) => {
                    let r = LampRecord {
                        light_flags: l.light_flags,
                        fixed_bones: &l.fixed_bones,
                        custom_data: custom.as_ref(),
                        transform: x,
                        bone_state: state,
                    };
                    let Some(s) = shell::hanging_lamp(&model, &r, materials).map_err(named)? else {
                        continue;
                    };
                    s
                }
                _ => shell::skeleton_object(&model, skeleton.flags, x, state, materials)
                    .map_err(named)?,
            };
            self.add_shell(o.id, &shell, &model).map_err(named)?;
        }
        Ok(())
    }

    /// Activates object `object`'s shell built from `model` (`shells` 2.1):
    /// active unless switched off (physics-resting.md 1).
    pub fn add_shell(&mut self, object: u16, shell: &Shell, model: &ShellModel) -> io::Result<()> {
        let params = *self.world.params();
        let body = self.world.solver_mut().activate_shell(&params, shell);
        let joints = body
            .joints()
            .map(|(j, id)| (model.bones[shell.joints[j].bone].name.clone(), id))
            .collect();
        let id = self.world.add(LevelBody::Shell(body));
        if !shell.switched_off {
            self.world.activate(id).map_err(|e| invalid(e.0))?;
        }
        let boxes = (shell.elements.iter().enumerate())
            .flat_map(|(i, e)| {
                (e.geometries.iter())
                    .filter(|g| matches!(g.shape, Shape::Box { .. }))
                    .map(move |g| (i, g.shape, e.material))
            })
            .collect();
        self.shells.push(LevelShell {
            object,
            id,
            joints,
            boxes,
        });
        Ok(())
    }

    fn shell(&self, object: u16) -> Option<&LevelShell> {
        self.shells.iter().find(|s| s.object == object)
    }

    /// The joint of object `object`'s shell made from bone `bone`.
    fn joint(&self, object: u16, bone: &str) -> Option<(ObjectId, JointId)> {
        let s = self.shell(object)?;
        let (_, j) = s
            .joints
            .iter()
            .find(|(b, _)| b.eq_ignore_ascii_case(bone))?;
        Some((s.id, *j))
    }

    /// A joint's motor on its axis (`door` 2.3: `set_max_force_and_velocity`
    /// on a hinge): it wakes the shell (physics-resting.md 2), then sets the hinge's
    /// maximum force and velocity. Nothing without the joint.
    pub fn set_max_force_and_velocity(
        &mut self,
        object: u16,
        bone: &str,
        force: f32,
        velocity: f32,
    ) -> io::Result<()> {
        let Some((id, joint)) = self.joint(object, bone) else {
            return Ok(());
        };
        let (body, solver) = self.world.with_solver(id);
        if let Some(LevelBody::Shell(s)) = body {
            s.wake(solver);
        }
        solver.set_hinge_motor(joint, force, velocity);
        self.world.activate(id).map_err(|e| invalid(e.0))
    }

    /// Object `object`'s shell, and whether it is active.
    pub fn shell_of(&self, object: u16) -> Option<(&OdeShell, bool)> {
        let s = self.shell(object)?;
        let Some(LevelBody::Shell(body)) = self.world.get(s.id) else {
            return None;
        };
        Some((body, self.world.is_active(s.id)))
    }

    /// The joint of object `object`'s shell made from bone `bone`.
    pub fn joint_of(&self, object: u16, bone: &str) -> Option<JointId> {
        self.joint(object, bone).map(|(_, j)| j)
    }

    /// The hinge angle of the joint of object `object`'s shell made from
    /// bone `bone` (physics-joints.md 2.2).
    pub fn hinge_angle(&self, object: u16, bone: &str) -> Option<f32> {
        let (_, joint) = self.joint(object, bone)?;
        self.world.solver().hinge_angle(joint)
    }

    /// The boxes of object `object`'s shell where its bodies hold them
    /// (`shells` 2.1, 2.2).
    pub fn boxes(&self, object: u16) -> Vec<BodyBox> {
        let Some(s) = self.shell(object) else {
            return Vec::new();
        };
        let Some(LevelBody::Shell(body)) = self.world.get(s.id) else {
            return Vec::new();
        };
        let solver = self.world.solver();
        (s.boxes.iter())
            .filter_map(|&(e, shape, material)| {
                let Shape::Box {
                    axes,
                    centre,
                    half_size,
                } = shape
                else {
                    return None;
                };
                let frame = body.frame(solver, e)?;
                Some(BodyBox {
                    centre: transform_point(&frame, centre),
                    axes: axes.map(|a| {
                        let w = turn(&frame, a);
                        normalize(w).unwrap_or(w)
                    }),
                    half_size,
                    material,
                })
            })
            .collect()
    }
}

/// The level's physics steps run with the movement's
/// ([`LevelPhysics::steps`]).
pub struct PhysicsSteps<'a> {
    physics: &'a mut LevelPhysics,
    now_ms: u64,
}

impl StepHook for PhysicsSteps<'_> {
    fn frame(&mut self, dt_f: f32) {
        self.physics.begin_frame(dt_f, self.now_ms);
    }

    fn step(&mut self, body: &mut PlayerBody) {
        self.physics.player_step(body);
    }
}

/// The root and visible bones of a Skeleton block (`shells` 3.1).
fn bone_state(skeleton: &Skeleton) -> BoneState {
    skeleton
        .bone_state
        .as_ref()
        .map_or_else(BoneState::default, |s| BoneState {
            root: usize::from(s.root_bone),
            visible: s.bones_mask,
        })
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
