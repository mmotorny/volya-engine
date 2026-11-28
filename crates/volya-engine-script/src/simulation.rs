// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The simulation registry and the game graph as scripts reach them
//! (script API K, docs/behavior/script-api-simulation-registry-and-game-graph.md;
//! section numbers are its): the global `alife()` and the class
//! `alife_simulator` over the engine's offline simulation
//! ([`volya_engine_core::simulation`]), the story-id classes
//! ([`story_ids`]), `game_graph()` with `CGameGraph` and
//! `GameGraph__CVertex` ([`graph`]), the smart-terrain task, brain and
//! movement classes of offline characters ([`movement`]) and the six
//! member-less base classes.
//!
//! What scripts change is the simulation itself, held by
//! [`AlifeSimulator`] while a game runs, and the events the engine acts
//! on ([`AlifeSimulator::take_events`]). The simulation objects scripts
//! get are spec J's objects of their kinds ([`SimulationObject`]), which refer to
//! the simulation's own object by id. See docs/architecture/scripts.md,
//! Simulation registry.

pub mod graph;
pub mod movement;
pub mod story_ids;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::rc::Rc;

use volya_engine_core::config::Config;
use volya_engine_core::simulation::{NO_ID, Simulation};
use volya_engine_core::switching::SwitchDistances;
use volya_engine_formats::cp1251;
use volya_engine_formats::object::{Kind, Object, ObjectRecord};

use crate::classes::{EngineClass, Objects, Operand};
use crate::entry_points::lua_text;
use crate::files::FileSystem;
use crate::game_object::GameObjects;
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::story::Story;
use crate::values::{Vector, to_f32};
use crate::{Error, ErrorKind, Value};

pub use movement::{BrainState, PatrolPathManager, SmartTerrainTask};

/// The six member-less classes (13), each with its base (the first listed
/// one: `ipure_alife_load_save_object`'s second base adds no member).
pub const MEMBERLESS_CLASSES: [(&str, Option<&str>); 6] = [
    ("ipure_alife_load_object", None),
    ("ipure_alife_save_object", None),
    (
        "ipure_alife_load_save_object",
        Some("ipure_alife_load_object"),
    ),
    ("ipure_server_object", Some("ipure_alife_load_save_object")),
    ("cpure_server_object", Some("ipure_server_object")),
    ("ipure_schedulable_object", None),
];

/// The state flags `set_switch_online`, `set_switch_offline` and
/// `set_interactive` set or clear (6.2).
pub const SWITCH_ONLINE: u32 = 0x02;
pub const SWITCH_OFFLINE: u32 = 0x04;
pub const INTERACTIVE: u32 = 0x08;

/// `remove_all_restrictions`'s type that empties the in-restrictions (8).
pub const IN_RESTRICTIONS: u32 = 4;

/// A simulation object as scripts hold it: spec J's, standing for the
/// simulation's object by id ([`SimulationObject::view`]).
pub use crate::simulation_objects::SimulationObject;

/// The part of the one `alife_simulator` object.
#[derive(Clone, Copy, Debug)]
struct AlifePart;

/// Where `create` gets a new object's record (7.1 step 1, 7.2 step 1):
/// the engine's server objects of a section and of a spawn file record.
pub trait ServerObjects {
    /// The server object of configuration section `section` (spec C
    /// section 6) as a record with its configuration's values; `None`
    /// when no class makes one.
    fn of_section(&mut self, section: &str) -> io::Result<Option<ObjectRecord>>;
    /// The object of the spawn file's record `spawn_id` (`OS` 8.1);
    /// `None` when the spawn file has no such record.
    fn of_spawn_record(&mut self, spawn_id: u16) -> io::Result<Option<ObjectRecord>>;
}

/// What the members ask of the engine, in order
/// ([`AlifeSimulator::take_events`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `release` of an online object (7.3): a destroy event for object
    /// `id`, stamped with the server's time `server_time`.
    Destroy { id: u16, server_time: u32 },
    /// `create` with an online parent (7.1): object `id` (the id the
    /// server's pool gives back, the one step 2 took), registered, is to be
    /// brought online in its parent at once, as the server does for a spawn
    /// message (believed).
    SpawnedOnline { id: u16, parent: u16 },
}

