// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's dialogs and trade through the shipped scripts,
//! checked against docs/behavior/script-api-dialogs-and-trade.md (cited by
//! section): the members the scripts call and those none calls (9), every
//! dialog with an `init_func` built by its shipped function on a started
//! game state (2.1, 3.5), the two functions no script defines fatal, the
//! generated help dialog's phrases (8) and the defaults' factors from the
//! shipped `[trade]` (5.2). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use shipped_scripts::{FSGAME, GameTrees, Token, tokens};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::dialogs::Dialogs;
use volya_engine_core::info_portions::Declarations;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::dialog_files::{Body, Dialog};
use volya_engine_script::ErrorKind;
use volya_engine_script::dialogs::{DialogsAndTrade, TradeParameters};
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::Prefix;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;

/// The names section 9 finds used by the scripts.
const USED: [&str; 16] = [
    "is_talking",
    "disable_talk",
    "enable_talk",
    "is_talk_enabled",
    "stop_talk",
    "AddAction",
    "AddPhrase",
    "AddPrecondition",
    "GetPhraseScript",
    "run_talk_dialog",
    "set_start_dialog",
    "AddGiveInfo",
    "restore_default_start_dialog",
    "set_trader_global_anim",
    "set_trader_head_anim",
    "set_trader_sound",
];

/// The members section 9 finds used by none.
const UNUSED: [&str; 9] = [
    "switch_to_talk",
    "switch_to_trade",
    "get_start_dialog",
    "enable_trade",
    "disable_trade",
    "is_trade_enabled",
    "AddHasInfo",
    "AddDontHasInfo",
    "AddDisableInfo",
];

/// The init functions no shipped script defines (2.1).
const UNDEFINED: [&str; 2] = [
    "dialog_manager.init_trade_info_dialog",
    "dialog_manager.init_action_info_dialog",
];

struct Game {
    trees: Rc<GameTrees>,
    aliases: PathAliases,
    declarations: Arc<Declarations>,
}

fn game() -> Option<Game> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let system =
        config::load_aliased(&trees.tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let declarations = Arc::new(Declarations::read(&trees.tree, &aliases, &system).unwrap());
    Some(Game {
        trees,
        aliases,
        declarations,
    })
}

/// A started game state (spec D) and its dialogs and trade.
fn start(g: &Game) -> (GameScripts, Rc<RefCell<DialogsAndTrade>>) {
    let tree = &g.trees.tree;
    let system = config::load_aliased(tree, &g.aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, &g.aliases, "$game_config$", "game.ltx").unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let files = Rc::new(RefCell::new(FileSystem::new(
        g.trees.clone(),
        g.aliases.clone(),
        Rc::new(system),
        Rc::new(game),
    )));
    let dialogs = Rc::new(RefCell::new(DialogsAndTrade::new()));
    let scripts = GameScripts::start(Startup {
        trees: g.trees.clone(),
        aliases: g.aliases.clone(),
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::clone(&dialogs),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
    })
    .unwrap();
    (scripts, dialogs)
}

/// Every dialog of the listed files with an `init_func` (files 4).
fn generated(g: &Game) -> Vec<Dialog> {
    let mut dialogs = Dialogs::new(Arc::clone(&g.declarations));
    (g.declarations.dialogs.ids().iter())
        .map(|id| dialogs.load(&g.trees.tree, &g.aliases, id).unwrap())
        .filter(|d| matches!(d.body, Body::InitFunc(_)))
        .map(|d| (*d).clone())
        .collect()
}

fn init_func(d: &Dialog) -> &str {
    match &d.body {
        Body::InitFunc(name) => name,
        Body::PhraseList(_) => "",
    }
}

