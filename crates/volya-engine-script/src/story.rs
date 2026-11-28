// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's story state as scripts reach it (script API P,
//! docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md;
//! section numbers are its): the `game_object` members for information
//! portions, tasks, news, talk messages and use tips, the classes `task`,
//! `CGameTask` and `SGameTaskObjective`, the `level` functions for map
//! spots and the game difficulty, the `game` functions for tutorials
//! ([`members`]), and the script functions the engine calls from tasks and
//! sequences.
//!
//! What scripts read and change is [`Story`]: the information portions
//! (docs/behavior/info-portions.md, [`InfoPortions`]), the player's tasks
//! ([`tasks`]), map spots ([`map_spots`]), news, use tips and sequences,
//! and the events for the engine ([`Event`]). Script functions are called
//! by the Lua side (`story.lua`): what a member's engine part asks of it
//! ([`Request`]) runs when the member returns, in order (TODO(#1066): the
//! original runs it at its place); the engine's own calls
//! ([`Scripts::update_player_tasks`] and the others) run it after each
//! step. See docs/architecture/scripts.md, Story state.

mod map_spots;
pub mod members;
pub mod tasks;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::rc::{Rc, Weak};

use volya_engine_core::info_portions::{self, InfoPortions, TASKS_FILE};
use volya_engine_core::vfs;
use volya_engine_formats::cp1251;
use volya_engine_formats::xml::{self, Document};

use crate::classes::EnginePart;
use crate::files::FileSystem;
use crate::files::index::IndexVfs;
use crate::game_object::callbacks::ObjectEvent;
use crate::game_object::{GameObjects, Kinds};
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::{Error, ErrorKind, Value};

pub use map_spots::{MapSpots, NO_OBJECT, Spot};
pub use tasks::{
    COMPLETED, FAIL, FunctionKey, IN_PROGRESS, Icon, List, Objective, ObjectiveRef, PlayerTasks,
    TASK_DUMMY, Task, TaskRef,
};

/// The registry key of the Lua side's entry function (`story.lua`).
pub(crate) const STORY_ENTRY: &str = "volya.story";

/// The sequence file (11.1), under `$game_config$`.
const SEQUENCE_FILE: &str = "ui\\game_tutorials.xml";

/// A news item (7.1).
#[derive(Clone, Debug, PartialEq)]
pub struct NewsItem {
    pub text: Vec<u8>,
    pub texture: Vec<u8>,
    /// The texture's rectangle: x1, y1, x2, y2 (a `Frect`).
    pub rect: [f32; 4],
    /// Shown for this long, ms.
    pub show_time: u32,
}

/// A sequence's item (11.2): its start and stop functions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SequenceItem {
    pub start: Vec<FunctionKey>,
    pub stop: Vec<FunctionKey>,
}

/// A tutorial or movie sequence (11.1). What its items show and when they
/// stop is `hud-scope` 18's (TODO(#1062)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sequence {
    /// The root child of the sequence file it was started from.
    pub name: Vec<u8>,
    pub items: Vec<SequenceItem>,
    /// The current item.
    pub item: usize,
    /// Its last item stopped: discarded on the next frame.
    pub ended: bool,
}

/// The current sequence and the previous one, discarded on the next frame
/// (11.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sequences {
    pub current: Option<Sequence>,
    pub previous: Option<Sequence>,
}

/// A texture description lookup (`set_icon_name`, 3.3): a texture id to
/// its texture file and rectangle (x, y, width, height).
pub type TextureLookup = Rc<dyn Fn(&[u8]) -> Option<Icon>>;

/// The game's information portions, shared by the level and the story.
pub type SharedInfoPortions = Rc<RefCell<InfoPortions>>;

