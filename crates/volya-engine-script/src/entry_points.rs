// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The engine's entry points into scripts and the New Game call order
//! (script API D, docs/behavior/script-api-engine-entry-points.md; section
//! numbers are its): finding a script function by name and calling it (1),
//! startup's `[common] script` list (2.1), the game-type function (2.2), the
//! start callback (2.3), the tutorial thresholds (2.4) and the weapon
//! parameters (2.5), on a [`GameScripts`] that keeps the game state across
//! its rebuilds and records each call. See docs/architecture/scripts.md,
//! Entry points.
//!
//! The Lua side (entry_points.lua) resolves and calls; Rust keeps the order
//! and turns failures into the fatal errors the original stops the game
//! with.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::{Config, PathAliases};
use volya_engine_formats::cp1251;
use volya_engine_formats::ltx::value;

use crate::classes::EngineClass;
use crate::dialogs::DialogsAndTrade;
use crate::effects_and_weather::EffectsAndWeather;
use crate::files::FileSystem;
use crate::game_object::GameObjects;
use crate::level::LevelState;
use crate::loader::{Common, Prefix, Scripts, Trees};
use crate::object_factory::{CreatedObject, class_id, id_text, keeps_lua};
use crate::services::EngineServices;
use crate::simulation::AlifeSimulator;
use crate::sound::ScriptSounds;
use crate::story::Story;
use crate::time::GameTimes;
use crate::window_files::WindowFiles;
use crate::{Error, ErrorKind, Lua, Value};

/// The registry key of the entry points' function (entry_points.lua).
const ENTRY: &str = "volya.entry_points";

/// The engine bases of script API A (its 1.3) the shipped scripts derive
/// from. Startup registers each one no spec has registered yet as an engine
/// class without members, so the scripts deriving from it load (`_G`'s
/// `pp_effector` derives from `effector`, and without it `_g.script` stops
/// before `start_game_callback`). TODO(#1022): each is replaced by its
/// spec's class.
pub const STAND_IN_BASES: [&str; 33] = [
    "property_evaluator",
    "action_base",
    "CUIScriptWnd",
    "object_binder",
    "effector",
    "CUIWindow",
    "CUIListItemEx",
    "cse_alife_item",
    "cse_alife_smart_zone",
    "game_sv_mp_script",
    "CGameObject",
    "CSE_AbstractVisual",
    "UIGame_custom_script",
    "cse_alife_car",
    "cse_alife_helicopter",
    "cse_alife_human_stalker",
    "cse_alife_item_artefact",
    "cse_alife_item_custom_outfit",
    "cse_alife_item_torch",
    "cse_alife_item_weapon",
    "cse_alife_item_weapon_magazined",
    "cse_alife_item_weapon_magazined_w_gl",
    "cse_alife_item_weapon_shotgun",
    "cse_alife_monster_base",
    "cse_alife_object_physic",
    "cse_alife_space_restrictor",
    "cse_alife_trader",
    "cse_anomalous_zone",
    "cse_zone_visual",
    "game_PlayerState",
    "game_cl_mp_script",
    "action_planner",
    "game_sv_Deathmatch",
];

/// The key of `[common]` naming the game-type function (2.2).
pub const GAME_TYPE_FUNCTION_KEY: &str = "game_type_clsid_factory";

/// The settings' section and key naming the start callback (2.3).
pub const START_CALLBACK: (&str, &str) = ("alife", "start_game_callback");

/// One of the engine's calls into scripts, as [`GameScripts`] records them.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineCall {
    /// Startup (3.1 step 2, C 2.3): `_G` loaded, the class registrators
    /// listed called (those that resolve), and the `<item>_initialize`
    /// functions of the `[common] script` list that were called (2.1).
    Startup {
        registrators: Vec<String>,
        initialized: Vec<String>,
    },
    /// The game-type function `function` asked for the game type
    /// `game_type`'s server (`server`) or client class id (2.2), and the
    /// text it returned.
    GameType {
        function: String,
        game_type: String,
        server: bool,
        result: String,
    },
    /// The script state rebuilt as the simulation is created (2.3 step 1):
    /// the old state closed; the new one's [`EngineCall::Startup`] follows.
    Rebuild,
    /// The start callback `function` (2.3 step 4).
    StartGameCallback(String),
    /// A tutorial-threshold function (2.4).
    Tutorial(&'static str),
    /// The weapon-parameter functions for an item section (2.5).
    WeaponParameters(String),
}

