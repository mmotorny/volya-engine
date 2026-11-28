// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script API Y's vehicles, helicopter and lamps
//! (docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md;
//! section numbers are its): the classes `holder` (6), `CCar` (7),
//! `CHelicopter` (8) and `hanging_lamp` (9).
//!
//! The car and the helicopter keep what their members set and read
//! ([`Car`], [`Helicopter`], in [`PhysicsState`]); what they do with it,
//! driving, flying, firing and falling, is the engine's (PH-21 and the
//! vehicles', TODO(#1057)), which reads that state and the events
//! the members raise ([`PhysicsEvent`]).
//!
//! `CCar` has two bases, `CGameObject` and `holder`; one base is
//! registered (`CGameObject`) and `holder`'s members are `CCar`'s own too.

use std::rc::Rc;

use volya_engine_core::config::{Config, value};

use super::engine_classes::{ClientObject, client_class};
use super::members::Handle;
use super::physics::{self, PhysicsEvent, PhysicsState, Vec3};
use super::{CLIENT_BASE, Context, GameObjects};
use crate::Value;
use crate::classes::{EngineClass, Objects};
use crate::values::{Vector, to_bits32, to_f32};

/// `CCar`'s constants (7).
pub const CAR_CONSTANTS: [(&str, i32); 6] = [
    ("eWpnDesiredDir", 1),
    ("eWpnDesiredPos", 2),
    ("eWpnFire", 3),
    ("eWpnActivate", 4),
    ("eWpnAutoFire", 5),
    ("eWpnToDefaultDir", 6),
];

/// `CHelicopter`'s constants (8.1).
pub const HELICOPTER_CONSTANTS: [(&str, i32); 13] = [
    ("eAlive", 0),
    ("eDead", 1),
    ("eMovNone", 0),
    ("eMovToPoint", 1),
    ("eMovPatrolPath", 2),
    ("eMovRoundPath", 3),
    ("eMovLanding", 4),
    ("eMovTakeOff", 5),
    ("eEnemyNone", 0),
    ("eEnemyPoint", 1),
    ("eEnemyEntity", 2),
    ("eBodyByPath", 0),
    ("eBodyToPoint", 1),
];

const DEAD: i32 = 1;
const MOV_TO_POINT: i32 = 1;
const MOV_PATROL_PATH: i32 = 2;
const MOV_ROUND_PATH: i32 = 3;
const ENEMY_NONE: i32 = 0;
const ENEMY_POINT: i32 = 1;
const ENEMY_ENTITY: i32 = 2;
const BODY_BY_PATH: i32 = 0;
const BODY_TO_POINT: i32 = 1;

/// `GetRealAltitude`'s ray (8.2).
const ALTITUDE_RANGE: f32 = 1000.0;

/// Degrees to radians as the helicopter's fire dispersion converts them
/// (8.4).
const DEGREES: f32 = 0.017_453_3;

/// The car's weapon part (6.2, 7): what its members set and read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CarWeapon {
    pub firing: bool,
    pub auto_fire: bool,
    /// Whether its first operation (believed activating) ran last.
    pub activated: bool,
    /// Its "can hit" flag (believed: the aim within its tolerance).
    pub can_hit: bool,
    /// Its two kept directions (believed its current and desired aim) and
    /// the direction it keeps (believed its default).
    pub current_dir: Vec3,
    pub desired_dir: Vec3,
    pub default_dir: Vec3,
    /// Its firing point in the world.
    pub fire_point: Vec3,
}

/// A car (7): its weapon part (the engine gives it one), its occupant
/// mark, and its explosion timer and mark.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Car {
    pub weapon: Option<CarWeapon>,
    /// Occupied (6.1, believed: by the player).
    pub engaged: bool,
    /// The timer's time, seconds, and its marks.
    pub explode_time: f32,
    pub timer_set: bool,
    pub timer_running: bool,
    pub exploded: bool,
}

/// The helicopter's enemy (8.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Enemy {
    #[default]
    None,
    Point(Vec3),
    Entity(u16),
}

/// A round path the movement built (8.2): its centre, radius and
/// direction.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundPath {
    pub centre: Vec3,
    pub radius: f32,
    pub clockwise: bool,
}