/// What the story state asks of the engine, in order ([`Story::take_events`]).
#[derive(Clone, Debug)]
pub enum Event {
    /// The callback `task_state` (spec O, id 13) on the player's object
    /// ([`Scripts::raise_story_callbacks`]).
    TaskState {
        task: TaskRef,
        objective: ObjectiveRef,
        state: i32,
    },
    /// The callback `map_location_added` (spec O, id 14) on the player's
    /// object ([`Scripts::raise_story_callbacks`]).
    MapLocationAdded { spot_type: Vec<u8>, object: u16 },
    /// What receiving and disabling portions ask besides their script
    /// functions (`IP` 3.4: the callbacks `article_info` and
    /// `inventory_info`, the second raised by
    /// [`Scripts::raise_story_callbacks`]; the dialogue window's mark).
    InfoPortion(info_portions::Event),
    /// The PDA is told that its contents changed (4.1 step 6, 5.2 step 6).
    PdaChanged,
    /// A news item added to the player's news (7.1): what it shows is
    /// PD-11's and `hud-scope` 9's.
    NewsAdded(NewsItem),
    /// An iconed line for the talk window (7.2, `talk-window` 6.2).
    TalkMessage {
        text: Vec<u8>,
        texture: Vec<u8>,
        rect: [f32; 4],
        template: Vec<u8>,
    },
    /// The difficulty applied to the player's object (10): its immunities
    /// from `actor_immunities_<name>` and its hit probability from
    /// `hit_probability_<name>`.
    Difficulty { value: i32, name: &'static str },
    /// A sequence started (11.1): `hud-scope` 18 plays it.
    SequenceStarted { name: Vec<u8> },
}

/// What the Lua side does for the engine part of a call, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Request {
    /// Resolve `name` (D 1.1) as the function of `key`; when it doesn't
    /// resolve and `log`, log `Cannot find script function described in
    /// task objective  <name>` (6.1).
    Resolve {
        key: FunctionKey,
        name: Vec<u8>,
        log: bool,
    },
    /// Look `text` up in `story_ids` and answer the target of `key`.
    StoryId { key: FunctionKey, text: Vec<u8> },
    /// Call the function of `key`, if it resolved, with a task's id and an
    /// objective's index (6.2); fatal as D 1.2 when it fails.
    Call {
        key: FunctionKey,
        task: Value,
        index: i32,
    },
    /// Call the function of `key` with no arguments (11.2).
    CallBare { key: FunctionKey },
    /// A portion's `action` (`IP` 3.3): resolve `function` and call it
    /// with the holder's handle and nil.
    Action { function: Vec<u8>, holder: u16 },
}

/// The player's story state scripts read and change.
#[derive(Default)]
pub struct Story {
    /// The information portions (`IP`), shared with the level, which sets
    /// them when the game starts.
    pub info_portions: Option<SharedInfoPortions>,
    pub tasks: PlayerTasks,
    pub map_spots: MapSpots,
    /// The player's news (`sav` 9 part 5), oldest first.
    pub news: Vec<NewsItem>,
    /// News items waiting for their due time on the frame clock, ms.
    pub queued_news: Vec<(u32, NewsItem)>,
    /// The usable parts' hint texts (8), by object id; none for an empty
    /// one.
    pub tip_texts: BTreeMap<u16, Vec<u8>>,
    /// Whether the single-player interface's talk window is shown (7.2).
    pub talk_window_shown: bool,
    /// The simulation's story registry (story id → object id) while the
    /// simulation runs (3.4); else the level's objects are searched.
    pub story_registry: Option<BTreeMap<i32, u16>>,
    /// The interface's texture descriptions (spec G3's data), for
    /// `set_icon_name`; an id it lacks, or no lookup, keeps the name with
    /// an empty rectangle (unestablished, 3.3).
    pub textures: Option<TextureLookup>,
    pub sequences: Sequences,
    events: Vec<Event>,
    requests: Vec<Request>,
    next_key: FunctionKey,
    /// Objectives waiting for their story id's answer, by key.
    story_ids: HashMap<FunctionKey, Weak<RefCell<Objective>>>,
    /// The tasks scripts constructed, by engine part (an objective's
    /// constructor names its task by its object).
    task_parts: HashMap<EnginePart, Weak<RefCell<Task>>>,
    /// The task file, opened on the first load and kept (3.4).
    task_file: Option<Rc<Document>>,
    sequence_file: Option<Rc<Document>>,
}

