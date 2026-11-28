// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Simulation objects as scripts see them (script API J,
//! docs/behavior/script-api-simulation-objects.md; section numbers are
//! its): the 60 engine classes `cse_…` and `CSE_AbstractVisual` with their
//! bases, constructors, properties and methods ([`classes`]), and the
//! engine's calls of the methods a script class overrides (section 4,
//! 5), which [`Scripts`] makes on behalf of the offline simulation.
//!
//! Every class's engine part is a [`SimulationObject`]: the class it was
//! constructed as, its section, and its state as the simulation holds it
//! (the record of docs/formats/sav.md section 7, once the engine has read
//! it). The engine reaches a dispatched method through the script
//! object's override (spec A 5.2) and every other one through the engine's
//! own implementation (4.1): [`dispatched`]. The classes spec K owns
//! (`cpure_server_object`, `ipure_schedulable_object`, the brains and the
//! smart-terrain task) are registered here without members, for this
//! spec's bases and results; spec K adds theirs. See
//! docs/architecture/scripts.md, Simulation objects.

pub mod classes;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use volya_engine_core::config::Config;
use volya_engine_core::simulation::class_of;
use volya_engine_formats::cp1251;
use volya_engine_formats::object::{Kind, ObjectRecord};

use crate::classes::{Conversion, EnginePart, OverrideFailure};
use crate::data_streams::NetPacket;
use crate::files::FileSystem;
use crate::game_object::game_bytes;
use crate::loader::Scripts;
use crate::object_factory::{CreatedObject, ObjectFactory, class_id, id_text};
use crate::services::EngineServices;
use crate::simulation::AlifeSimulator;
use crate::story::Story;
use crate::values::{lua_number, to_f32};
use crate::{Error, ErrorKind, Value};

pub use classes::{CLASSES, Class, dispatched, engine_class, engine_class_of};

/// What the engine's own implementation of a dispatched method left for
/// the engine to finish once the call returns ([`SimulationObject`]): the
/// effects that call the scripts again, or that are the simulation's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Ran {
    state_write: bool,
    on_register: bool,
    on_unregister: bool,
    update: bool,
}

/// A simulation object's engine part (2.2): an object of one of the 51
/// constructible classes for a configuration section, and once the engine
/// has read it (5.1), its state: its own, or that of the simulation's
/// object it stands for (spec K's objects; [`SimulationObject::view`]).
#[derive(Clone)]
pub struct SimulationObject {
    /// Its own part, which `init` returns (4.2); none for a view.
    pub part: Option<EnginePart>,
    /// The engine class it was constructed as: the engine base of its
    /// script class, which decides what the engine dispatches (4.1).
    pub class: &'static str,
    /// Its configuration section, the game's bytes (2.2).
    pub section: Vec<u8>,
    /// The class id of the section's `class` key (spec C 1).
    pub class_id: u64,
    /// Its state: the spawn and update packets' contents (sav.md 7.2 to
    /// 7.4), once the engine has read it; `None` before.
    pub record: Option<ObjectRecord>,
    /// The simulation's object whose state it is, by id: then `record` is
    /// unused.
    pub simulation: Option<(u16, Rc<RefCell<AlifeSimulator>>)>,
    /// While it has its in-game counterpart (3.1, offline 6.3); the engine
    /// sets it.
    pub online: bool,
    /// A monster's `rank` section key, fixed at construction (3.6).
    pub rank: i32,
    /// The `spawn_ini()` configuration, made at the first call (3.2).
    spawn_ini: Option<Rc<Config>>,
    /// The state the engine's `STATE_Read` takes (5.1).
    pending: Option<Pending>,
    ran: Ran,
}

impl SimulationObject {
    fn new(
        part: Option<EnginePart>,
        class: &'static str,
        section: Vec<u8>,
        system: &Config,
    ) -> Self {
        let text = cp1251::decode(&section);
        let class_id = class_of(system, &text)
            .map(|c| class_id(&game_bytes(c)))
            .unwrap_or(0);
        // 3.6: the integer value of `rank`, 0 when the key is absent.
        let rank = if classes::derives(class, "cse_alife_monster_abstract") {
            system
                .get(&text, "rank")
                .flatten()
                .map_or(0, |v| volya_engine_core::config::value::int(v) as i32)
        } else {
            0
        };
        SimulationObject {
            part,
            class,
            section,
            class_id,
            record: None,
            simulation: None,
            online: false,
            rank,
            spawn_ini: None,
            pending: None,
            ran: Ran::default(),
        }
    }

