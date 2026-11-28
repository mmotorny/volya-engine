// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Class ids and the object factory (script API C,
//! docs/behavior/script-api-class-ids-and-the-object-factory.md; section
//! numbers are its): class ids made from id texts (1), the factory's
//! entries, the engine's and the scripts' (2, 3), the `class_registrators`
//! call (4), the `clsid` table (5) and creating objects through the factory
//! (6). See docs/architecture/scripts.md, The object factory.
//!
//! The entries live in Rust ([`ObjectFactory`]); the class objects of the
//! scripts' entries, the factory object and `register` live in Lua
//! (object_factory.lua), which [`Scripts`] runs at its creation.

use std::rc::Rc;

use volya_engine_core::config::Config;
use volya_engine_core::simulation::class_of;
use volya_engine_formats::cp1251;

use crate::classes::{EngineClass, EnginePart};
use crate::loader::{Common, Scripts};
use crate::{Error, ErrorKind, Lua, Value};

/// The class id of id text `text` (1): its first 8 bytes, padded with
/// spaces, read as a big-endian number, each byte sign-extended before it
/// is combined (a byte of 80–FF makes every more significant byte FF).
pub fn class_id(text: &[u8]) -> u64 {
    let mut bytes = [b' '; 8];
    for (slot, &b) in bytes.iter_mut().zip(text) {
        *slot = b;
    }
    bytes.iter().enumerate().fold(0, |id, (i, &b)| {
        id | ((i64::from(b as i8) as u64) << (8 * (7 - i)))
    })
}

/// The id text of class id `id` (1): its 8 bytes, most significant first,
/// with their padding.
pub fn id_text(id: u64) -> [u8; 8] {
    id.to_be_bytes()
}

/// Whether the lowercased command line contains `-keep_lua` (2.3): then
/// creating the simulation keeps the script state and its factory.
pub fn keeps_lua(command_line: &str) -> bool {
    command_line.to_ascii_lowercase().contains("-keep_lua")
}

/// The sides of an engine entry (2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sides {
    ClientAndServer,
    /// Client and server, each side's kind chosen by the game type
    /// ([`GameTypeKind`]): `O_ACTOR`.
    ByGameType,
    ClientOnly,
    ServerOnly,
}

impl Sides {
    fn has(self, side: Side) -> bool {
        !matches!(
            (self, side),
            (Sides::ClientOnly, Side::Server) | (Sides::ServerOnly, Side::Client)
        )
    }
}

/// Which kind an entry of [`Sides::ByGameType`] creates (2.2): one for the
/// current game-type value 0 or 1, another for any other value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameTypeKind {
    ZeroOrOne,
    Other,
}

impl GameTypeKind {
    pub fn of(game_type: u32) -> Self {
        if game_type <= 1 {
            GameTypeKind::ZeroOrOne
        } else {
            GameTypeKind::Other
        }
    }
}

/// The two forms of `object_factory:register` (3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// `register(client class, server class, id text, class-id name)`.
    FourArguments,
    /// `register(client class, id text, class-id name)`: the client class
    /// is both sides.
    ThreeArguments,
}

/// What made an entry and what its sides create (2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Objects of fixed engine kinds.
    Engine(Sides),
    /// Class objects to call, kept in Lua under `slot`.
    Script { form: Form, slot: usize },
}

/// A factory entry (2.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    /// The class-id name (bytes, as Lua strings are).
    pub name: Vec<u8>,
    pub source: Source,
}

impl Entry {
    /// The id text, without its padding.
    pub fn id_text(&self) -> String {
        let text = id_text(self.id);
        cp1251::decode(text.trim_ascii_end())
    }
}

use Sides::{ByGameType, ClientAndServer as B, ClientOnly as Client, ServerOnly as Server};

