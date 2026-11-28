// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level as scripts reach it (script API N,
//! docs/behavior/script-api-level.md; section numbers are its): the
//! namespace `level`'s functions for the level's presence and name, objects
//! by id, level vertices ([`vertices`]), the level's bounding volume, the
//! physics world, phantoms and the client spawn manager (2, 3); the class
//! `patrol` over the game's patrol paths (4); the classes `cover_point` and
//! `object_params` (5); and the class `client_spawn_manager` with the
//! engine's calls of the functions handed to its `add` (6).
//!
//! What the engine gives is a [`LevelState`]: the level being played while
//! it exists ([`Level`]) and the game's store of patrol paths. The
//! namespace's functions are set on `level` (a plain table other specs add
//! to) by the Lua side (`level.lua`) over one hidden Rust function; the
//! spawn manager's table holds script functions, so it is the Lua side's
//! too. See docs/architecture/scripts.md, The level.

pub mod vertices;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use volya_engine_formats::cp1251;
use volya_engine_formats::level_ai::{LevelAi, LevelAiLinks};
use volya_engine_formats::patrol_path::{PatrolPath, PatrolPaths};

use crate::classes::{EngineClass, EnginePart, NOT_LOGGED, ObjectMaker, Objects, Operand};
use crate::files::FileSystem;
use crate::game_object::GameObjects;
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::{Fbox, Flags32, Vector};
use crate::{Error, ErrorKind, Value};

/// The registry key of the Lua side's entry function.
const ENTRY: &str = "volya.level";

/// The section `spawn_phantom` asks for (2.6).
pub const PHANTOM_SECTION: &str = "m_phantom";

/// The level's vertices (3): their positions, links and cover values, as
/// the level's `level.ai` holds them.
#[derive(Clone)]
pub struct LevelVertices {
    pub ai: Arc<LevelAi>,
    pub links: Arc<LevelAiLinks>,
}

/// The level being played (conventions), as the functions read it.
#[derive(Clone)]
pub struct Level {
    /// The name the level keeps (2.2): its directory under `$game_levels$`.
    pub name: Option<Vec<u8>>,
    /// The bounding box of its `level.cform` header (2.4): min, max.
    pub bounding_volume: ([f32; 3], [f32; 3]),
    /// Its level vertices; `None` for a level without them (our choice:
    /// the functions of 3 then read through a missing object).
    pub vertices: Option<LevelVertices>,
}

/// What the functions ask of the engine, in order
/// ([`LevelState::take_events`]).
#[derive(Clone, Debug, PartialEq)]
pub enum LevelEvent {
    /// `spawn_phantom` (2.6): a spawn request of a new object of section
    /// [`PHANTOM_SECTION`] at `position`, with no level vertex and no
    /// parent, for the game server.
    SpawnPhantom { position: [f32; 3] },
}

/// The level and the game's patrol paths as scripts see them.
#[derive(Default)]
pub struct LevelState {
    /// The level, while it exists.
    pub level: Option<Level>,
    /// The game's store of patrol paths (4.1); `None` is an empty store.
    pub patrol_paths: Option<Rc<PatrolPaths>>,
    /// The level's client spawn manager (2.7, 6), made with the level.
    spawn_manager: Option<EnginePart>,
    events: Vec<LevelEvent>,
}

impl LevelState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<LevelEvent> {
        std::mem::take(&mut self.events)
    }
}

/// The client spawn manager's engine part: the table is the Lua side's.
struct SpawnManager;

/// A cover point (5.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverPoint {
    pub position: [f32; 3],
    pub level_vertex: u32,
}

/// An object parameter record (5.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectParams {
    pub level_vertex: u32,
    pub position: [f32; 3],
}

/// A patrol object (4.2): the path looked up when it was made, and the
/// four values stored as given, for the members receiving a patrol object
/// (specs V and X).
#[allow(dead_code)]
struct Patrol {
    /// The store and the path's name, when the store had it.
    path: Option<(Rc<PatrolPaths>, String)>,
    start: i32,
    route: i32,
    random: bool,
    index: i32,
}