/// The simulation as scripts reach it, shared by the engine.
pub struct AlifeSimulator {
    /// The simulation while a game runs (2); `alife()` is nil without it.
    pub simulation: Option<Simulation>,
    /// The bytes `game.graph` was read from, for its level points (9).
    pub graph_file: Option<Rc<[u8]>>,
    /// The game-time chunk's second factor (`OS` 7.5), for the movement
    /// updates the brains run.
    pub normal_time_factor: f32,
    /// The spawn file's (spawn story id, spawn id) pairs, ordered by spawn
    /// story id (4.4).
    pub spawn_story_ids: BTreeMap<i32, u16>,
    /// The engine's server objects `create` makes.
    pub server_objects: Option<Box<dyn ServerObjects>>,
    /// Each offline character's brain state not kept by the simulation
    /// (12), made at its first use.
    pub(crate) brains: HashMap<u16, BrainState>,
    /// The simulation's own random generator (9): its state.
    pub(crate) random: u32,
    /// Script API Q's relations registry, ranking list and the player's
    /// statistics, for the game's length.
    pub relations: crate::relations::Relations,
    events: Vec<Event>,
}

impl Default for AlifeSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl AlifeSimulator {
    pub fn new() -> Self {
        Self {
            simulation: None,
            graph_file: None,
            normal_time_factor: 1.0,
            spawn_story_ids: BTreeMap::new(),
            server_objects: None,
            brains: HashMap::new(),
            random: 0,
            relations: Default::default(),
            events: Vec::new(),
        }
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The simulation ends (the game is left): `alife()` is nil again, and
    /// script API Q's relations go with it.
    pub fn end(&mut self) -> Option<Simulation> {
        self.brains.clear();
        self.relations = Default::default();
        self.events.clear();
        self.simulation.take()
    }

    /// The generator's next point index among `n` (9): *s* ← *s* ·
    /// 0x8088405 + 1, the index the high 32 bits of *n* · *s*.
    pub(crate) fn random_index(&mut self, n: u32) -> u32 {
        self.random = self.random.wrapping_mul(0x0808_8405).wrapping_add(1);
        ((u64::from(n) * u64::from(self.random)) >> 32) as u32
    }
}

/// What the members share.
#[derive(Clone)]
pub(crate) struct Context {
    pub state: Rc<RefCell<AlifeSimulator>>,
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub story: Rc<RefCell<Story>>,
    pub fatal: Rc<RefCell<Option<String>>>,
}

pub(crate) type Results = Result<Vec<Value>, String>;

impl Context {
    pub fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A failed check with description `description` and `arguments`
    /// (CL 3.8): fatal. The error to raise.
    pub fn failed_check(&self, function: &str, description: &str, arguments: &[&str]) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line: line!(),
                expression: "fatal error",
                description,
                arguments,
            },
        )
    }

    /// A read through no object (conventions): believed to end the program;
    /// fatal here, `what` our wording.
    pub fn no_object(&self, function: &str, what: &str) -> String {
        self.failed_check(function, "<no expression>", &[what])
    }

    /// A log line (CL).
    pub fn log(&self, line: &str) {
        self.services.borrow_mut().console.log_mut().write(line);
    }

    /// `f` on the running simulation; fatal without one (believed
    /// unreachable: scripts reach the members through `alife()`).
    pub fn with_simulation<R>(
        &self,
        function: &str,
        f: impl FnOnce(&mut AlifeSimulator) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut state = self.state.borrow_mut();
        if state.simulation.is_none() {
            drop(state);
            return Err(self.no_object(function, "there is no simulation"));
        }
        f(&mut state)
    }

    /// The game time (`OS` 7).
    pub fn game_time(&self) -> u64 {
        self.objects.borrow().game_time
    }

    /// The system configuration.
    pub fn system(&self) -> Rc<Config> {
        Rc::clone(&self.files.borrow().system)
    }
}

pub(crate) fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A number argument (our wording for a mismatch; depends on A).
pub(crate) fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// An integer argument's low 16 bits (conventions: truncation toward zero,
/// believed).
pub(crate) fn u16_arg(args: &[Value], i: usize, what: &str) -> Result<u16, String> {
    number(args, i, what).map(|n| n.trunc() as i64 as u16)
}

/// An integer argument's low 32 bits.
pub(crate) fn u32_arg(args: &[Value], i: usize, what: &str) -> Result<u32, String> {
    number(args, i, what).map(|n| n.trunc() as i64 as u32)
}