impl Story {
    pub fn new() -> Self {
        Self::default()
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// A new game's story (its level ended): everything the game changed
    /// forgotten, the interface's texture descriptions kept.
    pub fn new_game(&mut self) {
        let textures = self.textures.take();
        *self = Self {
            textures,
            ..Self::default()
        };
    }

    /// Whether the information portions have log lines or events the
    /// engine's own gives left ([`Scripts::info_portions_changed`]).
    pub fn portions_changed(&self) -> bool {
        self.info_portions
            .as_ref()
            .is_some_and(|p| p.borrow().has_pending())
    }

    fn new_key(&mut self) -> FunctionKey {
        self.next_key += 1;
        self.next_key
    }

    /// The icon of texture id `name` (3.3).
    fn icon(&self, name: &[u8]) -> Icon {
        self.textures
            .as_ref()
            .and_then(|lookup| lookup(name))
            .unwrap_or_else(|| Icon {
                texture: name.to_vec(),
                rect: [0.0; 4],
            })
    }

    fn remember_task(&mut self, part: EnginePart, task: &TaskRef) {
        self.task_parts.retain(|_, t| t.strong_count() > 0);
        self.task_parts.insert(part, Rc::downgrade(task));
    }

    fn task_of_part(&self, part: EnginePart) -> Option<TaskRef> {
        self.task_parts.get(&part).and_then(Weak::upgrade)
    }
}

/// What the story's members share.
#[derive(Clone)]
pub(crate) struct Context {
    pub story: Rc<RefCell<Story>>,
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    pub fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error (CL 3.8): the game's own stop, or one standing for a
    /// crash the spec believes (our wording). The error to raise.
    pub fn fatal(&self, function: &str, line: u32, text: &str) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description: "<no expression>",
                arguments: &[text],
            },
        )
    }

    /// The current game time, ms.
    pub fn now(&self) -> u64 {
        self.objects.borrow().game_time
    }

    /// The player's object, if it exists.
    pub fn player(&self) -> Option<u16> {
        self.objects
            .borrow()
            .iter()
            .find(|o| o.kinds.has(Kinds::PLAYER))
            .map(|o| o.id)
    }

    /// The player's object, which the task members reach through (4):
    /// without it, a fatal error standing for the believed crash.
    pub fn player_or_crash(&self, function: &str) -> Result<u16, String> {
        self.player()
            .ok_or_else(|| self.fatal(function, line!(), "the player's object doesn't exist"))
    }

    fn single_player(&self) -> bool {
        self.services.borrow().game_type.is_single()
    }

    /// `$game_config$` file `relative` as XML, its `#include` lines (paths
    /// under `$game_config$`) included.
    fn read_xml(&self, relative: &str) -> io::Result<Document> {
        let files = self.files.borrow();
        let tree = IndexVfs(&files.index);
        let path = files.aliases.path("$game_config$", relative)?;
        let text = vfs::read(&tree, &path)?;
        let text = xml::preprocess(&text, &mut |include| {
            let include = files
                .aliases
                .path("$game_config$", &cp1251::decode(include))?;
            vfs::read(&tree, &include)
        })?;
        xml::parse(&text)
    }

    /// Gives portion `id` to `holder` by a transfer (`IP` 3.1), then what
    /// receiving asks: its functions as requests, a task's data path, the
    /// rest as events. An undeclared id stops the game.
    pub fn give_portion(&self, story: &mut Story, holder: u16, id: &[u8]) -> Result<(), String> {
        let now = self.now();
        let Some(portions) = story.info_portions.clone() else {
            return Err("the information portions aren't set".into());
        };
        let given = portions.borrow_mut().give(holder, &cp1251::decode(id), now);
        self.after_portions(story, given)
    }

    /// Disables `id` on `holder` by a transfer (`IP` 4.1).
    pub fn disable_portion(&self, story: &mut Story, holder: u16, id: &[u8]) -> Result<(), String> {
        let Some(portions) = story.info_portions.clone() else {
            return Err("the information portions aren't set".into());
        };
        let disabled = portions.borrow_mut().disable(holder, &cp1251::decode(id));
        self.after_portions(story, disabled)
    }

    /// The log lines and events of the portions after `result`, and its
    /// error as the game's stop.
    fn after_portions(&self, story: &mut Story, result: io::Result<()>) -> Result<(), String> {
        let Some(portions) = story.info_portions.clone() else {
            return Ok(());
        };
        let (log, events) = {
            let mut portions = portions.borrow_mut();
            (portions.take_log(), portions.take_events())
        };
        {
            let mut services = self.services.borrow_mut();
            for line in &log {
                services.console.log_mut().write(line);
            }
        }
        if let Err(e) = result {
            return Err(self.fatal("CInfoPortion::Load", line!(), &e.to_string()));
        }
        for event in events {
            match event {
                info_portions::Event::Action { holder, function } => {
                    story.requests.push(Request::Action {
                        function: crate::game_object::game_bytes(&function),
                        holder,
                    });
                }
                info_portions::Event::TaskState { task } => {
                    self.give_task_by_portion(story, &crate::game_object::game_bytes(&task))?;
                }
                other => story.events.push(Event::InfoPortion(other)),
            }
        }
        Ok(())
    }

    /// The task file, opened on first use (3.4).
    fn task_file(&self, story: &mut Story) -> Result<Rc<Document>, String> {
        if let Some(file) = &story.task_file {
            return Ok(Rc::clone(file));
        }
        let name = format!("{TASKS_FILE}.xml");
        let file = self.read_xml(&format!("gameplay\\{name}")).map_err(|_| {
            self.fatal(
                "CGameTask::Load",
                line!(),
                &format!("error while parsing XML file {name}"),
            )
        })?;
        let file = Rc::new(file);
        story.task_file = Some(Rc::clone(&file));
        Ok(file)
    }

    /// `t:load(id)` (3.4): the function names to resolve and the story ids
    /// to look up become requests.
    pub fn load_task(&self, story: &mut Story, task: &TaskRef, id: &[u8]) -> Result<(), String> {
        task.borrow_mut().id = Some(id.to_vec());
        let file = self.task_file(story)?;
        let Some(element) = tasks::task_element(&file, id) else {
            return Err(self.fatal(
                "CGameTask::Load",
                line!(),
                &format!("NavigateToNode failed in XML file {TASKS_FILE}.xml"),
            ));
        };
        let mut next = story.next_key;
        let pending = {
            let story = &*story;
            tasks::load(
                task,
                element,
                &mut || {
                    next += 1;
                    next
                },
                &|name| story.icon(name),
            )
        };
        story.next_key = next;
        for (key, name) in pending.functions {
            story.requests.push(Request::Resolve {
                key,
                name,
                log: false,
            });
        }
        for (objective, text) in pending.story_ids {
            let key = story.new_key();
            story.story_ids.insert(key, Rc::downgrade(&objective));
            story.requests.push(Request::StoryId { key, text });
        }
        Ok(())
    }

    /// The story id `story_id` (from `story_ids`) answered for the
    /// objective of `key` (3.4): the object with that story id through the
    /// simulation's registry while it runs, else among the level's objects;
    /// none when no object has it.
    pub fn answer_story_id(&self, story: &mut Story, key: FunctionKey, story_id: i32) {
        let Some(objective) = story.story_ids.remove(&key).and_then(|o| o.upgrade()) else {
            return;
        };
        let target = match &story.story_registry {
            Some(registry) => registry.get(&story_id).copied(),
            None => self
                .objects
                .borrow()
                .iter()
                .find(|o| o.story_id == story_id)
                .map(|o| o.id),
        };
        objective.borrow_mut().target = target.unwrap_or(NO_OBJECT);
    }

    /// Adds a map spot (9.1) with `map_location_added` for a new one, in a
    /// single-player game with the player's object.
    pub fn add_spot(
        &self,
        story: &mut Story,
        object: u16,
        spot_type: &[u8],
        hint: &[u8],
        saved: bool,
    ) {
        let new = story.map_spots.add(object, spot_type, hint, saved);
        if new && self.single_player() && self.player().is_some() {
            story.events.push(Event::MapLocationAdded {
                spot_type: spot_type.to_vec(),
                object,
            });
        }
    }

    /// The callback `task_state` with objective 0 of `task` (4.1 step 7):
    /// a task without one reaches outside its list, fatal here.
    fn task_given(&self, story: &mut Story, task: &TaskRef) -> Result<(), String> {
        let Some(first) = task.borrow().objectives.first().cloned() else {
            return Err(self.fatal("give_task", line!(), "the task has no objective 0"));
        };
        let state = first.borrow().state;
        story.events.push(Event::TaskState {
            task: Rc::clone(task),
            objective: first,
            state,
        });
        Ok(())
    }

    /// `obj:give_task(t, time, check)` (4.1).
    pub fn give_task(
        &self,
        story: &mut Story,
        task: &TaskRef,
        time: u32,
        check: bool,
    ) -> Result<(), String> {
        self.player_or_crash("give_task")?;
        let id = task.borrow().id.clone();
        if check && id.as_deref().and_then(|id| story.tasks.find(id)).is_some() {
            return Ok(());
        }
        story.tasks.changed = true;
        story.tasks.list.push(Rc::clone(task));
        let now = self.now();
        {
            let mut t = task.borrow_mut();
            t.received = now;
            t.time_limit = now.wrapping_add(u64::from(time));
        }
        story.tasks.sort();
        let objectives = task.borrow().objectives.clone();
        for objective in &objectives {
            let o = objective.borrow().clone();
            if !o.article.is_empty()
                && let Some(portions) = story.info_portions.clone()
            {
                let mut portions = portions.borrow_mut();
                let learnt = portions.learn_article(&cp1251::decode(&o.article), now);
                let log = portions.take_log();
                drop(portions);
                for line in &log {
                    self.services.borrow_mut().console.log_mut().write(line);
                }
                if let Err(e) = learnt {
                    return Err(self.fatal("give_task", line!(), &e.to_string()));
                }
            }
            if o.has_spot() && o.shown_by_default {
                self.add_spot(story, o.target, &o.spot_type, &o.spot_hint, true);
            }
        }
        story.tasks.offer_active(task);
        story.events.push(Event::PdaChanged);
        self.task_given(story, task)
    }

    /// The data path (`IP` 3.4 step 2, 4.1): the task loaded by its id,
    /// received now, added, the active-task rule and the callback.
    /// TODO(#910): PD-6's other steps.
    fn give_task_by_portion(&self, story: &mut Story, id: &[u8]) -> Result<(), String> {
        let task = Rc::new(RefCell::new(Task::default()));
        self.load_task(story, &task, id)?;
        task.borrow_mut().received = self.now();
        story.tasks.changed = true;
        story.tasks.list.push(Rc::clone(&task));
        story.tasks.offer_active(&task);
        self.task_given(story, &task)
    }

    /// Changes objective `n` of `task` to `state` (5.2). Its portions are
    /// given now; its functions run when the caller drains the requests.
    pub fn change_state(
        &self,
        story: &mut Story,
        task: &TaskRef,
        n: usize,
        state: i32,
    ) -> Result<(), String> {
        story.tasks.changed = true;
        let Some(objective) = task.borrow().objectives.get(n).cloned() else {
            return Err(self.fatal(
                "set_task_state",
                line!(),
                &format!("objective {n} is outside the task's objectives"),
            ));
        };
        let finishing = state == FAIL || state == COMPLETED;
        // Step 2.
        if finishing {
            let (target, spot_type) = {
                let o = objective.borrow();
                (o.target, o.spot_type.clone())
            };
            if objective.borrow().has_spot() && story.map_spots.get(target, &spot_type).is_some() {
                story.map_spots.remove(target, &spot_type);
                let mut o = objective.borrow_mut();
                o.spot_type.clear();
                o.target = NO_OBJECT;
            }
        }
        // Step 3.
        objective.borrow_mut().state = state;
        if finishing {
            let (portions, functions) = if state == FAIL {
                (List::OnFail, List::OnFail)
            } else {
                (List::OnComplete, List::OnComplete)
            };
            let (given, keys, index, own) = {
                let o = objective.borrow();
                (
                    o.portions[portions as usize].clone(),
                    o.functions[functions as usize].clone(),
                    o.index,
                    o.own_task(),
                )
            };
            let player = self.player_or_crash("set_task_state")?;
            for id in &given {
                self.give_portion(story, player, id)?;
            }
            let task_id = task_id_value(own.as_ref());
            for key in keys {
                story.requests.push(Request::Call {
                    key,
                    task: task_id.clone(),
                    index,
                });
            }
            story.events.push(Event::TaskState {
                task: Rc::clone(task),
                objective: Rc::clone(&objective),
                state,
            });
        }
        // Step 4.
        let states: Vec<i32> = task
            .borrow()
            .objectives
            .iter()
            .map(|o| o.borrow().state)
            .collect();
        let rest_in_progress = states.iter().skip(1).any(|&s| s == IN_PROGRESS);
        if n == 0 || !rest_in_progress {
            if matches!(&story.tasks.active, Some((t, _)) if Rc::ptr_eq(t, task)) {
                story.tasks.active = None;
            }
            if n == 0 {
                let count = task.borrow().objectives.len();
                for i in 0..count {
                    let s = task.borrow().objectives[i].borrow().state;
                    if s != IN_PROGRESS {
                        continue;
                    }
                    if i == 0 {
                        // Believed to repeat without end (a crash): fatal.
                        return Err(self.fatal(
                            "set_task_state",
                            line!(),
                            "objective 0 changed to in progress repeats without end",
                        ));
                    }
                    self.change_state(story, task, i, state)?;
                }
            }
        } else if story.tasks.is_active(task, n) && n + 1 < states.len() {
            story.tasks.active = Some((Rc::clone(task), n + 1));
        }
        // Step 5.
        if (n == 0 && state == COMPLETED) || state == FAIL {
            task.borrow_mut().finished = self.now();
        }
        // Step 6.
        story.events.push(Event::PdaChanged);
        Ok(())
    }

    /// After the update's walk (5.1, 5.3, 5.4).
    fn after_walk(&self, story: &mut Story) {
        let active = story.tasks.active.as_ref().and_then(|(t, i)| {
            let t = t.borrow();
            let o = t.objectives.get(*i)?.borrow();
            o.has_spot().then(|| (o.target, o.spot_type.clone()))
        });
        story.map_spots.active = active;
        if !story.tasks.changed {
            return;
        }
        // 5.3.
        let list = story.tasks.list.clone();
        for task in &list {
            let objectives = task.borrow().objectives.clone();
            if objectives
                .first()
                .is_none_or(|o| o.borrow().state != IN_PROGRESS)
            {
                continue;
            }
            for (i, objective) in objectives.iter().enumerate() {
                let o = objective.borrow().clone();
                if o.shown_by_default || o.state != IN_PROGRESS || !o.has_spot() {
                    continue;
                }
                // TODO(#1046): gap 2, the simplest reading: objective 0 has
                // no previous objective, so never this spot.
                let Some(previous) = i.checked_sub(1).map(|p| &objectives[p]) else {
                    continue;
                };
                if previous.borrow().state == COMPLETED {
                    self.add_spot(story, o.target, &o.spot_type, &o.spot_hint, true);
                }
            }
        }
        story.tasks.choose_active();
        story.tasks.changed = false;
    }
}