impl Patrol {
    fn path(&self) -> Option<&PatrolPath> {
        let (paths, name) = self.path.as_ref()?;
        paths.get(name)
    }
}

/// What the functions share.
#[derive(Clone)]
struct Context {
    state: Rc<RefCell<LevelState>>,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
    /// The file system, whose system configuration holds the sections.
    files: Rc<RefCell<FileSystem>>,
    fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error standing for a crash the spec believes (our wording).
    fn crash(&self, function: &str, what: &str) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line: line!(),
                expression: "fatal error",
                description: "<no expression>",
                arguments: &[&format!("{function}: {what}")],
            },
        )
    }
}

/// What a state with the level registered keeps for the engine's calls.
pub(crate) struct Registered {
    ctx: Context,
}

type Results = Result<Vec<Value>, String>;

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A number argument (our wording for a mismatch; depends on A).
fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// A text argument (our wording for a mismatch; depends on A).
fn text(args: &[Value], i: usize, what: &str) -> Result<Vec<u8>, String> {
    match arg(args, i) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{what}: text expected")),
    }
}

fn vector_of(maker: &ObjectMaker, args: &[Value], i: usize, what: &str) -> Result<Vector, String> {
    maker
        .get::<Vector>(arg(args, i))
        .ok_or_else(|| format!("{what}: vector expected"))
}

/// A number as an integer, truncated toward zero (our reading where A
/// leaves the conversion; saturating beyond the range).
fn integer(n: f64) -> i64 {
    n.trunc() as i64
}

fn one(v: impl Into<Value>) -> Results {
    Ok(vec![v.into()])
}

fn new_vector(maker: &ObjectMaker, v: [f32; 3]) -> Value {
    maker.new_object("vector", Vector::new(v[0], v[1], v[2]))
}

/// The low 16 bits of an id argument (6.1).
fn id16(n: f64) -> u16 {
    integer(n) as u16
}

/// The functions of the namespace (1.2), the Lua side's in the order it
/// sets them.
pub const FUNCTIONS: [&str; 11] = [
    "name",
    "present",
    "object_by_id",
    "vertex_position",
    "vertex_in_direction",
    "cover_in_direction",
    "patrol_path_exists",
    "get_bounding_volume",
    "physics_world",
    "spawn_phantom",
    "client_spawn_manager",
];