    /// An object standing for the simulation's object `id` whose state is
    /// `record` (spec K), for an object the engine holds no server object
    /// for ([`Scripts::create_simulation_objects`]): of the class of its
    /// kind ([`engine_class_of`]), its state the simulation's.
    pub fn view(
        id: u16,
        record: &ObjectRecord,
        alife: Rc<RefCell<AlifeSimulator>>,
        system: &Config,
    ) -> Self {
        let class = engine_class_of(record);
        let mut o = Self::new(None, class, game_bytes(&record.header.section), system);
        o.simulation = Some((id, alife));
        o
    }

    /// Its state, or the error of a member reading it before the engine has
    /// read it (our wording: the original's state is the constructor's).
    pub fn state(&self) -> Result<StateRef<'_>, String> {
        if let Some((id, alife)) = &self.simulation {
            let a = alife
                .try_borrow()
                .map_err(|_| "the simulation is in use".to_owned())?;
            return Ref::filter_map(a, |a| {
                a.simulation
                    .as_ref()
                    .and_then(|s| s.object(*id))
                    .map(|o| &o.record)
            })
            .map(StateRef::Simulation)
            .map_err(|_| "the simulation has no such object".to_owned());
        }
        self.record
            .as_ref()
            .map(StateRef::Own)
            .ok_or_else(|| "the simulation object's state isn't read yet".to_owned())
    }

    pub fn state_mut(&mut self) -> Result<StateMut<'_>, String> {
        if let Some((id, alife)) = &self.simulation {
            let a = alife
                .try_borrow_mut()
                .map_err(|_| "the simulation is in use".to_owned())?;
            return RefMut::filter_map(a, |a| {
                a.simulation
                    .as_mut()
                    .and_then(|s| s.object_mut(*id))
                    .map(|o| &mut o.record)
            })
            .map(StateMut::Simulation)
            .map_err(|_| "the simulation has no such object".to_owned());
        }
        self.record
            .as_mut()
            .map(StateMut::Own)
            .ok_or_else(|| "the simulation object's state isn't read yet".to_owned())
    }

    /// Its id, 0xFFFF before its state is read.
    pub fn id(&self) -> u16 {
        match &self.simulation {
            Some((id, _)) => *id,
            None => self.record.as_ref().map_or(0xFFFF, |r| r.header.id),
        }
    }
}

/// What the engine's `STATE_Read` takes (5.1): the state it reads, and the
/// packet's read position where the scripted additions start (sav.md 7.5),
/// at which it leaves the packet.
#[derive(Clone)]
pub(crate) struct Pending {
    pub record: ObjectRecord,
    pub scripted_additions: u32,
}

/// How the simulation's objects came to be (offline-simulation.md 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Creation {
    /// A New Game's, from the spawn file's records and their supplies (8.1).
    NewGame,
    /// A load's, from the save's records (8.2).
    Load,
}

/// A simulation object's state as a member reads it.
pub enum StateRef<'a> {
    Own(&'a ObjectRecord),
    Simulation(Ref<'a, ObjectRecord>),
}

impl Deref for StateRef<'_> {
    type Target = ObjectRecord;

    fn deref(&self) -> &ObjectRecord {
        match self {
            StateRef::Own(r) => r,
            StateRef::Simulation(r) => r,
        }
    }
}

/// A simulation object's state as a member changes it.
pub enum StateMut<'a> {
    Own(&'a mut ObjectRecord),
    Simulation(RefMut<'a, ObjectRecord>),
}

impl Deref for StateMut<'_> {
    type Target = ObjectRecord;

    fn deref(&self) -> &ObjectRecord {
        match self {
            StateMut::Own(r) => r,
            StateMut::Simulation(r) => r,
        }
    }
}

impl DerefMut for StateMut<'_> {
    fn deref_mut(&mut self) -> &mut ObjectRecord {
        match self {
            StateMut::Own(r) => r,
            StateMut::Simulation(r) => r,
        }
    }
}

/// The engine's own state of simulation objects: the server objects the
/// engine holds for the simulation's objects by id
/// ([`Scripts::create_simulation_objects`]), which spec K's members hand
/// out; the ids of those spec K's `release` took from the simulation,
/// whose server objects the engine is to release
/// ([`Scripts::release_simulation_objects`]); and the game time the
/// engine's `on_death` stamps (5.8).
#[derive(Debug, Default)]
pub struct SimulationObjects {
    pub game_time: u64,
    held: HashMap<u16, Held>,
    released: Vec<u16>,
}