/// The engine's entries (2.2; ids, names and sides as 5.3's "engine" rows).
/// The 26 entries a dedicated server adds aren't covered (2.2).
pub const ENGINE_ENTRIES: [(&str, &str, Sides); 134] = [
    ("AF_BAST", "art_bast_artefact", B),
    ("AF_BDROP", "art_black_drops", B),
    ("AF_BGRAV", "art_gravi_black", B),
    ("AF_DUMMY", "art_dummy", B),
    ("AF_EBALL", "art_electric_ball", B),
    ("AF_FBALL", "art_faded_ball", B),
    ("AF_GALAN", "art_galantine", B),
    ("AF_GRAVI", "art_gravi", B),
    ("AF_MBALL", "art_mercury_ball", B),
    ("AF_NEEDL", "art_needles", B),
    ("AF_RHAIR", "art_rusty_hair", B),
    ("AF_THORN", "art_thorn", B),
    ("AF_ZUDA", "art_zuda", B),
    ("AI_BLOOD", "bloodsucker", B),
    ("AI_BOAR", "boar", B),
    ("AI_BURER", "burer", B),
    ("AI_CAT", "cat", B),
    ("AI_CONTR", "controller", B),
    ("AI_CROW", "crow", B),
    ("AI_DOG_B", "dog_black", B),
    ("AI_DOG_F", "psy_dog_phantom", B),
    ("AI_DOG_P", "psy_dog", B),
    ("AI_DOG_R", "dog_red", B),
    ("AI_FLESH", "flesh", B),
    ("AI_FLE_G", "flesh_group", Server),
    ("AI_FRACT", "fracture", B),
    ("AI_GIANT", "pseudo_gigant", B),
    ("AI_GRAPH", "graph_point", Server),
    ("AI_HIMER", "chimera", B),
    ("AI_PHANT", "phantom", B),
    ("AI_POLTR", "poltergeist", B),
    ("AI_SNORK", "snork", B),
    ("AI_STL", "stalker", B),
    ("AI_TRADE", "trader", B),
    ("AI_TUSH", "tushkano", B),
    ("AI_ZOM", "zombie", B),
    ("AMMO", "wpn_ammo", B),
    ("ARTEFACT", "artefact", B),
    ("A_M209", "wpn_ammo_m209", B),
    ("A_OG7B", "wpn_ammo_og7b", B),
    ("A_VOG25", "wpn_ammo_vog25", B),
    ("CL_AHUNT", "game_cl_artefact_hunt", Client),
    ("CL_DM", "game_cl_deathmatch", Client),
    ("CL_SINGL", "game_cl_single", Client),
    ("CL_TDM", "game_cl_team_deathmatch", Client),
    ("C_HLCPTR", "helicopter", B),
    ("C_NIVA", "car", B),
    ("D_PDA", "device_pda", B),
    ("D_SIMDET", "device_detector_simple", B),
    ("D_TORCH", "device_torch", B),
    ("EQU_EXO", "equ_exo", B),
    ("EQU_MLTR", "equ_military", B),
    ("EQU_SCIE", "equ_scientific", B),
    ("EQU_STLK", "equ_stalker", B),
    ("G_F1", "wpn_grenade_f1", B),
    ("G_FAKE", "wpn_grenade_fake", B),
    ("G_LEVEL", "level", Client),
    ("G_PERSIS", "game", Client),
    ("G_RGD5", "wpn_grenade_rgd5", B),
    ("G_RPG7", "wpn_grenade_rpg7", B),
    ("HUD_MNGR", "hud_manager", Client),
    ("II_ANTIR", "obj_antirad", B),
    ("II_ATTCH", "obj_attachable", B),
    ("II_BANDG", "obj_bandage", B),
    ("II_BOLT", "obj_bolt", B),
    ("II_BOTTL", "obj_bottle", B),
    ("II_DOC", "obj_document", B),
    ("II_EXPLO", "obj_explosive", B),
    ("II_FOOD", "obj_food", B),
    ("II_MEDKI", "obj_medkit", B),
    ("LVL_CHNG", "level_changer", B),
    ("MP_PLBAG", "mp_players_bag", B),
    ("ON_OFF_G", "online_offline_group", Server),
    ("O_ACTOR", "actor", ByGameType),
    ("O_BRKBL", "obj_breakable", B),
    ("O_CLMBL", "obj_climable", B),
    ("O_HLAMP", "hanging_lamp", B),
    ("O_INVBOX", "inventory_box", B),
    ("O_PHYSIC", "obj_physic", B),
    ("O_SEARCH", "projector", B),
    ("P_DSTRBL", "obj_phys_destroyable", B),
    ("P_SKELET", "obj_phskeleton", B),
    ("SCRIPTZN", "script_zone", B),
    ("SCRPTOBJ", "script_object", B),
    ("SMRTZONE", "smart_zone", B),
    ("SPACE_RS", "space_restrictor", B),
    ("SPECT", "spectator", B),
    ("SV_AHUNT", "game_sv_artefact_hunt", Client),
    ("SV_DM", "game_sv_deathmatch", Client),
    ("SV_SINGL", "game_sv_single", Client),
    ("SV_TDM", "game_sv_team_deathmatch", Client),
    ("UI_AHUNT", "game_ui_artefact_hunt", Client),
    ("UI_DM", "game_ui_deathmatch", Client),
    ("UI_SINGL", "game_ui_single", Client),
    ("UI_TDM", "game_ui_team_deathmatch", Client),
    ("W_AK74", "wpn_ak74", B),
    ("W_BINOC", "wpn_binocular", B),
    ("W_BM16", "wpn_bm16", B),
    ("W_FN2000", "wpn_fn2000", B),
    ("W_FORT", "wpn_fort", B),
    ("W_GLAUNC", "wpn_grenade_launcher", B),
    ("W_GROZA", "wpn_groza", B),
    ("W_HPSA", "wpn_hpsa", B),
    ("W_KNIFE", "wpn_knife", B),
    ("W_LR300", "wpn_lr300", B),
    ("W_MOUNTD", "wpn_mounted", B),
    ("W_PM", "wpn_pm", B),
    ("W_RG6", "wpn_rg6", B),
    ("W_RPG7", "wpn_rpg7", B),
    ("W_SCOPE", "wpn_scope", B),
    ("W_SHOTGN", "wpn_shotgun", B),
    ("W_SILENC", "wpn_silencer", B),
    ("W_STMGUN", "wpn_stat_mgun", B),
    ("W_SVD", "wpn_svd", B),
    ("W_SVU", "wpn_svu", B),
    ("W_USP45", "wpn_usp45", B),
    ("W_VAL", "wpn_val", B),
    ("W_VINT", "wpn_vintorez", B),
    ("W_WALTHR", "wpn_walther", B),
    ("W_WMAGAZ", "wpn_wmagaz", B),
    ("W_WMAGGL", "wpn_wmaggl", B),
    ("Z_ACIDF", "zone_acid_fog", B),
    ("Z_AMEBA", "ameba_zone", B),
    ("Z_BFUZZ", "zone_bfuzz", B),
    ("Z_DEAD", "zone_dead", B),
    ("Z_GALANT", "zone_galantine", B),
    ("Z_MBALD", "zone_mosquito_bald", B),
    ("Z_MINCER", "zone_mincer", B),
    ("Z_NOGRAV", "nogravity_zone", B),
    ("Z_RADIO", "zone_radioactive", B),
    ("Z_RUSTYH", "zone_rusty_hair", B),
    ("Z_TEAMBS", "team_base_zone", B),
    ("Z_TORRID", "torrid_zone", B),
    ("Z_ZONE", "zone", B),
];

