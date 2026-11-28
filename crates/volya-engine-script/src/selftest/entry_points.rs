// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-engine-entry-points.md on every target, one
//! check per section, with scripts of our own over [`GameScripts`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Cursor};
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::{self, DirEntry, Vfs, VfsFile};
use volya_engine_formats::ltx;

use super::ensure;
use crate::entry_points::{
    EngineCall, GameScripts, STAND_IN_BASES, Startup, TutorialFlags, TutorialFunction,
    TutorialSample, TutorialThresholds, check_server_options, game_type_text, tutorial_check,
};
use crate::files::FileSystem;
use crate::game_object::GameObjects;
use crate::loader::{Prefix, Trees};
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::story::Story;
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 finding and calling", finding_and_calling),
    ("2.1 [common] script", initialize),
    ("2.2 game-type function", game_type),
    ("2.3 start callback", start_callback),
    ("2.4 tutorial thresholds", tutorial_thresholds),
    ("2.5 weapon parameters", weapon_parameters),
    ("3 New Game call order", call_order),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("entry points {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Files of our own, by lowercase path.
struct Tree(HashMap<String, Vec<u8>>);

impl Vfs for Tree {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let key = vfs::normalize(path)?.to_ascii_lowercase();
        match self.0.get(&key) {
            Some(data) => Ok(Box::new(File(Cursor::new(data.clone())))),
            None => Err(io::Error::new(io::ErrorKind::NotFound, key)),
        }
    }

    fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
        Err(io::Error::other("not listed"))
    }
}

impl Trees for Tree {
    fn with_trees(&self, f: &mut dyn FnMut(&dyn Vfs, &dyn Vfs)) -> io::Result<()> {
        f(self, self);
        Ok(())
    }
}

struct File(Cursor<Vec<u8>>);

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Seek for File {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl VfsFile for File {
    fn size(&self) -> u64 {
        self.0.get_ref().len() as u64
    }
}

const FSGAME: &[u8] = b"$game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
    $game_scripts$ = true | false| $game_data$ | scripts\\\r\n\
    $game_config$ = true | false| $game_data$ | config\\\r\n";

const SCRIPT_LTX: &str = "[common]\r\nclass_registrators = registrator.register\r\n\
    script = _G, xr_init, xr_none\r\ngame_type_clsid_factory = game_types.get\r\n";

/// The scripts: `_G` keeps a log of what the engine called (`calls`),
/// counted per state (`state`); `game_types.get` answers as the global
/// `answer` says.
const SCRIPTS: &[(&str, &str)] = &[
    (
        "_g.script",
        "calls = {} state_loads = (state_loads or 0) + 1
         function note(what) calls[#calls + 1] = what end
         class 'stand_in' (effector)
         function start_game_callback() note('start ' .. tostring(alive)) started = (started or 0) + 1 end
         function bad_callback() error('callback failed', 0) end
         function on_actor_psy() note('psy') end",
    ),
    (
        "registrator.script",
        "function register(factory) _G.note('register') end",
    ),
    (
        "xr_init.script",
        "function _G.xr_init_initialize(...) _G.note('init ' .. select('#', ...)) end",
    ),
    (
        "game_types.script",
        "function get(game_type, server)
           _G.note('type ' .. game_type .. ' ' .. tostring(server))
           local answer = _G.answer
           if answer == 'error' then error('no type', 0) end
           if answer ~= nil then return answer end
           if server then return 'SV_SINGL' end
           return 'CL_SINGL'
         end",
    ),
    (
        "ui_wpn_params.script",
        "function GetRPM(s) _G.note('rpm ' .. s) return 1 end
         function GetDamage(s) _G.note('damage ' .. s) return '2.5' end
         function GetDamageMP(s) _G.note('damage_mp ' .. s) return 3 end
         function GetHandling(s) _G.note('handling ' .. s) return {} end
         function GetAccuracy(s) _G.note('accuracy ' .. s) return 4 end",
    ),
];

const SETTINGS: &str = "[alife]\r\nstart_game_callback = _G.start_game_callback\r\n\
    [tutorial_conditions_thresholds]\r\npower = 0.1\r\nmax_power = 0.75\r\nbleeding = 0.4\r\n\
    satiety = 0.5\r\nradiation = 0.1\r\nweapon_jammed = 0.9\r\npsy_health = 0.5\r\n";

/// A game state started over our scripts (`extra` adds or replaces files),
/// with its engine state.
fn started(
    extra: &[(&str, &str)],
    command_line: &str,
) -> Result<(GameScripts, Rc<RefCell<FileSystem>>), String> {
    let mut files: HashMap<String, Vec<u8>> = SCRIPTS
        .iter()
        .map(|(n, t)| (format!("gamedata/scripts/{n}"), t.as_bytes().to_vec()))
        .collect();
    files.insert(
        "gamedata/config/script.ltx".into(),
        SCRIPT_LTX.as_bytes().to_vec(),
    );
    for (path, text) in extra {
        files.insert((*path).to_owned(), text.as_bytes().to_vec());
    }
    let tree = Rc::new(Tree(files));
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let settings = ltx::parse(SETTINGS.as_bytes()).map_err(|e| e.to_string())?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, settings.clone())));
    let mut fs = FileSystem::new(
        tree.clone(),
        aliases.clone(),
        Rc::new(settings),
        Rc::new(ltx::Config::default()),
    );
    fs.command_line = command_line.to_owned();
    let fs = Rc::new(RefCell::new(fs));
    let scripts = GameScripts::start(Startup {
        trees: tree,
        aliases,
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        files: Rc::clone(&fs),
    })
    .map_err(|e| e.message)?;
    Ok((scripts, fs))
}