/// What startup makes and registers a game state from: the game's trees
/// and path aliases, the namespace prefix, and the engine state the
/// interface's names read (script APIs B, F, H, M, P, R, K).
pub struct Startup {
    pub trees: Rc<dyn Trees>,
    pub aliases: PathAliases,
    pub prefix: Prefix,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub objects: Rc<RefCell<GameObjects>>,
    pub story: Rc<RefCell<Story>>,
    pub dialogs: Rc<RefCell<DialogsAndTrade>>,
    pub times: Rc<RefCell<GameTimes>>,
    /// The sounds the scripts make (script API T).
    pub sounds: Rc<RefCell<ScriptSounds>>,
    pub simulation: Rc<RefCell<AlifeSimulator>>,
    /// The window description files' state (script API G3).
    pub window_files: Rc<RefCell<WindowFiles>>,
    pub level: Rc<RefCell<LevelState>>,
    /// The player's screen effects and the environment (script API U).
    pub effects_and_weather: Rc<RefCell<EffectsAndWeather>>,
}

impl Startup {
    /// The `[common]` lists of `$game_config$script.ltx`.
    fn common(&self) -> Result<Common, Error> {
        let mut common = Err(api("the game's files are unavailable"));
        let lent = self.trees.with_trees(&mut |tree, _| {
            common = Common::read(tree, &self.aliases).map_err(|e| api(e.to_string()));
        });
        lent.map_err(|e| api(e.to_string()))?;
        common
    }

    /// Startup (3.1 step 2; C 2.3, 5.1; 2.1): a new game state, the engine
    /// interface registered, `_G` loaded, the class registrators called,
    /// the `clsid` table built, the `[common] script` list processed.
    fn run(&self) -> Result<(Scripts, Common, EngineCall), Error> {
        let common = self.common()?;
        let mut scripts = Scripts::new(Rc::clone(&self.trees), &self.aliases, self.prefix)?;
        self.register_interface(&mut scripts)?;
        install(scripts.lua_mut())?;
        register_stand_ins(&mut scripts)?;
        scripts.load_globals()?;
        scripts.call_class_registrators(&common)?;
        let initialized = scripts.initialize(&common.scripts)?;
        let call = EngineCall::Startup {
            registrators: common.class_registrators.clone(),
            initialized,
        };
        Ok((scripts, common, call))
    }

    /// The engine interface of the specs implemented (B, F, H, M, N, P, R,
    /// T, Y, K, J, V, G3, S, U, Q, W; A, C and E [`Scripts::new`]
    /// registers).
    fn register_interface(&self, scripts: &mut Scripts) -> Result<(), Error> {
        scripts.register_engine_services(Rc::clone(&self.services))?;
        scripts
            .register_configuration_and_files(Rc::clone(&self.files), Rc::clone(&self.services))?;
        scripts.register_game_object_basics(
            Rc::clone(&self.objects),
            Rc::clone(&self.services),
            Rc::clone(&self.files),
        )?;
        scripts.register_physics_vehicles_and_lamps()?;
        scripts.register_story(Rc::clone(&self.story))?;
        scripts.register_dialogs_and_trade(Rc::clone(&self.dialogs))?;
        scripts.register_time(Rc::clone(&self.services), Rc::clone(&self.times))?;
        scripts.register_sound(Rc::clone(&self.sounds), Rc::clone(&self.trees))?;
        scripts.register_simulation(Rc::clone(&self.simulation))?;
        scripts.register_simulation_objects()?;
        scripts.register_scripted_actions()?;
        scripts.register_window_files(
            Rc::clone(&self.services),
            Rc::clone(&self.window_files),
            Rc::clone(&self.trees),
            self.aliases.clone(),
        )?;
        scripts.register_inventory()?;
        scripts.register_relations()?;
        scripts.register_action_planner()?;
        scripts.register_level(Rc::clone(&self.level))?;
        scripts.register_effects_and_weather(Rc::clone(&self.effects_and_weather))
    }
}

fn api(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::Api,
        message: message.into(),
    }
}

/// Runs the entry points' chunk on a state whose loader is installed.
pub(crate) fn install(lua: &mut Lua) -> Result<(), Error> {
    lua.register_kept_factory("__volya_kept")?;
    lua.exec_kept("=entry_points", include_bytes!("entry_points.lua"), ENTRY)
}

