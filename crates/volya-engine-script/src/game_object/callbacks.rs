// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Object event callbacks (script API O,
//! docs/behavior/script-api-object-event-callbacks.md; section numbers are
//! its): the class `callback` and its 34 event ids (2), the `game_object`
//! members `set_callback` (3), `set_fastcall` (5), `set_enemy_callback`
//! (7) and `set_patrol_extrapolate_callback` (8), and the `level`
//! functions `add_call`, `remove_call` and `remove_calls_for_object` (6).
//! `set_use_callback` is not registered (9).
//!
//! The tables and the level's call list live in Lua (callbacks.lua), which
//! holds the scripts' functions. The engine raises the events of 4 with
//! [`Scripts::raise`] ([`ObjectEvent`]), queues and delivers sounds with
//! [`Scripts::perceive_sound`] and [`Scripts::deliver_sounds`], updates the
//! call list once per level frame with [`Scripts::update_level_calls`] and
//! asks the fighters' callbacks with [`Scripts::enemy_callback`] and
//! [`Scripts::patrol_extrapolate_callback`]. A failing script function is
//! fatal (D 1.2): its message is logged (unless it is `cannot resume dead
//! coroutine`) and the game stops with `LUA error: <message>`.

use std::cell::RefCell;

use super::{Context, GameObjects, Kinds};
use crate::classes::{EngineClass, EnginePart, NOT_LOGGED};
use crate::loader::Scripts;
use crate::services::EngineServices;
use crate::{Error, ErrorKind, Value, call_kept_within};

/// The registry key of the entry function (callbacks.lua).
const ENTRY: &str = "volya.object_callbacks";

/// The 34 constants of `callback` (2).
pub const CALLBACK_IDS: [(&str, i32); 34] = [
    ("trade_start", 0),
    ("trade_stop", 1),
    ("trade_sell_buy_item", 2),
    ("trade_perform_operation", 3),
    ("zone_enter", 4),
    ("zone_exit", 5),
    ("level_border_exit", 6),
    ("level_border_enter", 7),
    ("death", 8),
    ("patrol_path_in_point", 9),
    ("inventory_pda", 10),
    ("inventory_info", 11),
    ("article_info", 12),
    ("task_state", 13),
    ("map_location_added", 14),
    ("use_object", 15),
    ("hit", 16),
    ("sound", 17),
    ("action_movement", 18),
    ("action_watch", 19),
    ("action_animation", 20),
    ("action_sound", 21),
    ("action_particle", 22),
    ("action_object", 23),
    ("actor_sleep", 24),
    ("helicopter_on_point", 25),
    ("helicopter_on_hit", 26),
    ("on_item_take", 27),
    ("on_item_drop", 28),
    ("script_animation", 29),
    ("trader_global_anim_request", 30),
    ("trader_head_anim_request", 31),
    ("trader_sound_end", 32),
    ("take_item_from_box", 33),
];

/// The part of a scripted action that became done (4, ids 19 … 23): its
/// number is the event's second argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionPart {
    Watch = 1,
    Animation = 2,
    Sound = 3,
    Particle = 4,
    Object = 5,
}