/// A text argument.
pub(crate) fn text(args: &[Value], i: usize, what: &str) -> Result<String, String> {
    match arg(args, i) {
        Value::String(s) => Ok(cp1251::decode(s)),
        _ => Err(format!("{what}: text expected")),
    }
}

/// A truth value (spec E 2.2: the runtime's).
pub(crate) fn truth(args: &[Value], i: usize) -> bool {
    !matches!(arg(args, i), Value::Nil | Value::Boolean(false))
}

/// A `vector` argument.
pub(crate) fn vector(o: &Objects, args: &[Value], i: usize, what: &str) -> Result<Vector, String> {
    match o.get::<Vector>(arg(args, i)) {
        Some(Operand::Other(v)) => Ok(v),
        _ => Err(format!("{what}: vector expected")),
    }
}

/// A simulation object argument's id, if it is one.
pub(crate) fn object_id(o: &Objects, args: &[Value], i: usize) -> Option<u16> {
    match o.get::<SimulationObject>(arg(args, i)) {
        Some(Operand::Other(s)) => Some(s.id()),
        _ => None,
    }
}

pub(crate) fn new_vector(o: &Objects, v: [f32; 3]) -> Value {
    o.new_object("vector", Vector::new(v[0], v[1], v[2]))
}

/// The simulation object `id` as scripts get it, or nil when the
/// simulation has none: an object of spec J's class of its kind, whose
/// state is the simulation's (TODO(#950): its script object's).
fn object_value(ctx: &Context, o: &Objects, sim: &Simulation, id: Option<u16>) -> Value {
    match id.and_then(|id| sim.object(id)) {
        Some(x) => {
            let view =
                SimulationObject::view(x.id, &x.record, Rc::clone(&ctx.state), &ctx.system());
            o.new_object(view.class, view)
        }
        None => Value::Nil,
    }
}

fn one(v: Value) -> Results {
    Ok(vec![v])
}

/// Whether `kind` is a creature (has the Creature block, sav.md 7.4.2).
fn is_creature(kind: Kind) -> bool {
    use Kind::*;
    matches!(
        kind,
        Player | Stalker | Trader | Monster | CrowOrPhantom | MonsterGroup
    )
}

/// Whether `kind` is a space restrictor (8, believed: the Shape block and
/// a restrictor type).
fn is_restrictor(kind: Kind) -> bool {
    use Kind::*;
    matches!(
        kind,
        Restrictor
            | Zone
            | Anomaly
            | VisualAnomaly
            | TorridZone
            | SmartZone
            | LevelChanger
            | TeamBase
    )
}

/// The members of `alife_simulator` (sections 3 to 9, 15).
const MEMBERS: [&str; 22] = [
    "object",
    "valid_object_id",
    "actor",
    "story_object",
    "spawn_id",
    "level_id",
    "level_name",
    "switch_distance",
    "set_switch_online",
    "set_switch_offline",
    "set_interactive",
    "create",
    "create_ammo",
    "release",
    "add_in_restriction",
    "add_out_restriction",
    "remove_in_restriction",
    "remove_out_restriction",
    "remove_all_restrictions",
    "kill_entity",
    "has_info",
    "dont_has_info",
];

fn alife_class(ctx: &Context) -> EngineClass {
    MEMBERS
        .into_iter()
        .fold(EngineClass::new("alife_simulator"), |class, name| {
            let c = ctx.clone();
            class.method_with_objects(name, move |_: &mut AlifePart, o, args| {
                c.stopped()?;
                member(&c, name, o, args)
            })
        })
}

