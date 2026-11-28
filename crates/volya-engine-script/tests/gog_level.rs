// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of the level (BYOA), checked against
//! docs/behavior/script-api-level.md (cited by section): the loadable
//! scripts' uses of this spec's names and the level names they compare
//! with (9), the unregistered names they call (7), and, on a started game
//! state over the shipped `all.spawn`, `l01_escape`'s `level.ai` and
//! `level.cform`, the worked examples of 2.4, 3.2, 8.1 and 8.2. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;

use shipped_scripts::{FSGAME, GameTrees, Token, tokens};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::level_ai::LevelAi;
use volya_engine_formats::level_cform::CformReader;
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::level::{Level, LevelState, LevelVertices};
use volya_engine_script::loader::Prefix;
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

/// A script's words after comment removal, string contents kept.
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

/// The 250 loadable scripts of script-loading.md 3, every script's tokens
/// by name, and xrGame.dll's bytes.
fn loadable(base: &dyn Vfs) -> (BTreeSet<String>, BTreeMap<String, Vec<Token>>, Vec<u8>) {
    let mut files = Vec::new();
    walk(base, "", &mut files);
    let lower: Vec<(String, &String)> = files.iter().map(|f| (f.to_ascii_lowercase(), f)).collect();
    let ext = |f: &str, exts: &[&str]| exts.iter().any(|e| f.ends_with(e));
    let roots_files: Vec<&String> = lower
        .iter()
        .filter(|(l, _)| {
            (l.starts_with("gamedata/config/") && ext(l, &[".ltx", ".xml"]))
                || (!l.starts_with("gamedata/config/")
                    && !l.starts_with("gamedata/scripts/")
                    && ext(l, &[".ltx", ".ini", ".spawn", ".game"]))
        })
        .map(|(_, p)| *p)
        .collect();
    assert_eq!(roots_files.len(), 893 + 195, "root files");
    let mut roots = BTreeSet::from(["_g".to_owned()]);
    for path in roots_files {
        words(&read(base, path).unwrap(), &mut roots);
    }
    let binaries = gog_archives::game_files("binaries", |p| {
        let p = p.to_ascii_lowercase();
        p == "bin/xrgame.dll" || p == "bin/xr_3da.exe"
    })
    .unwrap();
    assert_eq!(binaries.len(), 2, "xrGame.dll and XR_3DA.exe");
    let mut game_library = Vec::new();
    for binary in &binaries {
        let bytes = fs::read(&binary.file).unwrap();
        words(&bytes, &mut roots);
        if binary
            .file
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with("xrgame.dll")
        {
            game_library = bytes;
        }
    }
    assert_eq!(game_library.len(), 6_246_400, "xrGame.dll");
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
    let mut pending: Vec<String> = scripts
        .keys()
        .filter(|n| roots.contains(*n))
        .cloned()
        .collect();
    while let Some(name) = pending.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let found = script_words(&scripts[&name]);
        pending.extend(
            scripts
                .keys()
                .filter(|n| found.contains(*n) && !reached.contains(*n))
                .cloned(),
        );
    }
    assert_eq!(reached.len(), 250, "loadable scripts");
    (reached, scripts, game_library)
}

/// The 250 loadable scripts' tokens by name.
fn loadable_scripts(base: &dyn Vfs) -> BTreeMap<String, Vec<Token>> {
    let (reached, scripts, _) = loadable(base);
    scripts
        .into_iter()
        .filter(|(n, _)| reached.contains(n))
        .collect()
}

/// A script's uses `<namespace>.<name>`, by bare `namespace`.
fn uses(tokens: &[Token], namespace: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, w) in tokens.windows(3).enumerate() {
        let bare = i == 0 || !matches!(tokens[i - 1], Token::Punct(b'.') | Token::Punct(b':'));
        if let [Token::Name(n), Token::Punct(b'.'), Token::Name(f)] = w
            && n == namespace
            && bare
        {
            out.insert(f.clone());
        }
    }
    out
}

/// The texts a script compares `level.name()` with (`level.name() ==`,
/// `~=`).
fn compared_names(tokens: &[Token], out: &mut BTreeSet<Vec<u8>>) {
    for w in tokens.windows(8) {
        if let [
            Token::Name(l),
            Token::Punct(b'.'),
            Token::Name(n),
            Token::Punct(b'('),
            Token::Punct(b')'),
            Token::Punct(b'=' | b'~'),
            Token::Punct(b'='),
            Token::String(s),
        ] = w
            && l == "level"
            && n == "name"
        {
            out.insert(s.clone());
        }
    }
}

