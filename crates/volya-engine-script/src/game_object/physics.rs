// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script API Y's physics part
//! (docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md;
//! section numbers are its): the members of `game_object` that lead to a
//! shell, a vehicle, the helicopter or a lamp or act on an object's physics
//! (2), and the classes `physics_shell` (3), `physics_element` (4) and
//! `physics_joint` (5).
//!
//! What scripts see of a shell is a [`PhysicsShell`]: its elements and
//! joints with the values a script call stores, kept in [`PhysicsState`]
//! by object. What the engine's physics world holds (bodies, the library's
//! joints) the members reach through [`PhysicsWorld`], which the engine
//! implements (TODO(#1056): the app doesn't yet); a shell is **active** while the world has it (`shells`
//! 2.1). Level queries (rays, patrol paths) go through [`LevelQueries`].
//!
//! Shells, elements and joints reach scripts as new values referring to the
//! engine's objects by object id and index (1.3, believed references): a
//! value held after the engine dropped its object fails when used (our
//! choice; unestablished).

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use volya_engine_core::config::Config;
use volya_engine_core::physics::INITIAL_STEP;
use volya_engine_core::physics::ode::shell::stiffness;
use volya_engine_core::physics::shell::{JointKind as BuiltKind, Shell, ShellModel};
use volya_engine_formats::bone_physics::{AxisLimits, JointType, LimitedJoint};
use volya_engine_formats::cp1251;
use volya_engine_formats::model::Mat4;

use super::members::{Call, member};
use super::{Context, Kinds, ObjectClass};
use crate::Value;
use crate::classes::{EngineClass, Objects, Operand};
use crate::values::{Matrix, Vector, to_bits32, to_f32};

/// A point or direction in the world.
pub type Vec3 = [f32; 3];

/// The class a run-time class test of 2 looks for.
pub const CAR: &str = "CCar";
pub const HELICOPTER: &str = "CHelicopter";
pub const HANGING_LAMP: &str = "hanging_lamp";

/// The explode event's number (2.1).
pub const EXPLODE_EVENT: u16 = 23;

/// A body of an active element, as the world holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub linear: Vec3,
    pub angular: Vec3,
    /// Switched on (`shells` conventions, `resting`): the library's
    /// enabled flag.
    pub switched_on: bool,
    /// The element's frame in the world (`shells` 2.1): rows i, j, k, c.
    pub frame: Mat4,
}

/// The engine's physics world as the members reach it, by object id and
/// the shell's element and joint indices (those of its build, `shells` 3).
pub trait PhysicsWorld {
    /// The step length *h* (`world` 1.2).
    fn step(&self) -> f32;
    /// Whether object `object`'s shell is in the world (active, `shells`
    /// 2.1): its bodies and joints made.
    fn has_shell(&self, object: u16) -> bool;
    /// An element's body; `None` when the shell isn't active.
    fn body(&self, object: u16, element: usize) -> Option<Body>;
    /// The shell's force application (3.1).
    fn apply_force(&mut self, object: u16, force: Vec3);
    /// An element's force application (4.1).
    fn apply_element_force(&mut self, object: u16, element: usize, force: Vec3);
    /// The shell's waking (`resting` 2): every body switched on, the shell
    /// made active in the world.
    fn wake(&mut self, object: u16);
    /// Fixes an element's body (`shells` 5.1).
    fn fix(&mut self, object: u16, element: usize);
    /// Gives a fixed element's body back its own mass (4.3).
    fn release_fixed(&mut self, object: u16, element: usize);
    /// A constant force (`world` 4.3): `direction × size` for `ms`
    /// milliseconds; nothing without the shell.
    fn constant_force(&mut self, object: u16, direction: Vec3, size: f32, ms: u32);
    /// Whether joint `joint` is made in the library.
    fn joint_made(&self, object: u16, joint: usize) -> bool;
    /// Sets one of a made joint's library parameters (public 0.5.0's
    /// numbering, an axis's parameters offset by 256 per axis).
    fn set_joint_param(&mut self, object: u16, joint: usize, parameter: i32, value: f32);
    /// A made joint's angle of axis `axis` (5.4), as the library gives it.
    fn joint_angle(&self, object: u16, joint: usize, axis: i32) -> Option<f32>;
    /// A made joint's axis `axis` in the world (5.4).
    fn joint_axis(&self, object: u16, joint: usize, axis: i32) -> Option<Vec3>;
    /// A made joint's anchor in the world (5.4).
    fn joint_anchor(&self, object: u16, joint: usize) -> Option<Vec3>;
    /// One step's breaking check of the shell (3.3; PH-13's).
    fn breaking_check(&mut self, _object: u16) {}
}

/// The level's queries the members make.
pub trait LevelQueries {
    /// The explosive's ray query from `position` (2.1, believed below it):
    /// the normal of the level surface it finds; `None` when it finds
    /// nothing or an object.
    fn explosion_normal(&self, position: Vec3) -> Option<Vec3>;
    /// Whether the level (believed its static geometry) blocks the ray from
    /// `from` to `to`.
    fn ray_blocked(&self, from: Vec3, to: Vec3) -> bool;
    /// The distance from `from` straight down to the level, up to `range`.
    fn distance_below(&self, from: Vec3, range: f32) -> Option<f32>;
    /// The highest y of the level's box (believed `world` 4.5's).
    fn level_box_top(&self) -> f32;
    /// Point `point` of the level's patrol path `path` (spec N).
    fn patrol_point(&self, path: &[u8], point: u32) -> Option<Vec3>;
}

/// A joint's kind (5.1): which library joint it is made as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointKind {
    Ball,
    Hinge,
    TwoAxisHinge,
    BallWithAngularMotor,
    Slider,
}