/// Registers the [`STAND_IN_BASES`] not yet defined.
fn register_stand_ins(scripts: &mut Scripts) -> Result<(), Error> {
    let mut args = vec![Value::from("undefined")];
    args.extend(STAND_IN_BASES.iter().map(|&n| Value::from(n)));
    let undefined = scripts.guarded(|lua| lua.call_kept(ENTRY, &args))?;
    for name in undefined {
        if let Some(name) = name.as_str() {
            scripts.register_class(EngineClass::new(name))?;
        }
    }
    Ok(())
}

/// `text` as the game's bytes.
pub(crate) fn lua_text(text: &str) -> Value {
    Value::String(cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec()))
}

pub(crate) fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => cp1251::decode(s),
        v => format!("{v:?}"),
    }
}

/// How a call's first result is kept (1.2).
#[derive(Clone, Copy)]
pub(crate) enum Keep {
    Nothing,
    /// Lua's string conversion: nil for a non-string.
    Text,
    /// The runtime's truth value, as a boolean.
    Truth,
}

impl Scripts {
    /// The `[common] script` list (2.1): each item's namespace loaded, then
    /// its global `<item>_initialize` called if it is a function. Returns
    /// the names called. A script error is fatal (`LUA error: …`).
    fn initialize(&mut self, items: &[String]) -> Result<Vec<String>, Error> {
        let mut args = vec![Value::from("initialize")];
        args.extend(items.iter().map(|i| lua_text(i)));
        let results = self.guarded(|lua| lua.call_kept(ENTRY, &args))?;
        match results.split_first() {
            Some((Value::String(o), called)) if o == b"ok" => {
                Ok(called.iter().map(text_of).collect())
            }
            Some((Value::String(o), [message, ..])) if o == b"error" => {
                Err(self.stop(format!("LUA error: {}", text_of(message))))
            }
            // Our wording: the original asserts.
            Some((Value::String(o), [name, ..])) if o == b"unresolved" => Err(self.stop(format!(
                "failed to get the initialization function {}",
                text_of(name)
            ))),
            _ => Err(api(format!("unexpected outcome {results:?}"))),
        }
    }

    /// Resolves `function` and calls it with `args` (1): `Ok(None)` when it
    /// doesn't resolve, else its first result as `keep` keeps it (nil for
    /// [`Keep::Nothing`]). A script error is fatal (`LUA error: …`).
    pub(crate) fn call_entry(
        &mut self,
        function: &str,
        keep: Keep,
        args: &[Value],
    ) -> Result<Option<Value>, Error> {
        let conversion = match keep {
            Keep::Nothing => "none",
            Keep::Text => "text",
            Keep::Truth => "truth",
        };
        let mut all = vec![Value::from("call"), lua_text(function), conversion.into()];
        all.extend_from_slice(args);
        let results = self.guarded(|lua| lua.call_kept(ENTRY, &all))?;
        match results.as_slice() {
            [Value::String(o)] if o == b"missing" => Ok(None),
            [Value::String(o), result, ..] if o == b"ok" => Ok(Some(result.clone())),
            [Value::String(o)] if o == b"ok" => Ok(Some(Value::Nil)),
            [Value::String(o), message, ..] if o == b"error" => {
                Err(self.stop(format!("LUA error: {}", text_of(message))))
            }
            other => Err(api(format!("unexpected outcome {other:?}"))),
        }
    }
}

/// The server options' items the simulation checks (2.3 step 2): a
/// non-empty first item (the game or spawn name), `single` second and
/// `alife` third, `/`-separated. Otherwise the failed assertion's
/// description.
pub fn check_server_options(options: &str) -> Result<(), &'static str> {
    let items: Vec<&str> = options.split('/').collect();
    let item = |i: usize| items.get(i).copied().unwrap_or("");
    if item(0).is_empty() || item(2) != "alife" || item(1) != "single" {
        return Err("Invalid server options!");
    }
    Ok(())
}

/// The game-type text of the server options (2.2): their second
/// `/`-separated item.
pub fn game_type_text(options: &str) -> &str {
    options.split('/').nth(1).unwrap_or("")
}

/// The game's script state across its rebuilds (2.3 step 1), with the
/// engine's calls into it in order ([`EngineCall`]).
pub struct GameScripts {
    startup: Startup,
    scripts: Scripts,
    common: Common,
    calls: Vec<EngineCall>,
}

