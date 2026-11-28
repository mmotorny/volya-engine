// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The action planner as scripts reach it (script API W,
//! docs/behavior/script-api-action-planner.md; section numbers are its):
//! world properties and world states (2), the property store (3),
//! property evaluators (4), actions (5), planners (6) and how they plan
//! (7, [`search`]), actions that are planners (8), the constants
//! `stalker_ids` (9), a stalker's motivation manager and `ef_storage` (10).
//!
//! An action, a planner and a planner action share one engine part,
//! [`PlanPart`]: the action's [`Action`], the planner's [`Planner`], or
//! both. The planners' members are defined by the Lua side
//! (`action_planner.lua`) over one hidden Rust function, so a planner's
//! part is never held while its evaluators and actions run (they call it
//! back); the planner calls their methods through the Lua side too, which
//! runs a script class's override or the engine's own (A 5.2), an error in
//! it fatal (A 5.3). The Lua side also keeps the script objects a planner
//! holds (the engine takes them over, 6.2) in the planner object's own
//! fields.
//!
//! Each stalker gets its motivation manager as it comes online
//! ([`Scripts::spawn`]): a planner set up with the stalker, holding the
//! engine's own actions and evaluators of 12.2, which are the stalker
//! planner's (CH-17, TODO(#1185)); the engine updates it with
//! [`Scripts::update_motivation_manager`]. See docs/architecture/scripts.md,
//! The action planner.

pub mod search;
pub mod world;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

pub use world::{PropertyStorage, WorldProperty, WorldState};

use crate::classes::{EngineClass, EnginePart, NOT_LOGGED, ObjectMaker, Objects, Operand};
use crate::game_object::members::Handle;
use crate::game_object::sight::EvaluationFunctions;
use crate::game_object::{GameObjects, Kinds};
use crate::loader::Scripts;
use crate::services::EngineServices;
use crate::values::{lua_number, to_flags32};
use crate::{Error, ErrorKind, Value, call_kept_within};

type Results = Result<Vec<Value>, String>;

/// The spec's open outcomes: a member reaching one raises a script error
/// naming it.
const GAP: &str = "#1222";
/// The review marks, which leave their statements unestablished.
const REVIEW: &str = "#1213";
/// The stalker planner (CH-17), whose own actions and evaluators the
/// motivation manager holds.
const STALKER_PLANNER: &str = "#1185";

/// The registry key of the Lua side's entry function.
const ENTRY: &str = "volya.action_planner";

/// The current action id "none" (6.2).
pub const NONE: u32 = u32::MAX;

/// The 119 constants of `stalker_ids` (9): 58 actions and planners, 37
/// conditions, 24 sound kinds.
pub const STALKER_IDS: [(&str, i32); 119] = [
    ("action_dead", 0),
    ("action_dying", 1),
    ("action_gather_items", 2),
    ("action_no_alife", 3),
    ("action_smart_terrain_task", 4),
    ("action_solve_zone_puzzle", 5),
    ("action_reach_task_location", 6),
    ("action_accomplish_task", 7),
    ("action_reach_customer_location", 8),
    ("action_communicate_with_customer", 9),
    ("get_out_of_anomaly", 10),
    ("detect_anomaly", 11),
    ("action_get_item_to_kill", 12),
    ("action_find_item_to_kill", 13),
    ("action_make_item_killing", 14),
    ("action_find_ammo", 15),
    ("action_aim_enemy", 16),
    ("action_get_ready_to_kill", 17),
    ("action_kill_enemy", 19),
    ("action_retreat_from_enemy", 20),
    ("action_take_cover", 21),
    ("action_look_out", 22),
    ("action_hold_position", 23),
    ("action_get_distance", 24),
    ("action_detour_enemy", 25),
    ("action_search_enemy", 26),
    ("action_sudden_attack", 28),
    ("action_kill_enemy_if_not_visible", 29),
    ("action_reach_wounded_enemy", 30),
    ("action_prepare_wounded_enemy", 32),
    ("action_kill_wounded_enemy", 33),
    ("action_post_combat_wait", 34),
    ("action_kill_if_player_on_the_path", 35),
    ("action_critically_wounded", 36),
    ("action_kill_if_enemy_critically_wounded", 37),
    ("action_danger_unknown_planner", 39),
    ("action_danger_in_direction_planner", 40),
    ("action_danger_grenade_planner", 41),
    ("action_danger_by_sound_planner", 42),
    ("action_danger_unknown_take_cover", 43),
    ("action_danger_unknown_look_around", 44),
    ("action_danger_unknown_search", 45),
    ("action_danger_in_direction_take_cover", 46),
    ("action_danger_in_direction_look_out", 47),
    ("action_danger_in_direction_hold_position", 48),
    ("action_danger_in_direction_detour", 49),
    ("action_danger_in_direction_search", 50),
    ("action_danger_grenade_take_cover", 51),
    ("action_danger_grenade_wait_for_explosion", 52),
    ("action_danger_grenade_take_cover_after_explosion", 53),
    ("action_danger_grenade_look_around", 54),
    ("action_danger_grenade_search", 55),
    ("action_death_planner", 56),
    ("action_alife_planner", 57),
    ("action_combat_planner", 58),
    ("action_anomaly_planner", 59),
    ("action_danger_planner", 60),
    ("action_script", 61),
    ("property_alive", 0),
    ("property_dead", 1),
    ("property_already_dead", 2),
    ("property_alife", 3),
    ("property_puzzle_solved", 4),
    ("property_smart_terrain_task", 5),
    ("property_items", 6),
    ("property_enemy", 7),
    ("property_danger", 8),
    ("property_item_to_kill", 9),
    ("property_found_item_to_kill", 10),
    ("property_item_can_kill", 11),
    ("property_found_ammo", 12),
    ("property_ready_to_kill", 13),
    ("property_ready_to_detour", 14),
    ("property_see_enemy", 15),
    ("property_panic", 17),
    ("property_in_cover", 18),
    ("property_looked_out", 19),
    ("property_position_holded", 20),
    ("property_enemy_detoured", 21),
    ("property_use_suddenness", 22),
    ("property_pure_enemy", 23),
    ("property_use_crouch_to_look_out", 24),
    ("property_critically_wounded", 29),
    ("property_enemy_critically_wounded", 30),
    ("property_danger_unknown", 34),
    ("property_danger_in_direction", 35),
    ("property_danger_grenade", 36),
    ("property_danger_by_sound", 37),
    ("property_cover_actual", 38),
    ("property_cover_reached", 39),
    ("property_looked_around", 40),
    ("property_grenade_exploded", 41),
    ("property_anomaly", 42),
    ("property_inside_anomaly", 43),
    ("property_script", 44),
    ("sound_die", 0),
    ("sound_die_in_anomaly", 1),
    ("sound_injuring", 2),
    ("sound_humming", 3),
    ("sound_alarm", 4),
    ("sound_attack_no_allies", 5),
    ("sound_attack_allies_single_enemy", 6),
    ("sound_attack_allies_several_enemies", 7),
    ("sound_backup", 8),
    ("sound_detour", 9),
    ("sound_search1_with_allies", 10),
    ("sound_search1_no_allies", 11),
    ("sound_injuring_by_friend", 12),
    ("sound_panic_human", 13),
    ("sound_panic_monster", 14),
    ("sound_tolls", 15),
    ("sound_grenade_alarm", 16),
    ("sound_friendly_grenade_alarm", 17),
    ("sound_need_backup", 18),
    ("sound_running_in_danger", 19),
    ("sound_kill_wounded", 20),
    ("sound_enemy_critically_wounded", 21),
    ("sound_script", 23),
    // The 32-bit pattern D0001000 as a signed integer (believed, 9).
    ("sound_enemy_killed_or_wounded", -805_302_272),
];