/// The object factory's table of entries (2.1, 2.4).
#[derive(Clone, Debug)]
pub struct ObjectFactory {
    entries: Vec<Entry>,
    sorted: bool,
    scripts: usize,
}

impl Default for ObjectFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectFactory {
    /// A new factory with the engine's entries (2.2, 2.3).
    pub fn new() -> Self {
        let mut factory = ObjectFactory {
            entries: Vec::new(),
            sorted: true,
            scripts: 0,
        };
        for (text, name, sides) in ENGINE_ENTRIES {
            factory.add(Entry {
                id: class_id(text.as_bytes()),
                name: name.as_bytes().to_vec(),
                source: Source::Engine(sides),
            });
        }
        factory
    }

    /// Appends `entry` and marks the table unsorted (2.1): an id or a name
    /// already present is kept beside the earlier entry.
    pub fn add(&mut self, entry: Entry) {
        self.entries.push(entry);
        self.sorted = false;
    }

    /// Adds a script's entry (3.2): its class objects are kept in Lua under
    /// the slot returned.
    fn add_script(&mut self, text: &[u8], name: &[u8], form: Form) -> usize {
        self.scripts += 1;
        let slot = self.scripts;
        self.add(Entry {
            id: class_id(text),
            name: name.to_vec(),
            source: Source::Script { form, slot },
        });
        slot
    }

    /// The entries in order (2.4): by class id, ascending as unsigned
    /// numbers; entries of equal ids in the order they were added. Sorts
    /// first if the table is marked unsorted.
    pub fn entries(&mut self) -> &[Entry] {
        if !self.sorted {
            self.entries.sort_by_key(|e| e.id);
            self.sorted = true;
        }
        &self.entries
    }

    /// The number of class id `id` (2.4): the position of the first entry
    /// whose id isn't below it; the entry count if there is none.
    pub fn number(&mut self, id: u64) -> usize {
        self.entries().partition_point(|e| e.id < id)
    }

    /// The entry for class id `id` (2.4): the first entry whose id isn't
    /// below it, whether or not it is `id`'s; `None` past the end (where
    /// the original's behaviour is undefined).
    pub fn entry_for(&mut self, id: u64) -> Option<&Entry> {
        let n = self.number(id);
        self.entries().get(n)
    }
}