/// One member of `alife_simulator`.
fn member(ctx: &Context, name: &str, o: &Objects, args: &[Value]) -> Results {
    match name {
        "object" => find_object(ctx, o, args),
        "valid_object_id" => one(Value::Boolean(u16_arg(args, 0, name)? != NO_ID)),
        "actor" => ctx.with_simulation(name, |s| {
            let sim = s.simulation.as_ref().expect("running");
            let actor = sim
                .has_actor()
                .then_some(volya_engine_core::simulation::ACTOR_ID);
            one(object_value(ctx, o, sim, actor))
        }),
        "story_object" => {
            let story_id = number(args, 0, name)?.trunc() as i64 as u32 as i32;
            ctx.with_simulation(name, |s| {
                let sim = s.simulation.as_ref().expect("running");
                let id = (story_id != -1)
                    .then(|| sim.by_story_id(story_id))
                    .flatten();
                one(object_value(ctx, o, sim, id))
            })
        }
        "spawn_id" => {
            let key = number(args, 0, name)?.trunc() as i64 as u32 as i32;
            let state = ctx.state.borrow();
            // A key the table lacks: unestablished (4.4); none, our choice.
            let id = state.spawn_story_ids.get(&key).copied().unwrap_or(NO_ID);
            one(Value::Number(f64::from(id)))
        }
        "level_id" => ctx.with_simulation(name, |s| {
            match s.simulation.as_ref().and_then(Simulation::current_level) {
                Some(level) => one(Value::Number(f64::from(level))),
                None => Err(ctx.no_object(name, "there is no current level")),
            }
        }),
        "level_name" => {
            let id = u32_arg(args, 0, name)?;
            ctx.with_simulation(name, |s| {
                let sim = s.simulation.as_ref().expect("running");
                let level = u8::try_from(id).ok().and_then(|l| sim.graph.graph.level(l));
                match level {
                    Some(l) => one(lua_text(&l.name)),
                    None => Err(ctx.failed_check(
                        name,
                        "there is no specified level in the game graph :",
                        &[&id.to_string()],
                    )),
                }
            })
        }
        "switch_distance" => ctx.with_simulation(name, |s| {
            let sim = s.simulation.as_mut().expect("running");
            let d = &mut sim.config.distances;
            if args.is_empty() {
                return one(Value::Number(f64::from(d.distance)));
            }
            let v = to_f32(number(args, 0, name)?);
            *d = SwitchDistances::of(v, d.factor);
            Ok(Vec::new())
        }),
        "set_switch_online" | "set_switch_offline" | "set_interactive" => {
            let id = u16_arg(args, 0, name)?;
            let set = truth(args, 1);
            let bit = match name {
                "set_switch_online" => SWITCH_ONLINE,
                "set_switch_offline" => SWITCH_OFFLINE,
                _ => INTERACTIVE,
            };
            ctx.with_simulation(name, |s| {
                let sim = s.simulation.as_mut().expect("running");
                let Some(object) = sim.object_mut(id) else {
                    return Err(ctx.no_object(name, &format!("no object with id {id}")));
                };
                let flags = &mut object.record.object.object_block_mut().state_flags;
                if set {
                    *flags |= bit;
                } else {
                    *flags &= !bit;
                }
                Ok(Vec::new())
            })
        }
        "create" | "create_ammo" => create(ctx, name, o, args),
        "release" => release(ctx, o, args),
        "add_in_restriction"
        | "add_out_restriction"
        | "remove_in_restriction"
        | "remove_out_restriction" => restriction(ctx, name, o, args),
        "remove_all_restrictions" => remove_all_restrictions(ctx, args),
        "kill_entity" => kill_entity(ctx, o, args),
        "has_info" | "dont_has_info" => {
            let id = u16_arg(args, 0, name)?;
            let info = text(args, 1, name)?;
            let story = ctx.story.borrow();
            let holds = (story.info_portions.as_ref()).is_some_and(|p| p.borrow().holds(id, &info));
            one(Value::Boolean(holds == (name == "has_info")))
        }
        _ => Err(format!("alife_simulator: unknown member {name}")),
    }
}

/// `object(id)`, `object(id, flag)` and `object(name)` (3).
fn find_object(ctx: &Context, o: &Objects, args: &[Value]) -> Results {
    let name = match arg(args, 0) {
        Value::String(s) => Some(cp1251::decode(s)),
        _ => None,
    };
    let id = match name {
        Some(_) => None,
        None => Some(u16_arg(args, 0, "object")?),
    };
    ctx.with_simulation("object", |s| {
        let sim = s.simulation.as_ref().expect("running");
        let found = match (&name, id) {
            (Some(name), _) => sim
                .objects()
                .find(|x| x.record.header.name == **name)
                .map(|x| x.id),
            (None, id) => id,
        };
        one(object_value(ctx, o, sim, found))
    })
}

/// The name of an object created from `section` with id `id` (7.1 step 3).
pub fn created_name(section: &str, id: u16) -> String {
    format!("{section}{id:04}")
}

