// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of configuration and files (BYOA), checked
//! against docs/behavior/script-api-configuration-and-files.md: the 250
//! loadable scripts (docs/behavior/script-loading.md 3, recomputed here),
//! their uses of this spec's names (8), the trader door's logic reads (9)
//! through the loader's state, and every loadable script loaded with the
//! names registered. Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::rc::Rc;

use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_script::Value;
use volya_engine_script::files::FileSystem;
use volya_engine_script::loader::{FailureKind, Prefix, Scripts};
use volya_engine_script::services::EngineServices;

use shipped_scripts::{GameTrees, Token, tokens};

/// The aliases scripts use, with the saved-games folder.
const FSGAME: &[u8] = b"$app_data_root$ = true | false| {userdocs}\\Stalker-SHOC\r\n\
    $game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n\
    $game_saves$ =    true | false| $app_data_root$| savedgames\\\r\n";

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
/// tokens, by name.
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

/// The global functions and classes of this spec (1.1).
const GLOBALS: &[&str] = &[
    "system_ini",
    "game_ini",
    "create_ini_file",
    "getFS",
    "command_line",
    "user_name",
];
const CLASSES: &[&str] = &[
    "ini_file",
    "token",
    "token_list",
    "rtoken_list",
    "FS",
    "fs_file",
    "FS_file_list",
    "FS_file_list_ex",
    "FS_item",
];
/// The methods of this spec's classes (1.1).
const METHODS: &[&str] = &[
    "section_exist",
    "line_exist",
    "line_count",
    "r_string",
    "r_string_wq",
    "r_s32",
    "r_u32",
    "r_float",
    "r_bool",
    "r_vector",
    "r_clsid",
    "r_token",
    "r_line",
    "add",
    "remove",
    "clear",
    "id",
    "name",
    "count",
    "get",
    "path_exist",
    "update_path",
    "get_path",
    "append_path",
    "exist",
    "file_length",
    "get_file_age",
    "get_file_age_str",
    "file_delete",
    "file_rename",
    "file_copy",
    "dir_delete",
    "r_open",
    "r_close",
    "w_open",
    "w_close",
    "file_list_open",
    "file_list_open_ex",
    "Size",
    "GetAt",
    "Free",
    "Sort",
    "NameFull",
    "NameShort",
    "Modif",
    "ModifDigitOnly",
];
/// The properties of `token` and `fs_file` (4.1, 5.3).
const PROPERTIES: &[&str] = &[
    "name",
    "id",
    "vfs",
    "ptr",
    "size_real",
    "size_compressed",
    "modif",
];

/// A script's uses of this spec's names: globals and classes by bare name,
/// methods after `:`, properties after `.`, constants `FS.name`.
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
        if bare && (GLOBALS.contains(&n) || CLASSES.contains(&n)) {
            out.insert(n.to_owned());
        }
        if after(b':') && METHODS.contains(&n) && punct(i + 1, b'(') {
            out.insert(n.to_owned());
        }
        if after(b'.') && PROPERTIES.contains(&n) && !punct(i + 1, b'(') {
            out.insert(format!(".{n}"));
        }
        if n == "FS"
            && bare
            && punct(i + 1, b'.')
            && let Some(constant) = name(i + 2)
        {
            out.insert(format!("FS.{constant}"));
        }
    }
    out
}

/// The names other specs' classes declare too (8: `id`, `name`, `count`,
/// `add`, `get`, `clear`, `remove`, the `r_` readers of the data streams).
const SHARED: &[&str] = &[
    "id", "name", "count", "add", "get", "clear", "remove", "r_bool", "r_u32", "r_float", "r_s32",
    ".id", ".name",
];