/// A helicopter (8): its states, movement and weapon values, each starting
/// from its section.
#[derive(Clone, Debug, PartialEq)]
pub struct Helicopter {
    pub state: i32,
    pub movement: i32,
    pub hunt: i32,
    pub body: i32,
    /// Its position as its movement keeps it (the engine moves it).
    pub position: Vec3,
    pub destination: Vec3,
    pub max_velocity: f32,
    /// Its current speed (believed) and heading and pitch, which the engine
    /// sets.
    pub curr_velocity: f32,
    pub heading: f32,
    pub pitch: f32,
    pub speed_in_dest: f32,
    pub on_point_range: f32,
    pub linear_acc: (f32, f32),
    pub safe_altitude: f32,
    pub round_path: Option<RoundPath>,
    pub patrol_path: Option<(Vec<u8>, u32)>,
    pub enemy: Enemy,
    pub look_at: (Vec3, bool),
    pub use_rocket: bool,
    pub use_mgun: bool,
    pub rocket_dist: (f32, f32),
    pub mgun_dist: (f32, f32),
    pub time_between_rocket_attack: u32,
    pub syncronize_rocket: bool,
    pub fire_trail_length: f32,
    pub use_fire_trail: bool,
    /// Radians.
    pub fire_dispersion: f32,
    pub barrel_dir_tolerance: f32,
    pub engine_sound_volume: f32,
    pub flame_started: bool,
    pub light_started: bool,
    pub exploded: bool,
    pub dead: bool,
    /// Its per-frame processing runs (`Die` stops it).
    pub processing: bool,
    /// The section's `smoke_particle` (believed) and `broken_snd`.
    pub smoke_particle: Vec<u8>,
    pub broken_sound: Vec<u8>,
    dispersion: (f32, f32),
}

impl Helicopter {
    /// The helicopter of section `section` of `system` at `position`: a
    /// key the section lacks is a failed check (believed fatal); the
    /// error names it.
    pub fn from_section(system: &Config, section: &str, position: Vec3) -> Result<Self, String> {
        let read = |key: &str| {
            system
                .read(section, key)
                .map_err(|_| format!("no key {key} in section {section}"))
        };
        let float = |key: &str| read(key).map(value::float);
        let boolean = |key: &str| read(key).map(value::bool);
        let text = |key: &str| {
            system
                .get(section, key)
                .flatten()
                .map(|v| crate::game_object::game_bytes(v.trim()))
                .unwrap_or_default()
        };
        let dispersion = (
            float("fire_dispersion_null")?,
            float("fire_dispersion_base")?,
        );
        let use_fire_trail = boolean("use_fire_trail")?;
        Ok(Helicopter {
            state: 0,
            movement: 0,
            hunt: ENEMY_NONE,
            body: BODY_BY_PATH,
            position,
            destination: position,
            max_velocity: float("velocity")?,
            curr_velocity: 0.0,
            heading: 0.0,
            pitch: 0.0,
            speed_in_dest: 0.0,
            on_point_range: float("on_point_range_dist")?,
            linear_acc: (float("path_linear_acc_fw")?, float("path_linear_acc_bk")?),
            safe_altitude: float("safe_altitude")?,
            round_path: None,
            patrol_path: None,
            enemy: Enemy::None,
            look_at: ([0.0; 3], false),
            use_rocket: boolean("use_rocket")?,
            use_mgun: boolean("use_mgun")?,
            rocket_dist: (
                float("min_rocket_attack_dist")?,
                float("max_rocket_attack_dist")?,
            ),
            mgun_dist: (
                float("min_mgun_attack_dist")?,
                float("max_mgun_attack_dist")?,
            ),
            time_between_rocket_attack: read("time_between_rocket_attack").map(value::int)? as u32,
            syncronize_rocket: boolean("syncronize_rocket")?,
            fire_trail_length: float("fire_trail_length")?,
            use_fire_trail,
            fire_dispersion: Self::dispersion_of(dispersion, use_fire_trail),
            barrel_dir_tolerance: float("barrel_dir_tolerance")?,
            engine_sound_volume: 1.0,
            flame_started: false,
            light_started: false,
            exploded: false,
            dead: false,
            processing: true,
            smoke_particle: text("smoke_particle"),
            broken_sound: text("broken_snd"),
            dispersion,
        })
    }

