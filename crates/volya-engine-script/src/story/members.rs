// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The names of 1: the classes `task` (3.1), `CGameTask` (3.2) and
//! `SGameTaskObjective` (3.3) as engine classes, and the members the Lua
//! side (`story.lua`) defines over [`raw`]: those of `game_object` (2, 4,
//! 7, 8), `CGameTask:load` and `add_objective`, and the `level` (9, 10) and
//! `game` (11) functions. The Lua side runs what a member's engine part
//! asks ([`super::Request`]) when it returns.
//!
//! `game_object` members test their object's kind as script API M's do
//! (its 2.2, 2.3): on an object without it they do nothing and give the
//! fallback. `get_info_time` needs spec H's `CTime` (TODO(#1064)), and
//! `give_info_portion_via_pda` is absent (2.4): neither is registered.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::console::{self, DIFFICULTIES};

use super::tasks::{self, CONSTANTS, List, Objective, ObjectiveRef, TASK_DUMMY, Task, TaskRef};
use super::{Context, Event, NewsItem, Request, STORY_ENTRY, Story};
use crate::classes::{EngineClass, ObjectMaker};
use crate::game_object::Kinds;
use crate::game_object::members::Handle;
use crate::loader::Scripts;
use crate::values::{Frect, to_bits32};
use crate::{Error, Value};

/// A `CGameTask`'s part: the task, which `give_task` keeps itself.
#[derive(Clone, Debug)]
pub struct TaskPart(pub TaskRef);

/// An `SGameTaskObjective`'s part: an objective of its own, or one of a
/// task's (`get_objective`).
#[derive(Clone, Debug)]
pub struct ObjectivePart(pub ObjectiveRef);

/// The console command over the game difficulty (10).
const DIFFICULTY: &str = "g_game_difficulty";

type Results = Result<Vec<Value>, String>;

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A text argument (our wording for a mismatch; depends on A).
fn text(args: &[Value], i: usize, what: &str) -> Result<Vec<u8>, String> {
    match arg(args, i) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{what}: text expected")),
    }
}

fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

fn boolean(args: &[Value], i: usize, what: &str) -> Result<bool, String> {
    match arg(args, i) {
        Value::Boolean(b) => Ok(*b),
        _ => Err(format!("{what}: boolean expected")),
    }
}

/// A 32-bit integer argument, as the binding library converts it.
fn integer(args: &[Value], i: usize, what: &str) -> Result<i32, String> {
    number(args, i, what).map(to_bits32)
}

fn text_value(text: Option<&[u8]>) -> Value {
    text.map_or(Value::Nil, |t| Value::String(t.to_vec()))
}

fn one(v: impl Into<Value>) -> Results {
    Ok(vec![v.into()])
}

/// The object id of handle argument `i`.
fn handle(maker: &ObjectMaker, args: &[Value], i: usize, what: &str) -> Result<u16, String> {
    maker
        .get::<Handle>(arg(args, i))
        .map(|h| h.id)
        .ok_or_else(|| format!("{what}: game object expected"))
}

fn task_arg(maker: &ObjectMaker, args: &[Value], i: usize, what: &str) -> Result<TaskRef, String> {
    maker
        .get::<TaskPart>(arg(args, i))
        .map(|t| t.0)
        .ok_or_else(|| format!("{what}: CGameTask expected"))
}

/// Registers the classes, the raw function and the Lua side.
pub(crate) fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    scripts.register_class(
        CONSTANTS
            .into_iter()
            .fold(EngineClass::new("task"), |c, (name, value)| {
                c.constant(name, value)
            }),
    )?;
    scripts.register_class(game_task_class(ctx))?;
    scripts.register_class(objective_class(ctx))?;
    let c = ctx.clone();
    let lua = scripts.lua_mut();
    lua.register_with_objects("__volya_story", move |maker, args| raw(&c, maker, args))?;
    lua.register_kept_factory("__volya_kept")?;
    lua.exec_kept("=story", include_bytes!("../story.lua"), STORY_ENTRY)
}