/// The `calls` the scripts noted, joined by `|`.
fn noted(g: &mut GameScripts) -> Result<String, String> {
    let r = g
        .scripts()
        .exec("=noted", b"return table.concat(calls, '|')")
        .map_err(|e| e.message)?;
    Ok(r.first().and_then(Value::as_str).unwrap_or("").to_owned())
}

fn settings() -> Result<ltx::Config, String> {
    ltx::parse(SETTINGS.as_bytes()).map_err(|e| e.to_string())
}

fn is_fatal<T: std::fmt::Debug>(
    result: &Result<T, crate::Error>,
    message: &str,
) -> Result<(), String> {
    ensure(
        matches!(result, Err(e) if e.kind == ErrorKind::Fatal && e.message == message),
        || format!("{result:?}, want fatal {message:?}"),
    )
}

/// Section 1: names resolve as C 3.2 has them (the namespace loaded first,
/// its own entries only), and a failing call is fatal.
pub fn finding_and_calling() -> Result<(), String> {
    let (mut g, _) = started(
        &[(
            "gamedata/config/script.ltx",
            "[common]\r\nclass_registrators = registrator.register\r\nscript = _G\r\n\
             game_type_clsid_factory = xr_fallback.note\r\n",
        )],
        "",
    )?;
    // `xr_fallback.note` reaches `_G.note` only through the globals: not
    // found.
    let r = g.game_type_class_id("single", true);
    is_fatal(&r, "failed to get the game type function xr_fallback.note")?;
    // `_G.f` is found only if the globals hold it.
    let (mut g, _) = started(&[], "")?;
    let missing = ltx::parse(b"[alife]\r\nstart_game_callback = _G.missing\r\n")
        .map_err(|e| e.to_string())?;
    let r = g.create_simulation("all/single/alife/new", &missing);
    is_fatal(&r, "failed to get start game callback")?;
    // A script error ends the game: `LUA error: <message>`.
    let (mut g, _) = started(&[], "")?;
    let bad = ltx::parse(b"[alife]\r\nstart_game_callback = _G.bad_callback\r\n")
        .map_err(|e| e.to_string())?;
    let r = g.create_simulation("all/single/alife/new", &bad);
    is_fatal(&r, "LUA error: callback failed")?;
    let later = g.scripts().exec("=later", b"return 1");
    ensure(
        matches!(&later, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("the state goes on after it: {later:?}"),
    )
}

/// Section 2.1: each item's namespace loaded, `<item>_initialize` called
/// with no arguments where it is a global function; none for `_G` here.
pub fn initialize() -> Result<(), String> {
    let (mut g, _) = started(&[], "")?;
    ensure(
        g.calls()
            == [EngineCall::Startup {
                registrators: vec!["registrator.register".into()],
                initialized: vec!["xr_init_initialize".into()],
            }],
        || format!("calls: {:?}", g.calls()),
    )?;
    let noted = noted(&mut g)?;
    ensure(noted == "register|init 0", || format!("noted: {noted}"))?;
    // The engine bases no spec registers yet are stand-ins: `_G`'s class
    // deriving from `effector` loaded.
    let r = g
        .scripts()
        .exec("=t", b"return type(stand_in), type(effector)")
        .map_err(|e| e.message)?;
    ensure(r == ["userdata".into(), "userdata".into()], || {
        format!("stand-ins: {r:?}")
    })?;
    ensure(STAND_IN_BASES.len() == 33, || "33 engine bases".into())?;
    // A base a spec registers is its own, not a stand-in (script API M's
    // `CGameObject` with its overrides).
    let r = g
        .scripts()
        .exec("=t", b"return type(CGameObject.net_Spawn)")
        .map_err(|e| e.message)?;
    ensure(r == ["function".into()], || {
        format!("CGameObject is M's: {r:?}")
    })?;
    // A failing `_initialize` is fatal.
    let r = started(
        &[(
            "gamedata/scripts/xr_init.script",
            "function _G.xr_init_initialize() error('init failed', 0) end",
        )],
        "",
    );
    ensure(
        matches!(&r, Err(m) if m == "LUA error: init failed"),
        || "a failing _initialize is fatal".into(),
    )
}

/// Section 2.2: the function called with the game-type text and the
/// server flag; its result's text converted to a class id; no text or an
/// empty one fatal.
pub fn game_type() -> Result<(), String> {
    let (mut g, _) = started(&[], "")?;
    let id = g
        .game_type_class_id("single", true)
        .map_err(|e| e.message)?;
    ensure(id == class_id(b"SV_SINGL"), || format!("server id {id:#x}"))?;
    let made = g
        .create_game_object("single", false)
        .map_err(|e| e.message)?;
    ensure(made == "CL_SINGL", || format!("client object {made}"))?;
    let n = noted(&mut g)?;
    ensure(n.ends_with("type single true|type single false"), || {
        format!("noted: {n}")
    })?;
    ensure(game_type_text("all/single/alife/new") == "single", || {
        "the game type is the options' second item".into()
    })?;
    let answer = |g: &mut GameScripts, code: &str| {
        g.scripts()
            .exec("=a", format!("answer = {code}").as_bytes())
            .map(drop)
            .map_err(|e| e.message)
    };
    // A number becomes its text.
    answer(&mut g, "12")?;
    let id = g
        .game_type_class_id("single", false)
        .map_err(|e| e.message)?;
    ensure(id == class_id(b"12"), || format!("number id {id:#x}"))?;
    for code in ["true", "''"] {
        let (mut g, _) = started(&[], "")?;
        answer(&mut g, code)?;
        is_fatal(
            &g.game_type_class_id("single", true),
            "Unknown game type: single",
        )?;
    }
    let (mut g, _) = started(&[], "")?;
    answer(&mut g, "'error'")?;
    is_fatal(&g.game_type_class_id("single", true), "LUA error: no type")?;
    let (mut g, _) = started(
        &[(
            "gamedata/config/script.ltx",
            "[common]\r\nclass_registrators = registrator.register\r\nscript = _G\r\n",
        )],
        "",
    )?;
    is_fatal(
        &g.game_type_class_id("single", true),
        "no key game_type_clsid_factory in section [common] of script.ltx",
    )
}

/// Section 2.3: the rebuild (unless `-keep_lua`), the options' assertion,
/// then the callback called once with no arguments.
pub fn start_callback() -> Result<(), String> {
    let settings = settings()?;
    let (mut g, _) = started(&[], "")?;
    g.scripts()
        .exec("=t", b"alive = 'old'")
        .map_err(|e| e.message)?;
    g.create_simulation("all/single/alife/new", &settings)
        .map_err(|e| e.message)?;
    let r = g
        .scripts()
        .exec("=t", b"return state_loads, started, alive")
        .map_err(|e| e.message)?;
    ensure(r == [1.0.into(), 1.0.into(), Value::Nil], || {
        format!("a new state, called once: {r:?}")
    })?;
    let n = noted(&mut g)?;
    ensure(n == "register|init 0|start nil", || format!("noted: {n}"))?;
    ensure(
        matches!(g.calls(), [_, EngineCall::Rebuild, EngineCall::Startup { .. }, EngineCall::StartGameCallback(f)] if f == "_G.start_game_callback"),
        || format!("calls: {:?}", g.calls()),
    )?;
    // -keep_lua keeps the state.
    let (mut g, _) = started(&[], "XR_3DA.exe -KEEP_LUA")?;
    g.scripts()
        .exec("=t", b"alive = 'old'")
        .map_err(|e| e.message)?;
    g.create_simulation("all/single/alife/new", &settings)
        .map_err(|e| e.message)?;
    let n = noted(&mut g)?;
    ensure(n.ends_with("start old"), || format!("kept: {n}"))?;
    // The options: a name, `single` second, `alife` third.
    for options in [
        "all/single/alife/new",
        "all/single/alife",
        "x/single/alife/load",
    ] {
        ensure(check_server_options(options).is_ok(), || {
            format!("{options} is valid")
        })?;
    }
    for options in ["/single/alife/new", "all/alife/single", "all/single", ""] {
        ensure(
            check_server_options(options) == Err("Invalid server options!"),
            || format!("{options} is invalid"),
        )?;
    }
    let (mut g, _) = started(&[], "")?;
    is_fatal(
        &g.create_simulation("all/deathmatch/alife/new", &settings),
        "Invalid server options!",
    )
}

/// Section 2.4: the rules in order, at most one per check, the flags.
pub fn tutorial_thresholds() -> Result<(), String> {
    let t = TutorialThresholds::read(&settings()?)?;
    ensure(
        t == TutorialThresholds {
            power: 0.1,
            max_power: 0.75,
            bleeding: 0.4,
            satiety: 0.5,
            radiation: 0.1,
            weapon_jammed: 0.9,
            psy_health: 0.5,
        },
        || format!("thresholds: {t:?}"),
    )?;
    let healthy = TutorialSample {
        power: 1.0,
        max_power: 1.0,
        bleeding: 0.0,
        satiety: 1.0,
        radiation: 0.0,
        psy_health: 1.0,
        overweight: false,
        weapon_condition: None,
    };
    let mut flags = TutorialFlags::default();
    use TutorialFunction as F;
    // Healthy and not overweight: `on_actor_psy` at every check.
    for _ in 0..2 {
        let f = tutorial_check(&t, &mut flags, &healthy);
        ensure(f == Some(F::Psy), || format!("healthy: {f:?}"))?;
    }
    let low_psy = TutorialSample {
        psy_health: 0.5,
        ..healthy
    };
    let f = tutorial_check(&t, &mut flags, &low_psy);
    ensure(f == Some(F::CantWalkWeight), || {
        format!("psy at 0.5: {f:?}")
    })?;
    // Everything at once: one rule per check, in order, each flagged once.
    let bad = TutorialSample {
        power: 0.05,
        max_power: 0.5,
        bleeding: 0.5,
        satiety: 0.4,
        radiation: 0.2,
        psy_health: 0.4,
        overweight: true,
        weapon_condition: Some(0.5),
    };
    let mut flags = TutorialFlags::default();
    let order: Vec<_> = (0..8)
        .map(|_| tutorial_check(&t, &mut flags, &bad))
        .collect();
    let want = [
        Some(F::CriticalPower),
        Some(F::CriticalMaxPower),
        Some(F::Bleeding),
        Some(F::Satiety),
        Some(F::Radiation),
        Some(F::WeaponJammed),
        None,
        None,
    ];
    ensure(order == want, || format!("order: {order:?}"))?;
    // Strict comparisons; jammed only while overweight with a weapon.
    let mut flags = TutorialFlags::default();
    let edge = TutorialSample {
        power: 0.1,
        max_power: 0.75,
        bleeding: 0.4,
        satiety: 0.5,
        radiation: 0.1,
        psy_health: 0.5,
        overweight: true,
        weapon_condition: None,
    };
    let f = tutorial_check(&t, &mut flags, &edge);
    ensure(f.is_none() && flags == TutorialFlags::default(), || {
        format!("at the thresholds: {f:?}")
    })?;
    // The call: the function resolved in the globals and called.
    let (mut g, _) = started(&[], "")?;
    g.tutorial(F::Psy).map_err(|e| e.message)?;
    let n = noted(&mut g)?;
    ensure(n.ends_with("|psy"), || format!("noted: {n}"))?;
    is_fatal(
        &g.tutorial(F::Satiety),
        "failed to get the tutorial function _G.on_actor_satiety",
    )
}

/// Section 2.5: the four calls in order, their numbers, the functions kept
/// from the first update.
pub fn weapon_parameters() -> Result<(), String> {
    let (mut g, _) = started(&[], "")?;
    let bars = g.weapon_parameters("wpn_pm", true).map_err(|e| e.message)?;
    ensure(bars == [1.0, 4.0, 2.5, 0.0], || format!("bars: {bars:?}"))?;
    // Replacing a function after the first update changes nothing.
    g.scripts()
        .exec("=t", b"ui_wpn_params.GetRPM = function() return 9 end")
        .map_err(|e| e.message)?;
    let bars = g
        .weapon_parameters("wpn_ak74", false)
        .map_err(|e| e.message)?;
    ensure(bars == [1.0, 4.0, 3.0, 0.0], || format!("bars: {bars:?}"))?;
    let n = noted(&mut g)?;
    ensure(
        n.ends_with(
            "rpm wpn_pm|accuracy wpn_pm|damage wpn_pm|handling wpn_pm|\
             rpm wpn_ak74|accuracy wpn_ak74|damage_mp wpn_ak74|handling wpn_ak74",
        ),
        || format!("noted: {n}"),
    )?;
    let (mut g, _) = started(
        &[(
            "gamedata/scripts/ui_wpn_params.script",
            "function GetRPM() return 1 end",
        )],
        "",
    )?;
    is_fatal(
        &g.weapon_parameters("wpn_pm", true),
        "the weapon-parameter function ui_wpn_params.GetAccuracy is missing",
    )
}

/// Section 3: a New Game's calls in order: startup, the server's game
/// type in the old state, the rebuild with a second startup, the start
/// callback, the client's game type in the new state.
pub fn call_order() -> Result<(), String> {
    let settings = settings()?;
    let (mut g, _) = started(&[], "")?;
    let options = "all/single/alife/new";
    let server = g
        .create_game_object(game_type_text(options), true)
        .map_err(|e| e.message)?;
    let old = noted(&mut g)?;
    g.create_simulation(options, &settings)
        .map_err(|e| e.message)?;
    let client = g
        .create_game_object(game_type_text(options), false)
        .map_err(|e| e.message)?;
    let new = noted(&mut g)?;
    ensure(
        (server.as_str(), client.as_str()) == ("SV_SINGL", "CL_SINGL"),
        || format!("{server}, {client}"),
    )?;
    ensure(old == "register|init 0|type single true", || {
        format!("old state: {old}")
    })?;
    ensure(new == "register|init 0|start nil|type single false", || {
        format!("new state: {new}")
    })?;
    let startup = EngineCall::Startup {
        registrators: vec!["registrator.register".into()],
        initialized: vec!["xr_init_initialize".into()],
    };
    let game_type = |server: bool, result: &str| EngineCall::GameType {
        function: "game_types.get".into(),
        game_type: "single".into(),
        server,
        result: result.into(),
    };
    let want = [
        startup.clone(),
        game_type(true, "SV_SINGL"),
        EngineCall::Rebuild,
        startup,
        EngineCall::StartGameCallback("_G.start_game_callback".into()),
        game_type(false, "CL_SINGL"),
    ];
    ensure(g.calls() == want, || format!("calls: {:?}", g.calls()))
}