/// `create(section, position, level_vertex, game_vertex[, parent_id])`,
/// `create(spawn_id)` (7.1, 7.2) and `create_ammo` (7.2).
fn create(ctx: &Context, name: &str, o: &Objects, args: &[Value]) -> Results {
    if name == "create" && matches!(arg(args, 0), Value::Number(_)) {
        let spawn_id = u16_arg(args, 0, name)?;
        return create_from_spawn(ctx, o, spawn_id);
    }
    let section = text(args, 0, name)?;
    let position = vector(o, args, 1, name)?;
    let level_vertex = u32_arg(args, 2, name)?;
    let game_vertex = u16_arg(args, 3, name)?;
    let parent = match arg(args, 4) {
        Value::Nil if name == "create" => NO_ID,
        _ => u16_arg(args, 4, name)?,
    };
    let count = match name {
        "create_ammo" => Some(u16_arg(args, 5, name)?),
        _ => None,
    };
    let system = ctx.system();
    let mut state = ctx.state.borrow_mut();
    let state = &mut *state;
    let Some(sim) = state.simulation.as_mut() else {
        return Err(ctx.no_object(name, "there is no simulation"));
    };
    let parent_online = match sim.object(parent) {
        _ if parent == NO_ID => false,
        Some(p) => p.online,
        None => {
            ctx.log(&format!("! invalid parent id [{parent}] specified"));
            return one(Value::Nil);
        }
    };
    // Step 1: the server object of the section.
    // TODO(#1119): the engine's server objects of a section.
    let made = match state.server_objects.as_mut() {
        Some(source) => source.of_section(&section).map_err(|e| e.to_string())?,
        None => return Err(format!("{name}: the engine makes no server objects yet")),
    };
    let Some(mut record) = made else {
        return Err(ctx.failed_check(name, "Cannot find item with section", &[&section]));
    };
    // Step 2.
    let id = sim
        .take_id()
        .map_err(|_| ctx.failed_check(name, "Not enough IDs", &[]))?;
    record.header.id = id;
    record.header.parent_id = parent;
    record.header.position = [position.x, position.y, position.z];
    let block = record.object.object_block_mut();
    block.game_graph_vertex = game_vertex;
    block.level_graph_node = level_vertex;
    // Step 3.
    record.header.name = created_name(&section, id).into();
    if let (Some(count), Object::Ammunition(a)) = (count, &mut record.object) {
        // For an online parent before it is handed to the server (7.2).
        a.rounds_left = count;
    }
    // Step 4 (spec J's script calls, 7.4: TODO(#950)); the supplies of
    // step 5 are not created (TODO(#1221)).
    sim.register_created(record, None, &system)
        .map_err(|e| ctx.failed_check(name, &e.to_string(), &[]))?;
    if parent_online {
        if let Some(x) = sim.object_mut(id) {
            x.online = true;
        }
        state.events.push(Event::SpawnedOnline { id, parent });
    }
    let sim = state.simulation.as_ref().expect("running");
    one(object_value(ctx, o, sim, Some(id)))
}

/// `create(spawn_id)` (7.2).
fn create_from_spawn(ctx: &Context, o: &Objects, spawn_id: u16) -> Results {
    let what = "create";
    let system = ctx.system();
    let mut state = ctx.state.borrow_mut();
    let state = &mut *state;
    if state.simulation.is_none() {
        return Err(ctx.no_object(what, "there is no simulation"));
    }
    // TODO(#1119): the engine's objects of the spawn file's records.
    let made = match state.server_objects.as_mut() {
        Some(source) => source
            .of_spawn_record(spawn_id)
            .map_err(|e| e.to_string())?,
        None => return Err(format!("{what}: the engine makes no server objects yet")),
    };
    let Some(mut record) = made else {
        return Err(ctx.no_object(what, &format!("no spawn record {spawn_id}")));
    };
    let sim = state.simulation.as_mut().expect("running");
    if volya_engine_core::simulation::uses_level_graph(&record)
        && let Some(nav) = sim.navigation()
        && record.object.object_block().game_graph_vertex_on(sim)
        && !nav.valid(record.object.object_block().level_graph_node)
    {
        let name = record.header.name.to_string();
        return Err(ctx.failed_check(what, "Invalid vertex for object ", &[&name]));
    }
    let player = record.kind() == Kind::Player;
    let id = if player && !sim.has_actor() {
        volya_engine_core::simulation::ACTOR_ID
    } else {
        sim.take_id()
            .map_err(|_| ctx.failed_check(what, "Not enough IDs", &[]))?
    };
    record.header.id = id;
    sim.register_created(record, Some(u32::from(spawn_id)), &system)
        .map_err(|e| ctx.failed_check(what, &e.to_string(), &[]))?;
    one(object_value(ctx, o, sim, Some(id)))
}