/// The engine part of the Lua side's functions and its own operations:
/// `args[0]` names it.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("level: operation expected".into()),
    };
    let args = args.get(1..).unwrap_or(&[]);
    if op == "fatal" {
        // A script function the spawn manager called failed (6.4; D 1.2):
        // the message logged, the state stopped.
        let message = String::from_utf8_lossy(&text(args, 0, "fatal")?).into_owned();
        if message != NOT_LOGGED {
            ctx.services.borrow_mut().console.log_mut().write(&message);
        }
        let message = format!("LUA error: {message}");
        *ctx.fatal.borrow_mut() = Some(message.clone());
        return one(message.as_str());
    }
    ctx.stopped()?;
    let what = format!("level.{op}");
    let what = what.as_str();
    match op.as_str() {
        "id" => one(f64::from(id16(number(args, 0, "id")?))),
        "handle" => {
            let id = id16(number(args, 0, "handle")?);
            one(ctx.objects.borrow().handle_value(Some(id)))
        }
        // 2.1: the only function testing the level.
        "present" => one(ctx.state.borrow().level.is_some()),
        "name" => {
            let state = ctx.state.borrow();
            let level = level(ctx, &state, what)?;
            one(level.name.clone().map_or(Value::Nil, Value::String))
        }
        "object_by_id" => {
            let n = integer(number(args, 0, what)?);
            level(ctx, &ctx.state.borrow(), what)?;
            // The full number is compared: 65,536 or more is never found.
            let id = u16::try_from(n).ok();
            one(ctx.objects.borrow().handle_value(id))
        }
        "vertex_position" => {
            let vertex = integer(number(args, 0, what)?);
            let state = ctx.state.borrow();
            let v = vertices_of(ctx, &state, what)?;
            let p = vertex_index(v, vertex).and_then(|k| v.ai.position(k));
            let p = p.ok_or_else(|| ctx.crash(what, "a level vertex beyond the level"))?;
            one(new_vector(maker, p))
        }
        "vertex_in_direction" => {
            let vertex = integer(number(args, 0, what)?);
            let direction = vector_of(maker, args, 1, what)?;
            let distance = number(args, 2, what)? as f32;
            let state = ctx.state.borrow();
            let v = vertices_of(ctx, &state, what)?;
            let found = vertex_index(v, vertex).and_then(|k| {
                let d = [direction.x, direction.y, direction.z];
                vertices::vertex_in_direction(&v.ai, &v.links, k, d, distance)
            });
            let found = found.ok_or_else(|| ctx.crash(what, "a level vertex beyond the level"))?;
            one(f64::from(found))
        }
        "cover_in_direction" => {
            let vertex = integer(number(args, 0, what)?);
            let direction = vector_of(maker, args, 1, what)?;
            let state = ctx.state.borrow();
            let v = vertices_of(ctx, &state, what)?;
            let cover = vertex_index(v, vertex).and_then(|k| v.links.cover(k));
            let cover = cover.ok_or_else(|| ctx.crash(what, "a level vertex beyond the level"))?;
            let d = [direction.x, direction.y, direction.z];
            one(f64::from(vertices::cover_in_direction(cover, d)))
        }
        "patrol_path_exists" => {
            let name = cp1251::decode(&text(args, 0, what)?);
            let state = ctx.state.borrow();
            let paths = state.patrol_paths.as_ref();
            one(paths.is_some_and(|p| p.get(&name).is_some()))
        }
        "get_bounding_volume" => {
            let state = ctx.state.borrow();
            let (min, max) = level(ctx, &state, what)?.bounding_volume;
            let fbox = Fbox {
                min: Vector::new(min[0], min[1], min[2]),
                max: Vector::new(max[0], max[1], max[2]),
            };
            one(maker.new_object("Fbox", fbox))
        }
        // 2.5: the class is never registered for scripts (believed: a
        // crash; our wording).
        "physics_world" => Err(ctx.crash(what, "the physics world's class isn't registered")),
        "spawn_phantom" => {
            let position = vector_of(maker, args, 0, what)?;
            level(ctx, &ctx.state.borrow(), what)?;
            let system = Rc::clone(&ctx.files.borrow().system);
            if system.section(PHANTOM_SECTION).is_none() {
                return Err(report_fatal(
                    &ctx.services,
                    &ctx.fatal,
                    Fatal {
                        function: what,
                        file: file!(),
                        line: line!(),
                        expression: "assertion failed",
                        description: "Cannot find item with section",
                        arguments: &[PHANTOM_SECTION],
                    },
                ));
            }
            let position = [position.x, position.y, position.z];
            let event = LevelEvent::SpawnPhantom { position };
            ctx.state.borrow_mut().events.push(event);
            Ok(Vec::new())
        }
        "client_spawn_manager" => {
            let state = ctx.state.borrow();
            level(ctx, &state, what)?;
            let manager = state.spawn_manager.map(Value::Object);
            one(manager.ok_or_else(|| ctx.crash(what, "the level has no spawn manager"))?)
        }
        _ => Err(format!("level: unknown operation {op}")),
    }
}

/// The level, or the believed crash of a function called without one (2).
fn level<'a>(ctx: &Context, state: &'a LevelState, what: &str) -> Result<&'a Level, String> {
    state
        .level
        .as_ref()
        .ok_or_else(|| ctx.crash(what, "there is no level"))
}

fn vertices_of<'a>(
    ctx: &Context,
    state: &'a LevelState,
    what: &str,
) -> Result<&'a LevelVertices, String> {
    level(ctx, state, what)?
        .vertices
        .as_ref()
        .ok_or_else(|| ctx.crash(what, "the level has no level vertices"))
}

/// `vertex` when it is a vertex of the level (below *N*): beyond it is
/// believed a read past the records (3.2), a fatal error here.
fn vertex_index(v: &LevelVertices, vertex: i64) -> Option<u32> {
    u32::try_from(vertex)
        .ok()
        .filter(|&k| (k as usize) < v.ai.len().min(v.links.len()))
}