/// A task's id as the task functions get it (6.2): nil for none.
fn task_id_value(task: Option<&TaskRef>) -> Value {
    task.and_then(|t| t.borrow().id.clone())
        .map_or(Value::Nil, Value::String)
}

/// What evaluating an objective in progress gives (5.1).
enum Evaluation {
    Keep,
    Change(i32),
}

impl Request {
    /// As the Lua side reads it: `{op, ...}`.
    fn to_value(&self, objects: &GameObjects) -> Value {
        let n = |v: u32| Value::Number(f64::from(v));
        let items: Vec<Value> = match self {
            Request::Resolve { key, name, log } => vec![
                "resolve".into(),
                n(*key),
                Value::String(name.clone()),
                Value::Boolean(*log),
            ],
            Request::StoryId { key, text } => {
                vec!["story_id".into(), n(*key), Value::String(text.clone())]
            }
            Request::Call { key, task, index } => vec![
                "call".into(),
                n(*key),
                task.clone(),
                Value::Number(f64::from(*index)),
            ],
            Request::CallBare { key } => vec!["call_bare".into(), n(*key)],
            Request::Action { function, holder } => vec![
                "action".into(),
                Value::String(function.clone()),
                objects.handle_value(Some(*holder)),
            ],
        };
        Value::Table(
            items
                .into_iter()
                .enumerate()
                .filter(|(_, v)| *v != Value::Nil)
                .map(|(i, v)| (Value::Number((i + 1) as f64), v))
                .collect(),
        )
    }
}