impl GameScripts {
    /// Program start (3.1 step 2): startup on a new game state.
    // TODO(#997): gap 2, a game started from the command line (`-start`):
    // Volya Engine has no command line, so startup is followed only by the
    // main menu's New Game.
    pub fn start(startup: Startup) -> Result<Self, Error> {
        let (scripts, common, call) = startup.run()?;
        Ok(GameScripts {
            startup,
            scripts,
            common,
            calls: vec![call],
        })
    }

    /// The current game state.
    pub fn scripts(&mut self) -> &mut Scripts {
        &mut self.scripts
    }

    /// The engine state the services read and change.
    pub fn services(&self) -> &Rc<RefCell<EngineServices>> {
        &self.startup.services
    }

    /// The engine objects as scripts see them (script API M).
    pub fn objects(&self) -> &Rc<RefCell<GameObjects>> {
        &self.startup.objects
    }

    /// The player's story state as scripts reach it (script API P).
    pub fn story(&self) -> &Rc<RefCell<Story>> {
        &self.startup.story
    }

    /// The dialogs and trade's own state (script API R).
    pub fn dialogs(&self) -> &Rc<RefCell<DialogsAndTrade>> {
        &self.startup.dialogs
    }

    /// The game's clocks as scripts read them (script API H).
    pub fn times(&self) -> &Rc<RefCell<GameTimes>> {
        &self.startup.times
    }

    /// The sounds the scripts made and their requests (script API T).
    pub fn sounds(&self) -> &Rc<RefCell<ScriptSounds>> {
        &self.startup.sounds
    }

    /// The simulation as scripts reach it (script API K).
    pub fn simulation(&self) -> &Rc<RefCell<AlifeSimulator>> {
        &self.startup.simulation
    }

    /// The window description files' state (script API G3).
    pub fn window_files(&self) -> &Rc<RefCell<WindowFiles>> {
        &self.startup.window_files
    }

    /// The level and patrol paths script API N's names read.
    pub fn level(&self) -> &Rc<RefCell<LevelState>> {
        &self.startup.level
    }

    /// The player's screen effects and the environment (script API U).
    pub fn effects_and_weather(&self) -> &Rc<RefCell<EffectsAndWeather>> {
        &self.startup.effects_and_weather
    }

    /// The engine's calls into scripts so far, oldest first.
    pub fn calls(&self) -> &[EngineCall] {
        &self.calls
    }

    /// The game-type function asked for the class id of game type
    /// `game_type`'s server (`server`) or client game object (2.2): the
    /// function `[common] game_type_clsid_factory` names, called with the
    /// text and the flag, its result converted to a class id. Fatal when
    /// the key is missing, the function doesn't resolve or fails, or its
    /// result is no text or empty (`Unknown game type: <text>`).
    pub fn game_type_class_id(&mut self, game_type: &str, server: bool) -> Result<u64, Error> {
        let Some(function) = self.common.game_type_function.clone() else {
            // Our wording of ltx-config.md's missing key.
            return Err(self.scripts.stop(format!(
                "no key {GAME_TYPE_FUNCTION_KEY} in section [common] of script.ltx"
            )));
        };
        let result = self.scripts.call_entry(
            &function,
            Keep::Text,
            &[lua_text(game_type), Value::Boolean(server)],
        )?;
        let Some(result) = result else {
            // Our wording: the original asserts.
            return Err(self
                .scripts
                .stop(format!("failed to get the game type function {function}")));
        };
        let text = match result {
            Value::String(t) if !t.is_empty() => Some(t),
            _ => None,
        };
        self.calls.push(EngineCall::GameType {
            function,
            game_type: game_type.to_owned(),
            server,
            result: text.as_deref().map(cp1251::decode).unwrap_or_default(),
        });
        let Some(text) = text else {
            return Err(self.scripts.stop(format!("Unknown game type: {game_type}")));
        };
        Ok(class_id(&text))
    }

    /// The game object of game type `game_type`'s server or client side
    /// (3.3 steps 9.1, 9.2 and 11.2): the game-type function's class id,
    /// created through the factory. Returns the id's text; the engine makes
    /// the game object of an engine entry itself (C 6.1).
    pub fn create_game_object(&mut self, game_type: &str, server: bool) -> Result<String, Error> {
        let id = self.game_type_class_id(game_type, server)?;
        let created = self.scripts.create_client_object(id)?;
        if let CreatedObject::Script(part) = created.object {
            // No shipped game type has a script class; the engine has no
            // use for one here (our choice): it is released at once.
            self.scripts.release_object(part)?;
        }
        Ok(cp1251::decode(id_text(id).trim_ascii_end()))
    }