/// The start types then the route types (4.3), a later name replacing an
/// earlier: what scripts read.
pub const PATROL_CONSTANTS: [(&str, i32); 7] = [
    ("start", 0),
    ("stop", 0),
    ("nearest", 2),
    ("custom", 3),
    ("next", 4),
    ("dummy", -1),
    ("continue", 1),
];

/// `patrol`'s constructor defaults (4.2): start, route, random, index.
const PATROL_DEFAULTS: (i32, i32, bool, i32) = (2, 1, true, -1);

/// The truth of a value in Lua's sense.
fn truth(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}

fn patrol_class(ctx: &Context) -> EngineClass {
    let mut class = EngineClass::new("patrol").constructible();
    for params in 1..=5 {
        let c = ctx.clone();
        class = class.constructor(
            params,
            |args: &[Value]| {
                let ok = args.iter().enumerate().all(|(i, a)| match i {
                    0 => matches!(a, Value::String(_)),
                    3 => true,
                    _ => matches!(a, Value::Number(_)),
                });
                ok.then_some(0)
            },
            move |_, args| {
                let (start, route, random, index) = PATROL_DEFAULTS;
                let int = |i: usize, default: i32| match args.get(i) {
                    Some(Value::Number(n)) => integer(*n) as i32,
                    _ => default,
                };
                let name = cp1251::decode(&text(args, 0, "patrol")?);
                // A name the store lacks is kept as no path, with no
                // message.
                let paths = c.state.borrow().patrol_paths.clone();
                let path = paths.filter(|p| p.get(&name).is_some()).map(|p| (p, name));
                Ok(Patrol {
                    path,
                    start: int(1, start),
                    route: int(2, route),
                    random: args.get(3).map_or(random, truth),
                    index: int(4, index),
                })
            },
        );
    }
    let class = patrol_members(class, ctx);
    PATROL_CONSTANTS
        .into_iter()
        .fold(class, |class, (name, value)| class.constant(name, value))
}

/// The point index argument `i` of a member.
fn point_index(args: &[Value], i: usize, what: &str) -> Result<i64, String> {
    number(args, i, what).map(integer)
}

/// Adds patrol member `name` running `f` on the kept path, the believed
/// crash on a patrol object with no path (4.4).
fn patrol_member(
    class: EngineClass,
    ctx: &Context,
    name: &'static str,
    f: impl Fn(&Context, &PatrolPath, &Objects, &[Value]) -> Results + 'static,
) -> EngineClass {
    let c = ctx.clone();
    let what = format!("patrol:{name}");
    class.method_with_objects(name, move |p: &mut Patrol, o, args| {
        c.stopped()?;
        let path = p
            .path()
            .ok_or_else(|| c.crash(&what, "the patrol has no path"))?;
        f(&c, path, o, args)
    })
}

/// A point of `path` by index argument 0; the believed crash for an index
/// the path lacks (4.4).
fn point<'a>(
    ctx: &Context,
    path: &'a PatrolPath,
    args: &[Value],
    what: &str,
) -> Result<&'a volya_engine_formats::patrol_path::PathPoint, String> {
    let i = point_index(args, 0, what)?;
    usize::try_from(i)
        .ok()
        .and_then(|i| path.points.get(i))
        .ok_or_else(|| ctx.crash(what, "a point the path lacks"))
}

