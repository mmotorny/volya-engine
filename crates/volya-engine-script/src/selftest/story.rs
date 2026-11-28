// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md on
//! every target, one check per section, over a story of our own: a task
//! file, portions, a sequence file and a script of task functions.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::info_portions::{self, Declarations, InfoPortions};
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;
use volya_engine_formats::xml;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass, SpawnData};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::story::{COMPLETED, Event, FAIL, IN_PROGRESS, NO_OBJECT, Story};
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 information portions", portions),
    ("3 task values", task_values),
    ("4 the player's tasks", player_tasks),
    ("5 objective states", states),
    ("6 the task functions", functions),
    ("7 news and talk messages", news),
    ("8 use tips", tips),
    ("9 map spots", map_spots),
    ("10 game difficulty", difficulty),
    ("11 tutorials", tutorials),
    ("callbacks raised (spec O)", callbacks),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("story {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const PLAYER: u16 = 0;
const TRADER: u16 = 60;
const RIFLE: u16 = 20;
const TRADER_STORY_ID: i32 = 7;
const NOW: u64 = 1_000;

const TASKS: &str = r#"<game_tasks>
<game_task id="flash_task" prio="5">
  <title>Flash</title>
  <objective key="k0">
    <text>Find the flash</text>
    <icon x="1" y="2" width="3" height="4">ui\ui_icons_task</icon>
    <article>art_flash</article>
    <function_fail>tasks.never</function_fail>
  </objective>
  <objective>
    <text>Talk</text>
    <map_location_type hint="the trader">blue_location</map_location_type>
    <object_story_id>trader_id</object_story_id>
    <infoportion_complete>talked</infoportion_complete>
    <infoportion_set_complete>reward</infoportion_set_complete>
    <function_call_complete>tasks.on_done</function_call_complete>
  </objective>
  <objective>
    <text>Return</text>
    <map_location_type>red_location</map_location_type>
    <map_location_hidden/>
    <object_story_id>trader_id</object_story_id>
    <function_complete>tasks.missing</function_complete>
    <function_complete>tasks.returned</function_complete>
  </objective>
</game_task>
#include "gameplay\more_tasks.xml"
</game_tasks>"#;

const MORE_TASKS: &str = r#"<game_task id="other_task">
  <title>Other</title>
  <objective><text>o</text><icon>ui_icon_other</icon></objective>
  <objective><text>a</text></objective>
</game_task>"#;

const PORTIONS: &str = r#"<game_information_portions>
  <info_portion id="talked"/>
  <info_portion id="reward"/>
  <info_portion id="with_action"><action>tasks.on_portion</action></info_portion>
  <info_portion id="gives_task"><task>flash_task</task></info_portion>
</game_information_portions>"#;

const ARTICLES: &str =
    r#"<encyclopedia><article id="art_flash" article_type="task"/></encyclopedia>"#;

const SEQUENCES: &str = r#"<window>
  <intro>
    <item><function_on_start>tasks.seq_start</function_on_start><function_on_stop>tasks.seq_stop</function_on_stop></item>
    <item><function_on_stop>tasks.seq_stop</function_on_stop></item>
  </intro>
  <empty/>
</window>"#;

const SCRIPT: &str = r##"
calls = {}
local function note(text) table.insert(calls, text) end
function never(id, n) note("never " .. tostring(id) .. " " .. n) return false end
function on_done(id, n) note("done " .. id .. " " .. n) end
returned_flag = false
function returned(id, n) note("returned " .. n) return returned_flag end
function on_portion(holder, nothing) note("portion " .. holder:id() .. " " .. tostring(nothing)) end
function seq_start(...) note("start " .. select("#", ...)) end
function seq_stop() note("stop") end
function fails() error("boom", 0) end
function complete_flag() return complete_now end
"##;

/// A test state: the scripts, the story, the objects, the services.
struct State {
    s: Scripts,
    story: Rc<RefCell<Story>>,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
}

fn data(id: u16, name: &str, class: &str) -> SpawnData {
    SpawnData {
        id,
        name: name.as_bytes().to_vec(),
        section: name.as_bytes().to_vec(),
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

fn parse(text: &str) -> Result<xml::Document, String> {
    xml::parse(text.as_bytes()).map_err(|e| e.to_string())
}

/// A state with everything up to the story registered, `_G` loaded, a
/// global `obj(id)` giving handles, the player, a trader and a rifle, and
/// the portions.
fn state() -> Result<State, String> {
    let tree = Tree(
        [
            ("gamedata/scripts/_g.script", ""),
            ("gamedata/scripts/tasks.script", SCRIPT),
            ("gamedata/config/gameplay/game_tasks.xml", TASKS),
            ("gamedata/config/gameplay/more_tasks.xml", MORE_TASKS),
            ("gamedata/config/ui/game_tutorials.xml", SEQUENCES),
        ]
        .iter()
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect(),
    );
    let tree = Rc::new(tree);
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(|e| e.message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(b"[sys]\r\n").map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(|e| e.message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(|e| e.message)?;
    let story = Rc::new(RefCell::new(Story::new()));
    s.register_story(Rc::clone(&story)).map_err(|e| e.message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    let mut trader = data(TRADER, "trader", "AI_TRADE");
    trader.story_id = TRADER_STORY_ID;
    for (class, data) in [
        (ObjectClass::Player, data(PLAYER, "actor", "O_ACTOR")),
        (ObjectClass::Named("CAI_Trader"), trader),
        (
            ObjectClass::Named("CWeaponAK74"),
            data(RIFLE, "rifle", "WP_AK74"),
        ),
    ] {
        s.spawn_engine_object(class, data)
            .map_err(|e| e.error.message)?;
    }
    objects.borrow_mut().game_time = NOW;
    let declarations = Declarations::from_documents(
        &[("portions".into(), parse(PORTIONS)?)],
        &[("articles".into(), parse(ARTICLES)?)],
        &[("dialogs".into(), parse("<game_dialogs/>")?)],
        &parse(
            TASKS
                .replace("#include \"gameplay\\more_tasks.xml\"", MORE_TASKS)
                .as_str(),
        )?,
    )
    .map_err(|e| e.to_string())?;
    story.borrow_mut().info_portions = Some(Rc::new(RefCell::new(InfoPortions::new(
        Arc::new(declarations),
        PLAYER,
    ))));
    Ok(State {
        s,
        story,
        objects,
        services,
    })
}

/// The prelude of the Lua checks: `actor`, `trader`, `rifle` and
/// `story_ids` (spec K's, here a table).
fn prelude() -> String {
    format!(
        "local actor, trader, rifle = obj({PLAYER}), obj({TRADER}), obj({RIFLE})\n\
         story_ids = {{ trader_id = {TRADER_STORY_ID} }}\n"
    )
}

fn check_lua(t: &mut State, name: &str, body: &str) -> Result<(), String> {
    run(&mut t.s, name, &format!("{}{body}", prelude()))
}

/// `code` must stop the state with a fatal error whose message includes
/// `text`.
fn fatal(code: &str, text: &str) -> Result<(), String> {
    let mut t = state()?;
    let result = t.s.exec(
        "=fatal",
        format!("{}pcall(function() {code} end)", prelude()).as_bytes(),
    );
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal && e.message.contains(text)),
        || format!("{code}: {result:?}, not fatal with {text:?}"),
    )
}

fn log_has(t: &State, line: &str) -> bool {
    t.services
        .borrow()
        .console
        .log()
        .iter()
        .any(|l| l.contains(line))
}

/// Section 1: what is registered.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "1",
        r#"
for _, m in ipairs({"give_info_portion", "disable_info_portion", "has_info", "dont_has_info",
    "info_add", "info_clear", "give_task", "get_task_state", "set_task_state",
    "give_game_news", "give_talk_message", "set_tip_text", "set_tip_text_default"}) do
  eq(type(actor[m]), "function", m)
end
eq(actor.give_info_portion_via_pda, nil, "give_info_portion_via_pda is absent")
for _, f in ipairs({"map_add_object_spot", "map_add_object_spot_ser", "map_change_spot_hint",
    "map_has_object_spot", "map_remove_object_spot", "get_game_difficulty",
    "set_game_difficulty"}) do
  eq(type(level[f]), "function", "level." .. f)
end
eq(type(game.start_tutorial), "function", "game.start_tutorial")
eq(type(game.has_active_tutorial), "function", "game.has_active_tutorial")
eq(task.fail, 0, "task.fail")
eq(task.in_progress, 1, "task.in_progress")
eq(task.completed, 2, "task.completed")
eq(task.task_dummy, -1, "task.task_dummy")
raises(function() return task() end, "", "task has no constructor")
local t = CGameTask()
for _, m in ipairs({"load", "set_title", "get_title", "get_priority", "set_priority", "get_id",
    "set_id", "get_objectives_cnt", "add_objective", "get_objective"}) do
  eq(type(t[m]), "function", "CGameTask:" .. m)
end
local o = SGameTaskObjective(t, 1)
for _, m in ipairs({"add_complete_func", "add_complete_info", "add_fail_func", "add_fail_info",
    "add_on_complete_func", "add_on_complete_info", "add_on_fail_func", "add_on_fail_info",
    "get_description", "get_idx", "get_state", "set_article_id", "set_article_key",
    "set_description", "set_map_hint", "set_map_location", "set_object_id",
    "set_icon_name"}) do
  eq(type(o[m]), "function", "SGameTaskObjective:" .. m)
end
eq(o.def_ml_enabled, true, "def_ml_enabled")
"#,
    )
}

/// Section 2: giving, disabling and asking, on holders only; the script
/// part and the data path of a portion's task.
pub fn portions() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2",
        r#"
eq(trader:give_info_portion("talked"), true, "give on a holder")
eq(trader:has_info("talked"), true, "has_info")
eq(trader:dont_has_info("talked"), false, "dont_has_info")
eq(actor:has_info("talked"), false, "another holder")
eq(rifle:give_info_portion("talked"), false, "give on another object")
eq(rifle:has_info("talked"), false, "has_info on another object")
eq(rifle:dont_has_info("talked"), true, "dont_has_info on another object")
eq(rifle:disable_info_portion("talked"), false, "disable on another object")
eq(trader:disable_info_portion("talked"), true, "disable on a holder")
eq(trader:has_info("talked"), false, "disabled")
eq(actor:info_add("x"), nil, "info_add")
eq(actor:info_clear(), nil, "info_clear")
trader:give_info_portion("with_action")
eq(tasks.calls[1], "portion 60 nil", "the action, with the holder and nil")
actor:give_info_portion("gives_task")
"#,
    )?;
    let story = t.story.borrow();
    ensure(story.tasks.list.len() == 1, || {
        format!("the portion's task: {} tasks", story.tasks.list.len())
    })?;
    let task = story.tasks.list[0].borrow();
    ensure(
        task.id.as_deref() == Some(b"flash_task") && task.received == NOW,
        || format!("the portion's task: {:?} at {}", task.id, task.received),
    )?;
    drop(task);
    ensure(matches!(&story.tasks.active, Some((_, 1))), || {
        "the portion's task is active with objective 1".into()
    })?;
    drop(story);
    let events = t.story.borrow_mut().take_events();
    ensure(
        events
            .iter()
            .any(|e| matches!(e, Event::TaskState { state, .. } if *state == IN_PROGRESS)),
        || format!("the portion's task_state: {events:?}"),
    )?;
    ensure(
        events.iter().any(|e| matches!(e, Event::InfoPortion(info_portions::Event::InventoryInfo { portion }) if portion == "gives_task")),
        || "inventory_info".into(),
    )?;
    fatal(
        "actor:give_info_portion('undeclared')",
        "item not found, id undeclared",
    )
}

