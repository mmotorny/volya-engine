// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's sound through the shipped scripts, checked
//! against docs/behavior/script-api-sound.md (cited by section): the names
//! the scripts use and those none uses (8), and sound objects of the
//! game's files on a started game state, carried out by the engine's sound
//! system (2.3, 2.5, 2.6). Skips when `VOLYA_GOG_INSTALLER` is unset.

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
use volya_engine_core::sound::system::{NoDevice, SoundSystem};
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

/// The aliases scripts and sounds use (docs/formats/ltx-config.md 10).
const FSGAME: &[u8] = b"$game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_levels$ =   true | false| $game_data$    | levels\\\r\n\
    $game_sounds$ =   true | true | $game_data$    | sounds\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n\
    $level$ =         false| false| $game_levels$\r\n";

/// Section 8's method names the scripts call.
const USED: [&str; 14] = [
    "play_at_pos",
    "stop",
    "playing",
    "play",
    "set_position",
    "play_no_feedback",
    "length",
    "add_sound",
    "sound_prefix",
    "play_sound",
    "active_sound_count",
    "set_sound_mask",
    "external_sound_start",
    "external_sound_stop",
];

/// Section 8's members no script calls.
const UNUSED: [&str; 2] = ["get_sound_info", "memory_sound_objects"];

fn trees() -> Option<(Rc<GameTrees>, PathAliases)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    Some((Rc::new(GameTrees { tree, root }), aliases))
}

/// The scripts' names: file -> method names called, `level.` names, and
/// whether it calls `sound_object(`.
fn scan(tree: &dyn Vfs) -> BTreeMap<String, (BTreeSet<String>, BTreeSet<String>, bool)> {
    let mut out = BTreeMap::new();
    for e in tree.read_dir("gamedata/scripts").unwrap() {
        if e.kind != EntryKind::File || !e.name.to_ascii_lowercase().ends_with(".script") {
            continue;
        }
        let t = tokens(&read(tree, &format!("gamedata/scripts/{}", e.name)).unwrap());
        let mut called = BTreeSet::new();
        let mut level = BTreeSet::new();
        let mut constructs = false;
        for w in t.windows(3) {
            match w {
                [Token::Punct(b':'), Token::Name(n), Token::Punct(b'(')] => {
                    called.insert(n.clone());
                }
                [Token::Name(l), Token::Punct(b'.'), Token::Name(n)] if l == "level" => {
                    level.insert(n.clone());
                }
                [Token::Name(n), Token::Punct(b'('), _] if n == "sound_object" => {
                    constructs = true;
                }
                _ => {}
            }
        }
        out.insert(e.name.to_ascii_lowercase(), (called, level, constructs));
    }
    out
}

/// 8 (and 2.1, 2.4, 5.1): who uses which name.
#[test]
fn shipped_uses() {
    let Some((trees, _)) = trees() else {
        return;
    };
    let scripts = scan(&trees.tree);
    let callers = |name: &str| -> BTreeSet<&str> {
        (scripts.iter())
            .filter(|(_, (c, _, _))| c.contains(name))
            .map(|(f, _)| f.as_str())
            .collect()
    };
    for name in USED {
        assert!(!callers(name).is_empty(), "a script calls {name}");
    }
    for name in UNUSED {
        assert!(callers(name).is_empty(), "no script calls {name}");
    }
    assert_eq!(
        callers("play_no_feedback"),
        BTreeSet::from(["sr_teleport.script", "xr_effects.script"])
    );
    // The listing (`lua_help.script`) is no loadable script.
    let constructing: BTreeSet<&str> = (scripts.iter())
        .filter(|(f, (_, _, c))| *c && f.as_str() != "lua_help.script")
        .map(|(f, _)| f.as_str())
        .collect();
    assert_eq!(
        constructing,
        BTreeSet::from([
            "escape_heli.script",
            "xr_sound.script",
            "xr_zones_sound.script"
        ])
    );
    let level_callers = |name: &str| -> BTreeSet<&str> {
        (scripts.iter())
            .filter(|(_, (_, l, _))| l.contains(name))
            .map(|(f, _)| f.as_str())
            .collect()
    };
    assert_eq!(
        level_callers("set_snd_volume"),
        BTreeSet::from([
            "sr_aes_deadzone.script",
            "sr_postprocess.script",
            "sr_psy_antenna.script"
        ])
    );
    assert_eq!(
        level_callers("get_snd_volume"),
        BTreeSet::from(["sr_psy_antenna.script"])
    );
    for name in ["prefetch_sound", "iterate_sounds"] {
        assert_eq!(
            level_callers(name),
            BTreeSet::from(["sound_prefetch.script"]),
            "{name}"
        );
    }
}