impl JointKind {
    /// The axis index adjusted to the kind (5.1).
    pub fn adjust(self, n: i32) -> i32 {
        let n = n.max(-1);
        match self {
            JointKind::Ball => -1,
            JointKind::Hinge => 0,
            JointKind::TwoAxisHinge | JointKind::Slider => n.min(1),
            JointKind::BallWithAngularMotor => n.min(2),
        }
    }

    /// The axis records it is made with (5.1).
    pub fn axes(self) -> usize {
        match self {
            JointKind::Ball => 0,
            JointKind::Hinge => 1,
            JointKind::TwoAxisHinge | JointKind::Slider => 2,
            JointKind::BallWithAngularMotor => 5,
        }
    }

    /// The axes an index can name once adjusted (5.1).
    fn indexed(self) -> usize {
        match self {
            JointKind::BallWithAngularMotor => 3,
            kind => kind.axes(),
        }
    }
}

/// The frame an anchor or axis direction is given in (5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    World,
    FirstElement,
    SecondElement,
}

/// One axis record of a joint: the values the setters store (5.2 – 5.6).
#[derive(Clone, Debug, PartialEq)]
pub struct JointAxis {
    pub low: f32,
    pub high: f32,
    pub force: f32,
    pub velocity: f32,
    pub direction: Option<(Frame, Vec3)>,
    /// Its stops' error reduction and constraint-force mixing.
    pub stop_erp: f32,
    pub stop_cfm: f32,
}

impl Default for JointAxis {
    /// A record as made (5.1): limits −∞ … +∞, no force or velocity.
    fn default() -> Self {
        JointAxis {
            low: f32::NEG_INFINITY,
            high: f32::INFINITY,
            force: 0.0,
            velocity: 0.0,
            direction: None,
            stop_erp: 0.0,
            stop_cfm: 0.0,
        }
    }
}

/// A joint of a shell as scripts see it (5).
#[derive(Clone, Debug, PartialEq)]
pub struct ShellJoint {
    pub kind: JointKind,
    pub bone: u16,
    pub first: Option<usize>,
    pub second: Option<usize>,
    /// It has breaking data (PH-13).
    pub breakable: bool,
    pub axes: Vec<JointAxis>,
    pub anchor: Option<(Frame, Vec3)>,
    /// Its error reduction and constraint-force mixing (5.6).
    pub erp: f32,
    pub cfm: f32,
}

/// An element of a shell as scripts see it (4).
#[derive(Clone, Debug, PartialEq)]
pub struct ShellElement {
    /// The bone that made it.
    pub bone: Option<u16>,
    /// As set (`shells` 2.3).
    pub mass: f32,
    /// The sum of its geometries' volumes (`bone` 2.3).
    pub volume: f32,
    /// The fixed mark (4.3).
    pub fixed: bool,
    /// It has breaking data of its own (PH-13).
    pub breakable: bool,
}

/// A shell as scripts see it (3): its elements and joints in the order of
/// its build, its model's bone names and its breaking data.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsShell {
    pub elements: Vec<ShellElement>,
    pub joints: Vec<ShellJoint>,
    /// The model's bone names; `None` without a model.
    pub bones: Option<Vec<Vec<u8>>>,
    /// The breaking data (3.3): `Some(blocked)` for a shell that can break.
    pub breaking: Option<bool>,
}

/// `bone` 3.3's constants of the two-axis hinge.
const TWO_AXIS_SPRING: f32 = 20_000.0;
const TWO_AXIS_DAMPING: f32 = 1_000.0;

/// A joint's error reduction and mixing from `spring` and `damping` at
/// step `h` (`bone` 3.3).
fn joint_stiffness(kind: JointKind, spring: f32, damping: f32, h: f32) -> (f32, f32) {
    if kind == JointKind::TwoAxisHinge {
        let k = TWO_AXIS_SPRING * spring * h;
        let d = TWO_AXIS_DAMPING * damping;
        (k / (k + d), 1.0 / (k + d))
    } else {
        stiffness(spring, damping, h)
    }
}

/// An axis's stop stiffness (5.6): the two-axis hinge's are 1 and 0.
fn stop_stiffness(kind: JointKind, spring: f32, damping: f32, h: f32) -> (f32, f32) {
    if kind == JointKind::TwoAxisHinge {
        (1.0, 0.0)
    } else {
        stiffness(spring, damping, h)
    }
}

/// An axis record of a joint built from the bone data (5.1): the bone's
/// limits when the axis gets them (`bone` 3.2: a range below 2π; as handed
/// to the library, kept within −π … 0 and 0 … π, our reading), else
/// −∞ … +∞; no force or velocity (`joints` 1). Its stop stiffness from the
/// limit factors (`bone` 3.3) is our reading: 5.1 doesn't state it.
fn axis_of(kind: JointKind, limits: &AxisLimits, h: f32) -> JointAxis {
    let mut axis = JointAxis::default();
    if limits.limited() {
        (axis.low, axis.high) = limits.kept();
    }
    (axis.stop_erp, axis.stop_cfm) =
        stop_stiffness(kind, limits.spring_factor, limits.damping_factor, h);
    axis
}

