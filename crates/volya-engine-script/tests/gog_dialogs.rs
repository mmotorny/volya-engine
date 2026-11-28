// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shipped dialog functions of the Russian edition's first path
//! (docs/behavior/dialog-conditions.md 7.3, cited by section) called as
//! entry points (script API R 3.2, 3.3) on the shipped scripts, and the
//! trader's start dialog said with them over the scripts as its world (8):
//! the story's portions, P's members. The engine members they reach whose
//! specs are not implemented (K's simulation question, R's `stop_talk`,
//! S's `object`) are stand-ins of this test that note their calls. Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::dialogs::Dialogs;
use volya_engine_core::dialogs::conditions::{DialogInProgress, DialogWorld};
use volya_engine_core::info_portions::{Declarations, InfoPortions};
use volya_engine_core::vfs::StdVfs;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_formats::dialog_files::Dialog;
use volya_engine_formats::model::transform;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::{Prefix, Scripts};
use volya_engine_script::object_factory::class_id;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;

const PLAYER: u16 = 0;
const TRADER: u16 = 7;
const DIALOG: &str = "escape_trader_start_dialog";

/// The stand-ins: the simulation asked whether object 0 holds a portion
/// (the player's list, `IP` 2, asked through P's `has_info`), `stop_talk`
/// and `object`, the last two noting their calls.
const STAND_INS: &str = r#"
volya_test = { calls = {} }
local function note(text) table.insert(volya_test.calls, text) end
function volya_test.taken()
  local c = table.concat(volya_test.calls, ", ")
  volya_test.calls = {}
  return c
end
function volya_test_set_player(handle) volya_test.player = handle end
local simulation = {}
function simulation:has_info(id, portion)
  return id == 0 and volya_test.player:has_info(portion)
end
function alife() return simulation end
game_object.stop_talk = function(self) note("stop_talk " .. self:id()) end
game_object.object = function(self, section)
  note("object " .. self:id() .. " " .. section)
  return nil
end
"#;

struct Game {
    scripts: GameScripts,
    story: Rc<RefCell<Story>>,
    declarations: Arc<Declarations>,
    dialog: Arc<Dialog>,
}

fn spawn_data(id: u16, class: &[u8]) -> SpawnData {
    SpawnData {
        id,
        name: Vec::new(),
        // No section of the settings: nothing is bound.
        section: b"volya_test_character".to_vec(),
        class_id: class_id(class),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: Some(1.0),
    }
}

fn game() -> Option<Game> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let tree = &trees.tree;
    let system = config::load_aliased(tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, &aliases, "$game_config$", "game.ltx").unwrap();
    let declarations = Arc::new(Declarations::read(tree, &aliases, &system).unwrap());
    let mut dialogs = Dialogs::new(Arc::clone(&declarations));
    let dialog = dialogs.load(tree, &aliases, DIALOG).unwrap();
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
    let story = Rc::new(RefCell::new(Story::new()));
    let mut scripts = GameScripts::start(Startup {
        trees,
        aliases,
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::clone(&story),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
    })
    .unwrap();
    let s = scripts.scripts();
    let player =
        (s.spawn_engine_object(ObjectClass::Player, spawn_data(PLAYER, b"O_ACTOR"))).unwrap();
    let trader = ObjectClass::Named("CAI_Trader");
    s.spawn_engine_object(trader, spawn_data(TRADER, b"AI_TRADE"))
        .unwrap();
    s.exec("=stand_ins", STAND_INS.as_bytes()).unwrap();
    s.call("volya_test_set_player", &[Value::Object(player)])
        .unwrap();
    Some(Game {
        scripts,
        story,
        declarations,
        dialog,
    })
}

/// The scripts, the player character holding `held` and nothing else.
fn world<'a>(g: &'a mut Game, held: &[&str]) -> &'a mut Scripts {
    g.story.borrow_mut().info_portions = Some(Rc::new(RefCell::new(InfoPortions::new(
        Arc::clone(&g.declarations),
        PLAYER,
    ))));
    let s = g.scripts.scripts();
    for id in held {
        s.give_to_player(id).unwrap();
    }
    s
}

/// The stand-ins' calls since the last time.
fn taken(s: &mut Scripts) -> String {
    let r = s.exec("=taken", b"return volya_test.taken()").unwrap();
    match r.as_slice() {
        [Value::String(s)] => String::from_utf8_lossy(s).into_owned(),
        other => panic!("{other:?}"),
    }
}

fn precondition(s: &mut Scripts, function: &str, first: u16, second: u16) -> bool {
    (s.call_precondition(function, first, second, [DIALOG, "0", "12"])).unwrap()
}

