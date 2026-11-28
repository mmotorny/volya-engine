// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's entry points (BYOA), checked against
//! docs/behavior/script-api-engine-entry-points.md: the data that names
//! them (2.1 to 2.6), then a New Game's calls into the shipped scripts in
//! section 3's order over [`GameScripts`], as 3.6's worked example has
//! them before the level's client objects. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::memory_manager::GAME_SCRIPTS;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, read};
use volya_engine_script::Value;
use volya_engine_script::entry_points::{
    EngineCall, GameScripts, STAND_IN_BASES, Startup, TutorialThresholds,
};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::{Common, Prefix};
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;

/// The engine-read keys' values (2.1 to 2.6).
#[test]
fn the_data_naming_the_entry_points() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let script = config::load_aliased(&tree, &aliases, "$game_config$", "script.ltx").unwrap();
    let common = Common::read(&tree, &aliases).unwrap();
    // 2.1: the list is `_G`; 2.2: the game-type function.
    assert_eq!(common.scripts, ["_G"]);
    assert_eq!(
        common.game_type_function.as_deref(),
        Some("game_registrator.get_game_clsid")
    );
    // 2.6: read by no binary.
    assert_eq!(
        script.get("common", "ui_type_clsid_factory").flatten(),
        Some("ui_registrator.get_ui_clsid")
    );
    // TODO(#997): gap 3; the game-type script list is empty either way.
    let single = script.get("single", "script").flatten().unwrap_or("");
    assert_eq!(single.trim(), "", "[single] script");
    let system = config::load_aliased(&tree, &aliases, "$game_config$", "system.ltx").unwrap();
    // 2.3: the start callback.
    assert_eq!(
        system.get("alife", "start_game_callback").flatten(),
        Some("_G.start_game_callback")
    );
    // 2.4: the thresholds of `config\system.ltx`.
    let t = TutorialThresholds::read(&system).unwrap();
    assert_eq!(
        t,
        TutorialThresholds {
            power: 0.1,
            max_power: 0.75,
            bleeding: 0.4,
            satiety: 0.5,
            radiation: 0.1,
            weapon_jammed: 0.9,
            psy_health: 0.5,
        }
    );
    // 2.1: no script contains `_G_initialize`.
    let scripts = tree.read_dir("gamedata\\scripts").unwrap();
    let mut texts = 0;
    for entry in &scripts {
        let path = format!("gamedata\\scripts\\{}", entry.name);
        let Ok(bytes) = read(&tree, &path) else {
            continue;
        };
        texts += 1;
        assert!(!bytes.windows(13).any(|w| w == b"_G_initialize"), "{path}");
    }
    assert!(texts > 400, "{texts} scripts");
}

/// The engine state and startup over the shipped files.
fn started(trees: Rc<GameTrees>, aliases: &PathAliases) -> GameScripts {
    let tree = &trees.tree;
    let system = config::load_aliased(tree, aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, aliases, "$game_config$", "game.ltx").unwrap();
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
    GameScripts::start(Startup {
        trees,
        aliases: aliases.clone(),
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        files,
    })
    .unwrap()
}

/// Section 3 over the shipped scripts, to the start of the level's client
/// objects (3.6): startup, the server's game type, the rebuild with a
/// second startup, the start callback, the client's game type.
#[test]
fn a_new_games_calls_into_the_shipped_scripts() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let settings =
        config::load_aliased(&trees.tree, &aliases, "$game_config$", "system.ltx").unwrap();

    let before = GAME_SCRIPTS.in_use_bytes();
    let start = Instant::now();
    let mut g = started(trees.clone(), &aliases);
    let startup = start.elapsed();
    let after_startup = GAME_SCRIPTS.in_use_bytes();
    let options = "all/single/alife/new";
    let start = Instant::now();
    assert_eq!(g.create_game_object("single", true).unwrap(), "SV_SINGL");
    g.create_simulation(options, &settings).unwrap();
    assert_eq!(g.create_game_object("single", false).unwrap(), "CL_SINGL");
    let new_game = start.elapsed();
    let after_new_game = GAME_SCRIPTS.in_use_bytes();
    eprintln!(
        "startup {startup:.1?}, {} K; the New Game's calls {new_game:.1?}, {} K after",
        (after_startup - before) / 1024,
        (after_new_game - before) / 1024
    );

    let startup = EngineCall::Startup {
        registrators: vec![
            "class_registrator.register".into(),
            "game_registrator.register".into(),
        ],
        initialized: Vec::new(),
    };
    let game_type = |server: bool, result: &str| EngineCall::GameType {
        function: "game_registrator.get_game_clsid".into(),
        game_type: "single".into(),
        server,
        result: result.into(),
    };
    assert_eq!(
        g.calls(),
        [
            startup.clone(),
            game_type(true, "SV_SINGL"),
            EngineCall::Rebuild,
            startup,
            EngineCall::StartGameCallback("_G.start_game_callback".into()),
            game_type(false, "CL_SINGL"),
        ]
    );
    // 2.3: the callback's namespaces were loaded on first access, in the
    // new state; the stand-ins let `_g.script` run to its end.
    let loaded = g
        .scripts()
        .exec(
            "=t",
            b"local t = {} for _, n in ipairs({'task_manager', 'treasure_manager', \
              'xr_sound', 'dialog_manager'}) do t[#t + 1] = type(rawget(_G, n)) end \
              return table.concat(t, ' '), type(rawget(_G, 'on_actor_psy'))",
        )
        .unwrap();
    assert_eq!(
        loaded,
        [Value::from("table table table table"), "function".into()]
    );
    // Spec C 5.3: the scripts' registrations are in `clsid`, the trader's
    // and the stalker's among them.
    let ids = g
        .scripts()
        .exec(
            "=clsid",
            b"return clsid.script_stalker, clsid.script_trader",
        )
        .unwrap();
    assert_eq!(ids, [Value::Number(33.0), Value::Number(35.0)]);
    // What failed to load in the new state, for the record (engine names of
    // specs not implemented).
    let failures = g.scripts().take_failures();
    for f in &failures {
        eprintln!("not loaded: {} ({})", f.name, f.message);
    }
    assert_eq!(STAND_IN_BASES.len(), 33);
}