/// An event the engine raises on a receiver (4), with the arguments the
/// receiver's handle doesn't give. Objects are ids (passed as their
/// handles, nil for none or one without a handle); meanings the spec
/// believes are named so, the others are numbered. Which receivers get
/// which events, and when, is the caller's (the table of 4).
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectEvent {
    /// 0, on each partner of a trade that is a trader.
    TradeStart,
    /// 1.
    TradeStop,
    /// 2, on the player's object: the item, sold or bought, the money
    /// (believed).
    TradeSellBuyItem {
        item: Option<u16>,
        sold: bool,
        money: f64,
    },
    /// 3, on a trade partner that is a trader (meanings unestablished).
    TradePerformOperation(f64, f64),
    /// 4, on a script zone: the zone's handle, then the object's.
    ZoneEnter { object: Option<u16> },
    /// 5.
    ZoneExit { object: Option<u16> },
    /// 6, on the player's object: its handle.
    LevelBorderExit,
    /// 7.
    LevelBorderEnter,
    /// 8, on a living object: its handle, the killer's.
    Death { killer: Option<u16> },
    /// 8, on a breakable physics object a hit destroys: its handle, the
    /// last hitter as a `CGameObject` value (not a handle).
    BreakableDeath { last_hitter: Option<EnginePart> },
    /// 9, on a creature reaching a point of its patrol path: its handle, 0,
    /// the point's index.
    PatrolPathInPoint { point: u32 },
    /// 11, on the player's object: its handle (believed), the portion's id.
    InventoryInfo { portion: Vec<u8> },
    /// 12, on the player's object: its handle, the group, the name and the
    /// article type (believed).
    ArticleInfo {
        group: Vec<u8>,
        name: Vec<u8>,
        article_type: f64,
    },
    /// 13, on the player's object: the `CGameTask`, the
    /// `SGameTaskObjective`, the new state.
    TaskState {
        task: EnginePart,
        objective: EnginePart,
        state: f64,
    },
    /// 14, on the player's object: the location's type, the object's id.
    MapLocationAdded { location_type: Vec<u8>, object: u16 },
    /// 15, on the object being used: its handle, the user's.
    UseObject { user: Option<u16> },
    /// 15, on the player's object, an item of its inventory used up on it:
    /// the item (believed).
    UseItem { item: Option<u16> },
    /// 16: its handle (believed), the power, the direction (believed), the
    /// hitter's handle, the bone (believed).
    Hit {
        power: f64,
        direction: [f32; 3],
        hitter: Option<u16>,
        bone: f64,
    },
    /// 18, on an object running scripted actions: a handle and two numbers
    /// (meanings unestablished).
    ActionMovement {
        object: Option<u16>,
        first: f64,
        second: f64,
    },
    /// 19 … 23: a handle, the part's number.
    ActionDone {
        object: Option<u16>,
        part: ActionPart,
    },
    /// 25, on a helicopter: the distance, the position, the path point's
    /// index (believed).
    HelicopterOnPoint {
        distance: f64,
        position: [f32; 3],
        point: f64,
    },
    /// 26, on a helicopter: four numbers (believed power, impulse, hit type
    /// and the hitter's id).
    HelicopterOnHit {
        power: f64,
        impulse: f64,
        hit_type: f64,
        hitter: f64,
    },
    /// 27, on an inventory owner: the item.
    OnItemTake { item: Option<u16> },
    /// 28.
    OnItemDrop { item: Option<u16> },
    /// 29, on a `CAI_Stalker`.
    ScriptAnimation,
    /// 30, on a `CAI_Trader`.
    TraderGlobalAnimRequest,
    /// 31.
    TraderHeadAnimRequest,
    /// 32.
    TraderSoundEnd,
    /// 33, on the player's object: the box, the item.
    TakeItemFromBox {
        inventory_box: u16,
        item: Option<u16>,
    },
}

/// A vector argument as it crosses to callbacks.lua.
fn vector(v: [f32; 3]) -> Value {
    Value::Table(
        v.iter()
            .enumerate()
            .map(|(i, c)| (Value::Number(i as f64 + 1.0), Value::Number(f64::from(*c))))
            .collect(),
    )
}

fn object(part: Option<EnginePart>) -> Value {
    part.map_or(Value::Nil, Value::Object)
}

impl ObjectEvent {
    /// Its event id (2).
    pub fn id(&self) -> i32 {
        use ObjectEvent::*;
        match self {
            TradeStart => 0,
            TradeStop => 1,
            TradeSellBuyItem { .. } => 2,
            TradePerformOperation(..) => 3,
            ZoneEnter { .. } => 4,
            ZoneExit { .. } => 5,
            LevelBorderExit => 6,
            LevelBorderEnter => 7,
            Death { .. } | BreakableDeath { .. } => 8,
            PatrolPathInPoint { .. } => 9,
            InventoryInfo { .. } => 11,
            ArticleInfo { .. } => 12,
            TaskState { .. } => 13,
            MapLocationAdded { .. } => 14,
            UseObject { .. } | UseItem { .. } => 15,
            Hit { .. } => 16,
            ActionMovement { .. } => 18,
            ActionDone { part, .. } => 18 + *part as i32,
            HelicopterOnPoint { .. } => 25,
            HelicopterOnHit { .. } => 26,
            OnItemTake { .. } => 27,
            OnItemDrop { .. } => 28,
            ScriptAnimation => 29,
            TraderGlobalAnimRequest => 30,
            TraderHeadAnimRequest => 31,
            TraderSoundEnd => 32,
            TakeItemFromBox { .. } => 33,
        }
    }