/// `CGameTask` (3.2): `load` and `add_objective` are the Lua side's.
fn game_task_class(ctx: &Context) -> EngineClass {
    let story = Rc::clone(&ctx.story);
    let crash = ctx.clone();
    EngineClass::new("CGameTask")
        .constructor(
            0,
            |_| Some(0),
            move |part, _| {
                let task = Rc::new(RefCell::new(Task::default()));
                story.borrow_mut().remember_task(part, &task);
                Ok(TaskPart(task))
            },
        )
        .constructible()
        .method("get_id", |t: &mut TaskPart, _| {
            one(text_value(t.0.borrow().id.as_deref()))
        })
        .method("set_id", |t: &mut TaskPart, args| {
            t.0.borrow_mut().id = Some(text(args, 0, "set_id")?);
            Ok(Vec::new())
        })
        .method("get_title", |t: &mut TaskPart, _| {
            one(text_value(t.0.borrow().title.as_deref()))
        })
        .method("set_title", |t: &mut TaskPart, args| {
            t.0.borrow_mut().title = Some(text(args, 0, "set_title")?);
            Ok(Vec::new())
        })
        .method("get_priority", |t: &mut TaskPart, _| {
            one(f64::from(t.0.borrow().priority))
        })
        .method("set_priority", |t: &mut TaskPart, args| {
            t.0.borrow_mut().priority = integer(args, 0, "set_priority")?;
            Ok(Vec::new())
        })
        .method("get_objectives_cnt", |t: &mut TaskPart, _| {
            one(t.0.borrow().objectives.len() as f64)
        })
        .method_with_objects("get_objective", move |t: &mut TaskPart, o, args| {
            let n = integer(args, 0, "get_objective")?;
            let objective = usize::try_from(n)
                .ok()
                .and_then(|n| t.0.borrow().objectives.get(n).cloned());
            // Outside the list is believed undefined: fatal here.
            let Some(objective) = objective else {
                return Err(crash.fatal(
                    "CGameTask::GetObjective",
                    line!(),
                    &format!("objective {n} is outside the task's objectives"),
                ));
            };
            Ok(vec![o.new_object(
                "SGameTaskObjective",
                ObjectivePart(objective),
            )])
        })
}