/// A server object the engine holds ([`SimulationObjects`]).
#[derive(Clone, Copy, Debug)]
struct Held {
    part: EnginePart,
    /// Made by the object factory from a script entry (else an engine
    /// object).
    scripted: bool,
}

impl SimulationObjects {
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine part of the server object held for object `id`.
    pub fn held(&self, id: u16) -> Option<EnginePart> {
        self.held.get(&id).map(|h| h.part)
    }

    /// How many server objects the engine holds.
    pub fn held_count(&self) -> usize {
        self.held.len()
    }

    /// Objects `ids` left the simulation: their server objects are to be
    /// released.
    pub(crate) fn released(&mut self, ids: impl IntoIterator<Item = u16>) {
        self.released.extend(ids);
    }
}

/// What the members share.
#[derive(Clone)]
pub(crate) struct Context {
    pub state: Rc<RefCell<SimulationObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub factory: Rc<RefCell<Option<ObjectFactory>>>,
    pub fatal: Rc<RefCell<Option<String>>>,
    /// Spec K's simulation, when it is registered: the state of the
    /// objects it hands out.
    pub alife: Option<Rc<RefCell<AlifeSimulator>>>,
}

impl Context {
    pub fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }
}

/// The registration, kept on the state.
pub(crate) struct Registered {
    ctx: Context,
}

/// The truth of an override's result kept as a boolean (4.2): Lua's (our
/// choice: how another value converts is unestablished).
fn truth(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}

/// The engine's single-precision float of a result (4.2): Lua's number
/// conversion (a non-number gives 0, spec A 5.3).
fn float(v: &Value) -> f32 {
    to_f32(lua_number(v))
}

fn api(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::Api,
        message: message.into(),
    }
}

/// The value of an optional object argument.
fn object_or_nil(part: Option<EnginePart>) -> Value {
    part.map_or(Value::Nil, Value::Object)
}

impl Scripts {
    /// Registers the classes of section 2 with their members (section 3):
    /// after the game object basics, whose services and files they share,
    /// and after spec K's simulation registry when it is registered (whose
    /// objects these are, and whose classes they need; without it they are
    /// registered here without members), before [`Scripts::load_globals`].
    pub fn register_simulation_objects(&mut self) -> Result<(), Error> {
        let basics = self
            .game_object_state
            .as_ref()
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        // Spec K's members hand out the server objects held here.
        let state =
            (self.simulation_state.as_ref()).map_or_else(Rc::default, |r| Rc::clone(&r.ctx.held));
        let ctx = Context {
            state,
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            factory: Rc::clone(&self.factory),
            fatal: self.fatal_flag(),
            alife: self
                .simulation_state
                .as_ref()
                .map(|r| Rc::clone(&r.ctx.state)),
        };
        for class in classes::classes(&ctx) {
            self.register_class(class)?;
        }
        self.simulation_objects_state = Some(Registered { ctx });
        Ok(())
    }

    fn simulation_context(&self) -> Result<Context, Error> {
        self.simulation_objects_state
            .as_ref()
            .map(|r| r.ctx.clone())
            .ok_or_else(|| api("the simulation objects aren't registered"))
    }

    /// The engine's state of simulation objects.
    pub fn simulation_objects(&self) -> Result<Rc<RefCell<SimulationObjects>>, Error> {
        self.simulation_context().map(|c| c.state)
    }

    /// Runs `f` on the simulation object whose engine part is `part`.
    pub fn with_simulation_object<R>(
        &self,
        part: EnginePart,
        f: impl FnOnce(&mut SimulationObject) -> R,
    ) -> Option<R> {
        self.with_part(part, f)
    }

    fn simulation_object<R>(
        &self,
        part: EnginePart,
        f: impl FnOnce(&mut SimulationObject) -> R,
    ) -> Result<R, Error> {
        self.with_part(part, f)
            .ok_or_else(|| api("no simulation object has that engine part"))
    }

    /// The engine's own object of class `class` (one of the 51 with a
    /// constructor) for section `section`, as its constructor builds it
    /// (2.2), held by the engine for the state's lifetime unless released
    /// ([`crate::Lua::release_engine_object`]).
    pub fn new_simulation_object(
        &mut self,
        class: &str,
        section: &[u8],
    ) -> Result<EnginePart, Error> {
        let ctx = self.simulation_context()?;
        let class = classes::class(class)
            .filter(|c| c.constructible)
            .ok_or_else(|| api(format!("{class} isn't a constructible simulation class")))?
            .name;
        let system = Rc::clone(&ctx.files.borrow().system);
        let section = section.to_vec();
        self.lua_mut().new_engine_object(class, |part| {
            SimulationObject::new(Some(part), class, section, &system)
        })
    }