    /// Its arguments (4), `own` the receiver's handle.
    fn arguments(&self, own: Value, objects: &GameObjects) -> Vec<Value> {
        use ObjectEvent::*;
        let h = |id: &Option<u16>| objects.handle_value(*id);
        let n = Value::Number;
        let text = |t: &[u8]| Value::String(t.to_vec());
        match self {
            TradeStart
            | TradeStop
            | ScriptAnimation
            | TraderGlobalAnimRequest
            | TraderHeadAnimRequest
            | TraderSoundEnd => Vec::new(),
            TradeSellBuyItem { item, sold, money } => {
                vec![h(item), Value::Boolean(*sold), n(*money)]
            }
            TradePerformOperation(a, b) => vec![n(*a), n(*b)],
            ZoneEnter { object } | ZoneExit { object } => vec![own, h(object)],
            LevelBorderExit | LevelBorderEnter => vec![own],
            Death { killer } => vec![own, h(killer)],
            BreakableDeath { last_hitter } => vec![own, object(*last_hitter)],
            PatrolPathInPoint { point } => vec![own, n(0.0), n(f64::from(*point))],
            InventoryInfo { portion } => vec![own, text(portion)],
            ArticleInfo {
                group,
                name,
                article_type,
            } => vec![own, text(group), text(name), n(*article_type)],
            TaskState {
                task,
                objective,
                state,
            } => vec![Value::Object(*task), Value::Object(*objective), n(*state)],
            MapLocationAdded {
                location_type,
                object,
            } => vec![text(location_type), n(f64::from(*object))],
            UseObject { user } => vec![own, h(user)],
            UseItem { item } | OnItemTake { item } | OnItemDrop { item } => vec![h(item)],
            Hit {
                power,
                direction,
                hitter,
                bone,
            } => vec![own, n(*power), vector(*direction), h(hitter), n(*bone)],
            ActionMovement {
                object,
                first,
                second,
            } => vec![h(object), n(*first), n(*second)],
            ActionDone { object, part } => vec![h(object), n(f64::from(*part as i32))],
            HelicopterOnPoint {
                distance,
                position,
                point,
            } => vec![n(*distance), vector(*position), n(*point)],
            HelicopterOnHit {
                power,
                impulse,
                hit_type,
                hitter,
            } => vec![n(*power), n(*impulse), n(*hit_type), n(*hitter)],
            TakeItemFromBox {
                inventory_box,
                item,
            } => vec![h(&Some(*inventory_box)), h(item)],
        }
    }
}

/// A sound an object perceives (4, id 17): its source, the type, the
/// position and the power (believed).
#[derive(Clone, Debug, PartialEq)]
pub struct PerceivedSound {
    pub source: Option<u16>,
    pub sound_type: f64,
    pub position: [f32; 3],
    pub power: f64,
}

/// The `raise` call's arguments: event `id` on `receiver` with `args`.
fn raise_args(receiver: u16, id: i32, args: Vec<Value>) -> Vec<Value> {
    let mut out = vec![
        Value::from("raise"),
        Value::Number(f64::from(receiver)),
        Value::Number(f64::from(id)),
    ];
    out.extend(args);
    out
}

/// The receiver's handle and the event's arguments, or `None` when the
/// receiver doesn't exist (nothing is raised).
fn event_args(objects: &GameObjects, receiver: u16, event: &ObjectEvent) -> Option<Vec<Value>> {
    let own = objects
        .get(receiver)?
        .handle()
        .map_or(Value::Nil, Value::Object);
    Some(raise_args(
        receiver,
        event.id(),
        event.arguments(own, objects),
    ))
}