/// What a state with the story registered keeps for the engine's calls.
pub(crate) struct Registered {
    ctx: Context,
    /// The engine objects standing for tasks and objectives in the
    /// callback `task_state`, by the address of what they hold: one each,
    /// kept as long as the state (our choice: the original hands scripts
    /// the task itself).
    callback_objects: HashMap<usize, (Weak<dyn std::any::Any>, EnginePart)>,
}

impl Scripts {
    /// Registers the story's names (1) over `story`: after the game object
    /// basics (whose objects, services and files it shares), before
    /// [`Scripts::load_globals`].
    pub fn register_story(&mut self, story: Rc<RefCell<Story>>) -> Result<(), Error> {
        let Some(basics) = self.game_object_state.as_ref() else {
            return Err(Error {
                kind: ErrorKind::Api,
                message: "the game object basics aren't registered".into(),
            });
        };
        let ctx = Context {
            story,
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            fatal: self.fatal_flag(),
        };
        members::register(self, &ctx)?;
        self.story_state = Some(Registered {
            ctx,
            callback_objects: HashMap::new(),
        });
        Ok(())
    }

    fn story_context(&self) -> Result<Context, Error> {
        self.story_state
            .as_ref()
            .map(|r| r.ctx.clone())
            .ok_or_else(|| Error {
                kind: ErrorKind::Api,
                message: "the story isn't registered".into(),
            })
    }