/// Whether a game vertex is on the simulation's current level.
trait OnLevel {
    fn game_graph_vertex_on(&self, sim: &Simulation) -> bool;
}

impl OnLevel for volya_engine_formats::object::ObjectBlock {
    fn game_graph_vertex_on(&self, sim: &Simulation) -> bool {
        let level = sim.current_level();
        (sim.graph.graph.vertex(self.game_graph_vertex)).is_some_and(|v| Some(v.level_id) == level)
    }
}

/// `release(object, flag)` (7.3).
fn release(ctx: &Context, o: &Objects, args: &[Value]) -> Results {
    let what = "release";
    let Some(id) = object_id(o, args, 0) else {
        return Err(ctx.no_object(what, "no object to release"));
    };
    let server_time = ctx.objects.borrow().server_time;
    let mut state = ctx.state.borrow_mut();
    let state = &mut *state;
    let Some(sim) = state.simulation.as_mut() else {
        return Err(ctx.no_object(what, "there is no simulation"));
    };
    let Some(object) = sim.object(id) else {
        return Err(ctx.no_object(what, &format!("no object with id {id}")));
    };
    if object.online {
        state.events.push(Event::Destroy { id, server_time });
        return Ok(Vec::new());
    }
    // 4.3: an object whose story id has no entry gives a log line. Spec J's
    // `on_unregister` comes first (7.4): TODO(#950).
    let mut missing = Vec::new();
    let mut stack = vec![id];
    while let Some(x) = stack.pop() {
        if let Some(object) = sim.object(x) {
            let story_id = object.record.object.object_block().story_id;
            if story_id != -1 && sim.by_story_id(story_id) != Some(x) {
                missing.push(story_id);
            }
            stack.extend(
                sim.objects()
                    .filter(|c| c.record.header.parent_id == x && c.id != x)
                    .map(|c| c.id),
            );
        }
    }
    for x in sim.release(id) {
        state.brains.remove(&x);
    }
    for story_id in missing {
        ctx.log(&format!(
            "Cannot find story object with id [{story_id}] in the Story registry!"
        ));
    }
    Ok(Vec::new())
}

/// The four restriction members of one restrictor (8).
fn restriction(ctx: &Context, name: &str, o: &Objects, args: &[Value]) -> Results {
    let adding = name.starts_with("add");
    let inward = name.contains("_in_");
    let Some(id) = object_id(o, args, 0) else {
        return Err(ctx.no_object(name, "no creature given"));
    };
    let restrictor = u16_arg(args, 1, name)?;
    let verb = if adding { "add" } else { "remove" };
    let mut state = ctx.state.borrow_mut();
    let Some(sim) = state.simulation.as_mut() else {
        return Err(ctx.no_object(name, "there is no simulation"));
    };
    let because = |why: &str| {
        format!(
            "! cannot {verb} restriction with id {restrictor} to the entity with id {id}, because {why}"
        )
    };
    let line = match (sim.object(id), sim.object(restrictor)) {
        (None, _) => Some(because("there is no creature with the specified id")),
        (_, None) => Some(because(
            "there is no space restrictor with the specified id",
        )),
        (Some(c), _) if !is_creature(c.record.kind()) => Some(because(
            "there is an object with the specified id, but it is not a creature",
        )),
        (_, Some(r)) if !is_restrictor(r.record.kind()) => Some(because(
            "there is an object with the specified id, but it is not a space restrictor",
        )),
        (Some(c), Some(r)) => {
            let names = (
                c.record.header.name.to_string(),
                r.record.header.name.to_string(),
            );
            let creature = (sim.object_mut(id))
                .and_then(|c| c.record.object.creature_mut())
                .expect("a creature");
            let list = if inward {
                &mut creature.dynamic_in_restrictions
            } else {
                &mut creature.dynamic_out_restrictions
            };
            if adding {
                list.push(restrictor);
                None
            } else if let Some(i) = list.iter().position(|&x| x == restrictor) {
                list.remove(i);
                None
            } else {
                Some(format!(
                    "~ cannot remove restriction with id [{restrictor}][{}] to the entity with id [{id}][{}], because it is not added",
                    names.1, names.0
                ))
            }
        }
    };
    drop(state);
    if let Some(line) = line {
        ctx.log(&line);
    }
    Ok(Vec::new())
}