    /// The simulation is created with server options `options` (2.3 steps
    /// 1 to 4): the script state rebuilt unless the command line has
    /// `-keep_lua`, the options checked, then the start callback the
    /// settings' `[alife] start_game_callback` names called. Fatal when the
    /// options are invalid, the callback doesn't resolve or it fails.
    /// Starting the game (step 5) is the caller's.
    // TODO(#997): gap 1, whether `alife()` returns the simulation during
    // the callback: no implemented name gives scripts the simulation.
    pub fn create_simulation(&mut self, options: &str, settings: &Config) -> Result<(), Error> {
        let keep = keeps_lua(&self.startup.files.borrow().command_line);
        if !keep {
            self.rebuild()?;
        }
        if let Err(description) = check_server_options(options) {
            return Err(self.scripts.stop(description.to_owned()));
        }
        let (section, key) = START_CALLBACK;
        let Some(function) = settings.get(section, key).flatten().map(str::trim) else {
            // Our wording of ltx-config.md's missing key.
            return Err(self.scripts.stop(format!(
                "no key {key} in section [{section}] of the settings"
            )));
        };
        let function = function.to_owned();
        if self
            .scripts
            .call_entry(&function, Keep::Nothing, &[])?
            .is_none()
        {
            return Err(self
                .scripts
                .stop("failed to get start game callback".to_owned()));
        }
        self.calls.push(EngineCall::StartGameCallback(function));
        Ok(())
    }

    /// The rebuild (2.3 step 1; C 2.3): the old game state closed, then
    /// startup on a new one.
    pub fn rebuild(&mut self) -> Result<(), Error> {
        self.calls.push(EngineCall::Rebuild);
        // The old state closes before the new one opens: replaced by an
        // empty state first.
        let empty = Scripts::new(
            Rc::clone(&self.startup.trees),
            &self.startup.aliases,
            self.startup.prefix,
        )?;
        drop(std::mem::replace(&mut self.scripts, empty));
        let (scripts, common, call) = self.startup.run()?;
        self.scripts = scripts;
        self.common = common;
        self.calls.push(call);
        Ok(())
    }

    /// Calls the tutorial-threshold function `function` (2.4): fatal when
    /// it doesn't resolve or fails.
    pub fn tutorial(&mut self, function: TutorialFunction) -> Result<(), Error> {
        let name = function.name();
        if self.scripts.call_entry(name, Keep::Nothing, &[])?.is_none() {
            // Our wording: the original asserts.
            return Err(self
                .scripts
                .stop(format!("failed to get the tutorial function {name}")));
        }
        self.calls.push(EngineCall::Tutorial(name));
        Ok(())
    }

    /// The weapon-parameter window's update for item section `section`
    /// (2.5): the positions of its four bars, `GetRPM`, `GetAccuracy`,
    /// `GetDamage` (`single`, the client game's type value 1) or
    /// `GetDamageMP`, and `GetHandling`. A function that didn't resolve the
    /// first time is fatal when called (our choice: believed a crash); a
    /// script error is fatal.
    pub fn weapon_parameters(&mut self, section: &str, single: bool) -> Result<[f64; 4], Error> {
        let args = [
            Value::from("weapon"),
            lua_text(section),
            Value::Boolean(single),
        ];
        let results = self.scripts.guarded(|lua| lua.call_kept(ENTRY, &args))?;
        self.calls
            .push(EngineCall::WeaponParameters(section.to_owned()));
        match results.as_slice() {
            [Value::String(o), a, b, c, d] if o == b"ok" => {
                let n = |v: &Value| v.as_number().unwrap_or(0.0);
                Ok([n(a), n(b), n(c), n(d)])
            }
            [Value::String(o), name, ..] if o == b"missing" => Err(self.scripts.stop(format!(
                "the weapon-parameter function {} is missing",
                text_of(name)
            ))),
            [Value::String(o), message, ..] if o == b"error" => Err(self
                .scripts
                .stop(format!("LUA error: {}", text_of(message)))),
            other => Err(api(format!("unexpected outcome {other:?}"))),
        }
    }
}