/// 2.3, 2.5, 2.6: the trader's sounds (TC2 5's lengths) as sound objects
/// on a started state, played, stopped and ending in the sound system.
#[test]
fn sound_objects_of_the_games_files() {
    let Some((trees, aliases)) = trees() else {
        return;
    };
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
    let sounds = Rc::new(RefCell::new(ScriptSounds::new()));
    let mut scripts = GameScripts::start(Startup {
        trees: trees.clone(),
        aliases: aliases.clone(),
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::clone(&sounds),
        simulation: Rc::default(),
    })
    .unwrap();
    let s = scripts.scripts();
    let player = SpawnData {
        id: 0,
        name: b"owner".to_vec(),
        section: b"owner".to_vec(),
        class_id: class_id(b"O_PHYS_S"),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
    };
    // An object without a binder, the sounds' owner.
    (s.spawn_engine_object(ObjectClass::Named("CPhysicObject"), player)).unwrap();
    let actor = Value::Object(s.game_object(0).unwrap());
    s.exec(
        "=sounds",
        br#"
        function start(actor)
          long = sound_object([[characters_voice\scenario\trader\trader_monolog1]])
          short = sound_object([[characters_voice\scenario\trader\trader1b]])
          missing = sound_object([[no\such\sound]])
          long:play_at_pos(actor, vector():set(1, 2, 3), 0, sound_object.s3d)
          short:play(actor, 0, sound_object.s2d)
          return long:length(), short:length(), long:playing(), short:playing(), missing:playing()
        end
        function stop_long() long:stop() end
        function states() return long:playing(), short:playing() end
        "#,
    )
    .unwrap();
    let got = s.call("start", &[actor]).unwrap();
    assert_eq!(
        got,
        [
            Value::Number(26_369.0),
            Value::Number(3_237.0),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(false),
        ]
    );
    let mut system = SoundSystem::new(16);
    sounds
        .borrow_mut()
        .frame(&mut system, tree, &aliases)
        .unwrap();
    let emitters = system.emitters();
    let long = emitters
        .iter()
        .find(|e| e.name.ends_with("trader_monolog1"))
        .expect("the long sound's emitter");
    assert_eq!(long.position, [1.0, 2.0, 3.0]);
    assert!(!long.two_d);
    assert!(
        emitters
            .iter()
            .any(|e| e.name.ends_with("trader1b") && e.two_d)
    );
    // The short one ends by itself (3,237 ms), the long one is stopped.
    system.update(0, &mut NoDevice);
    system.update(4_000, &mut NoDevice);
    system.update(4_100, &mut NoDevice);
    sounds
        .borrow_mut()
        .frame(&mut system, tree, &aliases)
        .unwrap();
    let s = scripts.scripts();
    assert_eq!(
        s.call("states", &[]).unwrap(),
        [Value::Boolean(true), Value::Boolean(false)]
    );
    s.call("stop_long", &[]).unwrap();
    assert_eq!(
        s.call("states", &[]).unwrap(),
        [Value::Boolean(false), Value::Boolean(false)]
    );
    sounds
        .borrow_mut()
        .frame(&mut system, tree, &aliases)
        .unwrap();
    system.update(4_200, &mut NoDevice);
    assert!(
        system
            .emitters()
            .iter()
            .all(|e| !e.name.ends_with("trader_monolog1")
                || e.phase == volya_engine_core::sound::system::Phase::Ended),
        "the long sound stopped"
    );
}