    fn dispersion_of((null, base): (f32, f32), on: bool) -> f32 {
        (if on { null } else { base }) * DEGREES
    }
}

/// A `holder`'s part: the object it is the holder part of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HolderRef {
    pub object: u16,
}

type Results = Result<Vec<Value>, String>;

/// What a member of these classes gets: the engine objects, the physics
/// state, its object's id, the arguments.
struct VCall<'a> {
    ctx: &'a Context,
    objects: &'a mut GameObjects,
    p: &'a mut PhysicsState,
    system: &'a Option<Rc<Config>>,
    id: u16,
    args: &'a [Value],
    o: &'a Objects<'a>,
    name: &'static str,
}

impl VCall<'_> {
    fn arg(&self, i: usize) -> &Value {
        self.args.get(i).unwrap_or(&Value::Nil)
    }

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

    fn boolean(&self, i: usize) -> Result<bool, String> {
        match self.arg(i) {
            Value::Boolean(b) => Ok(*b),
            _ => Err(format!("{}: boolean expected", self.name)),
        }
    }

    fn vector(&self, i: usize) -> Result<Vec3, String> {
        physics::other(self.o.get::<Vector>(self.arg(i)))
            .map(|v| [v.x, v.y, v.z])
            .ok_or_else(|| format!("{}: vector expected", self.name))
    }

    /// The object of handle argument `i`; `None` for nil.
    fn handle(&self, i: usize) -> Result<Option<u16>, String> {
        match self.arg(i) {
            Value::Nil => Ok(None),
            v => physics::other(self.o.get::<Handle>(v))
                .map(|h| Some(h.id))
                .ok_or_else(|| format!("{}: game object expected", self.name)),
        }
    }

    fn new_vector(&self, v: Vec3) -> Value {
        self.o.new_object("vector", Vector::new(v[0], v[1], v[2]))
    }

    fn crash(&self, what: &str) -> String {
        self.ctx.fatal(
            self.name,
            line!(),
            "<no expression>",
            &format!("{}: {what}", self.name),
        )
    }

    fn position(&self, id: u16) -> Vec3 {
        let p = self.objects.get(id).map_or([0.0; 4], |o| o.placement[3]);
        [p[0], p[1], p[2]]
    }

    /// An object's centre (spec M's `center`): its visual's sphere centre,
    /// its position without a visual (our choice).
    fn centre(&self, id: u16) -> Vec3 {
        let Some(o) = self.objects.get(id) else {
            return [0.0; 3];
        };
        match &o.visual {
            Some(v) => volya_engine_formats::model::transform_point(&o.placement, v.centre),
            None => self.position(id),
        }
    }

    fn car(&mut self) -> &mut Car {
        self.p.cars.entry(self.id).or_default()
    }

    /// The helicopter, made from its section on first need.
    fn helicopter(&mut self) -> Result<&mut Helicopter, String> {
        if !self.p.helicopters.contains_key(&self.id) {
            let section = (self.objects.get(self.id))
                .and_then(|o| o.section.clone())
                .unwrap_or_default();
            let section = physics::decode(&section);
            let Some(system) = self.system.as_ref() else {
                return Err(self.crash("no settings"));
            };
            let made = Helicopter::from_section(system, &section, self.position(self.id));
            let h = made.map_err(|e| self.crash(&e))?;
            self.p.helicopters.insert(self.id, h);
        }
        Ok(self.p.helicopters.get_mut(&self.id).expect("made above"))
    }

    fn health(&mut self) -> Option<&mut f32> {
        let id = self.id;
        self.objects
            .get_mut(id)
            .and_then(|o| o.entity.as_mut())
            .map(|e| &mut e.health)
    }

    fn log(&self, line: &str) {
        self.ctx.services.borrow_mut().console.log_mut().write(line);
    }
}

fn boolean(b: bool) -> Vec<Value> {
    vec![Value::Boolean(b)]
}

fn number(n: impl Into<f64>) -> Vec<Value> {
    vec![Value::Number(n.into())]
}