    /// The engine calls method `method` of simulation object `part`: the
    /// script override when the method is dispatched for its class (4.1),
    /// else the engine's own implementation.
    fn simulation_call(
        &mut self,
        part: EnginePart,
        method: &str,
        args: &[Value],
        conversion: Conversion,
    ) -> Result<Value, OverrideFailure> {
        let class = self.simulation_object(part, |o| o.class)?;
        if dispatched(class, method) {
            self.dispatch(part, method, args, conversion)
        } else {
            self.guarded(|lua| Ok(lua.run_default(part, method, args, conversion)))?
        }
    }

    /// `init()` (2.2, 5.2): once, right after the object factory has made
    /// a server object; its result is the object to use. A missing result
    /// is fatal (our wording: the original asserts).
    pub fn initialize_server_object(
        &mut self,
        part: EnginePart,
    ) -> Result<EnginePart, OverrideFailure> {
        let result = self.simulation_call(part, "init", &[], Conversion::Value)?;
        let object = match result {
            Value::Object(object) => object,
            // An engine object's result is its part's id (spec A).
            Value::Number(n) => EnginePart::from_value(n),
            _ => {
                let message = "the server object's init gave no object".to_owned();
                *self.fatal.borrow_mut() = Some(message.clone());
                return Err(Error {
                    kind: ErrorKind::Fatal,
                    message,
                }
                .into());
            }
        };
        Ok(object)
    }

    /// The engine reads the spawn packet of `part` (5.1): its `STATE_Read`
    /// with the packet `record` writes (sav.md 7.2), an engine packet (spec
    /// I, [`Scripts::new_packet`]) positioned at the object state, and its
    /// data size (3.2). The engine's own implementation takes `record` as
    /// the object's state (unless the object stands for the simulation's,
    /// whose state it is) and leaves the packet where the scripted
    /// additions start (sav.md 7.5), from where a script class's override
    /// reads its own. The packet is released once the call returns.
    pub fn read_state(
        &mut self,
        part: EnginePart,
        record: ObjectRecord,
    ) -> Result<(), OverrideFailure> {
        let spawn = record
            .write_spawn_packet()
            .map_err(|e| api(e.to_string()))?;
        // The data size counts itself (sav.md 7.2).
        let size = spawn.bytes.len() - spawn.object_state + 2;
        let mut packet = NetPacket::holding(&spawn.bytes)
            .ok_or_else(|| api("a spawn packet larger than a packet's buffer"))?;
        packet.read_position = spawn.object_state as u32;
        let packet = self.new_packet(packet)?;
        let pending = Pending {
            record,
            scripted_additions: spawn.scripted_additions as u32,
        };
        self.simulation_object(part, |o| o.pending = Some(pending))?;
        let result = self.simulation_call(
            part,
            "STATE_Read",
            &[Value::Object(packet), Value::Number(size as f64)],
            Conversion::Nothing,
        );
        self.simulation_object(part, |o| o.pending = None)?;
        self.release_stream(packet)?;
        result.map(drop)
    }

    /// The engine sets the state of `part` without its `STATE_Read`: the
    /// state the engine holds for an object it made itself.
    pub fn set_simulation_state(
        &mut self,
        part: EnginePart,
        record: ObjectRecord,
    ) -> Result<(), Error> {
        self.simulation_object(part, |o| o.record = Some(record))
    }

    /// The engine writes the spawn packet of `part` (5.1): its
    /// `STATE_Write`. The object's state if the engine's own implementation
    /// wrote it (an override that skips its base call writes none of it);
    /// `packet` as for [`Scripts::read_state`].
    pub fn write_state(
        &mut self,
        part: EnginePart,
        packet: Value,
    ) -> Result<Option<ObjectRecord>, OverrideFailure> {
        self.simulation_object(part, |o| o.ran.state_write = false)?;
        self.simulation_call(part, "STATE_Write", &[packet], Conversion::Nothing)?;
        Ok(self.simulation_object(part, |o| {
            std::mem::take(&mut o.ran.state_write)
                .then(|| o.record.clone())
                .flatten()
        })?)
    }