/// A side of an entry: what creating a client or a server object of its id
/// does (2.1, 6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Client,
    Server,
}

impl Side {
    fn lua(self) -> &'static str {
        match self {
            Side::Client => "client",
            Side::Server => "server",
        }
    }
}

/// An object created through the factory (6.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Created {
    /// The class id the new object records: the one asked for (for a
    /// server object, its section's `class`), which an unregistered id's
    /// entry doesn't have (2.4).
    pub class_id: u64,
    pub object: CreatedObject,
}

/// What the entry created (6.1, 6.2).
#[derive(Clone, Debug, PartialEq)]
pub enum CreatedObject {
    /// An object of the engine kind of entry `entry_id` (2.2): the caller
    /// makes it (kinds are outside this spec; server kinds are those of
    /// sav.md 7.4.2 by class id). For [`Sides::ByGameType`], the kind of
    /// [`GameTypeKind::of`] the current game type.
    Engine { entry_id: u64, sides: Sides },
    /// The engine part of the script object the entry's class made (6.2),
    /// now the engine's: [`Scripts::release_object`] releases it. The
    /// caller runs the object's construction or initialisation (specs J,
    /// M).
    Script(EnginePart),
}

/// The fatal error of a check the original asserts or reports (6.1, 6.2).
fn fatal(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::Fatal,
        message: message.into(),
    }
}

/// The registry key of the factory's entry function (object_factory.lua).
pub(crate) const FACTORY_ENTRY: &str = "volya.object_factory";

/// Sets up the factory's Lua side on a state whose loader is installed:
/// the engine class `object_factory` (no constructor; `register` is its
/// only member, 3.1) and the factory object. `factory` is the state's
/// factory, made on first use (2.3).
pub(crate) fn install(
    lua: &mut Lua,
    factory: Rc<std::cell::RefCell<Option<ObjectFactory>>>,
) -> Result<(), Error> {
    lua.register_class(EngineClass::new("object_factory"))?;
    lua.open_factory_tools("__volya_factory_tools")?;
    lua.register_kept_factory("__volya_kept")?;
    lua.register("__volya_factory_add", move |args| {
        let [
            Value::String(text),
            Value::String(name),
            Value::Number(form),
        ] = args
        else {
            return Err(format!("entry expected, got {args:?}"));
        };
        let form = if *form == 4.0 {
            Form::FourArguments
        } else {
            Form::ThreeArguments
        };
        let mut factory = factory.borrow_mut();
        let slot = factory
            .get_or_insert_with(ObjectFactory::new)
            .add_script(text, name, form);
        Ok(vec![Value::Number(slot as f64)])
    })?;
    lua.exec_kept(
        "=object_factory",
        include_bytes!("object_factory.lua"),
        FACTORY_ENTRY,
    )
}

/// The engine class `clsid` (5.1): no constructor, no methods, a constant
/// per entry in order, its name with its position as the value. Names that
/// aren't UTF-8 are read as Windows-1251 (our choice; no shipped name has
/// a byte above 7F).
pub fn clsid_class(factory: &mut ObjectFactory) -> EngineClass {
    factory
        .entries()
        .iter()
        .enumerate()
        .filter_map(|(position, e)| {
            let name = match std::str::from_utf8(&e.name) {
                Ok(name) => name.to_owned(),
                Err(_) => cp1251::decode(&e.name),
            };
            // A name with a NUL byte can't be a constant (our choice).
            (!name.contains('\0')).then_some((name, position as i32))
        })
        .fold(EngineClass::new("clsid"), |class, (name, value)| {
            class.constant(&name, value)
        })
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => cp1251::decode(s),
        v => format!("{v:?}"),
    }
}