/// Section 3: `CGameTask`, `SGameTaskObjective` and `load`.
pub fn task_values() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "3",
        r#"
local t = CGameTask()
eq(t:get_id(), nil, "no id")
eq(t:get_title(), nil, "no title")
eq(t:get_objectives_cnt(), 0, "no objectives")
t:set_id("a") t:set_title("A") t:set_priority(3)
eq(t:get_id(), "a", "set_id") eq(t:get_title(), "A", "set_title") eq(t:get_priority(), 3, "set_priority")
local o = SGameTaskObjective(t, 4)
eq(o:get_idx(), 4, "the index") eq(o:get_state(), task.in_progress, "in progress")
eq(o:get_description(), nil, "no description")
o:set_description("d")
t:add_objective(o)
o:set_description("changed")
eq(t:get_objective(0):get_description(), "d", "add_objective copies")
t:get_objective(0):set_description("e")
eq(t:get_objective(0):get_description(), "e", "get_objective refers into the task")
o.def_ml_enabled = false
eq(o.def_ml_enabled, false, "def_ml_enabled writes")
local loaded = CGameTask()
loaded:load("flash_task")
eq(loaded:get_id(), "flash_task", "load sets the id")
eq(loaded:get_title(), "Flash", "the title")
eq(loaded:get_priority(), 5, "the priority")
eq(loaded:get_objectives_cnt(), 3, "the objectives")
eq(loaded:get_objective(2):get_idx(), 2, "the index in file order")
eq(loaded:get_objective(1):get_description(), "Talk", "the text")
local other = CGameTask()
other:load("other_task")
eq(other:get_priority(), -1, "no prio")
"#,
    )?;
    // The objectives as loaded.
    let task = Rc::new(RefCell::new(crate::story::Task::default()));
    t.s.with_story(|ctx, story| ctx.load_task(story, &task, b"flash_task"))
        .map_err(|e| e.message)?;
    let t0 = task.borrow();
    let (o0, o1, o2) = (
        t0.objectives[0].borrow(),
        t0.objectives[1].borrow(),
        t0.objectives[2].borrow(),
    );
    ensure(
        o0.article == b"art_flash" && o0.article_key == b"k0",
        || "objective 0's article and key".into(),
    )?;
    ensure(
        o0.icon.texture == b"ui\\ui_icons_task" && o0.icon.rect == [1.0, 2.0, 3.0, 4.0],
        || format!("objective 0's icon: {:?}", o0.icon),
    )?;
    ensure(
        o1.spot_type == b"blue_location" && o1.spot_hint == b"the trader" && o1.shown_by_default,
        || "objective 1's spot".into(),
    )?;
    ensure(o1.target == TRADER, || {
        format!("the story id's object: {}", o1.target)
    })?;
    ensure(!o2.shown_by_default, || "map_location_hidden".into())?;
    ensure(
        o1.portions[0] == [b"talked".to_vec()] && o1.portions[2] == [b"reward".to_vec()],
        || "the portion lists".into(),
    )?;
    ensure(o2.functions[0].len() == 2 && o2.names[0].is_empty(), || {
        "the function lists resolved, the names empty".into()
    })?;
    drop((o0, o1, o2));
    drop(t0);
    fatal(
        "CGameTask():load('no_such_task')",
        "NavigateToNode failed in XML file game_tasks.xml",
    )
}