/// A member of a class whose part is `T`, its object's id `id(part)`.
fn on<T: std::any::Any>(
    class: EngineClass,
    ctx: &Context,
    name: &'static str,
    id: fn(&T) -> Option<u16>,
    f: impl Fn(&mut VCall) -> Results + 'static,
) -> EngineClass {
    let ctx = ctx.clone();
    class.method_with_objects(name, move |part: &mut T, o, args| {
        ctx.stopped()?;
        // Before the object spawns there is no state (our wording).
        let Some(object) = id(part) else {
            return Err(format!("{name}: the object hasn't spawned"));
        };
        let mut objects = ctx.objects.borrow_mut();
        let mut p = ctx.physics.borrow_mut();
        let system = p.system.clone();
        let mut call = VCall {
            ctx: &ctx,
            objects: &mut objects,
            p: &mut p,
            system: &system,
            id: object,
            args,
            o,
            name,
        };
        f(&mut call)
    })
}

fn client_id(c: &ClientObject) -> Option<u16> {
    c.object
}

fn holder_id(h: &HolderRef) -> Option<u16> {
    Some(h.object)
}

fn on_client(
    class: EngineClass,
    ctx: &Context,
    name: &'static str,
    f: impl Fn(&mut VCall) -> Results + 'static,
) -> EngineClass {
    on::<ClientObject>(class, ctx, name, client_id, f)
}

fn is_car(c: &VCall) -> bool {
    c.objects
        .get(c.id)
        .is_some_and(|o| o.class == super::ObjectClass::Named(physics::CAR))
}

/// `engaged()` (6.1).
fn engaged(c: &mut VCall) -> Results {
    let engaged = is_car(c) && c.car().engaged;
    Ok(boolean(engaged))
}

/// `Action(id, value)` (6.1, 6.2): the car's; nothing for the other
/// holders here (scripts can't name their classes).
fn action(c: &mut VCall) -> Results {
    let (id, value) = (c.int(0)?, c.int(1)?);
    if !is_car(c) {
        return Ok(Vec::new());
    }
    let Some(w) = &mut c.car().weapon else {
        return Ok(Vec::new());
    };
    let on = value == 1;
    match id {
        3 => {
            w.firing = on;
            if !on {
                w.activated = false;
            }
        }
        4 => w.activated = on,
        5 => w.auto_fire = on,
        6 => w.desired_dir = w.default_dir,
        _ => {}
    }
    Ok(Vec::new())
}

/// `SetParam(id, v)` (6.1, 6.2).
fn set_param(c: &mut VCall) -> Results {
    let (id, point) = (c.int(0)?, c.vector(1)?);
    if !is_car(c) || id != 2 {
        return Ok(Vec::new());
    }
    let Some(w) = &mut c.car().weapon else {
        return Ok(Vec::new());
    };
    let d: Vec3 = std::array::from_fn(|k| point[k] - w.fire_point[k]);
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    // "Negligible": below the float's epsilon (our choice).
    w.desired_dir = if length > f32::EPSILON {
        d.map(|v| v / length)
    } else {
        d
    };
    Ok(Vec::new())
}

/// `holder` (6).
fn holder_class(ctx: &Context) -> EngineClass {
    let class = EngineClass::new("holder");
    let class = on::<HolderRef>(class, ctx, "engaged", holder_id, engaged);
    let class = on::<HolderRef>(class, ctx, "Action", holder_id, action);
    on::<HolderRef>(class, ctx, "SetParam", holder_id, set_param)
}

/// The angle between two directions, degrees.
fn degrees_between(a: Vec3, b: Vec3) -> f32 {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let la = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let lb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    if la == 0.0 || lb == 0.0 {
        return 0.0;
    }
    (dot / (la * lb)).clamp(-1.0, 1.0).acos().to_degrees()
}

fn health_members(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = on_client(class, ctx, "GetfHealth", |c| {
        Ok(number(c.health().map_or(0.0, |h| *h)))
    });
    on_client(class, ctx, "SetfHealth", |c| {
        let h = c.number(0)?;
        if let Some(health) = c.health() {
            *health = h;
        }
        Ok(Vec::new())
    })
}

