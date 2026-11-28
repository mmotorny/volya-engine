// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of the object event callbacks (BYOA), checked
//! against docs/behavior/script-api-object-event-callbacks.md: the 250
//! loadable scripts (docs/behavior/script-loading.md 3, recomputed here)
//! and their uses of this spec's names (1.1, 11), `set_use_callback`'s
//! scripts and its absence from the game library (9), the clearing form the
//! scripts use (3.1), and the trader door's binder and door scheme (10).
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_script::game_object::callbacks::CALLBACK_IDS;

use shipped_scripts::{Token, tokens};

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

/// `game_object`'s members of this spec (1.1).
const MEMBERS: [&str; 4] = [
    "set_callback",
    "set_fastcall",
    "set_enemy_callback",
    "set_patrol_extrapolate_callback",
];

/// The `level` functions of this spec (1.1).
const LEVEL: [&str; 3] = ["add_call", "remove_call", "remove_calls_for_object"];

/// Whether a script declares `name` itself: as a local or a function's
/// parameter.
fn declares(tokens: &[Token], name: &str) -> bool {
    let is = |t: &Token| matches!(t, Token::Name(n) if n == name);
    if tokens
        .windows(2)
        .any(|w| matches!(&w[0], Token::Name(l) if l == "local") && is(&w[1]))
    {
        return true;
    }
    let mut i = 0;
    while i < tokens.len() {
        if matches!(&tokens[i], Token::Name(f) if f == "function") {
            let Some(open) = tokens[i..].iter().position(|t| *t == Token::Punct(b'(')) else {
                break;
            };
            if tokens[i + open..]
                .iter()
                .take_while(|t| **t != Token::Punct(b')'))
                .any(is)
            {
                return true;
            }
            i += open;
        }
        i += 1;
    }
    false
}

/// A script's uses of this spec's names: `callback` by bare name (unless
/// the script declares its own), its constants as `callback.name`, the
/// members and `set_use_callback` after `:`, the `level` functions as
/// `level.name`.
fn uses(tokens: &[Token]) -> BTreeSet<String> {
    let name = |i: usize| match tokens.get(i) {
        Some(Token::Name(n)) => Some(n.as_str()),
        _ => None,
    };
    let punct = |i: usize, c: u8| tokens.get(i) == Some(&Token::Punct(c));
    let own_callback = declares(tokens, "callback");
    let mut out = BTreeSet::new();
    for i in 0..tokens.len() {
        let Some(n) = name(i) else { continue };
        let after = |c: u8| i > 0 && punct(i - 1, c);
        let bare = !after(b'.') && !after(b':');
        if bare && n == "callback" && !own_callback {
            out.insert(n.to_owned());
            if punct(i + 1, b'.')
                && let Some(constant) = name(i + 2)
            {
                out.insert(format!("callback.{constant}"));
            }
        }
        if after(b':') && (MEMBERS.contains(&n) || n == "set_use_callback") && punct(i + 1, b'(') {
            out.insert(n.to_owned());
        }
        if bare
            && n == "level"
            && punct(i + 1, b'.')
            && let Some(f) = name(i + 2)
            && LEVEL.contains(&f)
        {
            out.insert(format!("level.{f}"));
        }
    }
    out
}

/// Whether `bytes` contain `text`.
fn contains(bytes: &[u8], text: &[u8]) -> bool {
    bytes.windows(text.len()).any(|w| w == text)
}

