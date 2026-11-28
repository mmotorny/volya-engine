// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's scripted actions through the shipped scripts,
//! checked against docs/behavior/script-api-scripted-actions.md (cited by
//! section): the members no script uses and `particle`'s one user (12), the trader's
//! release on a section switch and the mobile schemes' capture through
//! `xr_logic` (2.2), and `car.script`'s actions through `_g.script`'s
//! `action` on a car (2.3, 2.4, 13). Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use shipped_scripts::{GameTrees, Token, tokens};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::model::transform;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::Prefix;
use volya_engine_script::object_factory::class_id;
use volya_engine_script::services::EngineServices;
use volya_engine_script::sound::ScriptSounds;
use volya_engine_script::story::Story;

const FSGAME: &[u8] = b"$game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_levels$ =   true | false| $game_data$    | levels\\\r\n\
    $game_sounds$ =   true | true | $game_data$    | sounds\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n\
    $level$ =         false| false| $game_levels$\r\n";

const TRADER: u16 = 10;
const CAR: u16 = 20;

fn trees() -> Option<(Rc<GameTrees>, PathAliases)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    Some((Rc::new(GameTrees { tree, root }), aliases))
}

/// Each script's method names called and global functions called,
/// the listing (`lua_help.script`) left out.
fn scan(tree: &dyn Vfs) -> BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> {
    let mut out = BTreeMap::new();
    for e in tree.read_dir("gamedata/scripts").unwrap() {
        let name = e.name.to_ascii_lowercase();
        if e.kind != EntryKind::File || !name.ends_with(".script") || name == "lua_help.script" {
            continue;
        }
        let t = tokens(&read(tree, &format!("gamedata/scripts/{}", e.name)).unwrap());
        let mut methods = BTreeSet::new();
        let mut functions = BTreeSet::new();
        for w in t.windows(3) {
            match w {
                [Token::Punct(b':'), Token::Name(n), Token::Punct(b'(')] => {
                    methods.insert(n.clone());
                }
                [before, Token::Name(n), Token::Punct(b'(')]
                    if !matches!(before, Token::Punct(b':' | b'.'))
                        && *before != Token::Name("function".into()) =>
                {
                    functions.insert(n.clone());
                }
                _ => {}
            }
        }
        out.insert(name, (methods, functions));
    }
    out
}

/// 12: the members no script calls, `can_script_capture`'s one caller,
/// `particle`'s one constructing script.
#[test]
fn shipped_uses() {
    let Some((trees, _)) = trees() else {
        return;
    };
    let scripts = scan(&trees.tree);
    let callers = |name: &str| -> BTreeSet<&str> {
        (scripts.iter())
            .filter(|(_, (m, _))| m.contains(name))
            .map(|(f, _)| f.as_str())
            .collect()
    };
    for name in [
        "get_script_name",
        "action_count",
        "action_by_index",
        "reset_action_queue",
    ] {
        assert_eq!(callers(name), BTreeSet::new(), "{name}");
    }
    assert_eq!(
        callers("can_script_capture"),
        BTreeSet::from(["mob_eluder.script"])
    );
    for name in ["script", "get_script", "command"] {
        assert!(!callers(name).is_empty(), "{name} is called");
    }
    let constructors = |name: &str| -> BTreeSet<&str> {
        (scripts.iter())
            .filter(|(_, (_, f))| f.contains(name))
            .map(|(f, _)| f.as_str())
            .collect()
    };
    // 12 counts the loadable scripts; of all the scripts one constructs a
    // `particle` (with a `particle_params`, spec U).
    assert_eq!(
        constructors("particle"),
        BTreeSet::from(["particle_flame.script"])
    );
}

fn spawn_data(id: u16, section: &str, class: &str) -> SpawnData {
    SpawnData {
        id,
        name: section.as_bytes().to_vec(),
        section: section.as_bytes().to_vec(),
        class_id: class_id(class.as_bytes()),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
        server: None,
    }
}

