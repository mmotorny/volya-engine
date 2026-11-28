// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's uses of the engine services (BYOA), checked
//! against docs/behavior/script-api-engine-services.md: the scripts' uses
//! the spec measures, the console commands they execute (each one the
//! console has), and every loadable script loaded through the loader with
//! the services registered. Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, read};
use volya_engine_script::Value;
use volya_engine_script::loader::{Prefix, Scripts};
use volya_engine_script::services::EngineServices;

use shipped_scripts::{FSGAME, GameTrees, Token, loadable_name, tokens};

/// What a script's tokens use of the services.
#[derive(Default, Debug)]
struct Uses {
    /// Global calls `name(`, not after `.` or `:`.
    calls: BTreeSet<String>,
    /// `device().field` and `device():method(`.
    device: BTreeSet<String>,
    /// `:name(` method calls.
    methods: BTreeSet<String>,
    /// The first word of a literal text a `:execute(` call starts with.
    executed: BTreeSet<String>,
}

fn uses(tokens: &[Token]) -> Uses {
    let name = |i: usize| match tokens.get(i) {
        Some(Token::Name(n)) => Some(n.as_str()),
        _ => None,
    };
    let punct = |i: usize, c: u8| tokens.get(i) == Some(&Token::Punct(c));
    let mut uses = Uses::default();
    for i in 0..tokens.len() {
        let Some(n) = name(i) else { continue };
        let after_member = i > 0 && (punct(i - 1, b'.') || punct(i - 1, b':'));
        if !after_member && punct(i + 1, b'(') {
            uses.calls.insert(n.to_owned());
        }
        if i > 0 && punct(i - 1, b':') && punct(i + 1, b'(') {
            uses.methods.insert(n.to_owned());
        }
        if n == "device"
            && !after_member
            && punct(i + 1, b'(')
            && punct(i + 2, b')')
            && (punct(i + 3, b'.') || punct(i + 3, b':'))
            && let Some(field) = name(i + 4)
        {
            uses.device.insert(field.to_owned());
        }
        if n == "execute"
            && i > 0
            && punct(i - 1, b':')
            && punct(i + 1, b'(')
            && let Some(Token::String(s)) = tokens.get(i + 2)
        {
            let text = String::from_utf8_lossy(s);
            if let Some(word) = text.split(' ').next().filter(|w| !w.is_empty()) {
                uses.executed.insert(word.to_owned());
            }
        }
    }
    uses
}