#[test]
fn shipped_uses_of_object_event_callbacks() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let base = DbVfs::open_game(root, None).unwrap();
    let (reached, scripts, game_library) = loadable(&base);
    let used: BTreeMap<&str, BTreeSet<String>> = scripts
        .iter()
        .filter(|(n, _)| n.as_str() != "lua_help")
        .map(|(n, t)| (n.as_str(), uses(t)))
        .collect();
    // The scripts using `n`, and how many of them are loadable.
    let using = |n: &str| -> (Vec<&str>, usize) {
        let all: Vec<&str> = used
            .iter()
            .filter(|(_, names)| names.contains(n))
            .map(|(s, _)| *s)
            .collect();
        let loadable = all.iter().filter(|s| reached.contains(**s)).count();
        (all, loadable)
    };
    let counts = |n: &str| {
        let (all, loadable) = using(n);
        (loadable, all.len())
    };

    // 1.1: 30 names, by 20 loadable scripts (30 in all); none of the
    // `level` functions.
    let names: BTreeSet<&String> = used.values().flatten().collect();
    let scripts_using: Vec<&str> = used
        .iter()
        .filter(|(_, n)| !n.is_empty())
        .map(|(s, _)| *s)
        .collect();
    let loadable_using = scripts_using
        .iter()
        .filter(|s| reached.contains(**s))
        .count();
    assert_eq!(
        (names.len(), loadable_using, scripts_using.len()),
        (30, 20, 30),
        "names used and the scripts using them: {names:?}"
    );
    for f in LEVEL {
        assert_eq!(counts(&format!("level.{f}")), (0, 0), "level.{f}");
    }

    // 11: the members' scripts.
    assert_eq!(counts("set_callback"), (15, 24), "set_callback");
    let (fastcall, loadable_fastcall) = using("set_fastcall");
    assert_eq!(
        (fastcall.as_slice(), loadable_fastcall),
        (["heli_combat", "ph_car", "ph_door"].as_slice(), 3),
        "set_fastcall"
    );
    assert_eq!(
        counts("set_patrol_extrapolate_callback"),
        (2, 2),
        "set_patrol_extrapolate_callback"
    );
    assert_eq!(counts("set_enemy_callback"), (1, 1), "set_enemy_callback");
    // 24 of the 34 constants, each in 1 to 4 loadable scripts, `hit` and
    // `use_object` in 4; every constant the scripts name is one of the 34.
    let constants: BTreeMap<&str, usize> = names
        .iter()
        .filter_map(|n| n.strip_prefix("callback."))
        .map(|c| (c, counts(&format!("callback.{c}")).0))
        .collect();
    assert_eq!(constants.len(), 24, "constants used: {constants:?}");
    for (c, n) in &constants {
        assert!(
            CALLBACK_IDS.iter().any(|(name, _)| name == c),
            "callback.{c} is a constant"
        );
        assert!((1..=4).contains(n), "callback.{c}: {n} loadable scripts");
    }
    assert_eq!(
        (constants["hit"], constants["use_object"]),
        (4, 4),
        "hit and use_object"
    );
    // `xr_motivator`'s sound callback is commented out.
    let motivator = read(&base, "gamedata/scripts/xr_motivator.script").unwrap();
    assert!(
        contains(&motivator, b"callback.sound"),
        "xr_motivator's text names callback.sound"
    );
    assert!(
        !used["xr_motivator"].contains("callback.sound"),
        "xr_motivator's code doesn't"
    );

    // 9: `set_use_callback` in 3 scripts, one loadable (`utils`), and
    // nowhere in the game library.
    let (use_callback, loadable_use) = using("set_use_callback");
    assert_eq!(
        (use_callback.len(), loadable_use),
        (3, 1),
        "set_use_callback: {use_callback:?}"
    );
    assert!(use_callback.contains(&"utils") && reached.contains("utils"));
    assert!(
        !contains(&game_library, b"set_use_callback"),
        "set_use_callback in xrGame.dll"
    );

    // 3.1: the scripts clear an id with `set_callback(id, nil)`.
    let clears = scripts["bind_stalker"].windows(8).any(|w| {
        matches!(w, [
            Token::Name(m),
            Token::Punct(b'('),
            Token::Name(c),
            Token::Punct(b'.'),
            Token::Name(_),
            Token::Punct(b','),
            Token::Name(n),
            Token::Punct(b')'),
        ] if m == "set_callback" && c == "callback" && n == "nil")
    });
    assert!(clears, "bind_stalker clears a callback with nil");

    // 10: the door's binder and scheme.
    for name in [
        "set_callback",
        "callback.hit",
        "callback.death",
        "callback.use_object",
    ] {
        assert!(
            used["bind_physic_object"].contains(name),
            "bind_physic_object uses {name}"
        );
    }
    assert!(
        used["ph_door"].contains("set_fastcall"),
        "ph_door uses set_fastcall"
    );
}