/// `CCar` (7).
fn car_class(ctx: &Context) -> EngineClass {
    let class = client_class(physics::CAR, CLIENT_BASE);
    let class = on_client(class, ctx, "engaged", engaged);
    let class = on_client(class, ctx, "Action", action);
    let class = on_client(class, ctx, "SetParam", set_param);
    let class = on_client(class, ctx, "HasWeapon", |c| {
        Ok(boolean(c.car().weapon.is_some()))
    });
    let class = on_client(class, ctx, "CanHit", |c| {
        Ok(boolean(c.car().weapon.as_ref().is_some_and(|w| w.can_hit)))
    });
    let class = on_client(class, ctx, "FireDirDiff", |c| {
        let diff = (c.car().weapon.as_ref())
            .map_or(0.0, |w| degrees_between(w.current_dir, w.desired_dir));
        Ok(number(diff))
    });
    let class = on_client(class, ctx, "IsObjectVisible", |c| {
        // The further part tested first is unestablished: not covered.
        let Some(target) = c.handle(0)? else {
            // Our wording of the log message.
            c.log("! CCar::IsObjectVisible: null argument");
            return Ok(boolean(false));
        };
        let mut from = c.centre(c.id);
        if let Some(w) = &c.car().weapon {
            from[1] = w.fire_point[1];
        }
        let to = c.centre(target);
        let blocked = (c.p.level.as_ref()).is_some_and(|l| l.ray_blocked(from, to));
        Ok(boolean(!blocked))
    });
    let class = on_client(class, ctx, "CurrentVel", |c| {
        let id = c.id;
        let first = c.p.shells.get(&id).filter(|s| !s.elements.is_empty());
        // Without a shell believed an invalid read: fatal here.
        if first.is_none() {
            return Err(c.crash("the car has no shell"));
        }
        let body = c.p.world.as_ref().and_then(|w| w.body(id, 0));
        let v = body
            .filter(|b| b.switched_on)
            .map_or([0.0; 3], |b| b.linear);
        Ok(vec![c.new_vector(v)])
    });
    let class = health_members(class, ctx);
    let class = on_client(class, ctx, "SetExplodeTime", |c| {
        let t = to_bits32(f64::from(c.number(0)?)) as u32;
        let car = c.car();
        if !car.timer_running {
            let seconds = 0.001 * t as f32;
            car.explode_time = if seconds < 1e-7 { 0.0 } else { seconds };
            car.timer_set = true;
        }
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "ExplodeTime", |c| {
        let now = c.ctx.services.borrow().device.time_global() as f32 / 1000.0;
        let car = c.car();
        if !car.timer_set {
            return Ok(number(0));
        }
        let left = if car.timer_running {
            car.explode_time - now
        } else {
            car.explode_time
        };
        Ok(number(left.round() * 1000.0))
    });
    let class = on_client(class, ctx, "CarExplode", |c| {
        let id = c.id;
        let position = c.position(id);
        let car = c.car();
        if car.exploded {
            return Ok(Vec::new());
        }
        car.exploded = true;
        if let Some(w) = &mut car.weapon {
            w.firing = false;
        }
        physics::explode(c.p, id, position, Some([0.0, 1.0, 0.0]));
        c.p.push(PhysicsEvent::CarExploded { object: id });
        Ok(Vec::new())
    });
    CAR_CONSTANTS
        .into_iter()
        .fold(class, |class, (name, value)| class.constant(name, value))
}

/// A property setter of a `ClientObject` part.
type PropertySetter = Box<dyn Fn(&mut ClientObject, &Objects, &Value) -> Result<(), String>>;

/// A property of the helicopter's values.
fn heli_property(
    class: EngineClass,
    ctx: &Context,
    name: &'static str,
    get: fn(&Helicopter) -> Value,
    set: Option<fn(&mut Helicopter, &Value)>,
) -> EngineClass {
    let with = move |ctx: &Context,
                     c: &ClientObject,
                     o: &Objects,
                     f: &mut dyn FnMut(&mut VCall) -> Result<(), String>| {
        ctx.stopped()?;
        let Some(object) = c.object else {
            return Err(format!("{name}: the object hasn't spawned"));
        };
        let mut objects = ctx.objects.borrow_mut();
        let mut p = ctx.physics.borrow_mut();
        let system = p.system.clone();
        let mut call = VCall {
            ctx,
            objects: &mut objects,
            p: &mut p,
            system: &system,
            id: object,
            args: &[],
            o,
            name,
        };
        f(&mut call)
    };
    let getter = ctx.clone();
    let setter = ctx.clone();
    class.property_with_objects(
        name,
        move |c: &ClientObject, o| {
            let mut out = Value::Nil;
            with(&getter, c, o, &mut |v| {
                out = get(v.helicopter()?);
                Ok(())
            })?;
            Ok(out)
        },
        set.map(|set| -> PropertySetter {
            Box::new(move |c: &mut ClientObject, o, value| {
                with(&setter, c, o, &mut |v| {
                    set(v.helicopter()?, value);
                    Ok(())
                })
            })
        }),
    )
}

fn flag(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}

fn float(v: &Value) -> f32 {
    to_f32(crate::values::lua_number(v))
}

/// The direction of heading `h` and pitch `p` (as `vector:setHP`, spec E:
/// our reading).
fn direction(h: f32, p: f32) -> Vec3 {
    [-(h.sin() * p.cos()), p.sin(), h.cos() * p.cos()]
}

/// `CHelicopter` (8).
fn helicopter_class(ctx: &Context) -> EngineClass {
    let class = client_class(physics::HELICOPTER, CLIENT_BASE);
    let state = |name: &'static str, get: fn(&Helicopter) -> i32| {
        move |c: &mut VCall| -> Results {
            let _ = name;
            Ok(number(get(c.helicopter()?)))
        }
    };
    let class = on_client(class, ctx, "GetState", state("GetState", |h| h.state));
    let class = on_client(
        class,
        ctx,
        "GetMovementState",
        state("GetMovementState", |h| h.movement),
    );
    let class = on_client(
        class,
        ctx,
        "GetHuntState",
        state("GetHuntState", |h| h.hunt),
    );
    let class = on_client(
        class,
        ctx,
        "GetBodyState",
        state("GetBodyState", |h| h.body),
    );
    // 8.2.
    let class = on_client(class, ctx, "SetDestPosition", |c| {
        let p = c.vector(0)?;
        let h = c.helicopter()?;
        h.destination = p;
        h.movement = MOV_TO_POINT;
        h.round_path = None;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "GoPatrolByPatrolPath", |c| {
        let name = match c.arg(0) {
            Value::String(s) => s.clone(),
            _ => return Err("GoPatrolByPatrolPath: text expected".into()),
        };
        let from = c.int(1)? as u32;
        let point = (c.p.level.as_ref()).and_then(|l| l.patrol_point(&name, from));
        // An unknown path: believed an invalid read, fatal here.
        let Some(point) = point else {
            let what = format!("no patrol path {} point {from}", physics::decode(&name));
            return Err(c.crash(&what));
        };
        let h = c.helicopter()?;
        h.destination = point;
        h.movement = MOV_PATROL_PATH;
        h.patrol_path = Some((name, from));
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "GoPatrolByRoundPath", |c| {
        let (centre, radius, clockwise) = (c.vector(0)?, c.number(1)?, c.boolean(2)?);
        let h = c.helicopter()?;
        // TODO(#1057): the minimum radius (from the speed and the
        // section's turning speeds, how not stated) and its log message,
        // the ring's points `heli_round_path_pt_%d` and the one it starts
        // from are the helicopter's flight's.
        let clockwise = if h.movement == MOV_ROUND_PATH {
            !clockwise
        } else {
            clockwise
        };
        h.round_path = Some(RoundPath {
            centre,
            radius,
            clockwise,
        });
        h.movement = MOV_ROUND_PATH;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "GetDistanceToDestPosition", |c| {
        let h = c.helicopter()?;
        let d: Vec3 = std::array::from_fn(|k| h.destination[k] - h.position[k]);
        Ok(number((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()))
    });
    let class = on_client(class, ctx, "SetMaxVelocity", |c| {
        let v = c.number(0)?;
        c.helicopter()?.max_velocity = v;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "GetMaxVelocity", |c| {
        Ok(number(c.helicopter()?.max_velocity))
    });
    let class = on_client(class, ctx, "GetCurrVelocity", |c| {
        Ok(number(c.helicopter()?.curr_velocity))
    });
    let class = on_client(class, ctx, "GetCurrVelocityVec", |c| {
        let h = c.helicopter()?;
        let d = direction(h.heading, h.pitch);
        Ok(vec![c.new_vector(d)])
    });
    let class = on_client(class, ctx, "SetSpeedInDestPoint", |c| {
        let v = c.number(0)?;
        c.helicopter()?.speed_in_dest = v;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "GetSpeedInDestPoint", |c| {
        // 0 while following a path in a state the code tests
        // (unestablished which): never, here.
        Ok(number(c.helicopter()?.speed_in_dest))
    });
    let class = on_client(class, ctx, "SetOnPointRangeDist", |c| {
        let d = c.number(0)?;
        c.helicopter()?.on_point_range = d;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "GetOnPointRangeDist", |c| {
        Ok(number(c.helicopter()?.on_point_range))
    });
    let class = on_client(class, ctx, "SetLinearAcc", |c| {
        let (fw, bk) = (c.number(0)?, c.number(1)?);
        c.helicopter()?.linear_acc = (fw, bk);
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "GetRealAltitude", |c| {
        let from = c.helicopter()?.position;
        // Nothing hit is unestablished: the ray's length, here.
        let d = (c.p.level.as_ref())
            .and_then(|l| l.distance_below(from, ALTITUDE_RANGE))
            .unwrap_or(ALTITUDE_RANGE);
        Ok(number(d))
    });
    let class = on_client(class, ctx, "GetSafeAltitude", |c| {
        let top = c.p.level.as_ref().map_or(0.0, |l| l.level_box_top());
        Ok(number(top + c.helicopter()?.safe_altitude))
    });
    // 8.3.
    let class = on_client(class, ctx, "SetEnemy", |c| {
        let enemy = match c.handle(0) {
            Ok(Some(id)) => Enemy::Entity(id),
            _ => Enemy::Point(c.vector(0)?),
        };
        let h = c.helicopter()?;
        h.hunt = match enemy {
            Enemy::Entity(_) => ENEMY_ENTITY,
            _ => ENEMY_POINT,
        };
        h.enemy = enemy;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "ClearEnemy", |c| {
        let h = c.helicopter()?;
        h.hunt = ENEMY_NONE;
        h.enemy = Enemy::None;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "LookAtPoint", |c| {
        let (p, on) = (c.vector(0)?, c.boolean(1)?);
        let h = c.helicopter()?;
        h.look_at = (p, on);
        h.body = if on { BODY_TO_POINT } else { BODY_BY_PATH };
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "isVisible", |c| {
        let target = c.handle(0)?;
        let from = c.helicopter()?.position;
        let to = target.map_or(from, |t| c.centre(t));
        let blocked = (c.p.level.as_ref()).is_some_and(|l| l.ray_blocked(from, to));
        Ok(boolean(!blocked))
    });
    // 8.4.
    let class = on_client(class, ctx, "SetFireTrailLength", |c| {
        let l = c.number(0)?;
        c.helicopter()?.fire_trail_length = l;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "UseFireTrail", |c| {
        if c.args.is_empty() {
            return Ok(boolean(c.helicopter()?.use_fire_trail));
        }
        let on = c.boolean(0)?;
        let h = c.helicopter()?;
        h.use_fire_trail = on;
        h.fire_dispersion = Helicopter::dispersion_of(h.dispersion, on);
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "SetBarrelDirTolerance", |c| {
        let t = c.number(0)?;
        c.helicopter()?.barrel_dir_tolerance = t;
        Ok(Vec::new())
    });
    // 8.5.
    let class = health_members(class, ctx);
    let class = on_client(class, ctx, "TurnLighting", |c| {
        let on = c.boolean(0)?;
        c.helicopter()?.light_started = on;
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "TurnEngineSound", |c| {
        let on = c.boolean(0)?;
        c.helicopter()?.engine_sound_volume = if on { 1.0 } else { 0.0 };
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "StartFlame", |c| {
        let id = c.id;
        let h = c.helicopter()?;
        if h.flame_started {
            return Ok(Vec::new());
        }
        h.flame_started = true;
        let particle = h.smoke_particle.clone();
        c.p.push(PhysicsEvent::Flame {
            object: id,
            particle,
        });
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "Die", |c| {
        let id = c.id;
        let h = c.helicopter()?;
        if h.state == DEAD {
            return Ok(Vec::new());
        }
        h.state = DEAD;
        h.dead = true;
        h.processing = false;
        let event = PhysicsEvent::HelicopterDied {
            object: id,
            sound: h.broken_sound.clone(),
            position: h.position,
        };
        c.p.push(event);
        Ok(Vec::new())
    });
    let class = on_client(class, ctx, "Explode", |c| {
        let id = c.id;
        let h = c.helicopter()?;
        h.exploded = true;
        let position = h.position;
        c.p.push(PhysicsEvent::HelicopterExploded { object: id });
        physics::explode(c.p, id, position, None);
        Ok(Vec::new())
    });
    // 8.4's properties.
    let class = heli_property(
        class,
        ctx,
        "m_use_rocket_on_attack",
        |h| Value::Boolean(h.use_rocket),
        Some(|h, v| h.use_rocket = flag(v)),
    );
    let class = heli_property(
        class,
        ctx,
        "m_use_mgun_on_attack",
        |h| Value::Boolean(h.use_mgun),
        Some(|h, v| h.use_mgun = flag(v)),
    );
    let class = heli_property(
        class,
        ctx,
        "m_min_rocket_dist",
        |h| Value::Number(f64::from(h.rocket_dist.0)),
        Some(|h, v| h.rocket_dist.0 = float(v)),
    );
    let class = heli_property(
        class,
        ctx,
        "m_max_rocket_dist",
        |h| Value::Number(f64::from(h.rocket_dist.1)),
        Some(|h, v| h.rocket_dist.1 = float(v)),
    );
    let class = heli_property(
        class,
        ctx,
        "m_min_mgun_dist",
        |h| Value::Number(f64::from(h.mgun_dist.0)),
        Some(|h, v| h.mgun_dist.0 = float(v)),
    );
    let class = heli_property(
        class,
        ctx,
        "m_max_mgun_dist",
        |h| Value::Number(f64::from(h.mgun_dist.1)),
        Some(|h, v| h.mgun_dist.1 = float(v)),
    );
    let class = heli_property(
        class,
        ctx,
        "m_time_between_rocket_attack",
        |h| Value::Number(f64::from(h.time_between_rocket_attack)),
        Some(|h, v| h.time_between_rocket_attack = to_bits32(crate::values::lua_number(v)) as u32),
    );
    let class = heli_property(
        class,
        ctx,
        "m_syncronize_rocket",
        |h| Value::Boolean(h.syncronize_rocket),
        Some(|h, v| h.syncronize_rocket = flag(v)),
    );
    let class = heli_property(
        class,
        ctx,
        "m_flame_started",
        |h| Value::Boolean(h.flame_started),
        None,
    );
    let class = heli_property(
        class,
        ctx,
        "m_light_started",
        |h| Value::Boolean(h.light_started),
        None,
    );
    let class = heli_property(
        class,
        ctx,
        "m_exploded",
        |h| Value::Boolean(h.exploded),
        None,
    );
    let class = heli_property(class, ctx, "m_dead", |h| Value::Boolean(h.dead), None);
    HELICOPTER_CONSTANTS
        .into_iter()
        .fold(class, |class, (name, value)| class.constant(name, value))
}

/// `hanging_lamp` (9): its switching, `lamps` 5.1, which the engine applies
/// ([`PhysicsEvent::LampSwitched`]).
fn lamp_class(ctx: &Context) -> EngineClass {
    let class = client_class(physics::HANGING_LAMP, CLIENT_BASE);
    let class = on_client(class, ctx, "turn_on", |c| {
        let id = c.id;
        c.p.push(PhysicsEvent::LampSwitched {
            object: id,
            on: true,
        });
        Ok(Vec::new())
    });
    on_client(class, ctx, "turn_off", |c| {
        let id = c.id;
        c.p.push(PhysicsEvent::LampSwitched {
            object: id,
            on: false,
        });
        Ok(Vec::new())
    })
}

/// The classes of 6 – 9, in registration order.
pub(super) fn classes(ctx: &Context) -> Vec<EngineClass> {
    vec![
        holder_class(ctx),
        car_class(ctx),
        helicopter_class(ctx),
        lamp_class(ctx),
    ]
}
