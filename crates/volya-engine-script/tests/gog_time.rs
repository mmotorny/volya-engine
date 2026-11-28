// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of the time names, checked against
//! docs/behavior/script-api-time.md (cited by section): the 250 loadable
//! scripts (docs/behavior/script-loading.md 3, recomputed here) and their
//! uses of this spec's names (8), the New Game clock from the shipped
//! `[alife]` (3.1), and the names on a game state started with the shipped
//! scripts, `_g.script`'s replacement for `time_global` left undefined (6).
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees, Token, tokens};
use volya_engine_core::clock::GameClock;
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::Prefix;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;
use volya_engine_script::time::GameTimes;

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

/// The 250 loadable scripts of script-loading.md 3 and every script's
/// tokens by name.
fn loadable(base: &dyn Vfs) -> (BTreeSet<String>, BTreeMap<String, Vec<Token>>) {
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
    (reached, scripts)
}

/// The members called after `:` (8).
const METHODS: [&str; 5] = [
    "diffSec",
    "setHMSms",
    "setHMS",
    "timeToString",
    "dateToString",
];

/// A script's uses of this spec's names: `time_global(` and
/// `profile_timer` by bare name, `game.` and `level.` functions, the
/// methods after `:`.
fn uses(tokens: &[Token]) -> BTreeSet<String> {
    let name = |i: usize| match tokens.get(i) {
        Some(Token::Name(n)) => Some(n.as_str()),
        _ => None,
    };
    let punct = |i: usize, c: u8| tokens.get(i) == Some(&Token::Punct(c));
    let mut out = BTreeSet::new();
    for i in 0..tokens.len() {
        let Some(n) = name(i) else { continue };
        let after = |c: u8| i > 0 && punct(i - 1, c);
        let bare = !after(b'.') && !after(b':');
        if bare && n == "time_global" && punct(i + 1, b'(') {
            out.insert(n.to_owned());
        }
        if bare && n == "profile_timer" {
            out.insert(n.to_owned());
        }
        if bare
            && (n == "game" || n == "level")
            && punct(i + 1, b'.')
            && let Some(f) = name(i + 2)
        {
            out.insert(format!("{n}.{f}"));
        }
        if after(b':') && METHODS.contains(&n) {
            out.insert(format!(":{n}"));
        }
    }
    out
}

#[test]
fn shipped_uses_of_the_time_names() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let base = DbVfs::open_game(root, None).unwrap();
    let (reached, scripts) = loadable(&base);
    let used: BTreeMap<&str, BTreeSet<String>> = scripts
        .iter()
        .filter(|(n, _)| n.as_str() != "lua_help")
        .map(|(n, t)| (n.as_str(), uses(t)))
        .collect();
    // Loadable scripts using `n`, and all scanned scripts.
    let counts = |n: &str| {
        let all: Vec<&str> = (used.iter())
            .filter(|(_, names)| names.contains(n))
            .map(|(s, _)| *s)
            .collect();
        let loadable = all.iter().filter(|s| reached.contains(**s)).count();
        (loadable, all.len())
    };
    // 8, with the scanned counts the spec gives.
    for (n, want) in [
        ("time_global", (40, 57)),
        ("game.get_game_time", (15, 18)),
        (":diffSec", (10, 12)),
        ("level.get_time_hours", (6, 7)),
        ("game.time", (4, 8)),
        ("game.CTime", (4, 6)),
        (":setHMSms", (2, 2)),
        (":setHMS", (0, 2)),
        (":dateToString", (0, 1)),
    ] {
        assert_eq!(counts(n), want, "{n}");
    }
    // 8, one loadable script each.
    for n in [
        "level.get_time_minutes",
        "level.get_time_days",
        "level.get_time_factor",
        "level.set_time_factor",
        ":timeToString",
        "profile_timer",
    ] {
        assert_eq!(counts(n).0, 1, "{n}");
    }
    // 7: the only user of `profile_timer` is `profiler`; 5.5: of
    // `timeToString`, `game_stats`; only `test_script` calls
    // `dateToString`, and it isn't loadable.
    let users = |n: &str| -> Vec<&str> {
        (used.iter())
            .filter(|(s, names)| names.contains(n) && reached.contains(**s))
            .map(|(s, _)| *s)
            .collect()
    };
    assert_eq!(users("profile_timer"), ["profiler"]);
    assert_eq!(users(":timeToString"), ["game_stats"]);
    assert!(used["test_script"].contains(":dateToString") && !reached.contains("test_script"));
    // No `CTime` outside `game`, nor a script `time_global` of its own
    // in a loadable script but `_g` (6).
    for (s, t) in &scripts {
        if !reached.contains(s) {
            continue;
        }
        let defines = t.windows(2).any(|w| {
            matches!(w, [Token::Name(f), Token::Name(n)] if f == "function" && n == "time_global")
        });
        assert!(!defines || s == "_g", "{s} defines time_global");
    }
}

/// 3.1, 5.5, 6: a game state started with the shipped scripts reads the
/// New Game clock of the shipped `[alife]`, and `time_global` is the
/// engine's.
#[test]
fn the_names_on_a_started_game_state() {
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
    let alife = config::load_aliased(tree, &aliases, "$game_config$", "alife.ltx").unwrap();
    let clock = GameClock::new_game(&alife).unwrap();
    assert_eq!(clock.time(), 63_471_533_400_000, "the New Game start");
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
    let times = Rc::new(RefCell::new(GameTimes::new()));
    let mut scripts = GameScripts::start(Startup {
        trees: trees.clone(),
        aliases,
        prefix: Prefix::Default,
        services: Rc::clone(&services),
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::clone(&times),
        sounds: Rc::default(),
    })
    .unwrap();
    times
        .borrow_mut()
        .level_frame(clock.time(), clock.time_factor());
    services.borrow_mut().device.frame(777, 0.016, false);
    let results = scripts
        .scripts()
        .exec(
            "=check",
            br#"
local t = game.get_game_time()
return t:timeToString(game.CTime.TimeToSeconds), t:dateToString(game.CTime.DateToDay),
  game.time(), level.get_time_hours(), level.get_time_minutes(), level.get_time_factor(),
  time_global()
"#,
        )
        .unwrap();
    assert_eq!(
        results,
        [
            Value::from("05:30:00"),
            Value::from("01/05/2012"),
            Value::from(506_699_712.0),
            Value::from(5.0),
            Value::from(30.0),
            Value::from(10.0),
            Value::from(777.0),
        ]
    );
}