/// A game state started with the shipped scripts, a trader and a car
/// spawned without binders, and the device's clock.
fn started() -> Option<(GameScripts, Rc<RefCell<EngineServices>>)> {
    let (trees, aliases) = trees()?;
    let tree = &trees.tree;
    let system = config::load_aliased(tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, &aliases, "$game_config$", "game.ltx").unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees.clone(),
        aliases.clone(),
        Rc::new(system),
        Rc::new(game),
    )));
    let mut scripts = GameScripts::start(Startup {
        trees: trees.clone(),
        aliases,
        prefix: Prefix::Default,
        services: Rc::clone(&services),
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::new(RefCell::new(ScriptSounds::new())),
        simulation: Rc::default(),
        window_files: Rc::default(),
    })
    .unwrap();
    let s = scripts.scripts();
    // Sections without a binding function, so no binder.
    let trader = spawn_data(TRADER, "test_trader", "AI_TRADE");
    s.spawn_engine_object(ObjectClass::Named("CAI_Trader"), trader)
        .unwrap();
    let car = spawn_data(CAR, "test_car", "SCRPTCAR");
    s.spawn_engine_object(ObjectClass::Named("CCar"), car)
        .unwrap();
    Some((scripts, services))
}

/// 2.2: `xr_logic`'s release on every section switch of a mobile object
/// (the trader's scheme), its capture, and `_g.script`'s `reset_action`.
#[test]
fn the_traders_section_switch() {
    let Some((mut scripts, _)) = started() else {
        return;
    };
    let s = scripts.scripts();
    let trader = Value::Object(s.game_object(TRADER).unwrap());
    s.exec(
        "=trader",
        br#"
        function check(npc)
          local got = {}
          xr_logic.mob_release(npc)
          got[#got + 1] = npc:get_script()
          xr_logic.mob_capture(npc, false)
          got[#got + 1] = xr_logic.mob_captured(npc)
          got[#got + 1] = npc:get_script_name()
          npc:command(entity_action(), false)
          xr_logic.mob_capture(npc, true)
          got[#got + 1] = npc:action_count()
          got[#got + 1] = npc:get_script()
          xr_logic.mob_release(npc)
          got[#got + 1] = npc:get_script()
          got[#got + 1] = npc:get_script_name()
          return unpack(got)
        end
        "#,
    )
    .unwrap();
    let got = s.call("check", &[trader]).unwrap();
    assert_eq!(
        got,
        [
            Value::Boolean(false),
            Value::Boolean(true),
            Value::string("xr_logic"),
            Value::Number(0.0),
            Value::Boolean(true),
            Value::Boolean(false),
            Value::string(""),
        ]
    );
}

/// 2.3, 2.4, 13: `car.script`'s six actions queued on a car through
/// `_g.script`'s `action`, each ending by its time condition (1000 to 8000
/// ms), the car's object parts naming its door bones.
#[test]
fn car_actions() {
    let Some((mut scripts, services)) = started() else {
        return;
    };
    let s = scripts.scripts();
    let car = Value::Object(s.game_object(CAR).unwrap());
    s.exec(
        "=car",
        br#"
        function queue(obj)
          obj:script(true, "car")
          action(obj, move(move.on), object("phy_door_left", object.activate),
            cond(cond.time_end, 1000))
          action(obj, move(move.back, 3), object("phy_door_right", object.activate),
            cond(cond.time_end, 4000))
          action(obj, move(move.fwd + move.left, 6), object("phy_door_left", object.deactivate),
            cond(cond.time_end, 4000))
          action(obj, move(move.fwd + move.right, 5), object("phy_door_right", object.deactivate),
            cond(cond.time_end, 4000))
          action(obj, move(move.back, 4), object("phy_door_rear", object.use),
            cond(cond.time_end, 4000))
          action(obj, move(move.back, 4), object("phy_door_rear", object.use),
            cond(cond.time_end, 8000))
          return obj:action_count()
        end
        function count(obj) return obj:action_count() end
        "#,
    )
    .unwrap();
    assert_eq!(
        s.call("queue", std::slice::from_ref(&car)).unwrap(),
        [Value::Number(6.0)]
    );
    // Each action ends by its time: the movement part (input keys) and the
    // door parts never become done.
    let mut ends = Vec::new();
    for ms in (0..=30_000).step_by(100) {
        services.borrow_mut().device.frame(ms, 0.1, false);
        let s = scripts.scripts();
        s.run_action_queue(CAR).unwrap();
        let left = s.call("count", std::slice::from_ref(&car)).unwrap();
        if let [Value::Number(n)] = left[..]
            && ends.len() < 6 - n as usize
        {
            ends.push(ms);
        }
    }
    // The first starts at 0, each next one when the one before ends.
    assert_eq!(ends, [1100, 5200, 9300, 13400, 17500, 25600]);
}