/// `SGameTaskObjective` (3.3).
fn objective_class(ctx: &Context) -> EngineClass {
    let story = Rc::clone(&ctx.story);
    let icons = Rc::clone(&ctx.story);
    let mut class = EngineClass::new("SGameTaskObjective")
        .constructor(
            2,
            |_| Some(0),
            move |_, args| {
                let task = match arg(args, 0) {
                    Value::Nil => None,
                    Value::Object(part) => story.borrow().task_of_part(*part),
                    _ => return Err("SGameTaskObjective: CGameTask or nil expected".into()),
                };
                let index = integer(args, 1, "SGameTaskObjective")?;
                Ok(ObjectivePart(Rc::new(RefCell::new(Objective::new(
                    task.as_ref().map(Rc::downgrade),
                    index,
                )))))
            },
        )
        .constructible();
    for (name, list) in [
        ("add_complete_info", List::Complete),
        ("add_fail_info", List::Fail),
        ("add_on_complete_info", List::OnComplete),
        ("add_on_fail_info", List::OnFail),
    ] {
        class = class.method(name, move |o: &mut ObjectivePart, args| {
            o.0.borrow_mut().portions[list as usize].push(text(args, 0, name)?);
            Ok(Vec::new())
        });
    }
    for (name, list) in [
        ("add_complete_func", List::Complete),
        ("add_fail_func", List::Fail),
        ("add_on_complete_func", List::OnComplete),
        ("add_on_fail_func", List::OnFail),
    ] {
        class = class.method(name, move |o: &mut ObjectivePart, args| {
            o.0.borrow_mut().names[list as usize].push(text(args, 0, name)?);
            Ok(Vec::new())
        });
    }
    type Setter = fn(&mut Objective, Vec<u8>);
    let setters: [(&str, Setter); 5] = [
        ("set_description", |o, t| o.description = Some(t)),
        ("set_article_id", |o, t| o.article = t),
        ("set_article_key", |o, t| o.article_key = t),
        ("set_map_location", |o, t| o.spot_type = t),
        ("set_map_hint", |o, t| o.spot_hint = t),
    ];
    for (name, set) in setters {
        class = class.method(name, move |o: &mut ObjectivePart, args| {
            set(&mut o.0.borrow_mut(), text(args, 0, name)?);
            Ok(Vec::new())
        });
    }
    class
        .method("set_object_id", |o: &mut ObjectivePart, args| {
            o.0.borrow_mut().target = integer(args, 0, "set_object_id")? as u16;
            Ok(Vec::new())
        })
        .method("set_icon_name", move |o: &mut ObjectivePart, args| {
            let name = text(args, 0, "set_icon_name")?;
            let icon = icons.borrow().icon(&name);
            o.0.borrow_mut().icon = icon;
            Ok(Vec::new())
        })
        .method("get_description", |o: &mut ObjectivePart, _| {
            one(text_value(o.0.borrow().description.as_deref()))
        })
        .method("get_state", |o: &mut ObjectivePart, _| {
            one(f64::from(o.0.borrow().state))
        })
        .method("get_idx", |o: &mut ObjectivePart, _| {
            one(f64::from(o.0.borrow().index))
        })
        .writable_property(
            "def_ml_enabled",
            |o: &ObjectivePart| Value::Boolean(o.0.borrow().shown_by_default),
            |o: &mut ObjectivePart, v| {
                o.0.borrow_mut().shown_by_default =
                    !matches!(v, Value::Nil | Value::Boolean(false));
                Ok(())
            },
        )
}

/// The engine part of the Lua side's members and its own operations:
/// `args[0]` names it.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("story: operation expected".into()),
    };
    let args = args.get(1..).unwrap_or(&[]);
    match op.as_str() {
        // Fatal as D 1.2: the state stops with the binding library's error.
        "fatal" => {
            let message = format!(
                "LUA error: {}",
                String::from_utf8_lossy(&text(args, 0, "fatal")?)
            );
            *ctx.fatal.borrow_mut() = Some(message.clone());
            return one(message.as_str());
        }
        _ => ctx.stopped()?,
    }
    let story = &mut *ctx.story.borrow_mut();
    match op.as_str() {
        "take" => {
            let requests = std::mem::take(&mut story.requests);
            let objects = ctx.objects.borrow();
            one(Value::Table(
                requests
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (Value::Number((i + 1) as f64), r.to_value(&objects)))
                    .collect(),
            ))
        }
        "log" => {
            let line = String::from_utf8_lossy(&text(args, 0, "log")?).into_owned();
            ctx.services.borrow_mut().console.log_mut().write(&line);
            Ok(Vec::new())
        }
        "story_id" => {
            let key = number(args, 0, "story_id")? as u32;
            let id = integer(args, 1, "story_id")?;
            ctx.answer_story_id(story, key, id);
            Ok(Vec::new())
        }
        _ => member(ctx, story, maker, &op, args),
    }
}

/// Whether object `id` is a holder (an inventory owner, 2.1).
fn is_holder(ctx: &Context, id: u16) -> bool {
    ctx.objects
        .borrow()
        .get(id)
        .is_some_and(|o| o.kinds.has(Kinds::INVENTORY_OWNER))
}

fn holds(story: &Story, holder: u16, id: &[u8]) -> bool {
    story
        .info_portions
        .as_ref()
        .is_some_and(|p| p.holds(holder, &volya_engine_formats::cp1251::decode(id)))
}