#[test]
fn shipped_uses_of_configuration_and_files() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    // The archive tree of `gamedata.db*` alone (script-loading.md, provenance).
    let base = DbVfs::open_game(root, None).unwrap();
    let (reached, scripts) = loadable(&base);

    // 8: the loadable scripts' uses.
    let used: BTreeMap<&String, BTreeSet<String>> = scripts
        .iter()
        .filter(|(n, _)| reached.contains(*n) && n.as_str() != "lua_help")
        .map(|(n, t)| (n, uses(t)))
        .collect();
    let mut by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (script, names) in &used {
        for n in names {
            by_name.entry(n).or_default().push(script);
        }
    }
    let count = |n: &str| by_name.get(n).map_or(0, Vec::len);
    let who = |n: &str| by_name.get(n).cloned().unwrap_or_default();
    for (name, scripts) in [
        ("ini_file", 18),
        ("system_ini", 11),
        ("create_ini_file", 1),
        ("section_exist", 25),
        ("line_exist", 18),
        ("r_string", 17),
        ("r_bool", 13),
        ("r_u32", 12),
        ("r_float", 8),
        ("r_s32", 2),
        ("line_count", 8),
        ("r_line", 8),
    ] {
        assert_eq!(count(name), scripts, "loadable scripts using {name}");
    }
    assert_eq!(who("create_ini_file"), ["gulag_tasks"]);
    let dialogs = ["ui_load_dialog", "ui_save_dialog", "ui_spawn_dialog"];
    for name in [
        "getFS",
        "file_list_open_ex",
        "GetAt",
        "Size",
        "Sort",
        "NameFull",
    ] {
        assert_eq!(who(name), dialogs, "loadable scripts using {name}");
    }
    for name in ["file_list_open", "exist", "Free"] {
        assert_eq!(
            who(name),
            ["ui_save_dialog"],
            "loadable scripts using {name}"
        );
    }
    assert_eq!(who("file_delete"), ["ui_load_dialog"]);
    assert_eq!(who("Modif"), ["ui_spawn_dialog"]);
    assert_eq!(who("ModifDigitOnly"), ["ui_load_dialog", "ui_save_dialog"]);
    for name in [
        "game_ini",
        "command_line",
        "user_name",
        "token",
        "token_list",
        "rtoken_list",
        "r_string_wq",
        "r_vector",
        "r_clsid",
        "r_token",
        "NameShort",
        "path_exist",
        "update_path",
        "get_path",
        "append_path",
        "r_open",
        "r_close",
        "w_open",
        "w_close",
        "file_rename",
        "file_copy",
        "dir_delete",
        "file_length",
        "get_file_age",
        "get_file_age_str",
    ] {
        assert_eq!(count(name), 0, "loadable scripts using {name}");
    }
    assert_eq!(
        by_name.len(),
        37,
        "distinct names used: {:?}",
        by_name.keys()
    );
    let using = |exclusive: bool| {
        used.values()
            .filter(|names| {
                names
                    .iter()
                    .any(|n| !exclusive || !SHARED.contains(&n.as_str()))
            })
            .count()
    };
    assert_eq!(
        using(false),
        141,
        "loadable scripts using a name of this spec"
    );
    // TODO(#1004): the spec counts 55 using a name no other spec has; this
    // scan, without the scoping spec's local-scope and receiver rules,
    // finds 44 (held at that until the spec states its count).
    assert_eq!(
        using(true),
        44,
        "loadable scripts using a name of this spec alone"
    );

    // Every loadable script through the loader, with the services and
    // configuration and files over the game's configurations.
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let system = config::load_aliased(&tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(&tree, &aliases, "$game_config$", "game.ltx").unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let trees = Rc::new(GameTrees { tree, root });
    let mut s = Scripts::new(trees.clone(), &aliases, Prefix::Default).unwrap();
    s.register_engine_services(Rc::clone(&services)).unwrap();
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees,
        aliases,
        Rc::new(system),
        Rc::new(game),
    )));
    s.register_configuration_and_files(Rc::clone(&files), services)
        .unwrap();
    s.load_globals().unwrap();
    for name in &reached {
        let name = if name == "_g" { "_G" } else { name.as_str() };
        s.load_namespace(name).unwrap();
    }
    // Scripts using engine classes of other specs fail to load; none fails
    // on a name of this spec, and none stops the state.
    let names: Vec<&str> = GLOBALS.iter().chain(CLASSES).copied().collect();
    for failure in s.take_failures() {
        assert_ne!(failure.kind, FailureKind::Fatal, "{failure:?}");
        assert!(
            !names
                .iter()
                .any(|n| failure.message.contains(&format!("'{n}'"))),
            "{failure:?}"
        );
    }

    // 9: the trader door's logic reads.
    let door = s
        .exec(
            "=door",
            br#"
local ini = ini_file("scripts\\esc_trader_door.ltx")
local sections = {}
for _, name in ipairs({"logic", "ph_door@locked", "ph_door@closed", "ph_door@open"}) do
  sections[#sections + 1] = tostring(ini:section_exist(name))
end
local absent = {}
for _, key in ipairs({"closed", "no_force", "show_tips", "tip_open", "tip_close",
    "snd_close_start", "snd_close_stop", "on_use", "hit_on_bone"}) do
  absent[#absent + 1] = tostring(ini:line_exist("ph_door@locked", key))
end
local custom = create_ini_file("[logic]\r\ncfg = scripts\\esc_trader_door.ltx\r\n[collide]\r\nignore_static\r\n")
return table.concat(sections, " "), ini:r_string("logic", "active"),
  ini:r_bool("ph_door@locked", "locked"), ini:r_string("ph_door@locked", "snd_open_start"),
  ini:r_string("ph_door@locked", "on_info"), table.concat(absent, " "),
  custom:r_string("logic", "cfg"), custom:r_string("collide", "ignore_static"),
  ini:section_exist("PH_DOOR@LOCKED")
"#,
        )
        .unwrap();
    assert_eq!(
        door,
        [
            Value::from("true true true true"),
            Value::from("ph_door@locked"),
            Value::Boolean(true),
            Value::from("trader_door_unlock"),
            Value::from(
                "{+esc_trader_can_leave}ph_door@closed%=play_snd(device\\door_servomotor)%"
            ),
            Value::from("false false false false false false false false false"),
            Value::from("scripts\\esc_trader_door.ltx"),
            Value::Nil,
            Value::Boolean(false),
        ],
        "the trader door's reads"
    );
    // 2.2's example: the file the archives provide.
    let found = s
        .exec(
            "=found",
            b"return getFS():exist('$game_config$', 'scripts\\\\esc_trader_door.ltx').name",
        )
        .unwrap();
    assert_eq!(
        found,
        [Value::from(
            "gamedata\\config\\scripts\\esc_trader_door.ltx"
        )]
    );
    assert!(files.borrow_mut().index.take_disk_changes().is_empty());
}