#[test]
fn shipped_uses_of_the_engine_services() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();

    // The loadable scripts (Lua names; `lua_help` is the listing).
    let dir = "gamedata/scripts";
    let mut scripts = BTreeMap::new();
    for entry in tree.read_dir(dir).unwrap() {
        let Some(name) = entry.name.strip_suffix(".script") else {
            continue;
        };
        let name = if name == "_g" { "_G" } else { name };
        if !loadable_name(name) || name == "lua_help" {
            continue;
        }
        let text = read(&tree, &format!("{dir}/{}", entry.name)).unwrap();
        scripts.insert(name.to_owned(), uses(&tokens(&text)));
    }
    // The spec's measurements (ES-13). Where it gives a count and the scan
    // finds more scripts (`get_console()`: 9, `device():time_global()`: 6,
    // `device().precache_frame`: 2, `log`: the two it names), the scan's
    // broader reading of a use is held only to include the spec's.
    let calling = |f: &str| -> Vec<&str> {
        scripts
            .iter()
            .filter(|(_, u)| u.calls.contains(f))
            .map(|(n, _)| n.as_str())
            .collect()
    };
    let reading = |field: &str| -> Vec<&str> {
        scripts
            .iter()
            .filter(|(_, u)| u.device.contains(field))
            .map(|(n, _)| n.as_str())
            .collect()
    };
    let methods = |m: &str| -> Vec<&str> {
        scripts
            .iter()
            .filter(|(_, u)| u.methods.contains(m))
            .map(|(n, _)| n.as_str())
            .collect()
    };
    // ES-13: who calls what.
    assert_eq!(
        calling("editor"),
        [
            "_G",
            "class_registrator",
            "game_registrator",
            "se_respawn",
            "se_zones",
            "smart_terrain",
            "utils"
        ],
        "editor() (4.2)"
    );
    assert_eq!(calling("verify_if_thread_is_running"), ["_G"], "4.3");
    assert_eq!(calling("IsGameTypeSingle"), ["se_zones"], "4.5");
    assert_eq!(calling("xrRender_test_r2_hw"), ["ui_mm_opt_main"], "4.6");
    assert_eq!(methods("dcast_PKinematicsAnimated"), ["ce_switcher"], "5.2");
    assert_eq!(methods("PlayCycle"), ["ce_switcher"], "5.2");
    assert_eq!(
        reading("width"),
        ["game_last_header_wnd", "game_last_ui"],
        "3.3"
    );
    assert_eq!(reading("height"), ["game_last_ui"], "3.3");
    for (f, named) in [
        ("log", &["_G", "profiler"][..]),
        ("prefetch", &["class_registrator"]),
        ("app_ready", &["bind_stalker"]),
    ] {
        let scripts = calling(f);
        assert!(
            named.iter().all(|n| scripts.contains(n)),
            "{f}: {scripts:?}"
        );
    }
    assert!(reading("time_global").contains(&"_G"), "3.3");
    assert!(reading("precache_frame").contains(&"bind_stalker"), "3.4");
    assert!(calling("get_console").len() >= 9, "2.2");
    assert!(reading("time_global").len() >= 6, "3.3");
    assert!(reading("precache_frame").len() >= 2, "3.4");
    // Nothing calls these.
    for f in ["flush", "error_log", "script_server_object_version"] {
        assert_eq!(calling(f), Vec::<&str>::new(), "{f}");
    }
    for field in ["is_paused", "pause"] {
        assert_eq!(reading(field), Vec::<&str>::new(), "device():{field}");
    }
    for m in [
        "execute_script",
        "get_string",
        "get_token",
        "get_integer",
        "get_float",
        "get_bool",
        "show",
        "hide",
    ] {
        assert_eq!(methods(m), Vec::<&str>::new(), ":{m}");
    }

    // 2.2: the commands executed as literal text.
    let executed: BTreeSet<&str> = scripts
        .values()
        .flat_map(|u| u.executed.iter().map(String::as_str))
        .collect();
    assert_eq!(
        executed.iter().copied().collect::<Vec<_>>(),
        [
            "check_for_updates",
            "disconnect",
            "flush",
            "g_game_difficulty",
            "jump_to_level",
            "load",
            "load_last_save",
            "main_menu",
            "quit",
            "save",
            "start",
            "unbindall"
        ]
    );
    // Each one the console has but `jump_to_level`, which no program
    // registers (docs/behavior/console-scope.md).
    for name in &executed {
        assert_eq!(
            Console::def(name).is_some(),
            *name != "jump_to_level",
            "{name}: a console command"
        );
    }

    // Every loadable script through the loader, with the services and
    // the shipped settings.
    let system = config::load_aliased(&tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system)));
    let mut s = Scripts::new(Rc::new(GameTrees { tree, root }), &aliases, Prefix::Default).unwrap();
    s.register_engine_services(Rc::clone(&services)).unwrap();
    s.load_globals().unwrap();
    for name in scripts.keys() {
        s.load_namespace(name).unwrap();
    }
    // Scripts using engine classes of other specs fail to load; none stops
    // the state.
    s.take_failures();
    // 4.7: the shipped value.
    let version = s
        .exec("=version", b"return script_server_object_version()")
        .unwrap();
    assert_eq!(
        version,
        [Value::Number(7.0)],
        "script_server_object_version()"
    );
    // 4.1, 4.2: no level started, not the editor.
    let ready = s.exec("=ready", b"return editor(), app_ready()").unwrap();
    assert_eq!(ready, [Value::Boolean(false), Value::Boolean(false)]);
}