impl PhysicsShell {
    /// The shell built as `shell` from `model` (`shells` 3; `None` for a
    /// shell without one), its values at step `h`. Elements without
    /// geometry and the fixed bones' are marked fixed, as activating fixes
    /// them (`shells` 5.1).
    pub fn of_shell(shell: &Shell, model: Option<&ShellModel>, h: f32) -> Self {
        let physics = |bone: usize| model.and_then(|m| m.bones.get(bone)).map(|b| &b.physics);
        let elements = (shell.elements.iter().enumerate())
            .map(|(i, e)| {
                // A rigid breakable bone joined to it is a part that can
                // split off (`bone` 3.5).
                let breakable = e.geometries.iter().any(|g| {
                    g.bone.is_some_and(|b| {
                        Some(b) != e.bone
                            && physics(b)
                                .is_some_and(|p| p.joint == JointType::Rigid && p.breakable())
                    })
                });
                ShellElement {
                    bone: e.bone.and_then(|b| u16::try_from(b).ok()),
                    mass: if e.has_geometry() {
                        e.mass_centre().0
                    } else {
                        0.0
                    },
                    volume: e.geometries.iter().map(|g| g.shape.volume()).sum(),
                    fixed: !e.has_geometry() || shell.fixed.contains(&i),
                    breakable,
                }
            })
            .collect::<Vec<_>>();
        let joints = (shell.joints.iter())
            .map(|j| {
                let p = physics(j.bone);
                let (kind, axes) = match (j.kind, p) {
                    (BuiltKind::Hinge { limits, .. }, _) => (
                        JointKind::Hinge,
                        vec![axis_of(JointKind::Hinge, &limits, h)],
                    ),
                    (BuiltKind::Other(t), p) => {
                        let kind = match t {
                            JointType::Ball => JointKind::Ball,
                            JointType::TwoAxisHinge => JointKind::TwoAxisHinge,
                            JointType::Slider => JointKind::Slider,
                            _ => JointKind::BallWithAngularMotor,
                        };
                        use volya_engine_formats::bone_physics::Axis::{X, Y, Z};
                        let names = match (kind, p.and_then(|p| p.limited_joint())) {
                            (JointKind::BallWithAngularMotor, Some(LimitedJoint::ThreeAxis(a))) => {
                                a.to_vec()
                            }
                            (JointKind::BallWithAngularMotor, _) => vec![Z, X, Y],
                            (JointKind::TwoAxisHinge, _) => vec![X, Z],
                            (JointKind::Slider, _) => vec![X, Y],
                            _ => Vec::new(),
                        };
                        let mut axes: Vec<JointAxis> = names
                            .into_iter()
                            .map(|a| {
                                let limits = p.map_or(AxisLimits::default(), |p| *p.limits(a));
                                axis_of(kind, &limits, h)
                            })
                            .collect();
                        // The records the bone data doesn't name, as made.
                        axes.resize(kind.axes(), JointAxis::default());
                        (kind, axes)
                    }
                };
                // The joint's own stiffness before a setter, unstated: the
                // bone's spring and damping (`bone` 3.3), our reading.
                let (erp, cfm) = joint_stiffness(kind, j.spring, j.damping, h);
                ShellJoint {
                    kind,
                    bone: u16::try_from(j.bone).unwrap_or(u16::MAX),
                    first: Some(j.first),
                    second: Some(j.second),
                    breakable: p.is_some_and(|p| p.breakable()),
                    axes,
                    anchor: None,
                    erp,
                    cfm,
                }
            })
            .collect::<Vec<_>>();
        // A shell can break when an element or joint of it can (PH-13's,
        // our reading: TODO(#1055)).
        let breakable = elements.iter().any(|e| e.breakable) || joints.iter().any(|j| j.breakable);
        PhysicsShell {
            elements,
            joints,
            bones: model.map(|m| {
                (m.bones.iter())
                    .map(|b| crate::game_object::game_bytes(&b.name))
                    .collect()
            }),
            breaking: breakable.then_some(false),
        }
    }

    /// The bone id of `name` in the shell's model; the model's "no bone" id
    /// (`0xFFFF`) for a name it lacks or without a model (3.2).
    pub fn bone_id(&self, name: &[u8]) -> u16 {
        (self.bones.as_ref())
            .and_then(|b| b.iter().position(|n| n.eq_ignore_ascii_case(name)))
            .and_then(|i| u16::try_from(i).ok())
            .unwrap_or(u16::MAX)
    }
}

/// What the members ask of the engine, in order
/// ([`PhysicsState::take_events`]).
#[derive(Clone, Debug, PartialEq)]
pub enum PhysicsEvent {
    /// The explosive's explode event (2.1, number [`EXPLODE_EVENT`]): what
    /// it does is the explosions' (unestablished).
    Explode {
        object: u16,
        initiator: u16,
        position: Vec3,
        normal: Vec3,
    },
    /// `CarExplode()`'s acts on the car's parts and its shell, and the
    /// destroyed object it may create (7; unestablished: PH-21's).
    CarExploded { object: u16 },
    /// `StartFlame()`'s particle effect (8.5), named by the section's
    /// `smoke_particle` (believed).
    Flame { object: u16, particle: Vec<u8> },
    /// `Die()` (8.5): the section's `broken_snd` at its position, its bones
    /// hidden, its shell handed to the death fall (PH-21's).
    HelicopterDied {
        object: u16,
        sound: Vec<u8>,
        position: Vec3,
    },
    /// `Explode()` (8.5): the flame stopped, the destroyed object it may
    /// create, the sound `Die` played stopped.
    HelicopterExploded { object: u16 },
    /// A lamp's switching on or off (9, `lamps` 5.1).
    LampSwitched { object: u16, on: bool },
}

/// The physics, vehicles and lamps as scripts see them.
#[derive(Default)]
pub struct PhysicsState {
    /// The physics world; `None` when there is none (2.2).
    pub world: Option<Box<dyn PhysicsWorld>>,
    pub level: Option<Box<dyn LevelQueries>>,
    /// Each object's shell, which the engine adds when it builds one.
    pub shells: BTreeMap<u16, PhysicsShell>,
    pub cars: BTreeMap<u16, super::vehicles::Car>,
    pub helicopters: BTreeMap<u16, super::vehicles::Helicopter>,
    /// The holder the player's object keeps (2: believed the vehicle it is
    /// in); the engine sets it.
    pub current_holder: Option<u16>,
    /// Explosives marked exploding (2.1).
    pub exploding: BTreeSet<u16>,
    events: Vec<PhysicsEvent>,
    /// `config\system.ltx`, the helicopters' sections (8).
    pub(super) system: Option<Rc<Config>>,
}