    /// The story registered, for the engine's own questions.
    pub(crate) fn registered_story(&self) -> Option<Rc<RefCell<Story>>> {
        self.story_state.as_ref().map(|r| Rc::clone(&r.ctx.story))
    }

    /// Runs what the engine parts asked of the Lua side.
    fn drain_story(&mut self) -> Result<(), Error> {
        self.guarded(|lua| lua.call_kept(STORY_ENTRY, &["drain".into()]).map(drop))
    }

    /// Runs `f` on the story and then its requests; `f`'s error stops the
    /// state when it is fatal, else is the API's.
    pub(crate) fn with_story(
        &mut self,
        f: impl FnOnce(&Context, &mut Story) -> Result<(), String>,
    ) -> Result<(), Error> {
        let ctx = self.story_context()?;
        let result = {
            let mut story = ctx.story.borrow_mut();
            f(&ctx, &mut story)
        };
        if let Err(message) = result {
            let kind = if ctx.fatal.borrow().is_some() {
                ErrorKind::Fatal
            } else {
                ErrorKind::Api
            };
            return Err(Error { kind, message });
        }
        self.drain_story()
    }

    /// Whether the name of task or sequence function `key` resolved (6.1,
    /// 11.2): only those are ever called.
    pub fn story_function_resolved(&mut self, key: FunctionKey) -> Result<bool, Error> {
        let args = ["resolved".into(), Value::Number(f64::from(key))];
        let result = self.guarded(|lua| lua.call_kept(STORY_ENTRY, &args))?;
        Ok(result.first() == Some(&Value::Boolean(true)))
    }