/// The conditions of the motivation manager's engine evaluators (12.2).
pub const MOTIVATION_CONDITIONS: [u32; 7] = [0, 2, 4, 6, 7, 8, 42];

/// (condition, value) pairs.
pub type Pairs = &'static [(u32, bool)];

/// The motivation manager's engine actions (12.2): id, preconditions,
/// effect.
pub const MOTIVATION_ACTIONS: [(u32, Pairs, (u32, bool)); 6] = [
    (
        2,
        &[(0, true), (7, false), (42, false), (8, false), (6, true)],
        (6, false),
    ),
    (56, &[(0, false), (2, false)], (2, true)),
    (
        57,
        &[
            (0, true),
            (7, false),
            (42, false),
            (8, false),
            (6, false),
            (4, false),
        ],
        (4, true),
    ),
    (58, &[(0, true), (42, false), (7, true)], (7, false)),
    (59, &[(0, true), (42, true)], (42, false)),
    (
        60,
        &[(0, true), (7, false), (42, false), (8, true)],
        (8, false),
    ),
];

/// The motivation manager's goals (12.2): alive, and dead.
const PUZZLE_SOLVED: u32 = 4;
const ALREADY_DEAD: u32 = 2;

/// An engine evaluator's own evaluation (another spec's, as the stalker
/// planner's): the value, or the error to raise.
pub type EngineEvaluate = Rc<dyn Fn() -> Result<bool, String>>;

/// What an evaluator's `evaluate` gives without an override (4).
#[derive(Clone)]
pub enum Evaluation {
    /// `property_evaluator`'s default: false.
    Default,
    /// `property_evaluator_const(v)`.
    Constant(bool),
    /// An evaluator the engine made itself.
    Engine(EngineEvaluate),
}

/// A property evaluator's part (4).
#[derive(Clone)]
pub struct Evaluator {
    /// A game object's handle.
    pub object: Option<EnginePart>,
    pub storage: Option<PropertyStorage>,
    pub evaluation: Evaluation,
}

/// The engine calls of an action (5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Initialize,
    Execute,
    Finalize,
}

/// What an action the engine made itself does at each engine call, after
/// the default (another spec's, as the stalker planner's).
pub type EngineStep = Rc<dyn Fn(Phase) -> Result<(), String>>;

/// An action (5). Its minimum weight (5) is 0 for every action of this
/// spec and never changes, so it never changes a weight and isn't kept.
pub struct Action {
    pub object: Option<EnginePart>,
    pub storage: Option<PropertyStorage>,
    pub preconditions: WorldState,
    pub effects: WorldState,
    pub weight: u16,
    pub start_time: u32,
    pub first_execution: bool,
    pub engine: Option<EngineStep>,
}

impl Action {
    pub fn new(object: Option<EnginePart>) -> Self {
        Action {
            object,
            storage: None,
            preconditions: WorldState::new(),
            effects: WorldState::new(),
            weight: 1,
            start_time: 0,
            first_execution: false,
            engine: None,
        }
    }
}

/// An action as a planner holds it: its id, its object, its part.
#[derive(Clone)]
pub struct HeldAction {
    pub id: u32,
    pub object: EnginePart,
    pub action: Rc<RefCell<Action>>,
}

/// A planner (6.1).
#[derive(Default)]
pub struct Planner {
    /// Whether `setup` has run: the actual flag, the object and the
    /// current action id are unset before it.
    set_up: bool,
    pub object: Option<EnginePart>,
    pub storage: PropertyStorage,
    /// By id, increasing.
    pub evaluators: Vec<(u32, EnginePart)>,
    /// By id, increasing.
    pub actions: Vec<HeldAction>,
    pub goal: WorldState,
    /// The world values the last planning computed (7.2).
    pub cached: BTreeMap<u32, bool>,
    pub plan: Vec<u32>,
    current: u32,
    pub initialized: bool,
    actual: bool,
}

impl Planner {
    /// `setup(object)`'s default (6.2).
    pub fn setup(&mut self, object: Option<EnginePart>) {
        self.goal.clear();
        self.cached.clear();
        self.plan.clear();
        self.actual = true;
        self.current = NONE;
        self.object = object;
        self.storage.clear();
        self.initialized = false;
        self.set_up = true;
    }

    fn require_setup(&self, what: &str) -> Result<(), String> {
        if self.set_up {
            Ok(())
        } else {
            Err(format!(
                "{what}: the planner isn't set up, what it reads is unestablished (TODO({GAP}))"
            ))
        }
    }

    /// `set_goal_world_state`'s rule (6.2).
    pub fn set_goal(&mut self, goal: &WorldState) {
        self.actual = self.actual && self.goal.same(goal);
        self.goal = goal.clone();
    }

    /// The current action id (6.2), once set up.
    pub fn current_action_id(&self) -> Option<u32> {
        self.set_up.then_some(self.current)
    }

    /// The place of the first action whose id is not below `id` (6.2).
    fn action_at(&self, id: u32) -> Option<usize> {
        let at = self.actions.partition_point(|a| a.id < id);
        (at < self.actions.len()).then_some(at)
    }

    fn action_from(&self, id: u32, what: &str) -> Result<HeldAction, String> {
        self.action_at(id)
            .map(|at| self.actions[at].clone())
            .ok_or_else(|| {
                format!("{what}: no action's id is at least {id}, the outcome is unestablished (TODO({GAP}))")
            })
    }

    fn evaluator_of(&self, id: u32) -> Option<EnginePart> {
        self.evaluators
            .binary_search_by_key(&id, |e| e.0)
            .ok()
            .map(|at| self.evaluators[at].1)
    }

    /// Adds `evaluator` under `id`, replacing one already there (6.1).
    pub fn put_evaluator(&mut self, id: u32, evaluator: EnginePart) {
        match self.evaluators.binary_search_by_key(&id, |e| e.0) {
            Ok(at) => self.evaluators[at].1 = evaluator,
            Err(at) => self.evaluators.insert(at, (id, evaluator)),
        }
    }