fn member(
    ctx: &Context,
    story: &mut Story,
    maker: &ObjectMaker,
    op: &str,
    args: &[Value],
) -> Results {
    let name = op;
    match op {
        // 2.1.
        "give_info_portion" | "disable_info_portion" => {
            let holder = handle(maker, args, 0, name)?;
            let id = text(args, 1, name)?;
            if !is_holder(ctx, holder) {
                return one(false);
            }
            if op == "give_info_portion" {
                ctx.give_portion(story, holder, &id)?;
            } else {
                ctx.disable_portion(story, holder, &id)?;
            }
            one(true)
        }
        "has_info" | "dont_has_info" => {
            let holder = handle(maker, args, 0, name)?;
            let id = text(args, 1, name)?;
            let held = is_holder(ctx, holder) && holds(story, holder, &id);
            one(if op == "has_info" { held } else { !held })
        }
        // 2.3: their implementations return at once.
        "info_add" | "info_clear" => {
            handle(maker, args, 0, name)?;
            Ok(Vec::new())
        }
        // 3.2.
        "load" => {
            let task = task_arg(maker, args, 0, name)?;
            let id = text(args, 1, name)?;
            ctx.load_task(story, &task, &id)?;
            Ok(Vec::new())
        }
        "add_objective" => {
            let task = task_arg(maker, args, 0, name)?;
            let Some(objective) = maker.get::<ObjectivePart>(arg(args, 1)) else {
                return Err("add_objective: SGameTaskObjective expected".into());
            };
            let mut next = story.next_key;
            let (copy, resolved) = tasks::copy_objective(&objective.0.borrow(), &mut || {
                next += 1;
                next
            });
            story.next_key = next;
            for (key, name) in resolved {
                story.requests.push(Request::Resolve {
                    key,
                    name,
                    log: true,
                });
            }
            task.borrow_mut()
                .objectives
                .push(Rc::new(RefCell::new(copy)));
            Ok(Vec::new())
        }
        // 4.
        "give_task" => {
            handle(maker, args, 0, name)?;
            let task = task_arg(maker, args, 1, name)?;
            let time = integer(args, 2, name)? as u32;
            let check = boolean(args, 3, name)?;
            ctx.give_task(story, &task, time, check)?;
            Ok(Vec::new())
        }
        "get_task_state" => {
            handle(maker, args, 0, name)?;
            let id = text(args, 1, name)?;
            let n = integer(args, 2, name)? as u32;
            ctx.player_or_crash(name)?;
            let Some(task) = story.tasks.find(&id) else {
                return one(f64::from(TASK_DUMMY));
            };
            let t = task.borrow();
            // "wrong objective num" is swallowed (M 2.3).
            let state = t
                .objectives
                .get(n as usize)
                .map_or(TASK_DUMMY, |o| o.borrow().state);
            one(f64::from(state))
        }
        "set_task_state" => {
            handle(maker, args, 0, name)?;
            let state = integer(args, 1, name)?;
            let id = text(args, 2, name)?;
            let n = integer(args, 3, name)? as u32;
            ctx.player_or_crash(name)?;
            let Some(task) = story.tasks.find(&id) else {
                let line = format!(
                    "actor does not has task [{}]",
                    volya_engine_formats::cp1251::decode(&id)
                );
                ctx.services.borrow_mut().console.log_mut().write(&line);
                return Ok(Vec::new());
            };
            ctx.change_state(story, &task, n as usize, state)?;
            Ok(Vec::new())
        }
        // 7.
        "give_game_news" => {
            handle(maker, args, 0, name)?;
            let text_ = text(args, 1, name)?;
            let texture = text(args, 2, name)?;
            let rect = frect(maker, args, 3, name)?;
            let delay = integer(args, 4, name)? as u32;
            let show = integer(args, 5, name)? as u32;
            let item = NewsItem {
                text: text_,
                texture,
                rect,
                show_time: if show == 0 { 5_000 } else { show },
            };
            if delay == 0 {
                story.news.push(item.clone());
                story.events.push(Event::NewsAdded(item));
            } else {
                let now = ctx.services.borrow().device.time_global();
                story.queued_news.push((now.wrapping_add(delay), item));
            }
            Ok(Vec::new())
        }
        "give_talk_message" => {
            handle(maker, args, 0, name)?;
            let text_ = text(args, 1, name)?;
            let texture = text(args, 2, name)?;
            let rect = frect(maker, args, 3, name)?;
            let template = match arg(args, 4) {
                Value::Nil => Vec::new(),
                _ => text(args, 4, name)?,
            };
            if story.talk_window_shown {
                story.events.push(Event::TalkMessage {
                    text: text_,
                    texture,
                    rect,
                    template: if template.is_empty() {
                        b"iconed_answer_item".to_vec()
                    } else {
                        template
                    },
                });
            }
            Ok(Vec::new())
        }
        // 8.
        "set_tip_text" | "set_tip_text_default" => {
            let id = handle(maker, args, 0, name)?;
            let tip = if op == "set_tip_text" {
                text(args, 1, name)?
            } else {
                Vec::new()
            };
            // On a handle with no object: nothing (the diagnostic swallowed).
            if ctx.objects.borrow().get(id).is_none() {
                return Ok(Vec::new());
            }
            if tip.is_empty() {
                story.tip_texts.remove(&id);
            } else {
                story.tip_texts.insert(id, tip);
            }
            Ok(Vec::new())
        }
        // 9.
        "map_add_object_spot" | "map_add_object_spot_ser" => {
            let object = integer(args, 0, name)? as u16;
            let spot_type = text(args, 1, name)?;
            let hint = text(args, 2, name)?;
            ctx.add_spot(story, object, &spot_type, &hint, op.ends_with("_ser"));
            Ok(Vec::new())
        }
        "map_remove_object_spot" => {
            let object = integer(args, 0, name)? as u16;
            story.map_spots.remove(object, &text(args, 1, name)?);
            Ok(Vec::new())
        }
        "map_has_object_spot" => {
            let object = integer(args, 0, name)? as u16;
            one(f64::from(
                story.map_spots.count(object, &text(args, 1, name)?),
            ))
        }
        "map_change_spot_hint" => {
            let object = integer(args, 0, name)? as u16;
            let spot_type = text(args, 1, name)?;
            story
                .map_spots
                .change_hint(object, &spot_type, &text(args, 2, name)?);
            Ok(Vec::new())
        }
        // 10.
        "get_game_difficulty" => {
            let value = match ctx.services.borrow().console.value(DIFFICULTY) {
                Some(console::Value::List(n)) => n,
                _ => 0,
            };
            one(value as f64)
        }
        "set_game_difficulty" => {
            let value = integer(args, 0, name)?;
            ctx.services
                .borrow_mut()
                .console
                .set_list_number(DIFFICULTY, i64::from(value));
            let difficulty = DIFFICULTIES
                .iter()
                .find(|(_, n)| *n == i64::from(value))
                .map_or("", |(name, _)| name);
            story.events.push(Event::Difficulty {
                value,
                name: difficulty,
            });
            Ok(Vec::new())
        }
        // 11.
        "start_tutorial" => {
            let sequence = text(args, 0, name)?;
            ctx.start_sequence(story, &sequence)?;
            Ok(Vec::new())
        }
        "has_active_tutorial" => one(story.sequences.current.is_some()),
        _ => Err(format!("story: unknown operation {op}")),
    }
}

/// A `Frect` argument as x1, y1, x2, y2.
fn frect(maker: &ObjectMaker, args: &[Value], i: usize, what: &str) -> Result<[f32; 4], String> {
    maker
        .get::<Frect>(arg(args, i))
        .map(|r| [r.x1, r.y1, r.x2, r.y2])
        .ok_or_else(|| format!("{what}: Frect expected"))
}