/// 9: the scripts' method calls of this spec's names.
#[test]
fn shipped_uses() {
    let Some(g) = game() else {
        return;
    };
    let tree = &g.trees.tree;
    let mut called = BTreeSet::new();
    let mut named = BTreeSet::new();
    for e in tree.read_dir("gamedata/scripts").unwrap() {
        if e.kind != EntryKind::File || !e.name.to_ascii_lowercase().ends_with(".script") {
            continue;
        }
        let t = tokens(&read(tree, &format!("gamedata/scripts/{}", e.name)).unwrap());
        for w in t.windows(3) {
            if let [Token::Punct(b':'), Token::Name(n), Token::Punct(b'(')] = w {
                called.insert(n.clone());
            }
        }
        for w in t.windows(2) {
            if let [Token::Name(n), Token::Punct(b'.')] = w {
                named.insert(n.clone());
            }
        }
    }
    for name in USED {
        assert!(called.contains(name), "a script calls {name}");
    }
    for name in UNUSED {
        assert!(!called.contains(name), "no script calls {name}");
    }
    assert!(
        !named.contains("game_difficulty"),
        "no script reads game_difficulty's constants"
    );
}

/// 2.1, 3.5 and 8: the 31 init functions on one started state, each
/// called with its dialog; 29 build phrases, phrase 0 first; the 2 no
/// script defines are fatal (each on a state of its own).
#[test]
fn shipped_init_functions_build_their_dialogs() {
    let Some(g) = game() else {
        return;
    };
    let dialogs = generated(&g);
    assert_eq!(dialogs.len(), 31, "dialogs with init_func");
    let (mut scripts, _) = start(&g);
    let mut built = 0;
    for d in dialogs
        .iter()
        .filter(|d| !UNDEFINED.contains(&init_func(d)))
    {
        let shared = scripts.scripts().init_dialog(d.clone()).unwrap();
        let shared = shared.borrow();
        let phrases = shared.phrases();
        assert!(
            phrases.first().is_some_and(|p| p.id == "0"),
            "{}: phrase 0 first, {phrases:?}",
            d.id
        );
        built += 1;
    }
    assert_eq!(built, 29);

    // 8: the generated help dialog.
    let medkit = dialogs
        .iter()
        .find(|d| d.id == "dm_help_wounded_medkit_dialog")
        .unwrap();
    assert_eq!(
        init_func(medkit),
        "dialog_manager.init_help_wounded_medkit_dialog"
    );
    let (mut fresh, _) = start(&g);
    let shared = fresh.scripts().init_dialog(medkit.clone()).unwrap();
    let d = shared.borrow();
    let p = d.phrases();
    assert_eq!(p[0].text, "dm_general_help_medkit");
    assert_eq!(p[0].goodwill, -10_000);
    assert_eq!(
        p[0].lists.action,
        [Some("dialogs.transfer_medkit".to_owned())]
    );
    // Its thank-you phrases come from `dialog_manager`'s own table, which
    // the game fills elsewhere (DG-9): whichever there are link from 0.
    assert_eq!(p[0].links, (1..p.len()).collect::<Vec<_>>());
    for phrase in &p[1..] {
        assert_eq!(
            phrase.lists.precondition,
            [Some("dialog_manager.precondition_help_thanks".to_owned())],
            "{}",
            phrase.id
        );
    }
    drop(d);

    for name in UNDEFINED {
        let d = dialogs.iter().find(|d| init_func(d) == name).unwrap();
        let (mut scripts, _) = start(&g);
        let e = scripts.scripts().init_dialog(d.clone()).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Fatal, "{name}: {e:?}");
    }
}

/// 5.2: the defaults from the shipped `[trade]`, all four factors 1, made
/// on the global functions' first use.
#[test]
fn the_shipped_default_trade_parameters() {
    let Some(g) = game() else {
        return;
    };
    let system =
        config::load_aliased(&g.trees.tree, &g.aliases, "$game_config$", "system.ltx").unwrap();
    let defaults = TradeParameters::defaults(&system).unwrap();
    assert_eq!(defaults.buy.default, (1.0, 1.0));
    assert_eq!(defaults.sell.default, (1.0, 1.0));
    let (mut scripts, dialogs) = start(&g);
    assert!(dialogs.borrow().default_trade.is_none());
    (scripts.scripts())
        .exec("=5.2", b"buy_condition(0.5, 2)")
        .unwrap();
    let d = dialogs.borrow();
    let d = d.default_trade.as_ref().unwrap();
    assert_eq!((d.buy.default, d.sell.default), ((0.5, 2.0), (1.0, 1.0)));
}