/// A script function the engine called failed (D 1.2, A 5.3): the message
/// is logged unless it is [`NOT_LOGGED`], and the game stops with `LUA
/// error: <message>`. The error to return.
fn failed(
    services: &RefCell<EngineServices>,
    fatal: &RefCell<Option<String>>,
    message: &Value,
) -> Error {
    let text = match message {
        Value::String(s) => String::from_utf8_lossy(s).into_owned(),
        v => format!("{v:?}"),
    };
    if text != NOT_LOGGED {
        services.borrow_mut().console.log_mut().write(&text);
    }
    let message = format!("LUA error: {text}");
    *fatal.borrow_mut() = Some(message.clone());
    Error {
        kind: ErrorKind::Fatal,
        message,
    }
}

/// What a protected call in callbacks.lua gave: its results, or the fatal
/// error of a failure.
fn outcome(
    services: &RefCell<EngineServices>,
    fatal: &RefCell<Option<String>>,
    results: Vec<Value>,
) -> Result<Vec<Value>, Error> {
    match results.as_slice() {
        [Value::String(o), message, ..] if o == b"error" => Err(failed(services, fatal, message)),
        _ => Ok(results),
    }
}

/// Registers the class `callback`, the members on `game_object` and the
/// `level` functions (1.2): after the class `game_object`.
pub(super) fn register(s: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    let class = CALLBACK_IDS
        .iter()
        .fold(EngineClass::new("callback"), |class, (name, value)| {
            class.constant(name, *value)
        });
    s.register_class(class)?;
    let objects = ctx.objects.clone();
    let lua = s.lua_mut();
    lua.register_with_objects("__volya_callback_object", move |_, args| {
        let objects = objects.borrow();
        let id = match args.first() {
            Some(Value::Object(part)) => objects.of_handle(*part),
            _ => None,
        };
        // Our wording: what a released or foreign handle gives is M's
        // choice (members on it raise).
        let Some(object) = id.and_then(|id| objects.get(id)) else {
            return Err("game object expected".into());
        };
        Ok(vec![
            Value::Number(f64::from(object.id)),
            Value::Boolean(object.kinds.has(Kinds::FIGHTER)),
        ])
    })?;
    lua.exec_kept("=object_callbacks", include_bytes!("callbacks.lua"), ENTRY)
}

/// Raises `event` on `receiver` from inside a member (the engine's own
/// `CGameObject:use`, M 10.1): the callback runs in the middle of it.
pub(super) fn raise_within(
    ctx: &Context,
    receiver: u16,
    event: &ObjectEvent,
) -> Result<(), String> {
    ctx.stopped()?;
    raise_inside(&ctx.objects, &ctx.services, &ctx.fatal, receiver, event)
}

/// Raises `event` on `receiver` from inside a member over the game
/// objects `objects` (not borrowed), as [`raise_within`].
pub(crate) fn raise_inside(
    objects: &RefCell<GameObjects>,
    services: &RefCell<EngineServices>,
    fatal: &RefCell<Option<String>>,
    receiver: u16,
    event: &ObjectEvent,
) -> Result<(), String> {
    let Some(args) = event_args(&objects.borrow(), receiver, event) else {
        return Ok(());
    };
    let results = call_kept_within(ENTRY, &args).map_err(|e| e.message)?;
    outcome(services, fatal, results)
        .map(drop)
        .map_err(|e| e.message)
}

impl Scripts {
    /// Calls the entry with `args`: its results, or the fatal error of a
    /// script function that failed.
    fn callbacks(&mut self, args: &[Value]) -> Result<Vec<Value>, Error> {
        let services = std::rc::Rc::clone(&self.registered()?.services);
        let fatal = self.fatal_flag();
        let results = self.guarded(|lua| lua.call_kept(ENTRY, args))?;
        outcome(&services, &fatal, results)
    }

    /// The engine raises `event` on `receiver` (3.2, 4): the function of the
    /// receiver's entry for the event's id is called, after its bound value
    /// if one is stored, with the event's arguments; nothing without one,
    /// or when there is no such object. Several events of one occurrence
    /// are raised in the caller's order (TODO(#1018): gap 3).
    pub fn raise(&mut self, receiver: u16, event: &ObjectEvent) -> Result<(), Error> {
        let args = {
            let objects = self.game_objects()?;
            let objects = objects.borrow();
            event_args(&objects, receiver, event)
        };
        match args {
            Some(args) => self.callbacks(&args).map(drop),
            None => Ok(()),
        }
    }