    /// Adds `action` under its id, making the plan invalid (6.2). An id
    /// already there is the review's (TODO(#1213)).
    pub fn put_action(&mut self, action: HeldAction) -> Result<(), String> {
        match self.actions.binary_search_by_key(&action.id, |a| a.id) {
            Ok(_) => Err(format!(
                "add_action: an action of id {} is there already, the outcome is unestablished (TODO({REVIEW}))",
                action.id
            )),
            Err(at) => {
                self.actions.insert(at, action);
                self.actual = false;
                Ok(())
            }
        }
    }
}

/// The engine part of an action, a planner, or a planner action (both).
#[derive(Clone)]
pub struct PlanPart {
    pub action: Option<Rc<RefCell<Action>>>,
    pub planner: Option<Rc<RefCell<Planner>>>,
}

impl PlanPart {
    pub fn action(action: Action) -> Self {
        PlanPart {
            action: Some(Rc::new(RefCell::new(action))),
            planner: None,
        }
    }

    pub fn planner(planner: Planner) -> Self {
        PlanPart {
            action: None,
            planner: Some(Rc::new(RefCell::new(planner))),
        }
    }

    fn the_action(&self, what: &str) -> Result<Rc<RefCell<Action>>, String> {
        (self.action.clone()).ok_or_else(|| format!("{what}: not called on an action"))
    }

    fn the_planner(&self, what: &str) -> Result<Rc<RefCell<Planner>>, String> {
        (self.planner.clone()).ok_or_else(|| format!("{what}: not called on a planner"))
    }
}

/// A stalker's motivation manager (10): the planner and the engine's own
/// parts in it.
struct Root {
    planner: EnginePart,
    parts: Vec<EnginePart>,
}

#[derive(Clone)]
struct Context {
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
    fatal: Rc<RefCell<Option<String>>>,
    roots: Rc<RefCell<HashMap<u16, Root>>>,
}

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A method the planner called failed (A 5.3): logged unless it is
    /// [`NOT_LOGGED`], and the game stops with `LUA error: <message>`; an
    /// earlier fatal error stands. The error to raise.
    fn failed(&self, message: String) -> String {
        if let Some(earlier) = self.fatal.borrow().clone() {
            return earlier;
        }
        if message != NOT_LOGGED {
            self.services.borrow_mut().console.log_mut().write(&message);
        }
        let message = format!("LUA error: {message}");
        *self.fatal.borrow_mut() = Some(message.clone());
        message
    }

    /// Calls `object`'s method `method` with `args` through the Lua side:
    /// a script class's override or the engine's own. Its first result.
    fn call(&self, object: EnginePart, method: &str, args: &[Value]) -> Result<Value, String> {
        let mut all = vec![
            Value::from("call"),
            Value::Object(object),
            Value::from(method),
        ];
        all.extend_from_slice(args);
        match call_kept_within(ENTRY, &all) {
            Ok(results) => Ok(results.into_iter().next().unwrap_or(Value::Nil)),
            Err(e) => Err(self.failed(e.message)),
        }
    }

    /// Whether `object`'s `weight` is a script class's own.
    fn weight_overridden(&self, object: EnginePart) -> Result<bool, String> {
        let args = [Value::from("weight_overridden"), Value::Object(object)];
        match call_kept_within(ENTRY, &args) {
            Ok(results) => Ok(results.first().is_some_and(truth)),
            Err(e) => Err(self.failed(e.message)),
        }
    }

    fn now(&self) -> u32 {
        self.services.borrow().device.time_global()
    }
}

pub(crate) struct Registered {
    ctx: Context,
}

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A truth value (conventions).
fn truth(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}

/// A number argument made an id (conventions; our wording for a
/// mismatch).
fn id(args: &[Value], i: usize, what: &str) -> Result<u32, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(to_flags32(*n)),
        _ => Err(format!("{what}: number expected")),
    }
}

fn number_value(n: u32) -> Value {
    Value::Number(f64::from(n))
}

fn object_value(part: Option<EnginePart>) -> Value {
    part.map_or(Value::Nil, Value::Object)
}

/// A game object argument (nil for none), as methods see it.
fn handle_arg(
    objects: &Objects,
    args: &[Value],
    i: usize,
    what: &str,
) -> Result<Option<EnginePart>, String> {
    match arg(args, i) {
        Value::Nil => Ok(None),
        v @ Value::Object(part) if objects.get::<Handle>(v).is_some() => Ok(Some(*part)),
        _ => Err(format!("{what}: game object expected")),
    }
}

/// A game object argument (nil for none), as the hidden function sees it.
fn handle_of(maker: &ObjectMaker, v: &Value, what: &str) -> Result<Option<EnginePart>, String> {
    match v {
        Value::Nil => Ok(None),
        Value::Object(part) if maker.is::<Handle>(v) => Ok(Some(*part)),
        _ => Err(format!("{what}: game object expected")),
    }
}

/// A property store argument (nil for none).
fn storage_arg(
    objects: &Objects,
    args: &[Value],
    i: usize,
    what: &str,
) -> Result<Option<PropertyStorage>, String> {
    match arg(args, i) {
        Value::Nil => Ok(None),
        v => match objects.get::<PropertyStorage>(v) {
            Some(Operand::Other(s)) => Ok(Some(s)),
            _ => Err(format!("{what}: property_storage expected")),
        },
    }
}

fn property_arg(objects: &Objects, args: &[Value], what: &str) -> Result<WorldProperty, String> {
    match objects.get::<WorldProperty>(arg(args, 0)) {
        Some(Operand::Other(p)) => Ok(p),
        _ => Err(format!("{what}: world_property expected")),
    }
}

/// The constructors' score of an optional object and a name (4, 5): the
/// object nil or a game object, the name a text.
fn object_and_name(maker: &ObjectMaker, args: &[Value]) -> Option<u32> {
    let object = match args.first() {
        None | Some(Value::Nil) => true,
        Some(v) => maker.is::<Handle>(v),
    };
    let name = match args.get(1) {
        None => true,
        Some(v) => matches!(v, Value::String(_)),
    };
    (object && name).then_some(0)
}

fn object_of_args(args: &[Value]) -> Option<EnginePart> {
    match args.first() {
        Some(Value::Object(p)) => Some(*p),
        _ => None,
    }
}

/// `class` with three constructors, `()`, `(object)` and `(object,
/// name)`, each making `make(object)`.
fn three_constructors<T: 'static>(
    class: EngineClass,
    make: fn(Option<EnginePart>) -> T,
) -> EngineClass {
    let mut class = class;
    for params in 0..3 {
        class = class.constructor_with_objects(params, object_and_name, move |_, _, args| {
            Ok(make(object_of_args(args)))
        });
    }
    class
}