impl PhysicsState {
    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<PhysicsEvent> {
        std::mem::take(&mut self.events)
    }

    pub(super) fn push(&mut self, event: PhysicsEvent) {
        self.events.push(event);
    }

    fn active(&self, object: u16) -> bool {
        self.world.as_ref().is_some_and(|w| w.has_shell(object))
    }

    fn step(&self) -> f32 {
        self.world.as_ref().map_or(INITIAL_STEP, |w| w.step())
    }
}

/// A `physics_shell`'s part: the object whose shell it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellRef {
    pub object: u16,
}

/// A `physics_element`'s part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementRef {
    pub object: u16,
    pub element: usize,
}

/// A `physics_joint`'s part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JointRef {
    pub object: u16,
    pub joint: usize,
}

type Results = Result<Vec<Value>, String>;

/// What a member of this module's classes gets.
struct PCall<'a> {
    ctx: &'a Context,
    p: &'a mut PhysicsState,
    args: &'a [Value],
    o: &'a Objects<'a>,
    name: &'static str,
}

impl PCall<'_> {
    fn arg(&self, i: usize) -> &Value {
        self.args.get(i).unwrap_or(&Value::Nil)
    }

    /// A number argument (our wording for a mismatch, depends on A).
    fn number(&self, i: usize) -> Result<f32, String> {
        match self.arg(i) {
            Value::Number(n) => Ok(to_f32(*n)),
            _ => Err(format!("{}: number expected", self.name)),
        }
    }

    fn int(&self, i: usize) -> Result<i32, String> {
        match self.arg(i) {
            Value::Number(n) => Ok(to_bits32(*n)),
            _ => Err(format!("{}: number expected", self.name)),
        }
    }

    fn text(&self, i: usize) -> Result<Vec<u8>, String> {
        match self.arg(i) {
            Value::String(s) => Ok(s.clone()),
            _ => Err(format!("{}: text expected", self.name)),
        }
    }

    fn xyz(&self) -> Result<Vec3, String> {
        Ok([self.number(0)?, self.number(1)?, self.number(2)?])
    }

    /// Writes `v` into the vector argument `i` (the result of a getter
    /// filling a script's vector, 3.1, 4.2, 5.4).
    fn write_vector(&self, i: usize, v: Vec3) -> Result<(), String> {
        if self.o.set(self.arg(i), Vector::new(v[0], v[1], v[2])) {
            Ok(())
        } else {
            Err(format!("{}: vector expected", self.name))
        }
    }

    /// A fatal error standing for a failed check or a crash the spec
    /// believes (our wording).
    fn crash(&self, what: &str) -> String {
        self.ctx.fatal(
            self.name,
            line!(),
            "<no expression>",
            &format!("{}: {what}", self.name),
        )
    }

    fn shell(&mut self, object: u16) -> Result<&mut PhysicsShell, String> {
        self.p
            .shells
            .get_mut(&object)
            .ok_or_else(|| format!("the shell of object {object} no longer exists"))
    }

    fn element(&mut self, r: ElementRef) -> Result<&mut ShellElement, String> {
        self.shell(r.object)?
            .elements
            .get_mut(r.element)
            .ok_or_else(|| "the element no longer exists".to_owned())
    }

    fn joint(&mut self, r: JointRef) -> Result<&mut ShellJoint, String> {
        self.shell(r.object)?
            .joints
            .get_mut(r.joint)
            .ok_or_else(|| "the joint no longer exists".to_owned())
    }

    fn world(&mut self) -> Option<&mut Box<dyn PhysicsWorld>> {
        self.p.world.as_mut()
    }
}

fn boolean(b: bool) -> Vec<Value> {
    vec![Value::Boolean(b)]
}

fn number(n: impl Into<f64>) -> Vec<Value> {
    vec![Value::Number(n.into())]
}

/// Adds member `name` of a class whose part is a `T`.
fn on<T: std::any::Any + Copy>(
    class: EngineClass,
    ctx: &Context,
    name: &'static str,
    f: impl Fn(&mut PCall, T) -> Results + 'static,
) -> EngineClass {
    let ctx = ctx.clone();
    class.method_with_objects(name, move |part: &mut T, o, args| {
        ctx.stopped()?;
        let physics = Rc::clone(&ctx.physics);
        let mut p = physics.borrow_mut();
        let mut call = PCall {
            ctx: &ctx,
            p: &mut p,
            args,
            o,
            name,
        };
        f(&mut call, *part)
    })
}

fn element_value(o: &Objects, object: u16, element: Option<usize>) -> Value {
    element.map_or(Value::Nil, |element| {
        o.new_object("physics_element", ElementRef { object, element })
    })
}

fn joint_value(o: &Objects, object: u16, joint: Option<usize>) -> Value {
    joint.map_or(Value::Nil, |joint| {
        o.new_object("physics_joint", JointRef { object, joint })
    })
}

/// `get_element_by_bone_id` (3.2): the element that drives the bone (the
/// one its bone made, `shells` 8.1) while active with a model, else the
/// first whose bone it is: both the element whose bone is `id`.
fn element_of_bone(shell: &PhysicsShell, id: u16) -> Option<usize> {
    shell.elements.iter().position(|e| e.bone == Some(id))
}

fn joint_of_bone(shell: &PhysicsShell, id: u16) -> Option<usize> {
    shell.joints.iter().position(|j| j.bone == id)
}