    fn boolean_call(&mut self, part: EnginePart, method: &str) -> Result<bool, OverrideFailure> {
        Ok(truth(&self.simulation_call(
            part,
            method,
            &[],
            Conversion::Value,
        )?))
    }

    /// `can_switch_online()` (3.3, 5.6): in the switch pass.
    pub fn can_switch_online(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.boolean_call(part, "can_switch_online")
    }

    /// `can_switch_offline()` (3.3, 5.6).
    pub fn can_switch_offline(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.boolean_call(part, "can_switch_offline")
    }

    /// `used_ai_locations()` (3.3, 5.6).
    pub fn used_ai_locations(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.boolean_call(part, "used_ai_locations")
    }

    /// `can_save()` (3.3, 5.6): which objects a save writes.
    pub fn can_save(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.boolean_call(part, "can_save")
    }

    /// `interactive()` (3.3, 5.6).
    pub fn interactive(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.boolean_call(part, "interactive")
    }

    /// `keep_saved_data_anyway()` (4.2, 5.6): the client-data rule's
    /// question.
    pub fn keep_saved_data_anyway(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.boolean_call(part, "keep_saved_data_anyway")
    }

    /// Whether `part` is a dynamic object (`cse_alife_dynamic_object`
    /// or derived from it), the classes with `on_spawn`,
    /// `on_before_register`, `on_register`, `on_unregister` and
    /// `keep_saved_data_anyway` (3.4): an object of another class gets none
    /// of these calls (our reading: it has no such method).
    fn dynamic(&self, part: EnginePart) -> Result<bool, Error> {
        let class = self.simulation_object(part, |o| o.class)?;
        Ok(classes::derives(class, "cse_alife_dynamic_object"))
    }

    /// `on_spawn()` (5.4): once for an object the simulation creates.
    pub fn on_spawn(&mut self, part: EnginePart) -> Result<(), OverrideFailure> {
        if !self.dynamic(part)? {
            return Ok(());
        }
        self.simulation_call(part, "on_spawn", &[], Conversion::Nothing)
            .map(drop)
    }

    /// `on_before_register()` (5.3): first in the object's registration.
    pub fn on_before_register(&mut self, part: EnginePart) -> Result<(), OverrideFailure> {
        if !self.dynamic(part)? {
            return Ok(());
        }
        self.simulation_call(part, "on_before_register", &[], Conversion::Nothing)
            .map(drop)
    }

    /// `on_register()` (5.3). `true` if the engine's own implementation
    /// ran: the engine then applies the client-data rule (sav.md 8), whose
    /// question is [`Scripts::keep_saved_data_anyway`]; asked after the
    /// call returns (our placement: the original asks within the base
    /// call).
    pub fn on_register(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        if !self.dynamic(part)? {
            return Ok(false);
        }
        self.simulation_object(part, |o| o.ran.on_register = false)?;
        self.simulation_call(part, "on_register", &[], Conversion::Nothing)?;
        Ok(self.simulation_object(part, |o| std::mem::take(&mut o.ran.on_register))?)
    }

    /// `on_unregister()` (5.5). The engine's own implementation removes the
    /// map spots of the object's id (believed); done after the call returns
    /// (our placement).
    pub fn on_unregister(&mut self, part: EnginePart) -> Result<(), OverrideFailure> {
        if !self.dynamic(part)? {
            return Ok(());
        }
        self.simulation_object(part, |o| o.ran.on_unregister = false)?;
        self.simulation_call(part, "on_unregister", &[], Conversion::Nothing)?;
        let (ran, id) =
            self.simulation_object(part, |o| (std::mem::take(&mut o.ran.on_unregister), o.id()))?;
        if ran && let Some(story) = self.registered_story() {
            remove_map_spots(&mut story.borrow_mut(), id);
        }
        Ok(())
    }

    /// `update()` (5.7): the scheduled update of a scheduled object. `true`
    /// if the engine's own implementation ran: a monster's or character's
    /// is offline 7.1's scheduled update, the simulation's to run.
    pub fn scheduled_object_update(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.simulation_object(part, |o| o.ran.update = false)?;
        self.simulation_call(part, "update", &[], Conversion::Nothing)?;
        Ok(self.simulation_object(part, |o| std::mem::take(&mut o.ran.update))?)
    }

    /// `bfUseful()` (3.10).
    pub fn useful(&mut self, part: EnginePart) -> Result<bool, OverrideFailure> {
        self.boolean_call(part, "bfUseful")
    }