fn world_property_class() -> EngineClass {
    EngineClass::new("world_property")
        .constructible()
        .constructor(
            2,
            |args| matches!(args.first(), Some(Value::Number(_))).then_some(0),
            |_, args| {
                Ok(WorldProperty::new(
                    id(args, 0, "world_property")?,
                    truth(arg(args, 1)),
                ))
            },
        )
        .method("condition", |p: &mut WorldProperty, _| {
            Ok(vec![number_value(p.condition)])
        })
        .method("value", |p: &mut WorldProperty, _| Ok(vec![p.value.into()]))
        .method_with_objects("__lt", |p: &mut WorldProperty, o, args| {
            let q = match o.get::<WorldProperty>(arg(args, 0)) {
                Some(Operand::This) => *p,
                Some(Operand::Other(q)) => q,
                None => return Err("world_property.__lt: world_property expected".into()),
            };
            Ok(vec![(*p < q).into()])
        })
        .method_with_objects("__eq", |p: &mut WorldProperty, o, args| {
            let q = match o.get::<WorldProperty>(arg(args, 0)) {
                Some(Operand::This) => *p,
                Some(Operand::Other(q)) => q,
                None => return Ok(vec![false.into()]),
            };
            Ok(vec![(*p == q).into()])
        })
}

fn other_state(
    s: &WorldState,
    o: &Objects,
    args: &[Value],
    what: &str,
) -> Result<WorldState, String> {
    match o.get::<WorldState>(arg(args, 0)) {
        Some(Operand::This) => Ok(s.clone()),
        Some(Operand::Other(t)) => Ok(t),
        None => Err(format!("{what}: world_state expected")),
    }
}

fn world_state_class() -> EngineClass {
    EngineClass::new("world_state")
        .constructible()
        .constructor(0, |_| Some(0), |_, _| Ok(WorldState::new()))
        .constructor_with_objects(
            1,
            |maker, args| maker.is::<WorldState>(arg(args, 0)).then_some(0),
            |_, maker, args| {
                (maker.get::<WorldState>(arg(args, 0)))
                    .ok_or_else(|| "world_state: world_state expected".into())
            },
        )
        .method_with_objects("add_property", |s: &mut WorldState, o, args| {
            let p = property_arg(o, args, "world_state:add_property")?;
            s.add(p);
            Ok(Vec::new())
        })
        .method("remove_property", |s: &mut WorldState, args| {
            let c = id(args, 0, "world_state:remove_property")?;
            match s.at_least(c) {
                Some(at) => {
                    s.remove_at(at);
                    Ok(Vec::new())
                }
                None => Err(format!(
                    "world_state:remove_property: no condition is at least {c}, the outcome is unestablished (TODO({GAP}))"
                )),
            }
        })
        .method_with_objects("property", |s: &mut WorldState, o, args| {
            let c = id(args, 0, "world_state:property")?;
            Ok(vec![match s.at_least(c) {
                Some(at) => o.new_object("world_property", s.properties()[at]),
                None => Value::Nil,
            }])
        })
        .method_with_objects("includes", |s: &mut WorldState, o, args| {
            let t = other_state(s, o, args, "world_state:includes")?;
            Ok(vec![s.includes(&t).into()])
        })
        .method("clear", |s: &mut WorldState, _| {
            s.clear();
            Ok(Vec::new())
        })
        .method_with_objects("__lt", |s: &mut WorldState, o, args| {
            let t = other_state(s, o, args, "world_state.__lt")?;
            Ok(vec![s.less(&t).into()])
        })
        .method_with_objects("__eq", |s: &mut WorldState, o, args| {
            Ok(vec![match o.get::<WorldState>(arg(args, 0)) {
                Some(Operand::This) => true.into(),
                Some(Operand::Other(t)) => s.hash_equal(&t).into(),
                None => false.into(),
            }])
        })
}

fn property_storage_class() -> EngineClass {
    EngineClass::new("property_storage")
        .constructible()
        .constructor(0, |_| Some(0), |_, _| Ok(PropertyStorage::default()))
        .method("set_property", |st: &mut PropertyStorage, args| {
            let i = id(args, 0, "property_storage:set_property")?;
            st.set(i, truth(arg(args, 1)));
            Ok(Vec::new())
        })
        .method("property", |st: &mut PropertyStorage, args| {
            let i = id(args, 0, "property_storage:property")?;
            match st.get(i) {
                Some(v) => Ok(vec![v.into()]),
                None => Err(format!(
                    "property_storage:property: {i} isn't stored, the outcome is unestablished (TODO({GAP}))"
                )),
            }
        })
}

fn storage_value(o: &Objects, storage: &Option<PropertyStorage>) -> Value {
    match storage {
        Some(s) => o.new_object("property_storage", s.clone()),
        None => Value::Nil,
    }
}

fn evaluator_classes() -> [EngineClass; 2] {
    let evaluator = three_constructors(
        EngineClass::new("property_evaluator").constructible(),
        |object| Evaluator {
            object,
            storage: None,
            evaluation: Evaluation::Default,
        },
    )
    .property_with_objects(
        "object",
        |e: &Evaluator, _| Ok(object_value(e.object)),
        None,
    )
    .property_with_objects(
        "storage",
        |e: &Evaluator, o| Ok(storage_value(o, &e.storage)),
        None,
    )
    .overridable_with_objects("setup", |e: &mut Evaluator, o, args| {
        e.object = handle_arg(o, args, 0, "property_evaluator:setup")?;
        e.storage = storage_arg(o, args, 1, "property_evaluator:setup")?;
        Ok(Vec::new())
    })
    .overridable("evaluate", |e: &mut Evaluator, _| {
        let v = match &e.evaluation {
            Evaluation::Default => false,
            Evaluation::Constant(v) => *v,
            Evaluation::Engine(f) => f()?,
        };
        Ok(vec![v.into()])
    });
    let constant = EngineClass::new("property_evaluator_const")
        .base("property_evaluator")
        .constructible()
        .constructor(
            1,
            |_| Some(0),
            |_, args| {
                Ok(Evaluator {
                    object: None,
                    storage: None,
                    evaluation: Evaluation::Constant(truth(arg(args, 0))),
                })
            },
        );
    [evaluator, constant]
}

/// The members `add_precondition` … `remove_effect` and `set_weight` (5),
/// of an action and a planner action alike.
fn condition_members(class: EngineClass, name: &'static str) -> EngineClass {
    let mut class = class;
    for (member, effects) in [("add_precondition", false), ("add_effect", true)] {
        let what = format!("{name}:{member}");
        class = class.method_with_objects(member, move |part: &mut PlanPart, o, args| {
            let p = property_arg(o, args, &what)?;
            let action = part.the_action(&what)?;
            let mut a = action.borrow_mut();
            let state = if effects {
                &mut a.effects
            } else {
                &mut a.preconditions
            };
            state.add(p);
            Ok(Vec::new())
        });
    }
    for (member, effects) in [("remove_precondition", false), ("remove_effect", true)] {
        let what = format!("{name}:{member}");
        class = class.method(member, move |part: &mut PlanPart, args| {
            let c = id(args, 0, &what)?;
            let action = part.the_action(&what)?;
            let mut a = action.borrow_mut();
            let state = if effects { &mut a.effects } else { &mut a.preconditions };
            match state.at_least(c) {
                Some(at) => {
                    state.remove_at(at);
                    Ok(Vec::new())
                }
                None => Err(format!(
                    "{what}: no condition is at least {c}, the outcome is unestablished (TODO({GAP}))"
                )),
            }
        });
    }
    let what = format!("{name}:set_weight");
    class.method("set_weight", move |part: &mut PlanPart, args| {
        let w = match arg(args, 0) {
            Value::Number(n) => weight_of(*n),
            _ => return Err(format!("{what}: number expected")),
        };
        part.the_action(&what)?.borrow_mut().weight = w;
        Ok(Vec::new())
    })
}