/// 7.3: the eight preconditions, each as its row states.
#[test]
fn the_first_paths_preconditions() {
    let Some(mut g) = game() else {
        return;
    };
    let w = world(&mut g, &[]);
    assert!(precondition(w, "escape_dialog.is_tutorial", PLAYER, TRADER));
    assert!(!precondition(
        w,
        "escape_dialog.is_not_tutorial",
        PLAYER,
        TRADER
    ));
    assert!(!precondition(
        w,
        "escape_dialog.trader_alredy_give_job",
        TRADER,
        PLAYER
    ));
    assert!(precondition(
        w,
        "escape_dialog.trader_dont_give_job",
        TRADER,
        PLAYER
    ));
    assert!(!precondition(
        w,
        "escape_dialog.trader_give_extra_job",
        TRADER,
        PLAYER
    ));
    assert!(!precondition(
        w,
        "escape_dialog.trader_need_extra_job",
        TRADER,
        PLAYER
    ));
    assert_eq!(taken(w), "");
    // The document tests ask the first argument's inventory.
    assert!(!precondition(
        w,
        "bar_dialogs.have_darklab_documents",
        PLAYER,
        TRADER
    ));
    assert!(precondition(
        w,
        "bar_dialogs.hasnt_darklab_documents",
        TRADER,
        PLAYER
    ));
    assert_eq!(taken(w), "object 0 dar_document4, object 7 dar_document4");

    let w = world(&mut g, &["tutorial_end"]);
    assert!(!precondition(
        w,
        "escape_dialog.is_tutorial",
        PLAYER,
        TRADER
    ));
    assert!(precondition(
        w,
        "escape_dialog.is_not_tutorial",
        PLAYER,
        TRADER
    ));
    for pair in [
        ["agroprom_military_case", "agroprom_military_case_done"],
        ["escape_blockpost_case", "escape_blockpost_case_done"],
        ["tutorial_artefact_start", "tutorial_artefact_done"],
        ["tutorial_wounded_start", "esc_serious_talk"],
    ] {
        let w = world(&mut g, &pair[..1]);
        let f = "escape_dialog.trader_alredy_give_job";
        assert!(precondition(w, f, TRADER, PLAYER), "{pair:?}");
        let f = "escape_dialog.trader_dont_give_job";
        assert!(!precondition(w, f, TRADER, PLAYER), "{pair:?}");
        let w = world(&mut g, &pair);
        let f = "escape_dialog.trader_alredy_give_job";
        assert!(!precondition(w, f, TRADER, PLAYER), "{pair:?} both");
    }
    let w = world(&mut g, &["escape_blockpost_case"]);
    assert!(precondition(
        w,
        "escape_dialog.trader_give_extra_job",
        TRADER,
        PLAYER
    ));
    let w = world(&mut g, &["esc_return"]);
    assert!(precondition(
        w,
        "escape_dialog.trader_need_extra_job",
        TRADER,
        PLAYER
    ));
    let w = world(&mut g, &["esc_return", "escape_blockpost_case"]);
    assert!(!precondition(
        w,
        "escape_dialog.trader_need_extra_job",
        TRADER,
        PLAYER
    ));
}

/// Says the sayable phrase `id`; whether the dialog finished.
fn say(talk: &mut DialogInProgress, id: &str, w: &mut Scripts) -> bool {
    let at = (talk.dialog().phrases().iter())
        .position(|p| p.id == id)
        .unwrap();
    talk.say(at, w).unwrap().finished
}

fn sayable(talk: &DialogInProgress) -> Vec<&str> {
    let phrases = talk.dialog().phrases();
    (talk.sayable().iter())
        .map(|&i| phrases[i].id.as_str())
        .collect()
}

/// 8 with the shipped functions: the first path, and the other answer.
#[test]
fn the_traders_start_dialog_with_the_shipped_functions() {
    let Some(mut g) = game() else {
        return;
    };
    let dialog = Arc::clone(&g.dialog);
    let w = world(&mut g, &[]);
    let mut talk = DialogInProgress::start(Arc::clone(&dialog), TRADER, PLAYER);
    say(&mut talk, "0", w);
    assert_eq!(sayable(&talk), ["12"]);
    for id in ["12", "3", "32", "321", "3211", "32111"] {
        assert!(!say(&mut talk, id, w), "{id}");
    }
    assert_eq!(taken(w), "");
    assert!(say(&mut talk, "321111", w));
    // `dialogs.break_dialog` stops the talk on the first, then the second.
    assert_eq!(taken(w), "stop_talk 0, stop_talk 7");
    for id in ["esc_trader_tutorial_chosen", "tutorial_wounded_start"] {
        assert!(w.player_holds(id), "{id}");
    }
    // The task `tutorial_wounded_start` gives (`IP` 3.4, P's data path).
    let tasks = g.story.borrow().tasks.list.len();
    assert_eq!(tasks, 1);

    // 31: `break_dialog`, then `run_tutorial_pda`; the trader's 311 follows.
    let w = world(&mut g, &[]);
    let mut talk = DialogInProgress::start(Arc::clone(&dialog), TRADER, PLAYER);
    for id in ["0", "12", "3"] {
        say(&mut talk, id, w);
    }
    assert!(!say(&mut talk, "31", w));
    assert_eq!(
        taken(w),
        "stop_talk 0, stop_talk 7, stop_talk 0, stop_talk 7"
    );
    let active = w.exec("=t", b"return game.has_active_tutorial()").unwrap();
    assert_eq!(active, [Value::Boolean(true)], "part_1_pda started");
    assert!(say(&mut talk, "311", w));
    assert_eq!(taken(w), "stop_talk 7, stop_talk 0");
    // A later talk offers 15 instead of 12.
    let mut talk = DialogInProgress::start(dialog, TRADER, PLAYER);
    say(&mut talk, "0", w);
    assert_eq!(sayable(&talk), ["15"]);
}
