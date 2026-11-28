// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's movement, sight and memory names, checked against
//! docs/behavior/script-api-movement-sight-and-memory.md (cited by
//! section): the shipped scripts' uses (14), the creature files' panic
//! thresholds (11.3), and on a game state started with the shipped
//! scripts, `mob_panic`'s reset on a monster and on the trader (11.3) and
//! `xr_effects`' weapon calls on the player (10.4). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees, Token, tokens};
use volya_engine_core::config::{self, PathAliases, value};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::model::transform;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{Event, GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::Prefix;
use volya_engine_script::object_factory::class_id;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;

/// Every file under `dir`, `/`-separated.
fn walk(vfs: &dyn Vfs, dir: &str, out: &mut Vec<String>) {
    for e in vfs.read_dir(dir).unwrap() {
        let path = if dir.is_empty() {
            e.name.clone()
        } else {
            format!("{dir}/{}", e.name)
        };
        match e.kind {
            EntryKind::Dir => walk(vfs, &path, out),
            EntryKind::File => out.push(path),
        }
    }
}

/// The words of `bytes`: runs of letters, digits and `_` not starting with
/// a digit, lowercase (script-loading.md 3).
fn words(bytes: &[u8], out: &mut BTreeSet<String>) {
    for run in bytes.split(|b| !(b.is_ascii_alphanumeric() || *b == b'_')) {
        if run.first().is_some_and(|b| !b.is_ascii_digit()) {
            out.insert(String::from_utf8_lossy(run).to_ascii_lowercase());
        }
    }
}

fn script_words(tokens: &[Token]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in tokens {
        match t {
            Token::Name(n) => {
                out.insert(n.to_ascii_lowercase());
            }
            Token::String(s) => words(s, &mut out),
            _ => {}
        }
    }
    out
}

/// The 250 loadable scripts of script-loading.md 3 and every script's
/// tokens by name (as tests/gog_time.rs computes them).
fn loadable(base: &dyn Vfs) -> (BTreeSet<String>, BTreeMap<String, Vec<Token>>) {
    let mut files = Vec::new();
    walk(base, "", &mut files);
    let lower: Vec<(String, &String)> = files.iter().map(|f| (f.to_ascii_lowercase(), f)).collect();
    let ext = |f: &str, exts: &[&str]| exts.iter().any(|e| f.ends_with(e));
    let mut roots = BTreeSet::from(["_g".to_owned()]);
    for (l, path) in &lower {
        let root = (l.starts_with("gamedata/config/") && ext(l, &[".ltx", ".xml"]))
            || (!l.starts_with("gamedata/config/")
                && !l.starts_with("gamedata/scripts/")
                && ext(l, &[".ltx", ".ini", ".spawn", ".game"]));
        if root {
            words(&read(base, path).unwrap(), &mut roots);
        }
    }
    let binaries = gog_archives::game_files("binaries", |p| {
        let p = p.to_ascii_lowercase();
        p == "bin/xrgame.dll" || p == "bin/xr_3da.exe"
    })
    .unwrap();
    for binary in &binaries {
        words(&fs::read(&binary.file).unwrap(), &mut roots);
    }
    let mut scripts = BTreeMap::new();
    for (l, path) in &lower {
        if let Some(name) = l
            .strip_prefix("gamedata/scripts/")
            .and_then(|n| n.strip_suffix(".script"))
        {
            scripts.insert(name.to_owned(), tokens(&read(base, path).unwrap()));
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending: Vec<String> = (scripts.keys())
        .filter(|n| roots.contains(*n))
        .cloned()
        .collect();
    while let Some(name) = pending.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let found = script_words(&scripts[&name]);
        pending.extend(
            (scripts.keys())
                .filter(|n| found.contains(*n) && !reached.contains(*n))
                .cloned(),
        );
    }
    assert_eq!(reached.len(), 250, "loadable scripts");
    (reached, scripts)
}

/// The names a script calls after `:`, and its bare names.
fn uses(tokens: &[Token]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, t) in tokens.iter().enumerate() {
        let Token::Name(n) = t else { continue };
        if i > 0 && tokens[i - 1] == Token::Punct(b':') {
            out.insert(format!(":{n}"));
        } else {
            out.insert(n.clone());
        }
    }
    out
}

/// 14: the loadable (all) scripts calling each name.
#[test]
fn shipped_uses() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let base = DbVfs::open_game(root, None).unwrap();
    let (reached, scripts) = loadable(&base);
    let used: BTreeMap<&str, BTreeSet<String>> = (scripts.iter())
        .filter(|(n, _)| n.as_str() != "lua_help")
        .map(|(n, t)| (n.as_str(), uses(t)))
        .collect();
    let counts = |n: &str| {
        let all: Vec<&str> = (used.iter())
            .filter(|(_, names)| names.contains(n))
            .map(|(s, _)| *s)
            .collect();
        let loadable = all.iter().filter(|s| reached.contains(**s)).count();
        (loadable, all.len())
    };
    for (n, want) in [
        ("set_desired_position", (11, 24)),
        ("set_desired_direction", (12, 25)),
        ("best_danger", (7, 7)),
        ("set_item", (3, 10)),
        ("add_animation", (3, 5)),
        ("get_monster_hit_info", (3, 3)),
        ("set_sight", (2, 9)),
        ("hide_weapon", (2, 3)),
        ("restore_weapon", (2, 3)),
        ("set_patrol_path", (1, 3)),
        ("add_restrictions", (1, 1)),
    ] {
        assert_eq!(counts(&format!(":{n}")), want, "{n}");
    }
    for n in [
        ":best_cover",
        ":safe_cover",
        ":memory_visible_objects",
        ":memory_hit_objects",
        "ef_storage",
    ] {
        assert_eq!(counts(n).1, 0, "{n}");
    }
    // 11.3: `mob_panic` is the only caller of the panic thresholds.
    for n in [
        ":set_custom_panic_threshold",
        ":set_default_panic_threshold",
    ] {
        let callers: Vec<&str> = (used.iter())
            .filter(|(_, names)| names.contains(n))
            .map(|(s, _)| *s)
            .collect();
        assert_eq!(callers, ["mob_panic"], "{n}");
    }
}

