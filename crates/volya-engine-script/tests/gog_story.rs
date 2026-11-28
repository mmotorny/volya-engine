// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's story state through the shipped scripts, checked
//! against docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md
//! (cited by section): every member a shipped script calls by this spec's
//! names is registered (12), every task of the task file loads through
//! `CGameTask:load` with the measured counts and every task function
//! resolves in its script (3.4, 6.3), and the portion of the trader's path
//! that gives the first task gives it with its objectives and callback
//! (`IP` 3.4, 4.1), whose update then runs its shipped functions (5.1).
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::info_portions::{self, Declarations, InfoPortions};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::xml;
use volya_engine_script::Value;
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::{FailureKind, Prefix, Scripts};
use volya_engine_script::object_factory::class_id;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::{Event, IN_PROGRESS, Story};

use shipped_scripts::{FSGAME, GameTrees, Token, tokens};

/// The `game_object` members of this spec (1.1) the scripts can call.
const MEMBERS: [&str; 15] = [
    "give_info_portion",
    "disable_info_portion",
    "has_info",
    "dont_has_info",
    "info_add",
    "info_clear",
    "give_task",
    "get_task_state",
    "set_task_state",
    "give_game_news",
    "give_talk_message",
    "set_tip_text",
    "set_tip_text_default",
    // Registered, and never called (2.2, 2.4).
    "get_info_time",
    "give_info_portion_via_pda",
];

/// Every `.script` file, `/`-separated.
fn scripts(vfs: &dyn Vfs) -> Vec<String> {
    vfs.read_dir("gamedata/scripts")
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EntryKind::File && e.name.to_ascii_lowercase().ends_with(".script"))
        .map(|e| format!("gamedata/scripts/{}", e.name))
        .collect()
}

