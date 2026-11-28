// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Tasks and their objectives (3), the player's tasks (4), objective
//! states (5): the values `CGameTask` and `SGameTaskObjective` hold, a task
//! loaded from the task file (3.4), and what giving a task, a state change
//! and the update do to the story state. Script functions are kept as keys
//! the Lua side resolves and calls ([`super::Request`]).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use volya_engine_formats::xml::{Document, Element};

use super::map_spots::NO_OBJECT;

/// `task.fail` (3.1).
pub const FAIL: i32 = 0;
/// `task.in_progress`.
pub const IN_PROGRESS: i32 = 1;
/// `task.completed`.
pub const COMPLETED: i32 = 2;
/// `task.task_dummy`: `get_task_state`'s "no such task or objective".
pub const TASK_DUMMY: i32 = -1;

/// The `task` constants (3.1).
pub const CONSTANTS: [(&str, i32); 4] = [
    ("fail", FAIL),
    ("in_progress", IN_PROGRESS),
    ("completed", COMPLETED),
    ("task_dummy", TASK_DUMMY),
];

/// The icon of objective 0 (3.3): a texture name and its rectangle (x, y,
/// width, height).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Icon {
    pub texture: Vec<u8>,
    pub rect: [f32; 4],
}

/// A key of a script function kept with an objective: the Lua side holds
/// the function it resolved to, if any (6.1).
pub type FunctionKey = u32;

/// The four lists of an objective (3.3), in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Complete = 0,
    Fail = 1,
    OnComplete = 2,
    OnFail = 3,
}

/// The task file's element names of the four portion lists, by [`List`].
const PORTION_ELEMENTS: [&str; 4] = [
    "infoportion_complete",
    "infoportion_fail",
    "infoportion_set_complete",
    "infoportion_set_fail",
];

/// The task file's element names of the four function lists, by [`List`].
pub const FUNCTION_ELEMENTS: [&str; 4] = [
    "function_complete",
    "function_fail",
    "function_call_complete",
    "function_call_fail",
];

/// An objective (3.3).
#[derive(Clone, Debug)]
pub struct Objective {
    /// The task given to its constructor, or none; a copy keeps it
    /// (TODO(#1046): gap 1).
    pub task: Option<Weak<RefCell<Task>>>,
    /// The stored index: what its functions and `task_state` get, and what
    /// the time-limit test reads (3.3, 5.1).
    pub index: i32,
    pub state: i32,
    pub description: Option<Vec<u8>>,
    pub article: Vec<u8>,
    pub article_key: Vec<u8>,
    pub spot_type: Vec<u8>,
    pub spot_hint: Vec<u8>,
    /// The target object; [`NO_OBJECT`] for none.
    pub target: u16,
    pub icon: Icon,
    /// Shown by default (`def_ml_enabled`).
    pub shown_by_default: bool,
    /// The four portion lists, by [`List`].
    pub portions: [Vec<Vec<u8>>; 4],
    /// The four lists of function names `add_*_func` fills, by [`List`].
    pub names: [Vec<Vec<u8>>; 4],
    /// The four lists of resolved functions, by [`List`].
    pub functions: [Vec<FunctionKey>; 4],
}

impl Objective {
    /// `SGameTaskObjective(t, n)` (3.3): in progress, no target, shown by
    /// default, every text and list empty.
    pub fn new(task: Option<Weak<RefCell<Task>>>, index: i32) -> Self {
        Objective {
            task,
            index,
            state: IN_PROGRESS,
            description: None,
            article: Vec::new(),
            article_key: Vec::new(),
            spot_type: Vec::new(),
            spot_hint: Vec::new(),
            target: NO_OBJECT,
            icon: Icon::default(),
            shown_by_default: true,
            portions: Default::default(),
            names: Default::default(),
            functions: Default::default(),
        }
    }

    /// Its constructor's task, while it exists.
    pub fn own_task(&self) -> Option<TaskRef> {
        self.task.as_ref().and_then(Weak::upgrade)
    }