/// `remove_all_restrictions(object_id, type)` (8).
fn remove_all_restrictions(ctx: &Context, args: &[Value]) -> Results {
    let what = "remove_all_restrictions";
    let id = u16_arg(args, 0, what)?;
    let kind = u32_arg(args, 1, what)?;
    let mut state = ctx.state.borrow_mut();
    let Some(sim) = state.simulation.as_mut() else {
        return Err(ctx.no_object(what, "there is no simulation"));
    };
    let line = match sim.object_mut(id) {
        None => Some(format!(
            "! cannot remove restrictions to the entity with id {id}, because there is no creature with the specified id"
        )),
        Some(o) => match o.record.object.creature_mut() {
            None => Some(format!(
                "! cannot remove restriction to the entity with id {id}, because there is an object with the specified id, but it is not a creature"
            )),
            Some(c) => {
                if kind == IN_RESTRICTIONS {
                    c.dynamic_in_restrictions.clear();
                } else {
                    c.dynamic_out_restrictions.clear();
                }
                None
            }
        },
    };
    drop(state);
    if let Some(line) = line {
        ctx.log(&line);
    }
    Ok(Vec::new())
}

/// `kill_entity(monster[, game_vertex[, killer]])` (9).
fn kill_entity(ctx: &Context, o: &Objects, args: &[Value]) -> Results {
    let what = "kill_entity";
    let Some(id) = object_id(o, args, 0) else {
        return Err(ctx.no_object(what, "no monster given"));
    };
    let vertex = match arg(args, 1) {
        Value::Nil => None,
        _ => Some(u16_arg(args, 1, what)?),
    };
    // A killer is asked whether it is an anomalous zone; no schedulable
    // object is one, so every killer takes the first rule of step 2.
    let mut state = ctx.state.borrow_mut();
    let state = &mut *state;
    let Some(sim) = state.simulation.as_mut() else {
        return Err(ctx.no_object(what, "there is no simulation"));
    };
    let Some(monster) = sim.object(id) else {
        return Err(ctx.no_object(what, &format!("no object with id {id}")));
    };
    let block = monster.record.object.object_block();
    let old_vertex = block.game_graph_vertex;
    let vertex = vertex.unwrap_or(old_vertex);
    // Step 1: the items it carries; their use is unestablished, and they
    // stay where they are (our choice).
    let items: Vec<u16> = sim
        .objects()
        .filter(|c| c.record.header.parent_id == id && c.id != id)
        .map(|c| c.id)
        .collect();
    // Step 2: the health 0, then the death place, one of the vertex's
    // level points. What a vertex without level points reads is
    // unestablished (every vertex of this edition has one): the position is
    // then kept (our choice).
    if let Some(c) = sim
        .object_mut(id)
        .and_then(|m| m.record.object.creature_mut())
    {
        c.health = 0.0;
    }
    let points = match (&state.graph_file, sim.graph.graph.vertex(vertex)) {
        (Some(file), Some(_)) => sim
            .graph
            .graph
            .level_points(file, vertex)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let on_level = sim
        .graph
        .graph
        .vertex(vertex)
        .is_some_and(|v| Some(v.level_id) == sim.current_level());
    let point = match u32::try_from(points.len()) {
        Ok(n) if n > 0 => {
            let i = state.random_index(n);
            points.get(i as usize).copied()
        }
        _ => None,
    };
    let sim = state.simulation.as_mut().expect("running");
    if let Some(p) = point
        && on_level
        && sim.navigation().is_some_and(|n| !n.valid(p.level_vertex))
    {
        return Err(ctx.failed_check(what, "Invalid vertex", &[]));
    }
    let monster = sim.object_mut(id).expect("the monster");
    let block = monster.record.object.object_block_mut();
    block.game_graph_vertex = vertex;
    if let Some(p) = point {
        block.level_graph_node = p.level_vertex;
        block.distance = p.distance;
        monster.record.header.position = p.position;
    }
    // The two game-vertex fields of the Monster-like update (believed).
    let update = match &mut monster.record.object {
        Object::Stalker(s) => Some(&mut s.update),
        Object::Monster(m) => Some(&mut m.update),
        _ => None,
    };
    if let Some(u) = update {
        u.next_game_vertex = vertex;
        u.previous_game_vertex = vertex;
    }
    // Step 3: the monster's own step does nothing. Step 4: any object it
    // carries besides the items of step 1 is fatal (what step 1's list does
    // with them is unestablished: they stay carried, our choice).
    let carries = sim
        .objects()
        .any(|c| c.record.header.parent_id == id && c.id != id && !items.contains(&c.id));
    if carries {
        return Err(ctx.failed_check(what, "the monster still carries an object", &[]));
    }
    // Step 5: its location, as for a relocation; the group and the list
    // of the simulation's are unestablished.
    if vertex != old_vertex {
        sim.enter_level_list(id);
    }
    Ok(Vec::new())
}

/// What a state with the simulation registry registered keeps.
pub(crate) struct Registered {
    pub(crate) ctx: Context,
}

impl Scripts {
    /// Registers the simulation registry and game graph's names (1) over
    /// `state`: after the game object basics and the story (whose objects,
    /// services, files and information portions it shares), before
    /// [`Scripts::load_globals`]. The story-id classes are built from the
    /// game configuration here (4.1): a fatal check fails the
    /// registration.
    pub fn register_simulation(&mut self, state: Rc<RefCell<AlifeSimulator>>) -> Result<(), Error> {
        let api = |message: &str| Error {
            kind: ErrorKind::Api,
            message: message.into(),
        };
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let story = (self.registered_story()).ok_or_else(|| api("the story isn't registered"))?;
        let ctx = Context {
            state,
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            story,
            fatal: self.fatal_flag(),
        };
        for (name, base) in MEMBERLESS_CLASSES {
            let class = EngineClass::new(name);
            self.register_class(match base {
                Some(base) => class.base(base),
                None => class,
            })?;
        }
        let game = Rc::clone(&ctx.files.borrow().game);
        for class in story_ids::classes(&ctx, &game)? {
            self.register_class(class)?;
        }
        self.register_class(alife_class(&ctx))?;
        for class in graph::classes(&ctx) {
            self.register_class(class)?;
        }
        for class in movement::classes(&ctx) {
            self.register_class(class)?;
        }
        let lua = self.lua_mut();
        let simulator = lua.new_engine_object("alife_simulator", |_| AlifePart)?;
        let c = ctx.clone();
        lua.register_object_function("alife", move |_| {
            c.stopped()?;
            Ok(c.state.borrow().simulation.is_some().then_some(simulator))
        })?;
        let game_graph = lua.new_engine_object("CGameGraph", |_| graph::GamePart)?;
        let c = ctx.clone();
        lua.register_object_function("game_graph", move |_| {
            c.stopped()?;
            Ok(Some(game_graph))
        })?;
        self.simulation_state = Some(Registered { ctx });
        Ok(())
    }

    /// The brain of simulation object `id` as scripts get it (12; spec J's
    /// `brain()` hands it out): a `CALifeHumanBrain` for a stalker, a
    /// `CALifeMonsterBrain` for another monster; nil for any other object.
    pub fn simulation_brain(&mut self, id: u16) -> Result<Value, Error> {
        let ctx = (self.simulation_state.as_ref().map(|r| r.ctx.clone())).ok_or_else(|| Error {
            kind: ErrorKind::Api,
            message: "the simulation registry isn't registered".into(),
        })?;
        let kind = {
            let state = ctx.state.borrow();
            let sim = state.simulation.as_ref();
            sim.and_then(|s| s.object(id)).map(|o| o.record.kind())
        };
        let class = match kind {
            Some(Kind::Stalker) => "CALifeHumanBrain",
            Some(Kind::Monster) => "CALifeMonsterBrain",
            _ => return Ok(Value::Nil),
        };
        let part = self
            .lua_mut()
            .new_engine_object(class, |_| movement::Brain { id })?;
        Ok(Value::Object(part))
    }
}