/// Section 4: giving a task, and its state by id.
pub fn player_tasks() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "4",
        r#"
local other = CGameTask() other:load("other_task")
actor:give_task(other, 0, false)
local flash = CGameTask() flash:load("flash_task")
trader:give_task(flash, 5000, false)
local again = CGameTask() again:load("flash_task")
actor:give_task(again, 0, true)
eq(actor:get_task_state("flash_task", 1), task.in_progress, "get_task_state")
eq(actor:get_task_state("flash_task", 3), task.task_dummy, "an objective beyond the count")
eq(actor:get_task_state("none", 0), task.task_dummy, "no such task")
actor:set_task_state(task.completed, "none", 1)
"#,
    )?;
    ensure(log_has(&t, "actor does not has task [none]"), || {
        "set_task_state of no task logs".into()
    })?;
    let story = t.story.borrow();
    let ids: Vec<Option<Vec<u8>>> = story
        .tasks
        .list
        .iter()
        .map(|t| t.borrow().id.clone())
        .collect();
    ensure(
        ids == [Some(b"flash_task".to_vec()), Some(b"other_task".to_vec())],
        || format!("sorted by priority, −1 last; checked: {ids:?}"),
    )?;
    let flash = story.tasks.list[0].borrow();
    ensure(
        flash.received == NOW && flash.time_limit == NOW + 5000,
        || "the received time and the limit".into(),
    )?;
    drop(flash);
    ensure(
        matches!(&story.tasks.active, Some((t, 1)) if t.borrow().id.as_deref() == Some(b"flash_task")),
        || "the task of lower priority becomes active".into(),
    )?;
    let spot = story.map_spots.get(TRADER, b"blue_location");
    ensure(
        spot.is_some_and(|s| s.count == 1 && s.saved && s.hint == b"the trader"),
        || format!("objective 1's spot: {spot:?}"),
    )?;
    ensure(
        story.map_spots.get(TRADER, b"red_location").is_none(),
        || "a hidden objective has no spot".into(),
    )?;
    let articles = story
        .info_portions
        .as_ref()
        .map(|p| p.borrow().articles().len());
    ensure(articles == Some(1), || {
        format!("objective 0's article: {articles:?}")
    })?;
    drop(story);
    let events = t.story.borrow_mut().take_events();
    let callbacks = events
        .iter()
        .filter(|e| matches!(e, Event::TaskState { .. }))
        .count();
    let added = events
        .iter()
        .filter(|e| matches!(e, Event::MapLocationAdded { object, .. } if *object == TRADER))
        .count();
    ensure(callbacks == 2 && added == 1, || {
        format!("task_state and map_location_added: {events:?}")
    })
}