/// A number made a weight (conventions).
fn weight_of(n: f64) -> u16 {
    to_flags32(n) as u16
}

/// The properties `object` and `storage` of an action.
fn action_properties(class: EngineClass) -> EngineClass {
    class
        .property_with_objects(
            "object",
            |part: &PlanPart, _| Ok(object_value(part.the_action("object")?.borrow().object)),
            None,
        )
        .property_with_objects(
            "storage",
            |part: &PlanPart, o| {
                Ok(storage_value(
                    o,
                    &part.the_action("storage")?.borrow().storage,
                ))
            },
            None,
        )
}

/// The default of `initialize` (5).
fn initialize_default(ctx: &Context, action: &Rc<RefCell<Action>>) -> Result<(), String> {
    let engine = {
        let mut a = action.borrow_mut();
        a.start_time = ctx.now();
        a.first_execution = true;
        a.engine.clone()
    };
    engine.map_or(Ok(()), |f| f(Phase::Initialize))
}

/// The default of `execute` (5).
fn execute_default(action: &Rc<RefCell<Action>>) -> Result<(), String> {
    let engine = {
        let mut a = action.borrow_mut();
        a.first_execution = false;
        a.engine.clone()
    };
    engine.map_or(Ok(()), |f| f(Phase::Execute))
}

/// The default of `finalize` (5).
fn finalize_default(action: &Rc<RefCell<Action>>) -> Result<(), String> {
    let engine = action.borrow().engine.clone();
    engine.map_or(Ok(()), |f| f(Phase::Finalize))
}

fn action_base_class(ctx: &Context) -> EngineClass {
    let class = three_constructors(EngineClass::new("action_base").constructible(), |object| {
        PlanPart::action(Action::new(object))
    });
    let class = action_properties(class);
    let class = condition_members(class, "action_base");
    let c = ctx.clone();
    class
        .overridable_with_objects("setup", |part: &mut PlanPart, o, args| {
            let object = handle_arg(o, args, 0, "action_base:setup")?;
            let storage = storage_arg(o, args, 1, "action_base:setup")?;
            let action = part.the_action("action_base:setup")?;
            let mut a = action.borrow_mut();
            a.object = object;
            a.storage = storage;
            Ok(Vec::new())
        })
        .overridable("initialize", move |part: &mut PlanPart, _| {
            initialize_default(&c, &part.the_action("action_base:initialize")?)?;
            Ok(Vec::new())
        })
        .overridable("execute", |part: &mut PlanPart, _| {
            execute_default(&part.the_action("action_base:execute")?)?;
            Ok(Vec::new())
        })
        .overridable("finalize", |part: &mut PlanPart, _| {
            finalize_default(&part.the_action("action_base:finalize")?)?;
            Ok(Vec::new())
        })
        .overridable("weight", |part: &mut PlanPart, _| {
            let w = part.the_action("action_base:weight")?.borrow().weight;
            Ok(vec![Value::Number(f64::from(w))])
        })
}

fn planner_classes() -> [EngineClass; 2] {
    let planner = EngineClass::new("action_planner")
        .constructible()
        .constructor(
            0,
            |_| Some(0),
            |_, _| Ok(PlanPart::planner(Planner::default())),
        )
        .property_with_objects(
            "object",
            |part: &PlanPart, _| {
                let planner = part.the_planner("object")?;
                let p = planner.borrow();
                p.require_setup("action_planner.object")?;
                Ok(object_value(p.object))
            },
            None,
        )
        .property_with_objects(
            "storage",
            |part: &PlanPart, o| {
                let planner = part.the_planner("storage")?;
                let storage = planner.borrow().storage.clone();
                Ok(o.new_object("property_storage", storage))
            },
            None,
        );
    // 8: the planner members act on the inner planner; the action's own
    // members are the Lua side's. Its `object` and `storage` are the
    // action's (our choice: both bases have them; after `setup` the
    // objects are the same).
    let planner_action = three_constructors(
        EngineClass::new("planner_action")
            .base("action_planner")
            .constructible(),
        |object| PlanPart {
            action: Some(Rc::new(RefCell::new(Action::new(object)))),
            planner: Some(Rc::new(RefCell::new(Planner::default()))),
        },
    );
    let planner_action = action_properties(planner_action);
    let planner_action = condition_members(planner_action, "planner_action");
    [planner, planner_action]
}

fn stalker_ids_class() -> EngineClass {
    (STALKER_IDS.iter()).fold(EngineClass::new("stalker_ids"), |class, (name, value)| {
        class.constant(name, *value)
    })
}

/// The hidden function's operation `op` on planner (or planner action)
/// `this`, whose remaining arguments are `args`.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    ctx.stopped()?;
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("action planner: operation expected".into()),
    };
    let op = op.as_str();
    let this = arg(args, 1);
    let rest = args.get(2..).unwrap_or(&[]);
    if op == "motivation_action_manager" {
        return motivation_action_manager(ctx, maker, this);
    }
    let what = |name: &str| match op {
        "pa_setup" | "pa_initialize" | "pa_execute" | "pa_finalize" | "pa_weight" => {
            format!("planner_action:{name}")
        }
        _ => format!("action_planner:{name}"),
    };
    let Some(part) = maker.get::<PlanPart>(this) else {
        return Err(format!("{}: not called on a planner", what(op)));
    };
    let call = Call { ctx, maker };
    match op {
        "pa_setup" => {
            let w = what("setup");
            let action = part.the_action(&w)?;
            let planner = part.the_planner(&w)?;
            let handle = handle_of(maker, arg(rest, 0), &w)?;
            let storage = match arg(rest, 1) {
                Value::Nil => None,
                v => Some(
                    maker
                        .get::<PropertyStorage>(v)
                        .ok_or_else(|| format!("{w}: property_storage expected"))?,
                ),
            };
            let effects = {
                let mut a = action.borrow_mut();
                a.object = handle;
                a.storage = storage;
                a.effects.clone()
            };
            let mut p = planner.borrow_mut();
            p.setup(handle);
            p.set_goal(&effects);
            Ok(Vec::new())
        }
        "pa_initialize" => {
            initialize_default(ctx, &part.the_action(&what("initialize"))?)?;
            Ok(Vec::new())
        }
        "pa_execute" => {
            let w = what("execute");
            let action = part.the_action(&w)?;
            action.borrow_mut().first_execution = false;
            call.update(&part.the_planner(&w)?)?;
            Ok(Vec::new())
        }
        "pa_finalize" => {
            let w = what("finalize");
            let planner = part.the_planner(&w)?;
            let current = {
                let p = planner.borrow();
                p.require_setup(&w)?;
                p.action_from(p.current, &w)?
            };
            ctx.call(current.object, "finalize", &[])?;
            planner.borrow_mut().initialized = false;
            Ok(Vec::new())
        }
        "pa_weight" => {
            let w = part.the_action(&what("weight"))?.borrow().weight;
            Ok(vec![Value::Number(f64::from(w))])
        }
        _ => {
            let planner = part.the_planner(&what(op))?;
            call.planner_op(op, &planner, rest)
        }
    }
}