/// The 8 tutorial-threshold functions (2.4), in their rules' order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TutorialFunction {
    CriticalPower,
    CriticalMaxPower,
    Bleeding,
    Satiety,
    Radiation,
    Psy,
    CantWalkWeight,
    WeaponJammed,
}

impl TutorialFunction {
    /// The name the game library writes.
    pub fn name(self) -> &'static str {
        match self {
            Self::CriticalPower => "_G.on_actor_critical_power",
            Self::CriticalMaxPower => "_G.on_actor_critical_max_power",
            Self::Bleeding => "_G.on_actor_bleeding",
            Self::Satiety => "_G.on_actor_satiety",
            Self::Radiation => "_G.on_actor_radiation",
            Self::Psy => "_G.on_actor_psy",
            Self::CantWalkWeight => "_G.on_actor_cant_walk_weight",
            Self::WeaponJammed => "_G.on_actor_weapon_jammed",
        }
    }
}

/// `[tutorial_conditions_thresholds]` of the settings (2.4), read at the
/// first check.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TutorialThresholds {
    pub power: f32,
    pub max_power: f32,
    pub bleeding: f32,
    pub satiety: f32,
    pub radiation: f32,
    pub weapon_jammed: f32,
    pub psy_health: f32,
}

impl TutorialThresholds {
    /// The section's 7 values; a missing one is our wording of
    /// ltx-config.md's missing key.
    pub fn read(settings: &Config) -> Result<Self, String> {
        const SECTION: &str = "tutorial_conditions_thresholds";
        let read = |key: &str| {
            settings
                .get(SECTION, key)
                .flatten()
                .map(value::float)
                .ok_or_else(|| format!("no key {key} in section [{SECTION}] of the settings"))
        };
        Ok(TutorialThresholds {
            power: read("power")?,
            max_power: read("max_power")?,
            bleeding: read("bleeding")?,
            satiety: read("satiety")?,
            radiation: read("radiation")?,
            weapon_jammed: read("weapon_jammed")?,
            psy_health: read("psy_health")?,
        })
    }
}

/// The player condition's notified flags (2.4), all clear when the
/// condition object is made; not saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TutorialFlags {
    pub power: bool,
    pub max_power: bool,
    pub bleeding: bool,
    pub satiety: bool,
    pub radiation: bool,
    /// No code sets it.
    pub psy: bool,
    pub jammed: bool,
}

/// The player's values a check compares (2.4; what each is belongs to the
/// player-condition spec).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TutorialSample {
    pub power: f32,
    pub max_power: f32,
    /// The total bleeding over the player's wounds.
    pub bleeding: f32,
    pub satiety: f32,
    pub radiation: f32,
    pub psy_health: f32,
    /// The walk-weight query's overweight mark.
    pub overweight: bool,
    /// The condition of the weapon in the active slot, if it holds one.
    pub weapon_condition: Option<f32>,
}

/// One check of the thresholds (2.4): the first rule that applies, its
/// flag marked, names the function to call; at most one per check. The
/// caller checks only while the game type is 0 or 1 and the player's
/// health is above 0.
pub fn tutorial_check(
    thresholds: &TutorialThresholds,
    flags: &mut TutorialFlags,
    player: &TutorialSample,
) -> Option<TutorialFunction> {
    let t = thresholds;
    let p = player;
    if !flags.power && p.power < t.power {
        flags.power = true;
        return Some(TutorialFunction::CriticalPower);
    }
    if !flags.max_power && p.max_power < t.max_power {
        flags.max_power = true;
        return Some(TutorialFunction::CriticalMaxPower);
    }
    if !flags.bleeding && p.bleeding > t.bleeding {
        flags.bleeding = true;
        return Some(TutorialFunction::Bleeding);
    }
    if !flags.satiety && p.satiety < t.satiety {
        flags.satiety = true;
        return Some(TutorialFunction::Satiety);
    }
    if !flags.radiation && p.radiation > t.radiation {
        flags.radiation = true;
        return Some(TutorialFunction::Radiation);
    }
    if !flags.psy && p.psy_health > t.psy_health {
        return Some(TutorialFunction::Psy);
    }
    if !p.overweight {
        return Some(TutorialFunction::CantWalkWeight);
    }
    if !flags.jammed
        && let Some(condition) = p.weapon_condition
        && condition < t.weapon_jammed
    {
        flags.jammed = true;
        return Some(TutorialFunction::WeaponJammed);
    }
    None
}