/// Section 5: the update, a state change, hidden spots, the active task.
pub fn states() -> Result<(), String> {
    let mut t = state()?;
    let give = r#"
local flash = CGameTask() flash:load("flash_task")
actor:give_task(flash, 500, false)
"#;
    check_lua(&mut t, "5 give", give)?;
    t.s.update_player_tasks().map_err(|e| e.message)?;
    check_lua(
        &mut t,
        "5 nothing yet",
        r#"
eq(actor:get_task_state("flash_task", 1), task.in_progress, "in progress")
eq(tasks.calls[1], "never flash_task 0", "objective 0's fail function")
eq(tasks.calls[2], "returned 2", "a later objective's complete function, past an unresolved one")
actor:give_info_portion("talked")
"#,
    )?;
    t.s.update_player_tasks().map_err(|e| e.message)?;
    check_lua(
        &mut t,
        "5 completed",
        r#"
eq(actor:get_task_state("flash_task", 1), task.completed, "the complete portions")
eq(actor:has_info("reward"), true, "the set-on-complete portion given")
local found = false
for _, c in ipairs(tasks.calls) do if c == "done flash_task 1" then found = true end end
eq(found, true, "the on-complete function, with the id and the index")
eq(level.map_has_object_spot(60, "blue_location"), 0, "its spot removed")
eq(level.map_has_object_spot(60, "red_location"), 1, "the hidden spot shown")
"#,
    )?;
    {
        let story = t.story.borrow();
        ensure(matches!(&story.tasks.active, Some((_, 2))), || {
            "the next objective is active".into()
        })?;
        let task = story.tasks.list[0].borrow();
        ensure(task.objectives[1].borrow().target == NO_OBJECT, || {
            "the completed objective's target is none".into()
        })?;
        ensure(
            story.map_spots.active == Some((TRADER, b"red_location".to_vec())),
            || format!("the active spot: {:?}", story.map_spots.active),
        )?;
    }
    // The time limit fails objective 0, and so every objective in progress.
    t.objects.borrow_mut().game_time = NOW + 501;
    t.s.update_player_tasks().map_err(|e| e.message)?;
    let story = t.story.borrow();
    let task = story.tasks.list[0].borrow();
    let states: Vec<i32> = task.objectives.iter().map(|o| o.borrow().state).collect();
    ensure(states == [FAIL, COMPLETED, FAIL], || {
        format!("the time limit: {states:?}")
    })?;
    ensure(task.finished == NOW + 501, || "the finished time".into())?;
    ensure(story.tasks.active.is_none(), || "no active task".into())?;
    drop(task);
    drop(story);
    // Completing objective 0 by script completes the rest.
    let mut t = state()?;
    check_lua(
        &mut t,
        "5 by script",
        &format!(
            "{give}actor:set_task_state(task.completed, 'flash_task', 0)\n\
             eq(actor:get_task_state('flash_task', 2), task.completed, 'objective 0 completes the rest')"
        ),
    )?;
    fatal(
        &format!("{give}actor:set_task_state(task.in_progress, 'flash_task', 0)"),
        "repeats without end",
    )
}