#[test]
fn shipped_uses_of_the_story_state() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();

    // 12: the methods the scripts call by this spec's member names.
    let mut called = BTreeSet::new();
    for path in scripts(&tree) {
        let t = tokens(&read(&tree, &path).unwrap());
        for w in t.windows(3) {
            if let [Token::Punct(b':'), Token::Name(n), Token::Punct(b'(')] = w
                && MEMBERS.contains(&n.as_str())
            {
                called.insert(n.clone());
            }
        }
    }
    for name in [
        "give_info_portion",
        "disable_info_portion",
        "has_info",
        "info_add",
        "info_clear",
        "give_task",
        "set_task_state",
        "give_game_news",
        "give_talk_message",
        "set_tip_text",
        "set_tip_text_default",
        "give_info_portion_via_pda",
    ] {
        assert!(called.contains(name), "a script calls {name}");
    }
    assert!(
        !called.contains("get_info_time"),
        "no script calls get_info_time"
    );

    let system = config::load_aliased(&tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(&tree, &aliases, "$game_config$", "game.ltx").unwrap();
    let declarations = Declarations::read(&tree, &aliases, &system).unwrap();
    let tasks_path = aliases
        .path("$game_config$", "gameplay\\game_tasks.xml")
        .unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let trees = Rc::new(GameTrees { tree, root });
    let task_file = {
        let text = read(&trees.tree, &tasks_path).unwrap();
        let text = xml::preprocess(&text, &mut |include| {
            let p = aliases
                .path(
                    "$game_config$",
                    &volya_engine_formats::cp1251::decode(include),
                )
                .unwrap();
            read(&trees.tree, &p)
        })
        .unwrap();
        xml::parse(&text).unwrap()
    };
    let mut s = Scripts::new(trees.clone(), &aliases, Prefix::Default).unwrap();
    s.register_engine_services(Rc::clone(&services)).unwrap();
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees,
        aliases,
        Rc::new(system.clone()),
        Rc::new(game),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .unwrap();
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .unwrap();
    let story = Rc::new(RefCell::new(Story::new()));
    s.register_story(Rc::clone(&story)).unwrap();
    s.load_globals().unwrap();
    let player = SpawnData {
        id: 0,
        name: b"actor".to_vec(),
        // A section without `script_binding`: the player's binder is spec L's.
        section: b"story_test_actor".to_vec(),
        class_id: class_id(b"O_ACTOR"),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: volya_engine_formats::model::transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: Some(1.0),
        server: None,
    };
    let actor = s.spawn_engine_object(ObjectClass::Player, player).unwrap();
    story.borrow_mut().info_portions = Some(Rc::new(RefCell::new(InfoPortions::new(
        std::sync::Arc::new(declarations),
        0,
    ))));

    let registered = s
        .exec(
            "=members",
            format!(
                "local names, found = {{{}}}, {{}}
                 for i, n in ipairs(names) do found[i] = type(game_object[n]) end
                 return unpack(found)",
                MEMBERS.map(|n| format!("{n:?}")).join(", ")
            )
            .as_bytes(),
        )
        .unwrap();
    let functions = Value::from("function");
    for (name, kind) in MEMBERS.iter().zip(&registered) {
        let want = !name.starts_with("get_info_time") && *name != "give_info_portion_via_pda";
        assert_eq!(*kind == functions, want, "game_object.{name}");
    }

    // 3.4: every task of the file through `CGameTask:load`, given.
    let ids: Vec<String> = task_file
        .root()
        .unwrap()
        .children_named("game_task")
        .map(|t| t.attr("id").unwrap().to_owned())
        .collect();
    assert_eq!(ids.len(), 164, "tasks");
    set_actor(&mut s, actor);
    for id in &ids {
        // The receiver isn't used (4).
        let code =
            format!("local t = CGameTask() t:load({id:?}) actor_handle:give_task(t, 0, false)");
        s.exec("=load", code.as_bytes()).unwrap();
    }
    for failure in s.take_failures() {
        assert_ne!(failure.kind, FailureKind::Fatal, "{failure:?}");
    }
    let story_ref = story.borrow();
    let list = &story_ref.tasks.list;
    assert_eq!(list.len(), 164, "tasks given");
    let objectives: Vec<_> = list
        .iter()
        .flat_map(|t| t.borrow().objectives.clone())
        .collect();
    assert_eq!(objectives.len(), 340, "objectives");
    let count = |f: &dyn Fn(&volya_engine_script::story::Objective) -> bool| {
        objectives.iter().filter(|o| f(&o.borrow())).count()
    };
    assert_eq!(count(&|o| !o.spot_type.is_empty()), 162, "map spot types");
    assert_eq!(count(&|o| !o.spot_hint.is_empty()), 141, "hints");
    assert_eq!(count(&|o| !o.shown_by_default), 25, "map_location_hidden");
    let lists: Vec<usize> = (0..4)
        .map(|l| {
            objectives
                .iter()
                .map(|o| o.borrow().functions[l].len())
                .sum()
        })
        .collect();
    assert_eq!(lists, [103, 113, 4, 1], "function elements by list");
    let keys: Vec<u32> = objectives
        .iter()
        .flat_map(|o| o.borrow().functions.concat())
        .collect();
    drop(story_ref);
    // 6.3: every one is defined in its script.
    for key in keys {
        assert!(s.story_function_resolved(key).unwrap(), "function {key}");
    }

    // `IP` 3.4, 4.1: the trader's path gives the first task by a portion.
    story.borrow_mut().tasks = Default::default();
    story.borrow_mut().take_events();
    s.exec(
        "=trader",
        b"for _, id in ipairs({'esc_trader_experienced', 'esc_trader_tutorial_chosen',
              'esc_trader_can_leave', 'tutorial_wounded_start'}) do
            assert(actor_handle:give_info_portion(id))
          end
          assert(actor_handle:has_info('esc_trader_can_leave'))",
    )
    .unwrap();
    {
        let story = story.borrow();
        let list = &story.tasks.list;
        assert_eq!(list.len(), 1, "the first task");
        let task = list[0].borrow();
        assert_eq!(task.id.as_deref(), Some(&b"esc_flash_task"[..]));
        assert!(task.objectives.len() > 1, "its objectives");
        assert_eq!(task.objectives[0].borrow().state, IN_PROGRESS);
    }
    let events = story.borrow_mut().take_events();
    assert!(
        events.iter().any(|e| matches!(e, Event::TaskState { .. })),
        "task_state for the first task"
    );
    let infos = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::InfoPortion(info_portions::Event::InventoryInfo { .. })
            )
        })
        .count();
    assert_eq!(infos, 4, "inventory_info per portion");
    // 5.1: its shipped functions run in the update, over a stand-in for
    // spec K's simulation (TODO(#215)).
    s.exec(
        "=stand_ins",
        b"local any = {}
          setmetatable(any, {__index = function() return any end, __call = function() return any end})
          function alife() return any end",
    )
    .unwrap();
    s.update_player_tasks().unwrap();
}

/// The global `actor_handle`: the player's handle.
fn set_actor(s: &mut Scripts, actor: volya_engine_script::classes::EnginePart) {
    s.exec("=actor", b"function __set_actor(h) actor_handle = h end")
        .unwrap();
    s.call("__set_actor", &[Value::Object(actor)]).unwrap();
}