fn patrol_members(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = patrol_member(class, ctx, "count", |_, path, _, _| {
        one(path.points.len() as f64)
    });
    let class = patrol_member(class, ctx, "point", |c, path, o, args| {
        let what = "patrol:point";
        let i = point_index(args, 0, what)?;
        let found = usize::try_from(i).ok().and_then(|i| path.points.get(i));
        // An index the path lacks: the diagnostic text goes to the routine
        // that returns at once (M 2.3), and the lowest-indexed point's
        // position is given.
        let p = match found {
            Some(p) => p,
            None => (path.points.first()).ok_or_else(|| c.crash(what, "the path has no points"))?,
        };
        let v = p.position;
        Ok(vec![o.new_object("vector", Vector::new(v[0], v[1], v[2]))])
    });
    let class = patrol_member(class, ctx, "name", |c, path, _, args| {
        let p = point(c, path, args, "patrol:name")?;
        one(Value::String(
            cp1251::encode(&p.name).unwrap_or_else(|| p.name.as_bytes().to_vec()),
        ))
    });
    let class = patrol_member(class, ctx, "level_vertex_id", |c, path, _, args| {
        // The level of the point's game vertex is read and discarded (4.4).
        one(f64::from(
            point(c, path, args, "patrol:level_vertex_id")?.level_vertex,
        ))
    });
    let class = patrol_member(class, ctx, "game_vertex_id", |c, path, _, args| {
        one(f64::from(
            point(c, path, args, "patrol:game_vertex_id")?.game_vertex,
        ))
    });
    let class = patrol_member(class, ctx, "flags", |c, path, o, args| {
        let flags = point(c, path, args, "patrol:flags")?.flags;
        Ok(vec![o.new_object("flags32", Flags32(flags))])
    });
    let class = patrol_member(class, ctx, "flag", |c, path, _, args| {
        let what = "patrol:flag";
        let flags = point(c, path, args, what)?.flags;
        // n taken as 0 … 255, then bit n mod 32.
        let n = integer(number(args, 1, what)?) as u8;
        one(flags & (1 << (n % 32)) != 0)
    });
    let class = patrol_member(class, ctx, "terminal", |c, path, _, args| {
        let what = "patrol:terminal";
        point(c, path, args, what)?;
        let i = point_index(args, 0, what)?;
        one(!path.links.iter().any(|l| i64::from(l.from) == i))
    });
    let class = patrol_member(class, ctx, "index", |_, path, _, args| {
        let name = cp1251::decode(&text(args, 0, "patrol:index")?);
        let found = path.points.iter().position(|p| p.name == name);
        one(found.map_or(-1.0, |i| i as f64))
    });
    patrol_member(class, ctx, "get_nearest", |c, path, o, args| {
        let what = "patrol:get_nearest";
        let Some(Operand::Other(v)) = o.get::<Vector>(arg(args, 0)) else {
            return Err(format!("{what}: vector expected"));
        };
        let square = |p: [f32; 3]| {
            let d = [p[0] - v.x, p[1] - v.y, p[2] - v.z];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
        };
        // On a tie the lowest index.
        let mut nearest: Option<(usize, f32)> = None;
        for (i, p) in path.points.iter().enumerate() {
            let d = square(p.position);
            if nearest.is_none_or(|(_, best)| d < best) {
                nearest = Some((i, d));
            }
        }
        let (i, _) = nearest.ok_or_else(|| c.crash(what, "the path has no points"))?;
        one(i as f64)
    })
}

/// `cover_point` and `object_params` (5): received from other specs'
/// members, never made by scripts.
fn cover_and_object_params_classes() -> [EngineClass; 2] {
    let cover = EngineClass::new("cover_point")
        .method_with_objects("position", |c: &mut CoverPoint, o, _| {
            // Our choice: a new vector, where the original refers to the
            // stored position (changing it depends on A).
            let p = c.position;
            Ok(vec![o.new_object("vector", Vector::new(p[0], p[1], p[2]))])
        })
        .method("level_vertex_id", |c: &mut CoverPoint, _| {
            Ok(vec![Value::Number(f64::from(c.level_vertex))])
        });
    let params = EngineClass::new("object_params")
        .property_with_objects(
            "level_vertex",
            |p: &ObjectParams, _| Ok(Value::Number(f64::from(p.level_vertex))),
            None,
        )
        .property_with_objects(
            "position",
            |p: &ObjectParams, o| {
                let v = p.position;
                Ok(o.new_object("vector", Vector::new(v[0], v[1], v[2])))
            },
            None,
        );
    [cover, params]
}

fn api(message: &str) -> Error {
    Error {
        kind: ErrorKind::Api,
        message: message.into(),
    }
}