/// Section 6: resolution and the calls.
pub fn functions() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "6",
        r#"
local t = CGameTask() t:load("other_task")
local o = SGameTaskObjective(t, 2)
o:add_complete_func("tasks.complete_flag")
o:add_complete_func("tasks.nowhere")
t:add_objective(o)
actor:give_task(t, 0, false)
"#,
    )?;
    ensure(
        log_has(
            &t,
            "Cannot find script function described in task objective  tasks.nowhere",
        ),
        || "an unresolved added name is logged".into(),
    )?;
    t.s.exec("=flag", b"complete_now = true")
        .map_err(|e| e.message)?;
    t.s.update_player_tasks().map_err(|e| e.message)?;
    let reached = t.story.borrow().tasks.list[0].borrow().objectives[2]
        .borrow()
        .state;
    ensure(reached == COMPLETED, || {
        format!("added complete functions: {reached}")
    })?;
    let mut t = state()?;
    check_lua(
        &mut t,
        "6 failing",
        r#"
local t = CGameTask() t:load("other_task")
local o = SGameTaskObjective(t, 2)
o:add_fail_func("tasks.fails")
t:add_objective(o)
actor:give_task(t, 0, false)
"#,
    )?;
    let failed = t.s.update_player_tasks();
    ensure(
        matches!(&failed, Err(e) if e.kind == ErrorKind::Fatal && e.message == "LUA error: boom"),
        || format!("a failing task function: {failed:?}"),
    )
}

