// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's effects and weather through the shipped scripts,
//! checked against docs/behavior/script-api-effects-and-weather.md (cited
//! by section): the `level` names the loadable scripts use (11), and on a
//! started game state the shipped animation files, weather names and
//! colour animations (4.1, 6.1, 7, 8, 12). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use shipped_scripts::{GameTrees, Token, loadable_name, tokens};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::model::transform;
use volya_engine_script::Value;
use volya_engine_script::effects_and_weather::EffectsAndWeather;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::Prefix;
use volya_engine_script::object_factory::class_id;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;

/// The aliases scripts and animation files use (docs/formats/ltx-config.md
/// 10).
const FSGAME: &[u8] = b"$game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_anims$ =    true | false| $game_data$    | anims\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n";

fn trees() -> Option<(Rc<GameTrees>, PathAliases)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    Some((Rc::new(GameTrees { tree, root }), aliases))
}

/// 11: which scripts with loadable names use the `level.` names of this
/// spec.
#[test]
fn shipped_uses() {
    let Some((trees, _)) = trees() else {
        return;
    };
    let tree = &trees.tree;
    let mut users: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in tree.read_dir("gamedata/scripts").unwrap() {
        let name = e.name.to_ascii_lowercase();
        if e.kind != EntryKind::File || !name.ends_with(".script") {
            continue;
        }
        if !loadable_name(name.trim_end_matches(".script")) {
            continue;
        }
        let t = tokens(&read(tree, &format!("gamedata/scripts/{}", e.name)).unwrap());
        let mut used = std::collections::BTreeSet::new();
        for w in t.windows(3) {
            if let [Token::Name(l), Token::Punct(b'.'), Token::Name(n)] = w
                && l == "level"
            {
                used.insert(n.clone());
            }
        }
        for n in used {
            users.entry(n).or_default().push(name.clone());
        }
    }
    // 11 counts the 250 loadable scripts of script-loading.md 3; a scan of
    // the names checks the users it names and the names none uses.
    let users_of = |n: &str| users.get(n).cloned().unwrap_or_default();
    for name in [
        "add_complex_effector",
        "remove_complex_effector",
        "get_weather",
        "environment",
    ] {
        assert_eq!(users_of(name), Vec::<String>::new(), "{name}");
    }
    let has = |n: &str, script: &str| users_of(n).iter().any(|s| s == script);
    for (name, script) in [
        ("rain_factor", "gulag_bar.script"),
        ("rain_factor", "sound_theme.script"),
        ("set_weather", "level_weathers.script"),
        ("is_wfx_playing", "xr_conditions.script"),
        ("add_cam_effector", "xr_effects.script"),
        ("set_pp_effector_factor", "xr_detector.script"),
        ("set_pp_effector_factor", "sr_psy_antenna.script"),
    ] {
        assert!(has(name, script), "{name}: {:?}", users_of(name));
    }
}

/// 4.1, 6.1, 7, 8, 12 on a started state with a player: the shipped files
/// found, the weather names of `environment.ltx`, `rain_factor` as the
/// level blends it, and the colour animations of `lanims.xr`.
#[test]
fn the_games_effects_and_weather() {
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
    let state = Rc::new(RefCell::new(EffectsAndWeather::new()));
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
        sounds: Rc::default(),
        simulation: Rc::default(),
        window_files: Rc::default(),
        level: Rc::default(),
        effects_and_weather: Rc::clone(&state),
    })
    .unwrap();
    let s = scripts.scripts();
    let player = SpawnData {
        id: 0,
        name: b"actor".to_vec(),
        section: b"actor".to_vec(),
        class_id: class_id(b"O_ACTOR"),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
        server: None,
    };
    // Bound by a function that does nothing: the player's own binding
    // needs the HUD (TODO(#1120)).
    s.exec("=binding", b"function no_binding(obj) end").unwrap();
    s.spawn_engine_object_with_binding(ObjectClass::Player, player, Some(b"no_binding"))
        .unwrap();
    s.exec(
        "=effects",
        br#"
        local function error_of(f, ...)
          local ok, e = pcall(f, ...)
          return ok and "no error" or e
        end
        function animated() return error_of(level.add_pp_effector, "blink.ppe", 2000, false) end
        function camera()
          return error_of(level.add_cam_effector2, [[camera_effects\earthquake.anm]], 1, false, "")
        end
        function weather() return error_of(level.set_weather, "default", true) end
        function weather_fx() return error_of(level.set_weather_fx, "surge_day") end
        function invalid() return level.set_weather("no_such_weather", false) end
        function rain() return level.rain_factor() end
        function psy() return color_animator([[levels\psy_antenna\starting]]):length() end
        function stopping() return color_animator([[levels\psy_antenna\stopping]]):length() end
        "#,
    )
    .unwrap();
    // The shipped files are found: what follows is the issues'. `default`
    // is a cycle and `surge_day` an effect of `environment.ltx`.
    for (f, issue) in [
        ("animated", "(#1193)"),
        ("camera", "(#1194)"),
        ("weather", "(#1195)"),
        ("weather_fx", "(#1195)"),
    ] {
        let error = match s.call(f, &[]).unwrap().as_slice() {
            [Value::String(e)] => String::from_utf8_lossy(e).into_owned(),
            other => panic!("{f}: {other:?}"),
        };
        assert!(error.ends_with(issue), "{f}: {error}");
    }
    assert_eq!(s.call("invalid", &[]).unwrap(), []);
    let log = services.borrow().console.log().join("\n");
    assert!(
        log.contains("! Invalid weather name: no_such_weather"),
        "{log}"
    );
    state.borrow_mut().environment.rain_density = 0.25;
    assert_eq!(s.call("rain", &[]).unwrap(), [Value::Number(0.25)]);
    // 8, 12: 100 frames at 5 and 10 frames per second.
    assert_eq!(s.call("psy", &[]).unwrap(), [Value::Number(20_000.0)]);
    assert_eq!(s.call("stopping", &[]).unwrap(), [Value::Number(10_000.0)]);
}