    /// Whether it has a target and a map spot type (4.1 step 4, 5.3).
    pub fn has_spot(&self) -> bool {
        self.target != NO_OBJECT && !self.spot_type.is_empty()
    }
}

/// An objective as values share it: `get_objective` refers into the task.
pub type ObjectiveRef = Rc<RefCell<Objective>>;

/// A task (3.2).
#[derive(Clone, Debug, Default)]
pub struct Task {
    pub id: Option<Vec<u8>>,
    pub title: Option<Vec<u8>>,
    /// Not set by the constructor (unestablished, 3.2): 0 here.
    pub priority: i32,
    pub objectives: Vec<ObjectiveRef>,
    /// Game times, ms: the three times of `sav` 9.8.
    pub received: u64,
    pub finished: u64,
    pub time_limit: u64,
}

/// A task as values and the player's tasks share it: `give_task` keeps the
/// task itself.
pub type TaskRef = Rc<RefCell<Task>>;

/// What [`load`] asks of the Lua side: the function names to resolve and
/// the story-id texts to look up, each for an objective.
#[derive(Default)]
pub struct Pending {
    /// (key, name) of each function, in order.
    pub functions: Vec<(FunctionKey, Vec<u8>)>,
    /// (objective, story-id text).
    pub story_ids: Vec<(ObjectiveRef, Vec<u8>)>,
}

/// The default icon texture of objective 0 (3.4), compared ignoring case.
const DEFAULT_ICON: &[u8] = b"ui\\ui_icons_task";

fn text_of(e: &Element) -> Vec<u8> {
    crate::game_object::game_bytes(e.value().unwrap_or(""))
}

/// An attribute as `atoi` reads it: leading digits with an optional sign,
/// 0 otherwise.
pub fn int_attribute(e: &Element, name: &str) -> Option<i32> {
    let text = e.attr(name)?.trim_start();
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1i64, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let n = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i64, |n, d| {
            (n * 10 + i64::from(d - b'0')).min(i64::from(u32::MAX))
        });
    Some((sign * n) as i32)
}

fn float_attribute(e: &Element, name: &str) -> f32 {
    e.attr(name)
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(0.0)
}

/// The `game_task` element of `id` in the task file `file`.
pub fn task_element<'a>(file: &'a Document, id: &[u8]) -> Option<&'a Element> {
    let id = volya_engine_formats::cp1251::decode(id);
    file.root()?
        .children_named("game_task")
        .find(|t| t.attr("id") == Some(id.as_str()))
}

/// `t:load(id)` (3.4) from the `game_task` element `element`: the title,
/// the priority and the objectives replaced; `key` gives each function a
/// new key; `icon` resolves a texture id (`set_icon_name`).
pub fn load(
    task: &TaskRef,
    element: &Element,
    key: &mut dyn FnMut() -> FunctionKey,
    icon: &dyn Fn(&[u8]) -> Icon,
) -> Pending {
    let mut pending = Pending::default();
    let weak = Rc::downgrade(task);
    let mut t = task.borrow_mut();
    t.title = Some(element.child("title").map(text_of).unwrap_or_default());
    t.priority = int_attribute(element, "prio").unwrap_or(-1);
    t.objectives.clear();
    for (i, e) in element.children_named("objective").enumerate() {
        let mut o = Objective::new(Some(weak.clone()), i as i32);
        o.description = Some(e.child("text").map(text_of).unwrap_or_default());
        if let Some(article) = e.child("article") {
            o.article = text_of(article);
        }
        o.article_key = crate::game_object::game_bytes(e.attr("key").unwrap_or(""));
        if i == 0 {
            let icon_element = e.child("icon");
            let name = icon_element.map(text_of).unwrap_or_default();
            if !name.is_empty() && !name.eq_ignore_ascii_case(DEFAULT_ICON) {
                o.icon = icon(&name);
            } else {
                let attr = |n| icon_element.map_or(0.0, |e| float_attribute(e, n));
                o.icon = Icon {
                    texture: name,
                    rect: [attr("x"), attr("y"), attr("width"), attr("height")],
                };
            }
        }
        if let Some(spot) = e.child("map_location_type") {
            o.spot_type = text_of(spot);
            o.spot_hint = crate::game_object::game_bytes(spot.attr("hint").unwrap_or(""));
        }
        if e.child("map_location_hidden").is_some() {
            o.shown_by_default = false;
        }
        for (list, name) in PORTION_ELEMENTS.iter().enumerate() {
            o.portions[list] = e.children_named(name).map(text_of).collect();
        }
        for (list, name) in FUNCTION_ELEMENTS.iter().enumerate() {
            for f in e.children_named(name) {
                let k = key();
                o.functions[list].push(k);
                pending.functions.push((k, text_of(f)));
            }
        }
        let story_id = e.child("object_story_id").map(text_of);
        let o = Rc::new(RefCell::new(o));
        if let Some(text) = story_id {
            pending.story_ids.push((Rc::clone(&o), text));
        }
        t.objectives.push(o);
    }
    pending
}