/// `physics_shell` (3).
fn shell_class(ctx: &Context) -> EngineClass {
    let class = EngineClass::new("physics_shell");
    let class = on(class, ctx, "apply_force", |c, s: ShellRef| {
        let force = c.xyz()?;
        if c.p.active(s.object)
            && let Some(w) = c.world()
        {
            w.apply_force(s.object, force);
        }
        Ok(Vec::new())
    });
    let velocity = |c: &mut PCall, s: ShellRef, angular: bool| -> Results {
        // The first element's (4.2); a shell without elements is believed
        // an invalid read: fatal here.
        if c.shell(s.object)?.elements.is_empty() {
            return Err(c.crash("the shell has no elements"));
        }
        let v = element_velocity(c, s.object, 0, angular);
        c.write_vector(0, v)?;
        Ok(Vec::new())
    };
    let class = on(class, ctx, "get_linear_vel", move |c, s: ShellRef| {
        velocity(c, s, false)
    });
    let class = on(class, ctx, "get_angular_vel", move |c, s: ShellRef| {
        velocity(c, s, true)
    });
    let class = on(class, ctx, "get_elements_number", |c, s: ShellRef| {
        Ok(number(c.shell(s.object)?.elements.len() as f64))
    });
    let class = on(class, ctx, "get_element_by_order", |c, s: ShellRef| {
        let n = c.int(0)? as u32 as usize;
        if n >= c.shell(s.object)?.elements.len() {
            // A failed check (believed fatal).
            return Err(c.crash("no element of that order"));
        }
        Ok(vec![element_value(c.o, s.object, Some(n))])
    });
    let class = on(class, ctx, "get_element_by_bone_id", |c, s: ShellRef| {
        let id = c.int(0)? as u16;
        let e = element_of_bone(c.shell(s.object)?, id);
        Ok(vec![element_value(c.o, s.object, e)])
    });
    let class = on(class, ctx, "get_element_by_bone_name", |c, s: ShellRef| {
        let name = c.text(0)?;
        let shell = c.shell(s.object)?;
        let e = element_of_bone(shell, shell.bone_id(&name));
        Ok(vec![element_value(c.o, s.object, e)])
    });
    let class = on(class, ctx, "get_joints_number", |c, s: ShellRef| {
        Ok(number(c.shell(s.object)?.joints.len() as f64))
    });
    let class = on(class, ctx, "get_joint_by_order", |c, s: ShellRef| {
        let n = c.int(0)? as u32 as usize;
        if n >= c.shell(s.object)?.joints.len() {
            // Not range-tested: believed an invalid read, fatal here.
            return Err(c.crash("no joint of that order"));
        }
        Ok(vec![joint_value(c.o, s.object, Some(n))])
    });
    let class = on(class, ctx, "get_joint_by_bone_id", |c, s: ShellRef| {
        let id = c.int(0)? as u16;
        let j = joint_of_bone(c.shell(s.object)?, id);
        Ok(vec![joint_value(c.o, s.object, j)])
    });
    let class = on(class, ctx, "get_joint_by_bone_name", |c, s: ShellRef| {
        let name = c.text(0)?;
        let shell = c.shell(s.object)?;
        let j = joint_of_bone(shell, shell.bone_id(&name));
        Ok(vec![joint_value(c.o, s.object, j)])
    });
    let class = on(class, ctx, "block_breaking", |c, s: ShellRef| {
        if let Some(blocked) = &mut c.shell(s.object)?.breaking {
            *blocked = true;
        }
        Ok(Vec::new())
    });
    let class = on(class, ctx, "unblock_breaking", |c, s: ShellRef| {
        let Some(blocked) = &mut c.shell(s.object)?.breaking else {
            return Ok(Vec::new());
        };
        *blocked = false;
        if c.p.active(s.object)
            && let Some(w) = c.world()
        {
            w.breaking_check(s.object);
        }
        Ok(Vec::new())
    });
    let class = on(class, ctx, "is_breaking_blocked", |c, s: ShellRef| {
        Ok(boolean(c.shell(s.object)?.breaking == Some(true)))
    });
    on(class, ctx, "is_breakable", |c, s: ShellRef| {
        Ok(boolean(c.shell(s.object)?.breaking == Some(false)))
    })
}

/// An element's velocity (4.2): the body's while active and switched on,
/// else zero.
fn element_velocity(c: &PCall, object: u16, element: usize, angular: bool) -> Vec3 {
    let body = c.p.world.as_ref().and_then(|w| w.body(object, element));
    match body {
        Some(b) if b.switched_on => {
            if angular {
                b.angular
            } else {
                b.linear
            }
        }
        _ => [0.0; 3],
    }
}

/// `physics_element` (4).
fn element_class(ctx: &Context) -> EngineClass {
    let class = EngineClass::new("physics_element");
    let class = on(class, ctx, "apply_force", |c, e: ElementRef| {
        let force = c.xyz()?;
        let fixed = c.element(e)?.fixed;
        if !fixed
            && c.p.active(e.object)
            && let Some(w) = c.world()
        {
            w.apply_element_force(e.object, e.element, force);
        }
        Ok(Vec::new())
    });
    let class = on(class, ctx, "get_linear_vel", |c, e: ElementRef| {
        c.element(e)?;
        let v = element_velocity(c, e.object, e.element, false);
        c.write_vector(0, v)?;
        Ok(Vec::new())
    });
    let class = on(class, ctx, "get_angular_vel", |c, e: ElementRef| {
        c.element(e)?;
        let v = element_velocity(c, e.object, e.element, true);
        c.write_vector(0, v)?;
        Ok(Vec::new())
    });
    let class = on(class, ctx, "get_mass", |c, e: ElementRef| {
        Ok(number(c.element(e)?.mass))
    });
    let class = on(class, ctx, "get_volume", |c, e: ElementRef| {
        Ok(number(c.element(e)?.volume))
    });
    let class = on(class, ctx, "get_density", |c, e: ElementRef| {
        let e = c.element(e)?;
        Ok(number(e.mass / e.volume))
    });
    let class = on(class, ctx, "global_transform", |c, e: ElementRef| {
        c.element(e)?;
        let body = c.p.world.as_ref().and_then(|w| w.body(e.object, e.element));
        // Read without a test: on an inactive element believed an invalid
        // read, fatal here.
        let Some(b) = body else {
            return Err(c.crash("the element is not active"));
        };
        let mut m = [0.0; 16];
        for (r, row) in b.frame.iter().enumerate() {
            m[4 * r..4 * r + 4].copy_from_slice(row);
        }
        Ok(vec![c.o.new_object("matrix", Matrix(m))])
    });
    let class = on(class, ctx, "fix", |c, e: ElementRef| {
        c.element(e)?.fixed = true;
        if c.p.active(e.object)
            && let Some(w) = c.world()
        {
            w.fix(e.object, e.element);
        }
        Ok(Vec::new())
    });
    let class = on(class, ctx, "release_fixed", |c, e: ElementRef| {
        let element = c.element(e)?;
        if !element.fixed {
            return Ok(Vec::new());
        }
        element.fixed = false;
        if c.p.active(e.object)
            && let Some(w) = c.world()
        {
            w.release_fixed(e.object, e.element);
        }
        Ok(Vec::new())
    });
    let class = on(class, ctx, "is_fixed", |c, e: ElementRef| {
        Ok(boolean(c.element(e)?.fixed))
    });
    on(class, ctx, "is_breakable", |c, e: ElementRef| {
        Ok(boolean(c.element(e)?.breakable))
    })
}