impl Scripts {
    /// The state's factory, created with the engine's entries on first use
    /// (2.3).
    pub fn object_factory(&mut self) -> std::cell::RefMut<'_, ObjectFactory> {
        std::cell::RefMut::map(self.factory.borrow_mut(), |f| {
            f.get_or_insert_with(ObjectFactory::new)
        })
    }

    /// The `class_registrators` call of startup (4), after `_G` is loaded:
    /// each of `common`'s registrators, its namespace loaded first, that
    /// names a function is called with the factory object; the others are
    /// skipped silently. A script error in a call is fatal (`LUA error:
    /// <text>`), and stops the state. Then the global `clsid` is built
    /// (5.1); loading `common`'s scripts is the caller's next step.
    pub fn call_class_registrators(&mut self, common: &Common) -> Result<(), Error> {
        drop(self.object_factory());
        let mut args = vec![Value::from("registrators")];
        args.extend(
            common
                .class_registrators
                .iter()
                .map(|n| Value::String(cp1251::encode(n).unwrap_or_else(|| n.as_bytes().to_vec()))),
        );
        let results = self.guarded(|lua| lua.call_kept(FACTORY_ENTRY, &args))?;
        if let [Value::String(outcome), message, ..] = results.as_slice()
            && outcome == b"error"
        {
            return Err(self.stop(format!("LUA error: {}", text_of(message))));
        }
        let class = clsid_class(&mut self.object_factory());
        self.register_class(class)
    }

    /// Stops the state with fatal error `message`, as the original stops
    /// the game.
    fn stop(&mut self, message: String) -> Error {
        *self.fatal.borrow_mut() = Some(message.clone());
        fatal(message)
    }

    /// Creates the client object of class id `id` (6.1): the entry for the
    /// id creates its client side. Fatal (stopping the state) when the
    /// entry has no client side, when a script entry's call fails or makes
    /// no object, and (our choice) when no entry is at or after the id.
    pub fn create_client_object(&mut self, id: u64) -> Result<Created, Error> {
        self.create(id, Side::Client, None)
    }

    /// Creates the server object of configuration section `section` (6.1):
    /// its `class` key's id, converted (1), finds the entry, whose server
    /// side creates the object for the section. Fatal as
    /// [`Scripts::create_client_object`] is; `Err` without stopping when
    /// the section has no `class` key.
    pub fn create_server_object(
        &mut self,
        system: &Config,
        section: &str,
    ) -> Result<Created, Error> {
        let class = class_of(system, section).map_err(|e| Error {
            kind: ErrorKind::Api,
            message: e.to_string(),
        })?;
        let text = cp1251::encode(class).unwrap_or_else(|| class.as_bytes().to_vec());
        self.create(class_id(&text), Side::Server, Some(section))
    }

    fn create(&mut self, id: u64, side: Side, section: Option<&str>) -> Result<Created, Error> {
        if let Some(message) = self.fatal.borrow().clone() {
            return Err(fatal(message));
        }
        let Some(entry) = self.object_factory().entry_for(id).cloned() else {
            let text = cp1251::decode(id_text(id).trim_ascii_end());
            return Err(self.stop(format!("no factory entry for class id {text}")));
        };
        let created = |object| Created {
            class_id: id,
            object,
        };
        let slot = match entry.source {
            Source::Engine(sides) if sides.has(side) => {
                return Ok(created(CreatedObject::Engine {
                    entry_id: entry.id,
                    sides,
                }));
            }
            Source::Engine(_) => {
                let side = side.lua();
                return Err(self.stop(format!(
                    "Cannot instantiate {side} object, because {side} class is not declared!"
                )));
            }
            Source::Script { slot, .. } => slot,
        };
        let mut args = vec![
            Value::from("create"),
            Value::Number(slot as f64),
            side.lua().into(),
        ];
        if let Some(section) = section {
            args.push(Value::String(
                cp1251::encode(section).unwrap_or_else(|| section.as_bytes().to_vec()),
            ));
        }
        let results = self.guarded(|lua| lua.call_kept(FACTORY_ENTRY, &args))?;
        match results.as_slice() {
            [Value::String(o), Value::Number(part), ..] if o == b"created" => Ok(created(
                CreatedObject::Script(EnginePart::from_value(*part)),
            )),
            [Value::String(o), message, ..] if o == b"error" => {
                Err(self.stop(format!("LUA error: {}", text_of(message))))
            }
            [Value::String(o), type_name, ..] if o == b"cast" => Err(self.stop(format!(
                "LUA error: cannot cast lua value to {}",
                text_of(type_name)
            ))),
            // Our wording: the original asserts (6.2).
            [Value::String(o), ..] if o == b"missing" => Err(self.stop(format!(
                "the {} class of {} made no object",
                side.lua(),
                entry.id_text()
            ))),
            other => Err(Error {
                kind: ErrorKind::Api,
                message: format!("unexpected creation outcome {other:?}"),
            }),
        }
    }

    /// Releases an object [`Scripts::create_client_object`] or
    /// [`Scripts::create_server_object`] made from a script entry: its
    /// engine part, and the script object the engine held for it. `false`
    /// if the engine didn't hold it.
    pub fn release_object(&mut self, part: EnginePart) -> Result<bool, Error> {
        let results = self.guarded(|lua| {
            lua.call_kept(FACTORY_ENTRY, &["release".into(), part.value().into()])
        })?;
        Ok(results.first() == Some(&Value::Boolean(true)))
    }
}