    /// Raises the story's callbacks on the player's object (spec O,
    /// [`Scripts::raise`]), in order, until they raise no more: `task_state`
    /// with the task and the objective as `CGameTask` and
    /// `SGameTaskObjective` engine objects, `map_location_added` and
    /// `inventory_info`. Without the player's object they are dropped. The
    /// story's other events are returned in order, for the engine.
    pub fn raise_story_callbacks(&mut self) -> Result<Vec<Event>, Error> {
        let ctx = self.story_context()?;
        let mut rest = Vec::new();
        loop {
            let events = ctx.story.borrow_mut().take_events();
            if events.is_empty() {
                return Ok(rest);
            }
            for event in events {
                let raised = match &event {
                    Event::TaskState {
                        task,
                        objective,
                        state,
                    } => {
                        let task_part =
                            self.callback_object(task, "CGameTask", members::TaskPart)?;
                        ctx.story.borrow_mut().remember_task(task_part, task);
                        let objective = self.callback_object(
                            objective,
                            "SGameTaskObjective",
                            members::ObjectivePart,
                        )?;
                        ObjectEvent::TaskState {
                            task: task_part,
                            objective,
                            state: f64::from(*state),
                        }
                    }
                    Event::MapLocationAdded { spot_type, object } => {
                        ObjectEvent::MapLocationAdded {
                            location_type: spot_type.clone(),
                            object: *object,
                        }
                    }
                    Event::InfoPortion(info_portions::Event::InventoryInfo { portion }) => {
                        ObjectEvent::InventoryInfo {
                            portion: crate::game_object::game_bytes(portion),
                        }
                    }
                    _ => {
                        rest.push(event);
                        continue;
                    }
                };
                if let Some(player) = ctx.player() {
                    self.raise(player, &raised)?;
                }
            }
        }
    }

    /// The engine object of class `class` holding `make(value)`, made on
    /// first use and kept by the state ([`Registered::callback_objects`]).
    fn callback_object<T: 'static, P: std::any::Any>(
        &mut self,
        value: &Rc<T>,
        class: &str,
        make: impl FnOnce(Rc<T>) -> P,
    ) -> Result<EnginePart, Error> {
        let key = Rc::as_ptr(value) as *const () as usize;
        let objects = self.story_state.as_mut().map(|r| &mut r.callback_objects);
        if let Some(objects) = objects {
            objects.retain(|_, (held, _)| held.strong_count() > 0);
            if let Some((_, part)) = objects.get(&key) {
                return Ok(*part);
            }
        }
        let held = Rc::clone(value);
        let part = self.lua_mut().new_engine_object(class, |_| make(held))?;
        let weak: Weak<dyn std::any::Any> = Rc::downgrade(value) as Weak<dyn std::any::Any>;
        if let Some(r) = self.story_state.as_mut() {
            r.callback_objects.insert(key, (weak, part));
        }
        Ok(part)
    }

    /// The engine gave or disabled portions itself (dialogs, documents,
    /// returning messages): what receiving asks is run now (`IP` 3.3, 3.4).
    pub fn info_portions_changed(&mut self) -> Result<(), Error> {
        self.with_story(|ctx, story| ctx.after_portions(story, Ok(())))
    }

    /// The task part of the player's scheduled update (5.1), in a
    /// single-player game: the objectives evaluated and changed, each
    /// change's functions run before the next evaluation; then the active
    /// spot, the hidden spots and the active task.
    pub fn update_player_tasks(&mut self) -> Result<(), Error> {
        let ctx = self.story_context()?;
        if !ctx.single_player() {
            return Ok(());
        }
        let list = ctx.story.borrow().tasks.list.clone();
        for task in &list {
            let objectives = task.borrow().objectives.clone();
            for (n, objective) in objectives.iter().enumerate() {
                if objective.borrow().state != IN_PROGRESS {
                    if n == 0 {
                        break;
                    }
                    continue;
                }
                if let Evaluation::Change(state) = self.evaluate(&ctx, objective)? {
                    self.with_story(|ctx, story| ctx.change_state(story, task, n, state))?;
                }
            }
        }
        self.with_story(|ctx, story| {
            ctx.after_walk(story);
            Ok(())
        })
    }

    /// Evaluates an objective in progress (5.1).
    fn evaluate(&mut self, ctx: &Context, objective: &ObjectiveRef) -> Result<Evaluation, Error> {
        let o = objective.borrow().clone();
        let own = o.own_task();
        if o.index == 0
            && let Some(task) = &own
        {
            let t = task.borrow();
            if t.time_limit != t.received && ctx.now() > t.time_limit {
                return Ok(Evaluation::Change(FAIL));
            }
        }
        let task_id = task_id_value(own.as_ref());
        let holds_all = |list: List| {
            let portions = &o.portions[list as usize];
            let story = ctx.story.borrow();
            let player = ctx.player();
            !portions.is_empty()
                && portions.iter().all(|id| {
                    matches!((&story.info_portions, player), (Some(p), Some(player))
                        if p.borrow().holds(player, &cp1251::decode(id)))
                })
        };
        if holds_all(List::Fail) {
            return Ok(Evaluation::Change(FAIL));
        }
        if self.conditions(&o.functions[List::Fail as usize], &task_id, o.index)? {
            return Ok(Evaluation::Change(FAIL));
        }
        if holds_all(List::Complete) {
            return Ok(Evaluation::Change(COMPLETED));
        }
        if self.conditions(&o.functions[List::Complete as usize], &task_id, o.index)? {
            return Ok(Evaluation::Change(COMPLETED));
        }
        Ok(Evaluation::Keep)
    }

    /// A list of complete or fail functions (6.2): called in order, the
    /// walk stopping at the first false; an entry that didn't resolve is
    /// skipped, keeping the running result, which starts false.
    fn conditions(
        &mut self,
        keys: &[FunctionKey],
        task: &Value,
        index: i32,
    ) -> Result<bool, Error> {
        let mut result = false;
        for &key in keys {
            let args = [
                "condition".into(),
                Value::Number(f64::from(key)),
                task.clone(),
                Value::Number(f64::from(index)),
            ];
            let outcome = self.guarded(|lua| lua.call_kept(STORY_ENTRY, &args))?;
            match outcome.first() {
                Some(Value::Boolean(true)) => result = true,
                Some(Value::Boolean(false)) => return Ok(false),
                _ => {}
            }
        }
        Ok(result)
    }

    /// The frame's story work: queued news due on the frame clock is added
    /// (7.1, believed), and the previous sequence and an ended one are
    /// discarded (11.1).
    pub fn story_frame(&mut self) -> Result<(), Error> {
        self.with_story(|ctx, story| {
            let now = ctx.services.borrow().device.time_global();
            let (due, waiting) = std::mem::take(&mut story.queued_news)
                .into_iter()
                .partition::<Vec<_>, _>(|(at, _)| *at <= now);
            story.queued_news = waiting;
            for (_, item) in due {
                story.news.push(item.clone());
                story.events.push(Event::NewsAdded(item));
            }
            story.sequences.previous = None;
            if story.sequences.current.as_ref().is_some_and(|s| s.ended) {
                story.sequences.current = None;
            }
            Ok(())
        })
    }

    /// The current sequence's item stopped (11.2, `hud-scope` 18): its stop
    /// functions are called last, then the next item starts with its start
    /// functions, or the sequence has ended.
    pub fn sequence_item_stopped(&mut self) -> Result<(), Error> {
        self.with_story(|_, story| {
            let Some(sequence) = story.sequences.current.as_mut() else {
                return Ok(());
            };
            if sequence.ended {
                return Ok(());
            }
            let stop = sequence.items[sequence.item].stop.clone();
            sequence.item += 1;
            let start = match sequence.items.get(sequence.item) {
                Some(item) => item.start.clone(),
                None => {
                    sequence.ended = true;
                    Vec::new()
                }
            };
            for key in stop.into_iter().chain(start) {
                story.requests.push(Request::CallBare { key });
            }
            Ok(())
        })
    }
}