    /// `g_team()`, `g_squad()`, `g_group()` (3.5): `method` one of them; the
    /// result taken as a byte (believed, 4.2: the low 8 bits of the
    /// integer, our choice).
    pub fn creature_byte(&mut self, part: EnginePart, method: &str) -> Result<u8, OverrideFailure> {
        let v = self.simulation_call(part, method, &[], Conversion::Integer)?;
        Ok(lua_number(&v) as i64 as u8)
    }

    /// The death of an online creature (5.8): the killer's id is stored in
    /// the victim's Creature block, then `on_death(killer)` gets the
    /// killer's simulation object; the engine's own stamps the death time
    /// with `game_time`.
    pub fn on_death(
        &mut self,
        victim: EnginePart,
        killer: Option<EnginePart>,
        game_time: u64,
    ) -> Result<(), OverrideFailure> {
        let killer_id = match killer {
            Some(k) => self.simulation_object(k, |o| o.id())?,
            None => 0xFFFF,
        };
        self.simulation_object(victim, |o| {
            if let Some(c) = o.record.as_mut().and_then(|r| r.object.creature_mut()) {
                c.killer_id = killer_id;
            }
        })?;
        self.simulation_objects()?.borrow_mut().game_time = game_time;
        self.simulation_call(
            victim,
            "on_death",
            &[object_or_nil(killer)],
            Conversion::Nothing,
        )
        .map(drop)
    }

    /// `detect_probability()` (3.8).
    pub fn detect_probability(&mut self, zone: EnginePart) -> Result<f32, OverrideFailure> {
        let v = self.simulation_call(zone, "detect_probability", &[], Conversion::Value)?;
        Ok(float(&v))
    }

    /// `smart_touch(obj)` (3.8).
    pub fn smart_touch(
        &mut self,
        zone: EnginePart,
        obj: EnginePart,
    ) -> Result<(), OverrideFailure> {
        self.simulation_call(
            zone,
            "smart_touch",
            &[Value::Object(obj)],
            Conversion::Nothing,
        )
        .map(drop)
    }

    /// `enabled(obj)` (3.8, 5.9): whether smart zone `zone` accepts `obj`.
    pub fn smart_zone_enabled(
        &mut self,
        zone: EnginePart,
        obj: EnginePart,
    ) -> Result<bool, OverrideFailure> {
        let v = self.simulation_call(zone, "enabled", &[Value::Object(obj)], Conversion::Value)?;
        Ok(truth(&v))
    }

    /// `suitable(obj)` (3.8, 5.9), kept as a single-precision float.
    pub fn smart_zone_suitable(
        &mut self,
        zone: EnginePart,
        obj: EnginePart,
    ) -> Result<f32, OverrideFailure> {
        let v = self.simulation_call(zone, "suitable", &[Value::Object(obj)], Conversion::Value)?;
        Ok(float(&v))
    }

    /// `register_npc(obj)` (3.8, 5.9).
    pub fn register_npc(
        &mut self,
        zone: EnginePart,
        obj: EnginePart,
    ) -> Result<(), OverrideFailure> {
        self.simulation_call(
            zone,
            "register_npc",
            &[Value::Object(obj)],
            Conversion::Nothing,
        )
        .map(drop)
    }

    /// `unregister_npc(obj)` (3.8).
    pub fn unregister_npc(
        &mut self,
        zone: EnginePart,
        obj: EnginePart,
    ) -> Result<(), OverrideFailure> {
        self.simulation_call(
            zone,
            "unregister_npc",
            &[Value::Object(obj)],
            Conversion::Nothing,
        )
        .map(drop)
    }

    /// `task(obj)` (3.8, 5.9): the task object as the call gave it (spec
    /// K's `CALifeSmartTerrainTask`), nil for none.
    pub fn smart_zone_task(
        &mut self,
        zone: EnginePart,
        obj: EnginePart,
    ) -> Result<Value, OverrideFailure> {
        self.simulation_call(zone, "task", &[Value::Object(obj)], Conversion::Value)
    }