/// Section 7: news and talk messages.
pub fn news() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "7",
        r#"
local r = Frect() r:set(1, 2, 3, 4)
trader:give_game_news("now", "ui\\tex", r, 0, 0)
actor:give_game_news("later", "ui\\tex", r, 100, 3000)
actor:give_talk_message("hidden", "ui\\tex", r, nil)
"#,
    )?;
    {
        let story = t.story.borrow();
        ensure(
            story.news.len() == 1
                && story.news[0].show_time == 5000
                && story.news[0].rect == [1.0, 2.0, 3.0, 4.0],
            || format!("news at once: {:?}", story.news),
        )?;
        ensure(story.queued_news.len() == 1, || "news queued".into())?;
    }
    t.story.borrow_mut().talk_window_shown = true;
    check_lua(
        &mut t,
        "7 talk",
        r#"actor:give_talk_message("shown", "ui\\tex", Frect(), nil)"#,
    )?;
    t.services.borrow_mut().device.frame(100, 0.1, false);
    t.s.story_frame().map_err(|e| e.message)?;
    let story = t.story.borrow();
    ensure(
        story.news.len() == 2 && story.news[1].show_time == 3000,
        || "queued news added when due".into(),
    )?;
    drop(story);
    let events = t.story.borrow_mut().take_events();
    let talk: Vec<&Event> = events
        .iter()
        .filter(|e| matches!(e, Event::TalkMessage { .. }))
        .collect();
    ensure(
        matches!(talk.as_slice(), [Event::TalkMessage { text, template, .. }]
            if text == b"shown" && template == b"iconed_answer_item"),
        || format!("talk messages only while shown: {talk:?}"),
    )
}

/// Section 8: use tips.
pub fn tips() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "8",
        r#"rifle:set_tip_text("take it") trader:set_tip_text("talk")"#,
    )?;
    ensure(
        t.story.borrow().tip_texts.get(&RIFLE).map(Vec::as_slice) == Some(&b"take it"[..]),
        || "set_tip_text".into(),
    )?;
    check_lua(&mut t, "8 default", "rifle:set_tip_text_default()")?;
    ensure(
        {
            let story = t.story.borrow();
            !story.tip_texts.contains_key(&RIFLE) && story.tip_texts.contains_key(&TRADER)
        },
        || "set_tip_text_default stores none".into(),
    )?;
    // hud-pickup-hint.md 1.4: an empty text is kept, not none.
    check_lua(&mut t, "8 empty", r#"trader:set_tip_text("")"#)?;
    ensure(
        t.story.borrow().tip_texts.get(&TRADER).map(Vec::as_slice) == Some(&b""[..]),
        || "set_tip_text of an empty text keeps it".into(),
    )
}

/// Section 9: the `level` map functions.
pub fn map_spots() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "9",
        r#"