/// The library's parameters (public 0.5.0) of axis `axis`: each axis's are
/// offset by 256.
mod param {
    pub const LO_STOP: i32 = 0;
    pub const HI_STOP: i32 = 1;
    pub const VEL: i32 = 2;
    pub const F_MAX: i32 = 3;
    pub const CFM: i32 = 6;
    pub const STOP_ERP: i32 = 7;
    pub const STOP_CFM: i32 = 8;
    pub const SUSPENSION_ERP: i32 = 9;
    pub const SUSPENSION_CFM: i32 = 10;

    pub fn of_axis(parameter: i32, axis: usize) -> i32 {
        parameter + 256 * axis as i32
    }
}

/// The axes a setter with adjusted index `n` acts on: `n`, or every axis
/// for −1 when `all` (5.1).
/// Every axis is those an index can name (our reading for the ball with
/// angular motor, whose five records an index reaches three of).
fn axes_of(joint: &ShellJoint, n: i32, all: bool) -> Vec<usize> {
    let reachable = joint.kind.indexed().min(joint.axes.len());
    if n >= 0 {
        let n = n as usize;
        if n < reachable { vec![n] } else { Vec::new() }
    } else if all {
        (0..reachable).collect()
    } else {
        Vec::new()
    }
}

/// Hands the joint's parameters to the library when it is made.
fn hand_on(c: &mut PCall, j: JointRef, values: &[(i32, f32)]) {
    let made =
        c.p.world
            .as_ref()
            .is_some_and(|w| w.joint_made(j.object, j.joint));
    if made && let Some(w) = c.world() {
        for &(parameter, value) in values {
            w.set_joint_param(j.object, j.joint, parameter, value);
        }
    }
}