/// `npc:motivation_action_manager()` (10).
fn motivation_action_manager(ctx: &Context, maker: &ObjectMaker, this: &Value) -> Results {
    const WHAT: &str = "game_object:motivation_action_manager";
    let Value::Object(handle) = this else {
        return Err(format!("{WHAT}: game object expected"));
    };
    if !maker.is::<Handle>(this) {
        return Err(format!("{WHAT}: game object expected"));
    }
    let objects = ctx.objects.borrow();
    let Some(object) = objects.of_handle(*handle).and_then(|id| objects.get(id)) else {
        return Err(format!("{WHAT}: game object expected"));
    };
    if !object.kinds.has(Kinds::STALKER) {
        // The text goes to M 2.3's routine, which returns at once; what
        // follows is unestablished.
        return Err(format!(
            "{WHAT}: not a stalker, the outcome is unestablished (TODO({GAP}))"
        ));
    }
    match ctx.roots.borrow().get(&object.id) {
        Some(root) => Ok(vec![Value::Object(root.planner)]),
        None => Err(format!("{WHAT}: the stalker has no motivation manager")),
    }
}

/// A member call inside the hidden function: the context and the maker of
/// new objects.
struct Call<'a> {
    ctx: &'a Context,
    maker: &'a ObjectMaker,
}