level.map_add_object_spot(60, "green_location", "a")
level.map_add_object_spot(60, "green_location", "")
eq(level.map_has_object_spot(60, "green_location"), 2, "a count")
level.map_change_spot_hint(60, "green_location", "b")
level.map_change_spot_hint(61, "green_location", "c")
level.map_remove_object_spot(60, "green_location")
eq(level.map_has_object_spot(60, "green_location"), 1, "one count removed")
level.map_remove_object_spot(60, "green_location")
eq(level.map_has_object_spot(60, "green_location"), 0, "none")
level.map_remove_object_spot(60, "green_location")
level.map_add_object_spot_ser(20, "treasure", "t")
"#,
    )?;
    let story = t.story.borrow();
    ensure(
        story
            .map_spots
            .get(RIFLE, b"treasure")
            .is_some_and(|s| s.saved && s.hint == b"t"),
        || "_ser marks it saved".into(),
    )?;
    drop(story);
    let added = t
        .story
        .borrow_mut()
        .take_events()
        .iter()
        .filter(|e| matches!(e, Event::MapLocationAdded { .. }))
        .count();
    ensure(added == 2, || {
        format!("map_location_added for new spots only: {added}")
    })
}

/// Section 10: the game difficulty.
pub fn difficulty() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "10",
        r#"
eq(level.get_game_difficulty(), 3, "the console's master")
level.set_game_difficulty(1)
eq(level.get_game_difficulty(), 1, "set")
"#,
    )?;
    let events = t.story.borrow_mut().take_events();
    ensure(
        matches!(
            events.as_slice(),
            [Event::Difficulty {
                value: 1,
                name: "gd_stalker"
            }]
        ),
        || format!("applied to the player: {events:?}"),
    )
}

/// Section 11: tutorials and their functions.
pub fn tutorials() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "11",
        r#"
eq(game.has_active_tutorial(), false, "none")
game.start_tutorial("intro")
eq(game.has_active_tutorial(), true, "started")
eq(tasks.calls[1], "start 0", "the first item's start function, no arguments")
"#,
    )?;
    t.s.sequence_item_stopped().map_err(|e| e.message)?;
    t.s.sequence_item_stopped().map_err(|e| e.message)?;
    check_lua(
        &mut t,
        "11 stopped",
        r#"
eq(tasks.calls[2], "stop", "the stop function")
eq(tasks.calls[3], "stop", "the last item's stop function")
eq(game.has_active_tutorial(), true, "until the next frame")
"#,
    )?;
    t.s.story_frame().map_err(|e| e.message)?;
    check_lua(
        &mut t,
        "11 discarded",
        r#"eq(game.has_active_tutorial(), false, "discarded")"#,
    )?;
    fatal("game.start_tutorial('empty')", "has no item")
}

/// The story's callbacks raised on the player's object through spec O
/// (`task_state` with engine objects over the task and its objective,
/// `map_location_added`, `inventory_info`), in order, those they raise
/// too; the other events returned.
pub fn callbacks() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "callbacks",
        r#"
got = {}
local function note(text) table.insert(got, text) end
actor:set_callback(callback.task_state, function(task, objective, state)
  note("task " .. task:get_id() .. " " .. objective:get_idx() .. " " .. state)
  if objective:get_idx() == 0 then
    actor:give_info_portion("talked")
  end
end)
actor:set_callback(callback.map_location_added, function(kind, id)
  note("spot " .. kind .. " " .. id)
end)
actor:set_callback(callback.inventory_info, function(who, portion)
  note("info " .. who:id() .. " " .. portion)
end)
local flash = CGameTask() flash:load("flash_task")
actor:give_task(flash, 0, false)
"#,
    )?;
    let rest = t.s.raise_story_callbacks().map_err(|e| e.message)?;
    ensure(
        matches!(
            rest.as_slice(),
            [
                Event::PdaChanged,
                Event::InfoPortion(info_portions::Event::DialogsChanged)
            ]
        ),
        || format!("the other events returned: {rest:?}"),
    )?;
    check_lua(
        &mut t,
        "callbacks raised",
        &format!(
            r#"
eq(#got, 3, "three callbacks")
eq(got[1], "spot blue_location {TRADER}", "map_location_added")
eq(got[2], "task flash_task 0 " .. task.in_progress, "task_state")
eq(got[3], "info {PLAYER} talked", "inventory_info, raised by a callback")
"#
        ),
    )?;
    ensure(
        t.s.raise_story_callbacks()
            .map_err(|e| e.message)?
            .is_empty(),
        || "the events taken".into(),
    )
}