/// `physics_joint` (5).
fn joint_class(ctx: &Context) -> EngineClass {
    let class = EngineClass::new("physics_joint");
    let class = on(class, ctx, "get_bone_id", |c, j: JointRef| {
        Ok(number(c.joint(j)?.bone))
    });
    let class = on(class, ctx, "get_first_element", |c, j: JointRef| {
        let e = c.joint(j)?.first;
        Ok(vec![element_value(c.o, j.object, e)])
    });
    let class = on(class, ctx, "get_stcond_element", |c, j: JointRef| {
        let e = c.joint(j)?.second;
        Ok(vec![element_value(c.o, j.object, e)])
    });
    let class = on(class, ctx, "is_breakable", |c, j: JointRef| {
        Ok(boolean(c.joint(j)?.breakable))
    });
    let class = on(class, ctx, "get_axes_number", |c, j: JointRef| {
        Ok(number(c.joint(j)?.axes.len() as f64))
    });
    let class = on(class, ctx, "set_limits", |c, j: JointRef| {
        let (low, high, n) = (c.number(0)?, c.number(1)?, c.int(2)?);
        let joint = c.joint(j)?;
        if joint.first.is_none() || joint.second.is_none() {
            return Ok(Vec::new());
        }
        let n = joint.kind.adjust(n);
        let axes = axes_of(joint, n, false);
        for &a in &axes {
            // The reference angle it also stores (believed; its use is
            // PH-7's) is not kept: how it is computed is not stated.
            joint.axes[a].low = low;
            joint.axes[a].high = high;
        }
        let values: Vec<(i32, f32)> = (axes.iter())
            .flat_map(|&a| {
                [
                    (param::of_axis(param::LO_STOP, a), low),
                    (param::of_axis(param::HI_STOP, a), high),
                ]
            })
            .collect();
        hand_on(c, j, &values);
        Ok(Vec::new())
    });
    let class = on(class, ctx, "get_limits", |c, j: JointRef| {
        // The first two arguments are read, their values not used (5.2).
        c.number(0)?;
        c.number(1)?;
        let n = c.int(2)?;
        let active = c.p.active(j.object);
        let shell = c.shell(j.object)?;
        let joint = shell
            .joints
            .get(j.joint)
            .ok_or_else(|| "the joint no longer exists".to_owned())?;
        let n = joint.kind.adjust(n);
        // With n adjusted to −1 the record before the first is read
        // (believed an invalid read): fatal here.
        let Some(axis) = usize::try_from(n).ok().and_then(|n| joint.axes.get(n)) else {
            return Err(c.crash("the joint has no such axis"));
        };
        let (low, high) = (axis.low, axis.high);
        let first_free = joint
            .first
            .and_then(|e| shell.elements.get(e))
            .is_some_and(|e| !e.fixed && active);
        let (a, b) = if first_free {
            (low, high)
        } else {
            (-high, -low)
        };
        Ok(vec![
            Value::Number(f64::from(a)),
            Value::Number(f64::from(b)),
        ])
    });
    let class = on(
        class,
        ctx,
        "set_max_force_and_velocity",
        |c, j: JointRef| {
            let (force, velocity, n) = (c.number(0)?, c.number(1)?, c.int(2)?);
            c.joint(j)?;
            // `resting` 2: every body of its activated shell switched on, the
            // shell made active.
            if c.p.active(j.object)
                && let Some(w) = c.world()
            {
                w.wake(j.object);
            }
            let joint = c.joint(j)?;
            let n = joint.kind.adjust(n);
            let axes = axes_of(joint, n, true);
            for &a in &axes {
                joint.axes[a].force = force;
                joint.axes[a].velocity = velocity;
            }
            let values: Vec<(i32, f32)> = (axes.iter())
                .flat_map(|&a| {
                    [
                        (param::of_axis(param::F_MAX, a), force),
                        (param::of_axis(param::VEL, a), velocity),
                    ]
                })
                .collect();
            hand_on(c, j, &values);
            Ok(Vec::new())
        },
    );
    let class = on(
        class,
        ctx,
        "get_max_force_and_velocity",
        |c, _: JointRef| {
            // Its first two arguments are references to engine values, which
            // scripts can't make: a call with numbers is believed not to match
            // (spec A; our wording).
            Err(format!("{}: no overload matches the arguments", c.name))
        },
    );
    let class = on(class, ctx, "get_axis_angle", |c, j: JointRef| {
        let n = c.int(0)?;
        let kind = c.joint(j)?.kind;
        let library = |c: &PCall, axis: i32| {
            // TODO(#750): a joint the engine hasn't made (only hinges are
            // made) is unestablished: 0, the pose it was built in, here.
            (c.p.world.as_ref())
                .and_then(|w| w.joint_angle(j.object, j.joint, axis))
                .unwrap_or(0.0)
        };
        let angle = match kind {
            JointKind::Hinge | JointKind::TwoAxisHinge => library(c, 0),
            JointKind::BallWithAngularMotor => library(c, n),
            JointKind::Slider if n == 0 || n == 1 => library(c, n),
            JointKind::Slider | JointKind::Ball => f32::INFINITY,
        };
        Ok(number(angle))
    });
    let class = on(class, ctx, "get_axis_dir", |c, j: JointRef| {
        let n = c.int(0)?;
        let kind = c.joint(j)?.kind;
        let n = kind.adjust(n);
        let axis = match kind {
            JointKind::Ball => None,
            JointKind::Hinge | JointKind::Slider => Some(0),
            JointKind::TwoAxisHinge => Some(if n == 0 { 0 } else { 1 }),
            JointKind::BallWithAngularMotor => Some(n),
        };
        // A joint not made leaves `v` unchanged (our choice).
        let v = axis
            .and_then(|a| (c.p.world.as_ref()).and_then(|w| w.joint_axis(j.object, j.joint, a)));
        if let Some(v) = v {
            c.write_vector(1, v)?;
        }
        Ok(Vec::new())
    });
    let class = on(class, ctx, "get_anchor", |c, j: JointRef| {
        if c.joint(j)?.kind == JointKind::Slider {
            return Err(c.crash("position of slider joint is undefinite"));
        }
        // A joint not made leaves `v` unchanged (our choice).
        let v = (c.p.world.as_ref()).and_then(|w| w.joint_anchor(j.object, j.joint));
        if let Some(v) = v {
            c.write_vector(0, v)?;
        }
        Ok(Vec::new())
    });
    let anchor = |frame: Frame| {
        move |c: &mut PCall, j: JointRef| -> Results {
            let p = c.xyz()?;
            c.joint(j)?.anchor = Some((frame, p));
            Ok(Vec::new())
        }
    };
    let class = on(class, ctx, "set_anchor_global", anchor(Frame::World));
    let class = on(
        class,
        ctx,
        "set_anchor_vs_first_element",
        anchor(Frame::FirstElement),
    );
    let class = on(
        class,
        ctx,
        "set_anchor_vs_second_element",
        anchor(Frame::SecondElement),
    );
    let direction = |frame: Frame| {
        move |c: &mut PCall, j: JointRef| -> Results {
            let (v, n) = (c.xyz()?, c.int(3)?);
            let joint = c.joint(j)?;
            let n = joint.kind.adjust(n);
            for a in axes_of(joint, n, false) {
                joint.axes[a].direction = Some((frame, v));
            }
            Ok(Vec::new())
        }
    };
    let class = on(class, ctx, "set_axis_dir_global", direction(Frame::World));
    let class = on(
        class,
        ctx,
        "set_axis_dir_vs_first_element",
        direction(Frame::FirstElement),
    );
    let class = on(
        class,
        ctx,
        "set_axis_dir_vs_second_element",
        direction(Frame::SecondElement),
    );
    let class = on(
        class,
        ctx,
        "set_joint_spring_dumping_factors",
        |c, j: JointRef| {
            let (spring, damping) = (c.number(0)?, c.number(1)?);
            let h = c.p.step();
            let joint = c.joint(j)?;
            let kind = joint.kind;
            let (erp, cfm) = joint_stiffness(kind, spring, damping, h);
            joint.erp = erp;
            joint.cfm = cfm;
            let axes = joint.axes.len();
            let values: Vec<(i32, f32)> = match kind {
                JointKind::TwoAxisHinge => {
                    vec![(param::SUSPENSION_ERP, erp), (param::SUSPENSION_CFM, cfm)]
                }
                JointKind::Ball => Vec::new(),
                JointKind::Hinge | JointKind::Slider => vec![(param::CFM, cfm)],
                JointKind::BallWithAngularMotor => (0..axes)
                    .map(|a| (param::of_axis(param::CFM, a), cfm))
                    .collect(),
            };
            hand_on(c, j, &values);
            Ok(Vec::new())
        },
    );
    on(
        class,
        ctx,
        "set_axis_spring_dumping_factors",
        |c, j: JointRef| {
            let (spring, damping, n) = (c.number(0)?, c.number(1)?, c.int(2)?);
            let h = c.p.step();
            let joint = c.joint(j)?;
            let n = joint.kind.adjust(n);
            let (erp, cfm) = stop_stiffness(joint.kind, spring, damping, h);
            let axes = axes_of(joint, n, true);
            for &a in &axes {
                joint.axes[a].stop_erp = erp;
                joint.axes[a].stop_cfm = cfm;
            }
            let values: Vec<(i32, f32)> = (axes.iter())
                .flat_map(|&a| {
                    [
                        (param::of_axis(param::STOP_ERP, a), erp),
                        (param::of_axis(param::STOP_CFM, a), cfm),
                    ]
                })
                .collect();
            hand_on(c, j, &values);
            Ok(Vec::new())
        },
    )
}