impl Scripts {
    /// Registers the level's names (1.2, 1.3) over `state`: after the game
    /// object basics (whose objects and services it shares, and whose
    /// object event callbacks made `level` a table), before
    /// [`Scripts::load_globals`].
    pub fn register_level(&mut self, state: Rc<RefCell<LevelState>>) -> Result<(), Error> {
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let ctx = Context {
            state,
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            fatal: self.fatal_flag(),
        };
        self.register_class(patrol_class(&ctx))?;
        for class in cover_and_object_params_classes() {
            self.register_class(class)?;
        }
        self.register_class(EngineClass::new("client_spawn_manager"))?;
        let c = ctx.clone();
        let lua = self.lua_mut();
        lua.register_with_objects("__volya_level", move |maker, args| raw(&c, maker, args))?;
        lua.exec_kept("=level", include_bytes!("level.lua"), ENTRY)?;
        self.level_state = Some(Registered { ctx });
        Ok(())
    }

    fn level_context(&self) -> Result<Context, Error> {
        (self.level_state.as_ref())
            .map(|r| r.ctx.clone())
            .ok_or_else(|| api("the level isn't registered"))
    }

    /// The started level's vertices, if any (FAKE(#1234): the fake
    /// stalker movement's).
    pub(crate) fn level_vertices(&self) -> Option<LevelVertices> {
        let state = self.level_state.as_ref()?.ctx.state.borrow();
        state.level.as_ref()?.vertices.clone()
    }

    /// The level is created (conventions): the functions read `level`, and
    /// its client spawn manager is made with it (2.7). A level already
    /// there is ended first ([`Scripts::end_level`]).
    pub fn start_level(&mut self, level: Level) -> Result<(), Error> {
        self.end_level()?;
        let ctx = self.level_context()?;
        let manager =
            (self.lua_mut()).new_engine_object("client_spawn_manager", |_| SpawnManager)?;
        let mut state = ctx.state.borrow_mut();
        state.level = Some(level);
        state.spawn_manager = Some(manager);
        Ok(())
    }

    /// The level removes all its objects and is destroyed (6.5): the spawn
    /// manager's table emptied, none of it called, and the manager deleted
    /// (a value scripts keep then fails when used: our choice).
    pub fn end_level(&mut self) -> Result<(), Error> {
        let ctx = self.level_context()?;
        self.spawn_manager_call(&[Value::from("clear")])?;
        let manager = {
            let mut state = ctx.state.borrow_mut();
            state.level = None;
            state.spawn_manager.take()
        };
        if let Some(manager) = manager {
            self.lua_mut().release_engine_object(manager)?;
        }
        Ok(())
    }

    /// The level spawned object `id` on the client side and its spawn
    /// routine succeeded (6.4): every entry awaiting it is called, in
    /// increasing requester id, with its handle (nil if it has none), then
    /// all of them dropped. A failing function is fatal (D 1.2).
    pub fn client_spawned(&mut self, id: u16) -> Result<(), Error> {
        let handle = self.game_object(id).map_or(Value::Nil, Value::Object);
        let args = [Value::from("spawned"), Value::Number(f64::from(id)), handle];
        self.spawn_manager_call(&args)
    }

    /// Object `id`'s spawn failed, or its engine object is deleted while it
    /// is a creature (6.5): every entry whose requester it is removed, from
    /// every awaited id.
    pub fn spawn_requester_gone(&mut self, id: u16) -> Result<(), Error> {
        self.spawn_manager_call(&[Value::from("gone"), Value::Number(f64::from(id))])
    }

    fn spawn_manager_call(&mut self, args: &[Value]) -> Result<(), Error> {
        let ctx = self.level_context()?;
        ctx.stopped().map_err(|message| Error {
            kind: ErrorKind::Fatal,
            message,
        })?;
        let results = self.guarded(|lua| lua.call_kept(ENTRY, args))?;
        match results.as_slice() {
            [Value::String(o), message, ..] if o == b"error" => {
                let text = match message {
                    Value::String(s) => String::from_utf8_lossy(s).into_owned(),
                    v => format!("{v:?}"),
                };
                if text != NOT_LOGGED {
                    ctx.services.borrow_mut().console.log_mut().write(&text);
                }
                let message = format!("LUA error: {text}");
                *ctx.fatal.borrow_mut() = Some(message.clone());
                Err(Error {
                    kind: ErrorKind::Fatal,
                    message,
                })
            }
            _ => Ok(()),
        }
    }
}