impl Call<'_> {
    fn storage_value(&self, planner: &Rc<RefCell<Planner>>) -> Value {
        let storage = planner.borrow().storage.clone();
        self.maker.new_object("property_storage", storage)
    }

    /// `part:setup(pl.object, pl.storage)` for a part added (6.2).
    fn set_up_part(
        &self,
        planner: &Rc<RefCell<Planner>>,
        part: EnginePart,
        what: &str,
    ) -> Result<(), String> {
        let object = {
            let p = planner.borrow();
            p.require_setup(what)?;
            object_value(p.object)
        };
        let storage = self.storage_value(planner);
        self.ctx.call(part, "setup", &[object, storage]).map(drop)
    }

    fn planner_op(&self, op: &str, planner: &Rc<RefCell<Planner>>, args: &[Value]) -> Results {
        let what = format!("action_planner:{op}");
        let what = what.as_str();
        match op {
            "setup" => {
                let object = handle_of(self.maker, arg(args, 0), what)?;
                planner.borrow_mut().setup(object);
                Ok(Vec::new())
            }
            "update" => {
                self.update(planner)?;
                Ok(Vec::new())
            }
            "add_action" => {
                let i = id(args, 0, what)?;
                let value = arg(args, 1);
                let action = (self.maker.get::<PlanPart>(value))
                    .and_then(|p| p.action)
                    .ok_or_else(|| format!("{what}: action_base expected"))?;
                let Value::Object(part) = value else {
                    return Err(format!("{what}: action_base expected"));
                };
                planner.borrow().require_setup(what)?;
                planner.borrow_mut().put_action(HeldAction {
                    id: i,
                    object: *part,
                    action,
                })?;
                self.set_up_part(planner, *part, what)?;
                Ok(vec![number_value(i)])
            }
            "remove_action" => {
                let i = id(args, 0, what)?;
                let mut p = planner.borrow_mut();
                let at = p.action_at(i).ok_or_else(|| {
                    format!("{what}: no action's id is at least {i}, the outcome is unestablished (TODO({GAP}))")
                })?;
                let removed = p.actions.remove(at);
                p.actual = false;
                Ok(vec![number_value(removed.id)])
            }
            "action" => {
                let i = id(args, 0, what)?;
                let a = planner.borrow().action_from(i, what)?;
                Ok(vec![Value::Object(a.object)])
            }
            "add_evaluator" => {
                let i = id(args, 0, what)?;
                let value = arg(args, 1);
                let Value::Object(part) = value else {
                    return Err(format!("{what}: property_evaluator expected"));
                };
                if !self.maker.is::<Evaluator>(value) {
                    return Err(format!("{what}: property_evaluator expected"));
                }
                planner.borrow().require_setup(what)?;
                planner.borrow_mut().put_evaluator(i, *part);
                self.set_up_part(planner, *part, what)?;
                Ok(vec![number_value(i)])
            }
            "remove_evaluator" => {
                let i = id(args, 0, what)?;
                let mut p = planner.borrow_mut();
                match p.evaluators.binary_search_by_key(&i, |e| e.0) {
                    Ok(at) => {
                        p.evaluators.remove(at);
                        p.actual = false;
                        Ok(vec![number_value(i)])
                    }
                    Err(at) if at < p.evaluators.len() => Err(format!(
                        "{what}: no evaluator of id {i}, the outcome is unestablished (TODO({REVIEW}))"
                    )),
                    Err(_) => Err(format!(
                        "{what}: no evaluator of id {i}, the outcome is unestablished (TODO({GAP}))"
                    )),
                }
            }
            "evaluator" => {
                let i = id(args, 0, what)?;
                match planner.borrow().evaluator_of(i) {
                    Some(e) => Ok(vec![Value::Object(e)]),
                    None => Err(format!(
                        "{what}: no evaluator of id {i}, the outcome is unestablished (TODO({GAP}))"
                    )),
                }
            }
            "set_goal_world_state" => {
                let goal = (self.maker.get::<WorldState>(arg(args, 0)))
                    .ok_or_else(|| format!("{what}: world_state expected"))?;
                let mut p = planner.borrow_mut();
                p.require_setup(what)?;
                p.set_goal(&goal);
                Ok(Vec::new())
            }
            "current_action_id" => {
                let p = planner.borrow();
                p.require_setup(what)?;
                Ok(vec![number_value(p.current)])
            }
            "current_action" => {
                let a = {
                    let p = planner.borrow();
                    p.require_setup(what)?;
                    p.action_from(p.current, what)?
                };
                Ok(vec![Value::Object(a.object)])
            }
            "initialized" => Ok(vec![planner.borrow().initialized.into()]),
            "actual" => {
                planner.borrow().require_setup(what)?;
                Ok(vec![self.actual(planner)?.into()])
            }
            "clear" => {
                let p = planner.borrow();
                if p.actions.is_empty() && p.evaluators.is_empty() {
                    Ok(Vec::new())
                } else {
                    Err(format!(
                        "{what}: whether the actual flag changes is unestablished (TODO({REVIEW}))"
                    ))
                }
            }
            _ => Err(format!("action planner: unknown operation {op}")),
        }
    }

    /// The actuality test (6.3 step 1).
    fn actual(&self, planner: &Rc<RefCell<Planner>>) -> Result<bool, String> {
        let cached: Vec<(u32, bool)> = {
            let p = planner.borrow();
            if !p.actual {
                return Ok(false);
            }
            p.cached.iter().map(|(c, v)| (*c, *v)).collect()
        };
        for (c, v) in cached {
            if self.evaluate(planner, c)? != v {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Condition `c`'s evaluator's `evaluate` (4), read as a truth value.
    fn evaluate(&self, planner: &Rc<RefCell<Planner>>, c: u32) -> Result<bool, String> {
        let evaluator = planner.borrow().evaluator_of(c).ok_or_else(|| {
            format!("action_planner: no evaluator of condition {c}, the outcome is unestablished (TODO({GAP}))")
        })?;
        Ok(truth(&self.ctx.call(evaluator, "evaluate", &[])?))
    }

    /// `update()`'s default (6.3).
    fn update(&self, planner: &Rc<RefCell<Planner>>) -> Result<(), String> {
        planner.borrow().require_setup("action_planner:update")?;
        if !self.actual(planner)? {
            {
                let mut p = planner.borrow_mut();
                p.actual = true;
                p.cached.clear();
            }
            if let Some(plan) = self.plan(planner)? {
                planner.borrow_mut().plan = plan;
            }
        }
        let first = planner.borrow().plan.first().copied().ok_or_else(|| {
            format!("action_planner:update: the plan is empty, the outcome is unestablished (TODO({GAP}))")
        })?;
        let (initialized, current) = {
            let p = planner.borrow();
            (p.initialized, p.current)
        };
        if !initialized {
            {
                let mut p = planner.borrow_mut();
                p.initialized = true;
                p.current = first;
            }
            let a = planner
                .borrow()
                .action_from(first, "action_planner:update")?;
            self.ctx.call(a.object, "initialize", &[])?;
        } else if current != first {
            let a = planner
                .borrow()
                .action_from(current, "action_planner:update")?;
            self.ctx.call(a.object, "finalize", &[])?;
            planner.borrow_mut().current = first;
            let a = planner
                .borrow()
                .action_from(first, "action_planner:update")?;
            self.ctx.call(a.object, "initialize", &[])?;
        }
        let current = planner.borrow().current;
        let a = planner
            .borrow()
            .action_from(current, "action_planner:update")?;
        self.ctx.call(a.object, "execute", &[])?;
        Ok(())
    }

    /// Plans (7): the search from the current world to the goal.
    fn plan(&self, planner: &Rc<RefCell<Planner>>) -> Result<Option<Vec<u32>>, String> {
        let (goal, actions) = {
            let p = planner.borrow();
            (p.goal.clone(), p.actions.clone())
        };
        let conditions: Vec<(WorldState, WorldState)> = (actions.iter())
            .map(|a| {
                let a = a.action.borrow();
                (a.preconditions.clone(), a.effects.clone())
            })
            .collect();
        let steps: Vec<search::Step> = (actions.iter())
            .zip(&conditions)
            .map(|(a, (pre, eff))| search::Step {
                id: a.id,
                preconditions: pre,
                effects: eff,
            })
            .collect();
        let mut planning = Planning {
            call: self,
            planner,
            actions: &actions,
            overridden: vec![None; actions.len()],
        };
        search::search(&mut planning, &goal, &steps)
    }
}

/// What the search asks of a planner (7.2, 7.3).
struct Planning<'a, 'b> {
    call: &'a Call<'b>,
    planner: &'a Rc<RefCell<Planner>>,
    actions: &'a [HeldAction],
    /// Whether each action's `weight` is a script class's own, once asked.
    overridden: Vec<Option<bool>>,
}

impl search::Planning for Planning<'_, '_> {
    fn world_value(&mut self, condition: u32) -> Result<bool, String> {
        if let Some(v) = self.planner.borrow().cached.get(&condition) {
            return Ok(*v);
        }
        let v = self.call.evaluate(self.planner, condition)?;
        self.planner.borrow_mut().cached.insert(condition, v);
        Ok(v)
    }

    fn weight(&mut self, step: usize, new: &WorldState, from: &WorldState) -> Result<u16, String> {
        let action = &self.actions[step];
        let overridden = match self.overridden[step] {
            Some(o) => o,
            None => {
                let o = self.call.ctx.weight_overridden(action.object)?;
                self.overridden[step] = Some(o);
                o
            }
        };
        if !overridden {
            return Ok(action.action.borrow().weight);
        }
        let s1 = self.call.maker.new_object("world_state", new.clone());
        let s2 = self.call.maker.new_object("world_state", from.clone());
        let result = self.call.ctx.call(action.object, "weight", &[s1, s2])?;
        Ok(weight_of(lua_number(&result)))
    }
}

/// Registers this spec's classes and functions (1.2): after the game
/// object basics (`game_object`, `cef_storage`).
fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    scripts.register_class(world_property_class())?;
    scripts.register_class(world_state_class())?;
    scripts.register_class(property_storage_class())?;
    for class in evaluator_classes() {
        scripts.register_class(class)?;
    }
    scripts.register_class(action_base_class(ctx))?;
    for class in planner_classes() {
        scripts.register_class(class)?;
    }
    scripts.register_class(stalker_ids_class())?;
    let lua = scripts.lua_mut();
    // 10: the store of evaluation functions, spec X 12's.
    let storage = lua.new_engine_object("cef_storage", |_| EvaluationFunctions)?;
    let c = ctx.clone();
    lua.register_object_function("ef_storage", move |_| {
        c.stopped()?;
        Ok(Some(storage))
    })?;
    let c = ctx.clone();
    lua.register_with_objects("__volya_action_planner", move |maker, args| {
        raw(&c, maker, args)
    })?;
    lua.exec_kept(
        "=action_planner",
        include_bytes!("action_planner.lua"),
        ENTRY,
    )
}

fn api(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::Api,
        message: message.into(),
    }
}

impl Scripts {
    /// Registers script API W (1.2): after the game object basics.
    pub fn register_action_planner(&mut self) -> Result<(), Error> {
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let ctx = Context {
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            fatal: self.fatal_flag(),
            roots: Rc::default(),
        };
        register(self, &ctx)?;
        self.action_planner_state = Some(Registered { ctx });
        Ok(())
    }

    fn planner_context(&self) -> Result<Context, Error> {
        (self.action_planner_state.as_ref())
            .map(|r| r.ctx.clone())
            .ok_or_else(|| api("the action planner isn't registered"))
    }

