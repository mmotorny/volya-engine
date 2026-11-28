// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of the inventory names, checked against
//! docs/behavior/script-api-inventory.md (cited by section): the 250
//! loadable scripts (docs/behavior/script-loading.md 3, recomputed here)
//! and their calls of this spec's members (10), and the members on a game
//! state started with the shipped scripts on the trader's path: his start
//! dialog's document tests through `object` (3.2), `transfer_item` between
//! him and the player (5.3), and `buy_supplies` with the shipped
//! `trade_trader.ltx`'s `[supplies_start]` in its sorted order (5.6).
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
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::model::transform;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::{Prefix, Scripts};
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

/// Section 10: loadable scripts (all scripts) calling each used name
/// after `:`, but `object` (which other specs' classes declare too).
const USED: [(&str, (usize, usize)); 20] = [
    ("money", (10, 10)),
    ("active_item", (8, 10)),
    ("best_weapon", (3, 8)),
    ("enable_attachable_item", (3, 5)),
    ("activate_slot", (3, 3)),
    ("item_in_slot", (3, 3)),
    ("transfer_item", (3, 3)),
    ("transfer_money", (2, 2)),
    ("get_ammo_in_magazine", (1, 2)),
    ("active_slot", (1, 1)),
    ("buy_supplies", (1, 1)),
    ("drop_item", (1, 1)),
    ("drop_item_and_teleport", (1, 1)),
    ("eat", (1, 1)),
    ("give_money", (1, 1)),
    ("inventory_for_each", (1, 1)),
    ("is_inv_box_empty", (1, 1)),
    ("iterate_inventory", (1, 1)),
    ("marked_dropped", (1, 1)),
    ("unload_magazine", (1, 1)),
];

/// Section 10: the names no loadable script uses.
const UNUSED: [&str; 8] = [
    "attachable_item_enabled",
    "best_item",
    "get_ammo_total",
    "get_current_outfit",
    "get_current_outfit_protection",
    "mark_item_dropped",
    "object_count",
    "set_ammo_elapsed",
];

/// The names a script calls after `:`.
fn methods(tokens: &[Token]) -> BTreeSet<String> {
    tokens
        .windows(2)
        .filter_map(|w| match w {
            [Token::Punct(b':'), Token::Name(n)] => Some(n.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn shipped_uses_of_the_inventory_names() {
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
        .map(|(n, t)| (n.as_str(), methods(t)))
        .collect();
    let counts = |n: &str| {
        let all: Vec<&str> = (used.iter())
            .filter(|(_, names)| names.contains(n))
            .map(|(s, _)| *s)
            .collect();
        let loadable = all.iter().filter(|s| reached.contains(**s)).count();
        (loadable, all.len())
    };
    let got: Vec<(&str, (usize, usize))> = USED.iter().map(|(n, _)| (*n, counts(n))).collect();
    assert_eq!(got, USED);
    for n in UNUSED {
        assert_eq!(counts(n).0, 0, "{n}");
    }
}

const PLAYER: u16 = 0;
const TRADER: u16 = 7;
const DOCUMENT: u16 = 100;

fn spawn_data(id: u16, section: &[u8], class: &[u8]) -> SpawnData {
    SpawnData {
        id,
        name: Vec::new(),
        section: section.to_vec(),
        class_id: class_id(class),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: Some(1.0),
        server: None,
    }
}

/// A game state started with the shipped scripts, with the player, the
/// trader and a document lying in the world.
fn started() -> Option<(GameScripts, Rc<RefCell<GameObjects>>)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let system =
        config::load_aliased(&trees.tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(&trees.tree, &aliases, "$game_config$", "game.ltx").unwrap();
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
        trees,
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
    // No section of the settings for the characters: nothing is bound.
    let character = b"volya_test_character";
    (s.spawn_engine_object(
        ObjectClass::Player,
        spawn_data(PLAYER, character, b"O_ACTOR"),
    ))
    .unwrap();
    let trader = ObjectClass::Named("CAI_Trader");
    (s.spawn_engine_object(trader, spawn_data(TRADER, character, b"AI_TRADE"))).unwrap();
    let document = ObjectClass::Unnamed(class_id(b"II_DOC"));
    (s.spawn_engine_object(document, spawn_data(DOCUMENT, b"dar_document4", b"II_DOC"))).unwrap();
    s.exec(
        "=volya_test",
        b"function volya_test_transfer(from, item, to) from:transfer_item(item, to) end\n\
          function volya_test_buy(npc, file, section) npc:buy_supplies(ini_file(file), section) end",
    )
    .unwrap();
    Some((scripts, objects))
}

fn precondition(s: &mut Scripts, function: &str, first: u16, second: u16) -> bool {
    let texts = ["escape_trader_start_dialog", "0", "12"];
    s.call_precondition(function, first, second, texts).unwrap()
}

fn handle(s: &Scripts, id: u16) -> Value {
    Value::Object(s.game_object(id).unwrap())
}

/// 3.2 and 5.3 on the trader's path: the start dialog's document tests ask
/// the first character's `object`, and `transfer_item` moves the document
/// from the trader to the player once the server's events are served.
#[test]
fn the_traders_document_tests_and_transfer() {
    let Some((mut scripts, objects)) = started() else {
        return;
    };
    let s = scripts.scripts();
    let has = "bar_dialogs.have_darklab_documents";
    let hasnt = "bar_dialogs.hasnt_darklab_documents";
    assert!(!precondition(s, has, PLAYER, TRADER));
    s.take_item(TRADER, DOCUMENT).unwrap();
    assert!(!precondition(s, hasnt, TRADER, PLAYER));
    assert!(precondition(s, hasnt, PLAYER, TRADER));
    let args = [handle(s, TRADER), handle(s, DOCUMENT), handle(s, PLAYER)];
    s.call("volya_test_transfer", &args).unwrap();
    let parent = |o: &Rc<RefCell<GameObjects>>| o.borrow().get(DOCUMENT).and_then(|d| d.parent);
    assert_eq!(parent(&objects), Some(TRADER), "nothing at once");
    assert!(s.serve_inventory_events().unwrap().is_empty());
    assert_eq!(parent(&objects), Some(PLAYER));
    assert!(precondition(s, has, PLAYER, TRADER));
}

/// 5.6's example: `[supplies_start]` of `trade_trader.ltx` taken in its
/// sorted order.
#[test]
fn the_traders_supplies_in_sorted_order() {
    let Some((mut scripts, objects)) = started() else {
        return;
    };
    let s = scripts.scripts();
    let args = [
        handle(s, TRADER),
        Value::string(&b"misc\\trade_trader.ltx"[..]),
        Value::string(&b"supplies_start"[..]),
    ];
    s.call("volya_test_buy", &args).unwrap();
    let inventories = objects.borrow().inventories();
    let record: Vec<String> = (inventories.borrow().owner(TRADER).unwrap().restock.iter())
        .map(|(section, _)| String::from_utf8_lossy(section).into_owned())
        .collect();
    assert_eq!(
        record,
        [
            "ammo_5.45x39_fmj",
            "antirad",
            "bandage",
            "bread",
            "conserva",
            "energy_drink",
            "killer_outfit",
            "kolbasa",
            "medkit",
            "vodka",
            "wpn_groza_m1",
        ]
    );
}