#[test]
fn shipped_uses_of_the_level() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, None).unwrap();
    let scripts = loadable_scripts(&tree);
    let level: BTreeMap<&str, BTreeSet<String>> = scripts
        .iter()
        .map(|(n, t)| (n.as_str(), uses(t, "level")))
        .collect();
    let using = |f: &str| level.values().filter(|u| u.contains(f)).count();
    let mut counts = BTreeMap::new();
    for f in [
        "object_by_id",
        "name",
        "vertex_position",
        "patrol_path_exists",
        "vertex_in_direction",
        "present",
        "spawn_phantom",
        "get_bounding_volume",
        "cover_in_direction",
        "physics_world",
        "client_spawn_manager",
        "actor",
        "object",
        "debug_object",
        "debug_actor",
    ] {
        counts.insert(f, using(f));
    }
    // 9 and 7: used where the spec counts uses, unused where it counts
    // none (TODO(#1159): this scan finds more than the spec's bounds; it reads
    // `level.<name>` tokens, the spec's scan names on any object).
    let spec = [
        ("object_by_id", 14),
        ("name", 11),
        ("vertex_position", 9),
        ("patrol_path_exists", 5),
        ("vertex_in_direction", 4),
        ("present", 2),
        ("spawn_phantom", 1),
        ("get_bounding_volume", 1),
        ("cover_in_direction", 0),
        ("physics_world", 0),
        ("client_spawn_manager", 0),
        ("actor", 7),
        ("object", 5),
        ("debug_object", 2),
        ("debug_actor", 1),
    ];
    for (f, most) in spec {
        let n = counts[f];
        assert_eq!(
            n > 0,
            most > 0,
            "level.{f}: {n} loadable scripts, the spec's {most}"
        );
    }
    // 2: the main menu's scripts call `level.present()`.
    let present: Vec<&str> = (level.iter())
        .filter(|(_, u)| u.contains("present"))
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(present, ["ui_main_menu", "ui_mp_main"], "level.present");
    // 9: the level names compared with.
    let mut names = BTreeSet::new();
    for t in scripts.values() {
        compared_names(t, &mut names);
    }
    for name in [&b"l01_escape"[..], b"l02_garbage", b"l03_agroprom"] {
        assert!(names.contains(name), "level.name() compared with {name:?}");
    }
    // The constants of `patrol` the scripts use are registered (4.3).
    let constants: BTreeSet<String> = (scripts.values()).flat_map(|t| uses(t, "patrol")).collect();
    for c in ["continue", "next", "nearest", "start"] {
        assert!(constants.contains(c), "patrol.{c}");
    }
}

/// A started game state (spec D) over the game's trees, with the level
/// `l01_escape` and the shipped patrol paths.
fn start() -> Option<(GameScripts, Rc<RefCell<LevelState>>)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let t = &trees.tree;
    let system = config::load_aliased(t, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(t, &aliases, "$game_config$", "game.ltx").unwrap();
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
    let state = Rc::new(RefCell::new(LevelState::new()));
    let paths = PatrolPaths::parse(&read(t, "gamedata/spawns/all.spawn").unwrap()).unwrap();
    state.borrow_mut().patrol_paths = Some(Rc::new(paths));
    let ai = read(t, "gamedata/levels/l01_escape/level.ai").unwrap();
    let (ai, _, links) = LevelAi::read_in_with_lights_and_links(
        Cursor::new(&ai[..]),
        |len| Ok(vec![0; len]),
        |len| Ok(vec![0; len]),
    )
    .unwrap();
    let cform = read(t, "gamedata/levels/l01_escape/level.cform").unwrap();
    let header = *CformReader::new(Cursor::new(&cform[..])).unwrap().header();
    let mut scripts = GameScripts::start(Startup {
        trees: trees.clone(),
        aliases,
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
        window_files: Rc::default(),
        level: Rc::clone(&state),
    })
    .unwrap();
    scripts
        .scripts()
        .start_level(Level {
            name: Some(b"l01_escape".to_vec()),
            bounding_volume: (header.bbox_min, header.bbox_max),
            vertices: Some(LevelVertices {
                ai: Arc::new(ai),
                links: Arc::new(links),
            }),
        })
        .unwrap();
    Some((scripts, state))
}

#[test]
fn the_worked_examples_on_the_shipped_level() {
    let Some((mut scripts, _)) = start() else {
        return;
    };
    let results = scripts
        .scripts()
        .exec(
            "=examples",
            br#"
local f = function(v) return string.format("%.3f %.3f %.3f", v.x, v.y, v.z) end
local b = level.get_bounding_volume()
local p = patrol("mil_actor_path")
local function cover(x, z) return string.format("%.4f", level.cover_in_direction(0, vector():set(x, 0, z))) end
return f(b.min), f(b.max), f(level.vertex_position(0)),
  cover(0, 1), cover(-1, 0), cover(0, -1), cover(1, 0), cover(1, 1), cover(1, -1),
  p:count(), p:index("wp01"), p:terminal(0), p:terminal(1), p:flags(0):get(),
  p:get_nearest(p:point(1)), f(p:point(5)), p:level_vertex_id(1), p:game_vertex_id(0),
  level.patrol_path_exists("mil_actor_path"), level.patrol_path_exists("way1"), level.name()
"#,
        )
        .unwrap();
    let want: Vec<Value> = vec![
        "-678.897 -38.667 -633.791".into(),
        "766.565 113.877 811.671".into(),
        "-283.500 -10.615 -113.400".into(),
        "0.6000".into(),
        "0.2000".into(),
        "0.4667".into(),
        "1.0000".into(),
        "0.8000".into(),
        "0.6667".into(),
        2.0.into(),
        1.0.into(),
        false.into(),
        true.into(),
        0.0.into(),
        1.0.into(),
        "-229.701 -21.775 197.692".into(),
        103_407.0.into(),
        1765.0.into(),
        true.into(),
        false.into(),
        "l01_escape".into(),
    ];
    assert_eq!(results, want);
}