/// `t:add_objective(o)`'s copy of `o` (3.2): its lists of names resolved
/// again under new keys, (key, name) in order.
pub fn copy_objective(
    o: &Objective,
    key: &mut dyn FnMut() -> FunctionKey,
) -> (Objective, Vec<(FunctionKey, Vec<u8>)>) {
    let mut copy = o.clone();
    let mut resolved = Vec::new();
    for list in 0..4 {
        copy.functions[list] = o.names[list]
            .iter()
            .map(|name| {
                let k = key();
                resolved.push((k, name.clone()));
                k
            })
            .collect();
    }
    (copy, resolved)
}

/// The player's tasks (4, 5.4): the simulation's registry of `sav` 9 part
/// 8, the active task and objective and the changed mark.
#[derive(Default)]
pub struct PlayerTasks {
    pub list: Vec<TaskRef>,
    /// The active task and the objective's place in its list.
    pub active: Option<(TaskRef, usize)>,
    /// Marked changed (5.1).
    pub changed: bool,
}

impl PlayerTasks {
    /// The first task with `id` (4.2).
    pub fn find(&self, id: &[u8]) -> Option<TaskRef> {
        self.list
            .iter()
            .find(|t| t.borrow().id.as_deref() == Some(id))
            .cloned()
    }

    /// Sorted by priority, lowest first, compared unsigned (4.1 step 3);
    /// equal priorities keep their order (unestablished; our choice).
    pub fn sort(&mut self) {
        self.list.sort_by_key(|t| t.borrow().priority as u32);
    }

    /// Whether objective `n` of `task` is the active one.
    pub fn is_active(&self, task: &TaskRef, n: usize) -> bool {
        matches!(&self.active, Some((t, i)) if Rc::ptr_eq(t, task) && *i == n)
    }

    /// 4.1 step 5 (also the data path's rule): `task` becomes the active
    /// task with objective 1 when there is none, or the active task's
    /// priority is greater. How they compare is disputed in review: our
    /// choice is unsigned, as step 3 sorts, so an active task of priority
    /// −1 gives way to any.
    pub fn offer_active(&mut self, task: &TaskRef) {
        let replace = match &self.active {
            None => true,
            Some((active, _)) => active.borrow().priority as u32 > task.borrow().priority as u32,
        };
        if replace {
            self.active = Some((Rc::clone(task), 1));
        }
    }

    /// 5.4 after a marked change: with no active objective, the first task
    /// whose objective 0 is in progress and that has an objective from 1 on
    /// in progress gets the first such as the active one. Disputed in
    /// review (the condition); the text's here.
    pub fn choose_active(&mut self) {
        if self.active.is_some() {
            return;
        }
        for task in &self.list {
            let t = task.borrow();
            let in_progress = |o: &ObjectiveRef| o.borrow().state == IN_PROGRESS;
            if !t.objectives.first().is_some_and(in_progress) {
                continue;
            }
            if let Some(i) = (1..t.objectives.len()).find(|&i| in_progress(&t.objectives[i])) {
                self.active = Some((Rc::clone(task), i));
                return;
            }
        }
    }
}