    /// The New Game's or the load's objects created through the object
    /// factory (spec C 6.1; offline-simulation.md 8.1 step 3, 8.2), with
    /// spec J's calls: the server object of each object of spec K's
    /// simulation, held by the engine from then on, a script entry's
    /// initialised (2.2), an engine entry's the engine's own object of its
    /// kind's class ([`engine_class`]), each standing for the simulation's
    /// object (its state the simulation's). In creation order: a New Game's
    /// records in increasing spawn index, each followed by its supplies
    /// (8.1 step 3.4), a load's objects in increasing id. Each object made
    /// from a record (a New Game's spawn record, a save's record) reads its
    /// packet (`STATE_Read`, 5.1); each then gets `on_before_register` (its
    /// registration, 5.3); at a New Game each gets `on_spawn` (5.4), a
    /// supply right after its registration, a record once its supplies
    /// are made. Then the post-registration step (8.1 step 4, sav.md 11.2
    /// step 7): `on_register` for each in increasing id, the engine's own
    /// applying the client-data rule (sav.md 8) with the object's
    /// `keep_saved_data_anyway`. The core simulation has registered every
    /// object first (our placement: the original makes and registers each
    /// as it creates the object from its record). Objects already held are
    /// kept and get no calls. A script error in a call is fatal (spec A
    /// 5.3).
    pub fn create_simulation_objects(
        &mut self,
        system: &Config,
        creation: Creation,
    ) -> Result<(), OverrideFailure> {
        let ctx = self.simulation_context()?;
        let alife =
            (ctx.alife.clone()).ok_or_else(|| api("the simulation registry isn't registered"))?;
        // (group, a supply, id, section).
        let mut order: Vec<(u32, bool, u16, String)> = {
            let a = alife.borrow();
            let sim = (a.simulation.as_ref()).ok_or_else(|| api("there is no simulation"))?;
            // A supply's group is the spawn index of the record it was
            // created after: its outermost carrier's.
            let group = |mut id: u16| {
                for _ in 0..sim.len() {
                    let Some(o) = sim.object(id) else { break };
                    if let Some(index) = o.spawn_index {
                        return index;
                    }
                    id = o.record.header.parent_id;
                }
                u32::MAX
            };
            sim.objects()
                .map(|o| {
                    let section = o.record.header.section.to_string();
                    match creation {
                        Creation::NewGame => {
                            let supply = o.spawn_index.is_none();
                            (group(o.id), supply, o.id, section)
                        }
                        Creation::Load => (0, false, o.id, section),
                    }
                })
                .collect()
        };
        order.sort_unstable_by_key(|&(group, supply, id, _)| (group, supply, id));
        let mut made = Vec::new();
        // A New Game's record whose `on_spawn` waits for its supplies.
        let mut record_spawned = None;
        for (_, supply, id, section) in order {
            if ctx.state.borrow().held.contains_key(&id) {
                continue;
            }
            let held = self.new_server_object(system, &section)?;
            ctx.state.borrow_mut().held.insert(id, held);
            self.simulation_object(held.part, |o| {
                o.record = None;
                o.simulation = Some((id, Rc::clone(&alife)));
            })?;
            if !supply {
                if let Some(part) = record_spawned.take() {
                    self.on_spawn(part)?;
                }
                let record = {
                    let a = alife.borrow();
                    let sim = a.simulation.as_ref().expect("checked");
                    sim.object(id).map(|o| o.record.clone())
                };
                let record = record.ok_or_else(|| api("the simulation has no such object"))?;
                self.read_state(held.part, record)?;
            }
            self.on_before_register(held.part)?;
            if creation == Creation::NewGame {
                if supply {
                    self.on_spawn(held.part)?;
                } else {
                    record_spawned = Some(held.part);
                }
            }
            made.push((id, held.part));
        }
        if let Some(part) = record_spawned {
            self.on_spawn(part)?;
        }
        made.sort_unstable_by_key(|&(id, _)| id);
        for (id, part) in made {
            if self.on_register(part)? {
                let keeps = self.keep_saved_data_anyway(part)?;
                if let Some(sim) = alife.borrow_mut().simulation.as_mut() {
                    sim.client_data_rule(id, keeps);
                }
            }
        }
        Ok(())
    }

    /// The server object of configuration section `section` (spec C 6.1):
    /// through the factory, a script entry's object initialised (2.2), an
    /// engine entry's the engine's own object of its kind's class. An
    /// entry of no simulation kind is fatal (the original asserts "Can't
    /// create entity.").
    fn new_server_object(&mut self, system: &Config, section: &str) -> Result<Held, Error> {
        let created = self.create_server_object(system, section)?;
        Ok(match created.object {
            CreatedObject::Script(part) => Held {
                part,
                scripted: true,
            },
            CreatedObject::Engine { entry_id, .. } => {
                let text = cp1251::decode(id_text(entry_id).trim_ascii_end());
                let Ok(kind) = Kind::from_class_id(&text) else {
                    let message = "Can't create entity.".to_owned();
                    *self.fatal.borrow_mut() = Some(message.clone());
                    return Err(Error {
                        kind: ErrorKind::Fatal,
                        message,
                    });
                };
                let class = engine_class(&text, kind);
                Held {
                    part: self.new_simulation_object(class, &game_bytes(section))?,
                    scripted: false,
                }
            }
        })
    }