    /// Object `receiver` perceives `sound` (4, id 17): queued when it has a
    /// source and the receiver's entry 17 has a function now. Whether it
    /// was queued.
    pub fn perceive_sound(&mut self, receiver: u16, sound: &PerceivedSound) -> Result<bool, Error> {
        if sound.source.is_none() {
            return Ok(false);
        }
        let args = {
            let objects = self.game_objects()?;
            let objects = objects.borrow();
            let Some(object) = objects.get(receiver) else {
                return Ok(false);
            };
            vec![
                Value::from("perceive"),
                Value::Number(f64::from(receiver)),
                object.handle().map_or(Value::Nil, Value::Object),
                objects.handle_value(sound.source),
                Value::Number(sound.sound_type),
                vector(sound.position),
                Value::Number(sound.power),
            ]
        };
        let results = self.callbacks(&args)?;
        Ok(results.first() == Some(&Value::Boolean(true)))
    }

    /// Whether object `object`'s entry `id` has a function now (3.1).
    pub fn has_callback(&mut self, object: u16, id: i32) -> Result<bool, Error> {
        let args = [
            "has".into(),
            Value::Number(f64::from(object)),
            Value::Number(f64::from(id)),
        ];
        let results = self.callbacks(&args)?;
        Ok(results.first() == Some(&Value::Boolean(true)))
    }

    /// Object `receiver`'s update delivers its queued sounds (4, id 17), in
    /// order, and empties the queue.
    pub fn deliver_sounds(&mut self, receiver: u16) -> Result<(), Error> {
        self.callbacks(&["deliver".into(), Value::Number(f64::from(receiver))])
            .map(drop)
    }

    /// The level's per-frame update of the call list (6.1): after the
    /// physics steps and the objects' updates.
    pub fn update_level_calls(&mut self) -> Result<(), Error> {
        self.callbacks(&["update".into()]).map(drop)
    }

    /// The level removes all of its objects or is destroyed (6.5): the call
    /// list's entries are dropped, none called.
    pub fn clear_level_calls(&mut self) -> Result<(), Error> {
        self.callbacks(&["clear".into()]).map(drop)
    }

    /// The entries of the call list, fast calls included (6.1).
    pub fn level_calls(&mut self) -> Result<usize, Error> {
        let results = self.callbacks(&["count".into()])?;
        Ok(results.first().and_then(Value::as_number).unwrap_or(0.0) as usize)
    }

    /// The enemy callback of fighter `fighter` on `candidate` (7), asked once
    /// the engine's own tests accept the candidate: whether it is an enemy.
    /// True without a callback.
    pub fn enemy_callback(&mut self, fighter: u16, candidate: u16) -> Result<bool, Error> {
        let args = {
            let objects = self.game_objects()?;
            let objects = objects.borrow();
            vec![
                Value::from("enemy"),
                Value::Number(f64::from(fighter)),
                objects.handle_value(Some(fighter)),
                objects.handle_value(Some(candidate)),
            ]
        };
        self.ask(&args)
    }

    /// The patrol extrapolate callback of fighter `fighter` at the patrol
    /// path's current point `point` (8): its result's truth, true without a
    /// callback. What the result changes is spec X's.
    pub fn patrol_extrapolate_callback(&mut self, fighter: u16, point: u32) -> Result<bool, Error> {
        self.ask(&[
            Value::from("patrol"),
            Value::Number(f64::from(fighter)),
            Value::Number(f64::from(point)),
        ])
    }

    fn ask(&mut self, args: &[Value]) -> Result<bool, Error> {
        let results = self.callbacks(args)?;
        Ok(match results.as_slice() {
            [Value::String(o), Value::Boolean(b), ..] if o == b"result" => *b,
            _ => true,
        })
    }

    /// Object `id` is destroyed: its callbacks and queued sounds go, and a
    /// physical object's fast call (5).
    pub(super) fn forget_callbacks(&mut self, id: u16, kinds: Kinds) -> Result<(), Error> {
        self.callbacks(&[
            "forget".into(),
            Value::Number(f64::from(id)),
            Value::Boolean(kinds.has(Kinds::PHYSICAL)),
        ])
        .map(drop)
    }
}