/// The classes of 3 – 5, in registration order.
pub(super) fn classes(ctx: &Context) -> Vec<EngineClass> {
    vec![shell_class(ctx), element_class(ctx), joint_class(ctx)]
}

/// The client part of object `id` when its class is `class` (a run-time
/// class test, 2). An object spawned without its client class has none
/// (our reading).
fn client_of(c: &mut Call, class: &'static str) -> Result<Value, String> {
    let o = c.object()?;
    Ok(if o.class == ObjectClass::Named(class) {
        o.client.map_or(Value::Nil, Value::Object)
    } else {
        // The diagnostic goes to the routine that returns at once (M 2.3).
        Value::Nil
    })
}

/// The members of `game_object` of 2.
pub(super) fn game_object_members(class: EngineClass, ctx: &Context) -> EngineClass {
    let physics = Rc::clone(&ctx.physics);
    let class = member(class, ctx, "get_physics_shell", move |c| {
        let id = c.id;
        let physical = c.object()?.kinds.has(Kinds::PHYSICAL);
        let has = physics.borrow().shells.contains_key(&id);
        Ok(vec![if physical && has {
            c.o.new_object("physics_shell", ShellRef { object: id })
        } else {
            Value::Nil
        }])
    });
    let class = member(class, ctx, "get_car", |c| Ok(vec![client_of(c, CAR)?]));
    let class = member(class, ctx, "get_helicopter", |c| {
        Ok(vec![client_of(c, HELICOPTER)?])
    });
    let class = member(class, ctx, "get_hanging_lamp", |c| {
        Ok(vec![client_of(c, HANGING_LAMP)?])
    });
    let class = member(class, ctx, "get_holder_class", |c| {
        let id = c.id;
        Ok(vec![if c.object()?.kinds.has(Kinds::HOLDER) {
            c.o.new_object("holder", super::vehicles::HolderRef { object: id })
        } else {
            Value::Nil
        }])
    });
    let physics = Rc::clone(&ctx.physics);
    let class = member(class, ctx, "get_current_holder", move |c| {
        let player = c.object()?.kinds.has(Kinds::PLAYER);
        let holder = physics.borrow().current_holder;
        Ok(vec![match holder {
            Some(object) if player => {
                c.o.new_object("holder", super::vehicles::HolderRef { object })
            }
            _ => Value::Nil,
        }])
    });
    let physics = Rc::clone(&ctx.physics);
    let class = member(class, ctx, "explode", move |c| {
        let id = c.id;
        let o = c.object()?;
        if !o.kinds.has(Kinds::EXPLOSIVE) || o.parent.is_some() {
            return Ok(Vec::new());
        }
        let p = o.placement[3];
        explode(&mut physics.borrow_mut(), id, [p[0], p[1], p[2]], None);
        Ok(Vec::new())
    });
    let physics = Rc::clone(&ctx.physics);
    member(class, ctx, "set_const_force", move |c| {
        let direction = c.vector(0)?;
        let power = to_f32(c.number(1)?);
        let time = to_bits32(c.number(2)?) as u32;
        // Asked through the physical kind untested: believed a crash
        // without it, fatal here.
        if !c.object()?.kinds.has(Kinds::PHYSICAL) {
            return Err(c.crash("the object is not physical"));
        }
        let id = c.id;
        let mut p = physics.borrow_mut();
        let has_shell = p.shells.contains_key(&id);
        if has_shell && let Some(w) = p.world.as_mut() {
            w.constant_force(id, [direction.x, direction.y, direction.z], power, time);
        }
        Ok(Vec::new())
    })
}

/// The explosive part's explode (2.1) of object `id` at `position`: the
/// normal by its ray query unless `normal` is given, the event, the mark.
/// The event is sent only where the game is the server side: always, here
/// (the single-player game is its own server; believed).
pub(super) fn explode(p: &mut PhysicsState, id: u16, position: Vec3, normal: Option<Vec3>) {
    let normal = normal.unwrap_or_else(|| {
        (p.level.as_ref())
            .and_then(|l| l.explosion_normal(position))
            .unwrap_or([0.0, 1.0, 0.0])
    });
    p.push(PhysicsEvent::Explode {
        object: id,
        initiator: id,
        position,
        normal,
    });
    p.exploding.insert(id);
}

/// A bone name in the log's text.
pub(super) fn decode(name: &[u8]) -> String {
    cp1251::decode(name)
}

/// `Operand`'s other object, for the vehicle members.
pub(super) fn other<T: Clone>(operand: Option<Operand<T>>) -> Option<T> {
    match operand {
        Some(Operand::Other(t)) => Some(t),
        _ => None,
    }
}