    /// Stalker `id` comes online with handle `handle` (10): its motivation
    /// manager is made, set up with it, holding the engine's own
    /// evaluators and actions of 12.2. What they do is the stalker
    /// planner's (TODO(#1185)): until then each raises a script error
    /// naming it when the engine calls it. Nothing for another object or
    /// when the planner isn't registered.
    pub(crate) fn motivation_manager_comes_online(
        &mut self,
        id: u16,
        handle: EnginePart,
    ) -> Result<(), Error> {
        let Ok(ctx) = self.planner_context() else {
            return Ok(());
        };
        let stalker = (ctx.objects.borrow().get(id)).is_some_and(|o| o.kinds.has(Kinds::STALKER));
        if !stalker {
            return Ok(());
        }
        let mut root = Planner::default();
        root.setup(Some(handle));
        let planner = self
            .lua_mut()
            .new_engine_object("action_planner", |_| PlanPart::planner(root))?;
        let mut parts = Vec::new();
        for condition in MOTIVATION_CONDITIONS {
            let evaluate: EngineEvaluate = Rc::new(move || {
                Err(format!(
                    "the stalker planner's evaluator of condition {condition} isn't implemented yet (TODO({STALKER_PLANNER}))"
                ))
            });
            parts.push(self.add_engine_evaluator(
                planner,
                condition,
                Evaluation::Engine(evaluate),
            )?);
        }
        for (action, preconditions, effect) in MOTIVATION_ACTIONS {
            let step: EngineStep = Rc::new(move |phase| {
                Err(format!(
                    "the stalker planner's action {action} ({phase:?}) isn't implemented yet (TODO({STALKER_PLANNER}))"
                ))
            });
            let preconditions: Vec<WorldProperty> = (preconditions.iter())
                .map(|&(c, v)| WorldProperty::new(c, v))
                .collect();
            let effects = [WorldProperty::new(effect.0, effect.1)];
            parts.push(self.add_engine_action(
                planner,
                action,
                &WorldState::of(&preconditions),
                &WorldState::of(&effects),
                Some(step),
            )?);
        }
        ctx.roots.borrow_mut().insert(id, Root { planner, parts });
        Ok(())
    }

    /// Stalker `id` goes offline or is removed: its motivation manager and
    /// the engine's parts in it are released; the script objects it held go
    /// with it.
    pub(crate) fn motivation_manager_goes_offline(&mut self, id: u16) -> Result<(), Error> {
        let Ok(ctx) = self.planner_context() else {
            return Ok(());
        };
        let Some(root) = ctx.roots.borrow_mut().remove(&id) else {
            return Ok(());
        };
        let lua = self.lua_mut();
        lua.release_engine_object(root.planner)?;
        for part in root.parts {
            lua.release_engine_object(part)?;
        }
        Ok(())
    }

    /// Stalker `id`'s motivation manager (10), an `action_planner`.
    pub fn motivation_manager(&self, id: u16) -> Option<EnginePart> {
        let ctx = self.planner_context().ok()?;
        ctx.roots.borrow().get(&id).map(|r| r.planner)
    }

    /// Runs `f` on the planner (or planner action's inner planner) `part`.
    pub fn with_planner<R>(
        &self,
        part: EnginePart,
        f: impl FnOnce(&mut Planner) -> R,
    ) -> Option<R> {
        let planner = self.with_part(part, |p: &mut PlanPart| p.planner.clone())??;
        let mut p = planner.borrow_mut();
        Some(f(&mut p))
    }

    /// Runs `f` on the action (or planner action) `part`.
    pub fn with_action<R>(&self, part: EnginePart, f: impl FnOnce(&mut Action) -> R) -> Option<R> {
        let action = self.with_part(part, |p: &mut PlanPart| p.action.clone())??;
        let mut a = action.borrow_mut();
        Some(f(&mut a))
    }

    /// Adds an evaluator the engine made itself to planner `planner` under
    /// `id`, set up with the planner's object and store (4, 6.2). Its part.
    pub fn add_engine_evaluator(
        &mut self,
        planner: EnginePart,
        id: u32,
        evaluation: Evaluation,
    ) -> Result<EnginePart, Error> {
        let (object, storage) = self
            .with_planner(planner, |p| (p.object, p.storage.clone()))
            .ok_or_else(|| api("not a planner"))?;
        let part = self
            .lua_mut()
            .new_engine_object("property_evaluator", |_| Evaluator {
                object,
                storage: Some(storage),
                evaluation,
            })?;
        self.with_planner(planner, |p| p.put_evaluator(id, part));
        Ok(part)
    }

    /// Adds an action the engine made itself to planner `planner` under
    /// `id`, with its conditions, set up with the planner's object and
    /// store (5, 6.2). Its part.
    pub fn add_engine_action(
        &mut self,
        planner: EnginePart,
        id: u32,
        preconditions: &WorldState,
        effects: &WorldState,
        engine: Option<EngineStep>,
    ) -> Result<EnginePart, Error> {
        let (object, storage) = self
            .with_planner(planner, |p| (p.object, p.storage.clone()))
            .ok_or_else(|| api("not a planner"))?;
        let mut action = Action::new(object);
        action.storage = Some(storage);
        action.preconditions = preconditions.clone();
        action.effects = effects.clone();
        action.engine = engine;
        let action = Rc::new(RefCell::new(action));
        let held = Rc::clone(&action);
        let part = self
            .lua_mut()
            .new_engine_object("action_base", |_| PlanPart {
                action: Some(held),
                planner: None,
            })?;
        let put = self.with_planner(planner, |p| {
            p.put_action(HeldAction {
                id,
                object: part,
                action,
            })
        });
        put.ok_or_else(|| api("not a planner"))?.map_err(api)?;
        Ok(part)
    }

    /// The engine updates stalker `id`'s motivation manager (12.2): the
    /// goal set first, `property_puzzle_solved` true, or
    /// `property_already_dead` true when its health is at most 0
    /// (believed), with `set_goal_world_state`'s rule; then its update
    /// (6.3). When it runs is the stalker planner's (CH-17).
    pub fn update_motivation_manager(&mut self, id: u16) -> Result<(), Error> {
        let ctx = self.planner_context()?;
        let root = (ctx.roots.borrow().get(&id).map(|r| r.planner))
            .ok_or_else(|| api(format!("object {id} has no motivation manager")))?;
        let dead = (ctx.objects.borrow().get(id))
            .and_then(|o| o.entity.as_ref())
            .is_some_and(|e| e.health <= 0.0);
        let goal = if dead {
            WorldProperty::new(ALREADY_DEAD, true)
        } else {
            WorldProperty::new(PUZZLE_SOLVED, true)
        };
        self.with_planner(root, |p| p.set_goal(&WorldState::of(&[goal])));
        self.guarded(|lua| lua.call_kept(ENTRY, &["update".into(), Value::Object(root)]))
            .map(drop)
    }
}