impl Context {
    /// `game.start_tutorial(name)` (11.1).
    pub fn start_sequence(&self, story: &mut Story, name: &[u8]) -> Result<(), String> {
        if let Some(current) = story.sequences.current.take() {
            story.sequences.previous = Some(current);
        }
        let file = match &story.sequence_file {
            Some(file) => Rc::clone(file),
            None => {
                let file = self.read_xml(SEQUENCE_FILE).map_err(|_| {
                    self.fatal(
                        "CUISequencer::Start",
                        line!(),
                        "error while parsing XML file game_tutorials.xml",
                    )
                })?;
                let file = Rc::new(file);
                story.sequence_file = Some(Rc::clone(&file));
                file
            }
        };
        let decoded = cp1251::decode(name);
        let element = file.root().and_then(|r| r.child(&decoded));
        let mut items = Vec::new();
        for item in element.into_iter().flat_map(|e| e.children_named("item")) {
            let mut keys = |child: &str| -> Vec<FunctionKey> {
                item.children_named(child)
                    .map(|f| {
                        let key = story.new_key();
                        story.requests.push(Request::Resolve {
                            key,
                            name: crate::game_object::game_bytes(f.value().unwrap_or("")),
                            log: false,
                        });
                        key
                    })
                    .collect()
            };
            let start = keys("function_on_start");
            let stop = keys("function_on_stop");
            items.push(SequenceItem { start, stop });
        }
        let Some(first) = items.first() else {
            // Starting the first item reaches through a missing one
            // (believed a crash): fatal here.
            return Err(self.fatal(
                "CUISequencer::Start",
                line!(),
                &format!("the sequence {decoded} has no item"),
            ));
        };
        for &key in &first.start {
            story.requests.push(Request::CallBare { key });
        }
        story.sequences.current = Some(Sequence {
            name: name.to_vec(),
            items,
            item: 0,
            ended: false,
        });
        story.events.push(Event::SequenceStarted {
            name: name.to_vec(),
        });
        Ok(())
    }
}