/// 11.3: the creature files' panic thresholds, 0 to 0.5.
#[test]
fn creature_panic_thresholds() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let mut seen = BTreeMap::new();
    for e in tree.read_dir("gamedata/config/creatures").unwrap() {
        if e.kind != EntryKind::File || !e.name.ends_with(".ltx") {
            continue;
        }
        let text = read(&tree, &format!("gamedata/config/creatures/{}", e.name)).unwrap();
        for line in text.split(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(line);
            let line = line.split(';').next().unwrap_or("");
            if let Some((key, v)) = line.split_once('=')
                && key.trim() == "panic_threshold"
            {
                let v = value::float(v.trim());
                assert!((0.0..=0.5).contains(&v), "{}: {v}", e.name);
                *seen.entry(v.to_bits()).or_insert(0) += 1;
            }
        }
    }
    for common in [0.05f32, 0.1, 0.2, 0.5] {
        assert!(seen.contains_key(&common.to_bits()), "{common}");
    }
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

/// 11.3 and 10.4 on a started game state: the shipped `mob_panic` sets a
/// dog's threshold from its scheme's `panic` and back to the section's
/// `panic_threshold`, does nothing on the trader; `xr_effects` hides and
/// restores the player's weapon.
#[test]
fn the_traders_scheme_calls() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let tree = &trees.tree;
    let system = config::load_aliased(tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, &aliases, "$game_config$", "game.ltx").unwrap();
    let dog_threshold = value::float(system.get("dog_weak", "panic_threshold").unwrap().unwrap());
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
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    let mut scripts = GameScripts::start(Startup {
        trees: trees.clone(),
        aliases,
        prefix: Prefix::Default,
        services,
        objects: Rc::clone(&objects),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
        window_files: Rc::default(),
    })
    .unwrap();
    let s = scripts.scripts();
    // Bound by an empty binding, which finds no function (spec L 3.1):
    // their binders' own calls aren't this test's.
    for (class, data) in [
        (ObjectClass::Player, spawn_data(0, "actor", "O_ACTOR")),
        (
            ObjectClass::Named("CAI_Dog"),
            spawn_data(1, "dog_weak", "AI_DOG_R"),
        ),
        (
            ObjectClass::Named("CAI_Trader"),
            spawn_data(2, "m_trader", "AI_TRADE"),
        ),
    ] {
        (s.spawn_engine_object_with_binding(class, data, Some(b""))).unwrap();
    }
    let handle = |id: u16| Value::Object(s.game_object(id).unwrap());
    let (actor, dog, trader) = (handle(0), handle(1), handle(2));
    s.exec(
        "=check",
        br#"
        function reset(npc, text)
          mob_panic.reset_panic(npc, "mob_panic", { ini = create_ini_file(text) }, "s")
        end
        function weapon(actor)
          db.actor = actor
          xr_effects.disable_ui_elements(actor, nil, { "weapon" })
          xr_effects.enable_ui_elements(actor, nil, { "weapon" })
        end
        "#,
    )
    .unwrap();
    let threshold = |id: u16| {
        let o = objects.borrow();
        let m = o.get(id).and_then(|o| o.movement.monster.as_ref());
        m.map(|m| (m.panic_threshold, m.default_panic_threshold))
    };
    assert_eq!(threshold(1), Some((dog_threshold, dog_threshold)), "loaded");
    s.call("reset", &[dog.clone(), Value::from("[s]\npanic = 0.3\n")])
        .unwrap();
    assert_eq!(threshold(1), Some((0.3, dog_threshold)), "the scheme's");
    s.call("reset", &[dog, Value::from("[s]\n")]).unwrap();
    assert_eq!(
        threshold(1),
        Some((dog_threshold, dog_threshold)),
        "the default"
    );
    for text in ["[s]\npanic = 0.3\n", "[s]\n"] {
        s.call("reset", &[trader.clone(), Value::from(text)])
            .unwrap();
    }
    assert_eq!(threshold(2), None, "the trader is no monster");
    objects.borrow_mut().take_events();
    s.call("weapon", &[actor]).unwrap();
    assert_eq!(
        objects.borrow_mut().take_events(),
        [
            Event::HideWeapon {
                player: 0,
                hide: true,
                slot: -1
            },
            Event::HideWeapon {
                player: 0,
                hide: false,
                slot: -1
            },
        ]
    );
}