    /// The server object the engine holds for simulation object `record`
    /// (its id), for the online object spawning from it to get (spec M
    /// 10.2, spec L 5.1): the one [`Scripts::create_simulation_objects`]
    /// made, or one made now as it makes them; its state the simulation's
    /// object's when spec K's simulation holds it, else `record`: read by a
    /// new one (`STATE_Read`, 5.1), set on a held one each time. (Spec K's
    /// objects get theirs from [`Scripts::create_simulation_objects`]; one
    /// made here gets no registration calls, TODO(#1253).)
    pub fn server_object_for(
        &mut self,
        system: &Config,
        record: &ObjectRecord,
    ) -> Result<EnginePart, OverrideFailure> {
        let ctx = self.simulation_context()?;
        let id = record.header.id;
        let held = ctx.state.borrow().held.get(&id).copied();
        let part = match held {
            Some(h) => h.part,
            None => {
                let held = self.new_server_object(system, record.header.section.as_str())?;
                ctx.state.borrow_mut().held.insert(id, held);
                held.part
            }
        };
        // The simulation's own object, when spec K's simulation holds it:
        // its state is that one's; otherwise a copy of `record`.
        let live = ctx.alife.as_ref().filter(|a| {
            a.borrow()
                .simulation
                .as_ref()
                .is_some_and(|s| s.object(id).is_some())
        });
        match live {
            Some(alife) => {
                let alife = Rc::clone(alife);
                self.simulation_object(part, |o| {
                    o.record = None;
                    o.simulation = Some((id, alife));
                })?;
            }
            // A new one reads it (5.1).
            None if held.is_none() => self.read_state(part, record.clone())?,
            None => self.set_simulation_state(part, record.clone())?,
        }
        Ok(part)
    }

    /// The engine destroyed objects `removed` (they left the simulation),
    /// and spec K's `release` took others from it: the server objects held
    /// for them are released (a script entry's with
    /// [`Scripts::release_object`], spec C 6.2). Ids without one are
    /// skipped.
    pub fn release_simulation_objects(&mut self, removed: &[u16]) -> Result<(), Error> {
        let Ok(state) = self.simulation_objects() else {
            return Ok(());
        };
        let held: Vec<Held> = {
            let mut s = state.borrow_mut();
            let mut ids = std::mem::take(&mut s.released);
            ids.extend_from_slice(removed);
            ids.iter().filter_map(|id| s.held.remove(id)).collect()
        };
        for h in held {
            self.release_held(h)?;
        }
        Ok(())
    }

    fn release_held(&mut self, h: Held) -> Result<(), Error> {
        if h.scripted {
            self.release_object(h.part)?;
        } else {
            self.lua_mut().release_engine_object(h.part)?;
        }
        Ok(())
    }

    /// The simulation is destroyed (the level is left or a New Game starts
    /// over it): each server object the engine holds, in increasing id
    /// (the registry's order, offline-simulation.md 6.1), gets its
    /// `on_unregister` (5.5) and is released, while the simulation still
    /// holds its object. Those of objects that already left it (spec K's
    /// `release`) are released without the call (TODO(#1252)). A script
    /// error in a call is fatal (spec A 5.3).
    pub fn release_server_objects(&mut self) -> Result<(), OverrideFailure> {
        let Ok(state) = self.simulation_objects() else {
            return Ok(());
        };
        let (held, gone): (Vec<(u16, Held)>, Vec<u16>) = {
            let mut s = state.borrow_mut();
            let gone = std::mem::take(&mut s.released);
            let mut held: Vec<(u16, Held)> = s.held.drain().collect();
            held.sort_unstable_by_key(|&(id, _)| id);
            (held, gone)
        };
        for (id, h) in held {
            if !gone.contains(&id) {
                self.on_unregister(h.part)?;
            }
            self.release_held(h)?;
        }
        Ok(())
    }
}

/// The engine's `on_unregister` (5.5): the map spots of object `id` go.
fn remove_map_spots(story: &mut Story, id: u16) {
    story.map_spots.spots.retain(|s| s.object != id);
    if story
        .map_spots
        .active
        .as_ref()
        .is_some_and(|(object, _)| *object == id)
    {
        story.map_spots.active = None;
    }
}
