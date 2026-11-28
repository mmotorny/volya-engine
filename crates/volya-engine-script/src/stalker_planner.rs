// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The stalker planner (CH-17, docs/behavior/stalker-planner.md; section
//! numbers are its): a stalker's root planner, its motivation manager of
//! script API W (1), with its properties and evaluators (1.2), actions
//! (1.3) and goals (1.4); the inner planners for death, life without a
//! fight and anomalies (2); the engine's own actions in them (3); the think
//! step of the stalker's scheduled update (5); what a save holds of it (6).
//! The scripts reach the root through W's members and add their own parts
//! to it (4); the root's update hands them control by planning (W 6.3).
//!
//! The combat and danger planners are CH-18's and CH-19's: the root holds
//! them as inner planners whose start raises a script error naming their
//! issue. What the engine's actions ask of systems Volya Engine doesn't run
//! yet (sounds, the item handler, sight, motions) has no receiver, named by
//! TODOs; what the spec leaves open raises a script error naming its
//! issue. See docs/architecture/scripts.md, The stalker planner.

use std::cell::RefCell;
use std::rc::Rc;

use crate::action_planner::{
    EngineEvaluate, EngineStep, Evaluation, Phase, PlanPart, Planner, PropertyStorage, RootParts,
    WorldProperty, WorldState,
};
use crate::classes::EnginePart;
use crate::game_object::movement::MovementRequest;
use crate::game_object::{GameObject, GameObjects};
use crate::inventory::Inventories;
use crate::loader::Scripts;
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::simulation::AlifeSimulator;
use crate::{Error, ErrorKind};

/// The combat planner (CH-18).
const COMBAT: &str = "#1227";
/// The danger planner (CH-19).
const DANGER: &str = "#1228";
/// The spec's open outcomes.
const OPEN: &str = "#1229";
/// The drop of a dying stalker's slot items (CH-6, CH-21).
const DROPS: &str = "#1230";
/// The review's marks, which leave their statements unestablished.
const REVIEW: &str = "#1205";
/// Smart-terrain jobs (CH-28): the task of 2.2.
const TASKS: &str = "#1215";
/// Paths and restrictions (CH-9): the position clear of the zones.
const PATHS: &str = "#1130";

/// The properties (conventions, `stalker_ids`).
pub const ALIVE: u32 = 0;
pub const DEAD: u32 = 1;
pub const ALREADY_DEAD: u32 = 2;
pub const ALIFE: u32 = 3;
pub const PUZZLE_SOLVED: u32 = 4;
pub const SMART_TERRAIN_TASK: u32 = 5;
pub const ITEMS: u32 = 6;
pub const ENEMY: u32 = 7;
pub const DANGER_PROPERTY: u32 = 8;
pub const ANOMALY: u32 = 42;
pub const INSIDE_ANOMALY: u32 = 43;

/// The actions (conventions, `stalker_ids`).
pub const ACTION_DEAD: u32 = 0;
pub const ACTION_DYING: u32 = 1;
pub const ACTION_GATHER_ITEMS: u32 = 2;
pub const ACTION_NO_ALIFE: u32 = 3;
pub const ACTION_SMART_TERRAIN_TASK: u32 = 4;
pub const ACTION_SOLVE_ZONE_PUZZLE: u32 = 5;
pub const GET_OUT_OF_ANOMALY: u32 = 10;
pub const DETECT_ANOMALY: u32 = 11;
pub const ACTION_DEATH_PLANNER: u32 = 56;
pub const ACTION_ALIFE_PLANNER: u32 = 57;
pub const ACTION_COMBAT_PLANNER: u32 = 58;
pub const ACTION_ANOMALY_PLANNER: u32 = 59;
pub const ACTION_DANGER_PLANNER: u32 = 60;

/// How long "enemy" stays true after the last update of the enemy choice
/// that had an enemy, ms (1.2).
pub const ENEMY_KEPT_MS: u32 = 3_000;

/// The movement targets of `X` 3 the engine's actions set (3).
const GAME_PATH: i32 = 0;
const LEVEL_PATH: i32 = 1;
const DETAIL_LINE: i32 = 0;
const STANDING: i32 = 1;
const WALK: i32 = 0;
const MENTAL_DANGER: i32 = 0;
const MENTAL_FREE: i32 = 1;

/// The radiation zone's class id (1.2, believed).
const RADIATION_ZONE: &[u8] = b"Z_RADIO";
/// The bolt's class id (3.2).
const BOLT: &[u8] = b"II_BOLT";

/// (property, value) pairs.
type Pairs<'a> = &'a [(u32, bool)];

fn state(pairs: Pairs) -> WorldState {
    let properties: Vec<WorldProperty> = (pairs.iter())
        .map(|&(c, v)| WorldProperty::new(c, v))
        .collect();
    WorldState::of(&properties)
}

/// What the engine's evaluators and actions of one stalker read and write.
#[derive(Clone)]
struct Stalker {
    id: u16,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
    inventories: Rc<RefCell<Inventories>>,
    simulation: Option<Rc<RefCell<AlifeSimulator>>>,
}

impl Stalker {
    fn now(&self) -> u32 {
        self.services.borrow().device.time_global()
    }

    fn with<R>(&self, f: impl FnOnce(&GameObject) -> R) -> Result<R, String> {
        let objects = self.objects.borrow();
        let o = objects
            .get(self.id)
            .ok_or_else(|| format!("the stalker planner: object {} is gone", self.id))?;
        Ok(f(o))
    }

    fn choices(&self) -> Result<crate::game_object::Choices, String> {
        self.with(|o| {
            o.stalker
                .as_ref()
                .map(|s| s.choices.clone())
                .unwrap_or_default()
        })
    }

    /// Its health (CH-5) is above 0 (1.2).
    fn alive(&self) -> Result<bool, String> {
        self.with(|o| o.entity.as_ref().is_some_and(|e| e.health > 0.0))
    }

    /// The enemy rule (1.2).
    fn enemy(&self) -> Result<bool, String> {
        let choices = self.choices()?;
        if choices.enemy.is_some() {
            return Ok(true);
        }
        let now = self.now();
        match choices.enemy_chosen_at {
            Some(at) => Ok(now.wrapping_sub(at) < ENEMY_KEPT_MS),
            // The recorded time before any such update is disputed: it
            // decides only while the clock is below 3,000 ms (from 0).
            None if now < ENEMY_KEPT_MS => Err(format!(
                "the stalker planner's enemy evaluator: the time recorded before the enemy choice first had an enemy is unestablished (TODO({REVIEW}), TODO({OPEN}))"
            )),
            None => Ok(false),
        }
    }

    /// Whether it touches a zone object that is not a radiation zone (1.2,
    /// 2.3).
    fn touches_anomaly(&self) -> Result<bool, String> {
        let objects = self.objects.borrow();
        let o = objects
            .get(self.id)
            .ok_or_else(|| format!("the stalker planner: object {} is gone", self.id))?;
        let touching = o
            .stalker
            .as_ref()
            .map(|s| s.touching.as_slice())
            .unwrap_or_default();
        let radiation = class_id(RADIATION_ZONE);
        Ok(touching.iter().any(|t| {
            objects.get(*t).is_some_and(|z| {
                z.kinds.has(crate::game_object::Kinds::ANOMALOUS_ZONE) && z.class_id != radiation
            })
        }))
    }

    /// In an anomaly, the anomaly rule's condition (1.2): true without a
    /// chosen enemy; with one, the group rating of CH-18 decides.
    fn anomaly_condition(&self, what: &str) -> Result<bool, String> {
        if self.choices()?.enemy.is_none() {
            Ok(true)
        } else {
            Err(format!(
                "the stalker planner's {what} evaluator: in an anomaly with an enemy, the group rating of CH-18 decides (TODO({COMBAT}))"
            ))
        }
    }

    /// The simulation exists (2.2).
    fn simulation_exists(&self) -> bool {
        (self.simulation.as_ref()).is_some_and(|s| s.borrow().simulation.is_some())
    }

    /// Sets movement targets of `X` 3 (3).
    fn request(&self, f: impl FnOnce(&mut MovementRequest)) -> Result<(), String> {
        let mut objects = self.objects.borrow_mut();
        let request = objects
            .get_mut(self.id)
            .and_then(|o| o.movement.stalker.as_mut())
            .map(|p| &mut p.request)
            .ok_or_else(|| format!("the stalker planner: object {} has no movement", self.id))?;
        f(request);
        Ok(())
    }

    /// The common start of 3.3 – 3.5: no desired direction, detail path
    /// line, standing, walking, with `path` and `mental`.
    fn start_targets(&self, path: Option<i32>, mental: i32) -> Result<(), String> {
        self.request(|r| {
            r.desired_direction_set = false;
            if let Some(path) = path {
                r.target.path_type = path;
            }
            r.detail_path_type = DETAIL_LINE;
            r.target.body_state = STANDING;
            r.target.movement_type = WALK;
            r.target.mental_state = mental;
        })
    }

    /// Its slot items, the one in the active slot apart: (active, others),
    /// never a bolt (3.2).
    fn slot_items(&self) -> (Option<u16>, Vec<u16>) {
        let inventories = self.inventories.borrow();
        let Some(owner) = inventories.owner(self.id) else {
            return (None, Vec::new());
        };
        let inventory = &owner.inventory;
        let bolt = class_id(BOLT);
        let objects = self.objects.borrow();
        let is_bolt = |item: u16| objects.get(item).is_some_and(|o| o.class_id == bolt);
        let active = (inventory.active_slot)
            .and_then(|s| inventory.slots.get(usize::from(s)).copied().flatten())
            .filter(|&i| !is_bolt(i));
        let others = (inventory.slots.iter().flatten().copied())
            .filter(|&i| Some(i) != active && !is_bolt(i))
            .collect();
        (active, others)
    }
}

/// 3.1: what every engine action does besides its own step: starting
/// records the clock as its start time (the action's default) and clears
/// a flag of the root that CH-18's actions set (TODO(#1227): no action
/// sets it yet); finishing resets the stalker's motion manager (CH-11,
/// believed its script animation request; TODO(#1129): no motion manager
/// yet).
fn engine_action(step: impl Fn(Phase) -> Result<(), String> + 'static) -> Option<EngineStep> {
    Some(Rc::new(step))
}

fn constant(value: bool) -> Evaluation {
    Evaluation::Constant(value)
}

fn engine(f: impl Fn() -> Result<bool, String> + 'static) -> Evaluation {
    let f: EngineEvaluate = Rc::new(f);
    Evaluation::Engine(f)
}

/// What building a planner makes.
struct Builder<'a> {
    s: &'a mut Scripts,
    parts: Vec<EnginePart>,
}

impl Builder<'_> {
    fn evaluator(&mut self, planner: EnginePart, id: u32, e: Evaluation) -> Result<(), Error> {
        let part = self.s.add_engine_evaluator(planner, id, e)?;
        self.parts.push(part);
        Ok(())
    }

    fn action(
        &mut self,
        planner: EnginePart,
        id: u32,
        pre: Pairs,
        effect: (u32, bool),
        step: Option<EngineStep>,
    ) -> Result<(), Error> {
        let part = (self.s).add_engine_action(planner, id, &state(pre), &state(&[effect]), step)?;
        self.parts.push(part);
        Ok(())
    }

    fn planner(
        &mut self,
        planner: EnginePart,
        id: u32,
        pre: Pairs,
        effect: (u32, bool),
        step: Option<EngineStep>,
    ) -> Result<EnginePart, Error> {
        let part = (self.s).add_engine_planner_action(
            planner,
            id,
            &state(pre),
            &state(&[effect]),
            step,
        )?;
        self.parts.push(part);
        Ok(part)
    }

    fn storage(&self, planner: EnginePart) -> Result<PropertyStorage, Error> {
        (self.s.with_planner(planner, |p| p.storage.clone())).ok_or_else(|| Error {
            kind: ErrorKind::Api,
            message: "not a planner".into(),
        })
    }
}

impl Scripts {
    /// Builds stalker `id`'s root, set up with its handle `handle` (1.1):
    /// the seven evaluators of 1.2, then the six actions of 1.3 (the inner
    /// planners built as they are added, 2), its goal the life goal until
    /// its first update chooses (1.4). The root and every engine part made.
    pub(crate) fn build_root(&mut self, id: u16, handle: EnginePart) -> Result<RootParts, Error> {
        let (objects, services) = self.planner_shared()?;
        let inventories = objects.borrow().inventories();
        let simulation = (self.simulation_state.as_ref()).map(|r| Rc::clone(&r.ctx.state));
        let stalker = Stalker {
            id,
            objects,
            services,
            inventories,
            simulation,
        };
        let root = self.new_engine_planner(Some(handle))?;
        let mut b = Builder {
            s: self,
            parts: Vec::new(),
        };
        let root_storage = b.storage(root)?;
        // 1.2, in their order.
        b.evaluator(root, ALREADY_DEAD, constant(false))?;
        b.evaluator(root, PUZZLE_SOLVED, constant(false))?;
        let st = stalker.clone();
        b.evaluator(root, ALIVE, engine(move || st.alive()))?;
        let st = stalker.clone();
        b.evaluator(root, ENEMY, engine(move || st.enemy()))?;
        let st = stalker.clone();
        b.evaluator(
            root,
            DANGER_PROPERTY,
            engine(move || Ok(st.choices()?.danger)),
        )?;
        let st = stalker.clone();
        let stored = root_storage.clone();
        b.evaluator(
            root,
            ANOMALY,
            engine(move || {
                if st.touches_anomaly()? || stored.get(ANOMALY) == Some(true) {
                    st.anomaly_condition("anomaly")
                } else {
                    Ok(false)
                }
            }),
        )?;
        let st = stalker.clone();
        b.evaluator(
            root,
            ITEMS,
            engine(move || Ok(st.choices()?.item.is_some())),
        )?;
        // 1.3, in their order.
        let death = b.planner(
            root,
            ACTION_DEATH_PLANNER,
            &[(ALIVE, false), (ALREADY_DEAD, false)],
            (ALREADY_DEAD, true),
            None,
        )?;
        build_death_planner(&mut b, death, &stalker)?;
        let alife = b.planner(
            root,
            ACTION_ALIFE_PLANNER,
            &[
                (ALIVE, true),
                (ENEMY, false),
                (ANOMALY, false),
                (DANGER_PROPERTY, false),
                (ITEMS, false),
                (PUZZLE_SOLVED, false),
            ],
            (PUZZLE_SOLVED, true),
            None,
        )?;
        build_alife_planner(&mut b, alife, &stalker)?;
        b.planner(
            root,
            ACTION_COMBAT_PLANNER,
            &[(ALIVE, true), (ANOMALY, false), (ENEMY, true)],
            (ENEMY, false),
            engine_action(|phase| match phase {
                Phase::Initialize => Err(format!(
                    "the stalker's combat planner is CH-18's (TODO({COMBAT}))"
                )),
                _ => Ok(()),
            }),
        )?;
        b.planner(
            root,
            ACTION_DANGER_PLANNER,
            &[
                (ALIVE, true),
                (ENEMY, false),
                (ANOMALY, false),
                (DANGER_PROPERTY, true),
            ],
            (DANGER_PROPERTY, false),
            engine_action(|phase| match phase {
                Phase::Initialize => Err(format!(
                    "the stalker's danger planner is CH-19's (TODO({DANGER}))"
                )),
                _ => Ok(()),
            }),
        )?;
        // 2.3: each update of the anomaly planner, after its general
        // update, copies its stored `anomaly` into the root's set; the
        // planner's own set is known once it is made.
        let own: Rc<RefCell<Option<PropertyStorage>>> = Rc::default();
        let copied = Rc::clone(&own);
        let to_root = root_storage.clone();
        let anomaly = b.planner(
            root,
            ACTION_ANOMALY_PLANNER,
            &[(ALIVE, true), (ANOMALY, true)],
            (ANOMALY, false),
            engine_action(move |phase| {
                if phase == Phase::Execute
                    && let Some(own) = &*copied.borrow()
                    && let Some(v) = own.get(ANOMALY)
                {
                    to_root.set(ANOMALY, v);
                }
                Ok(())
            }),
        )?;
        *own.borrow_mut() = Some(b.storage(anomaly)?);
        build_anomaly_planner(&mut b, anomaly, &stalker, &root_storage)?;
        let st = stalker.clone();
        b.action(
            root,
            ACTION_GATHER_ITEMS,
            &[
                (ALIVE, true),
                (ENEMY, false),
                (ANOMALY, false),
                (DANGER_PROPERTY, false),
                (ITEMS, true),
            ],
            (ITEMS, false),
            engine_action(move |phase| gather_items(&st, phase)),
        )?;
        // 1.4: the life goal until the first update chooses.
        let life = state(&[(PUZZLE_SOLVED, true)]);
        b.s.with_planner(root, |p| p.set_goal(&life));
        Ok((root, b.parts))
    }

    /// The think step of stalker `id` (5.2), or, when scripts have taken
    /// it over (CH-24), the scripted actions instead (5.1 step 3; the
    /// local-control flag believed always set): the root's update (1.4:
    /// the goal choice, then the general update), then, when it is alive,
    /// its movement (CH-10; TODO(#1129): nothing moves yet). Its place in
    /// the scheduled update is after the binder's `update` (5.1 step 2).
    pub fn think_step(&mut self, id: u16) -> Result<(), Error> {
        let (objects, _) = self.planner_shared()?;
        let scripted = (objects.borrow().get(id))
            .and_then(|o| o.scripted.as_ref())
            .is_some_and(|c| c.controlled);
        if scripted {
            return self.run_action_queue(id);
        }
        self.update_motivation_manager(id)
    }

    /// The root's saved state of stalker `id` (6): the end of its saved
    /// state.
    pub fn save_motivation_manager(&self, id: u16) -> Result<Vec<u8>, Error> {
        let root = self.motivation_manager(id).ok_or_else(|| no_root(id))?;
        let mut out = Vec::new();
        self.save_planner(root, &mut out)?;
        Ok(out)
    }

    /// Loads stalker `id`'s root from `bytes` as [`Self::save_motivation_manager`]
    /// wrote it (6): the bytes read. W defines no "loaded" mark, so none
    /// is kept (TODO(#1229)).
    pub fn load_motivation_manager(&self, id: u16, bytes: &[u8]) -> Result<usize, Error> {
        let root = self.motivation_manager(id).ok_or_else(|| no_root(id))?;
        self.load_planner(root, bytes)
    }

    /// A planner's state (6): its evaluators' own state (none of the
    /// engine's or W's evaluators writes any), its actions' own state (an
    /// inner planner writes its planner state the same way; the others
    /// none), then its stored set.
    fn save_planner(&self, planner: EnginePart, out: &mut Vec<u8>) -> Result<(), Error> {
        let (actions, storage) = self.planner_parts(planner)?;
        for action in actions {
            if self.inner_planner(action).is_some() {
                self.save_planner(action, out)?;
            }
        }
        storage.save(out);
        Ok(())
    }

    fn load_planner(&self, planner: EnginePart, bytes: &[u8]) -> Result<usize, Error> {
        let (actions, storage) = self.planner_parts(planner)?;
        let mut at = 0;
        for action in actions {
            if self.inner_planner(action).is_some() {
                at += self.load_planner(action, &bytes[at..])?;
            }
        }
        let read = (bytes.get(at..))
            .and_then(|rest| storage.load(rest))
            .ok_or_else(|| Error {
                kind: ErrorKind::Runtime,
                message: "the stalker planner's saved state ends early".into(),
            })?;
        Ok(at + read)
    }

    /// A planner's actions' parts in id order, and its stored set.
    fn planner_parts(
        &self,
        planner: EnginePart,
    ) -> Result<(Vec<EnginePart>, PropertyStorage), Error> {
        self.with_planner(planner, |p| {
            (
                p.actions.iter().map(|a| a.object).collect(),
                p.storage.clone(),
            )
        })
        .ok_or_else(|| Error {
            kind: ErrorKind::Api,
            message: "not a planner".into(),
        })
    }

    fn inner_planner(&self, part: EnginePart) -> Option<Rc<RefCell<Planner>>> {
        self.with_part(part, |p: &mut PlanPart| p.planner.clone())
            .flatten()
    }
}

fn no_root(id: u16) -> Error {
    Error {
        kind: ErrorKind::Api,
        message: format!("object {id} has no motivation manager"),
    }
}

/// 2.1: building stores dead = false in its own set, then adds its parts.
fn build_death_planner(b: &mut Builder, death: EnginePart, stalker: &Stalker) -> Result<(), Error> {
    let own = b.storage(death)?;
    own.set(DEAD, false);
    b.evaluator(death, ALREADY_DEAD, constant(false))?;
    let stored = own.clone();
    b.evaluator(
        death,
        DEAD,
        engine(move || Ok(stored.get(DEAD) == Some(true))),
    )?;
    let st = stalker.clone();
    b.action(
        death,
        ACTION_DYING,
        &[(DEAD, false)],
        (DEAD, true),
        engine_action(move |phase| dying(&st, &own, phase)),
    )?;
    b.action(
        death,
        ACTION_DEAD,
        &[(DEAD, true)],
        (ALREADY_DEAD, true),
        engine_action(|_| Ok(())),
    )
}

/// 3.2, `action_dying`. Its condition is on the item in the active slot
/// and on a time (unestablished): without an active item it doesn't hold
/// (our reading), so at each run every slot item but the active one is
/// dropped (TODO(#1230): dropping isn't carried out, so a stalker with
/// such items stops here) and dead = true is stored.
fn dying(st: &Stalker, own: &PropertyStorage, phase: Phase) -> Result<(), String> {
    let (active, others) = st.slot_items();
    match phase {
        Phase::Initialize | Phase::Execute if active.is_some() => Err(format!(
            "action_dying: with an item in the active slot, its condition is unestablished (TODO({OPEN}))"
        )),
        Phase::Execute if !others.is_empty() => Err(format!(
            "action_dying: dropping the slot items {others:?} isn't carried out yet (TODO({DROPS}))"
        )),
        Phase::Execute => {
            own.set(DEAD, true);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 2.2: building removes its parts (it has none), then adds them.
fn build_alife_planner(b: &mut Builder, alife: EnginePart, stalker: &Stalker) -> Result<(), Error> {
    b.evaluator(alife, PUZZLE_SOLVED, constant(false))?;
    // True when the simulation exists and the stalker's simulation object
    // has a task (believed a smart terrain's job), CH-28's: nothing assigns
    // one yet, so its field is "none" (TODO(#1215)).
    b.evaluator(alife, SMART_TERRAIN_TASK, engine(|| Ok(false)))?;
    let st = stalker.clone();
    b.evaluator(alife, ALIFE, engine(move || Ok(st.simulation_exists())))?;
    let st = stalker.clone();
    b.action(
        alife,
        ACTION_NO_ALIFE,
        &[(ALIFE, false), (PUZZLE_SOLVED, false)],
        (PUZZLE_SOLVED, true),
        engine_action(move |phase| zone_puzzle(&st, phase, true)),
    )?;
    let st = stalker.clone();
    b.action(
        alife,
        ACTION_SMART_TERRAIN_TASK,
        &[(ALIFE, true), (SMART_TERRAIN_TASK, true)],
        (SMART_TERRAIN_TASK, false),
        engine_action(move |phase| match phase {
            Phase::Initialize => st.start_targets(None, MENTAL_FREE),
            Phase::Execute => Err(format!(
                "action_smart_terrain_task: the task's place is CH-28's (TODO({TASKS}))"
            )),
            Phase::Finalize => Ok(()),
        }),
    )?;
    let st = stalker.clone();
    b.action(
        alife,
        ACTION_SOLVE_ZONE_PUZZLE,
        &[
            (ALIFE, true),
            (SMART_TERRAIN_TASK, false),
            (PUZZLE_SOLVED, false),
        ],
        (PUZZLE_SOLVED, true),
        engine_action(move |phase| zone_puzzle(&st, phase, false)),
    )
}

/// 3.4, `action_no_alife` (`no_alife`) and `action_solve_zone_puzzle`.
/// Starting: no desired direction, game path, detail line, standing,
/// walking, mental state free; `action_no_alife` also drops the desired
/// position. The time each records is not kept: what it changes and its
/// random part are unestablished (TODO(#1229)). Each run: `action_no_alife`
/// plays the humming sound kind (TODO(#1217): no engine sound requests
/// yet); both renew the best weapon and ask the item handler to hold it
/// idle (TODO(#1132): no item handler yet).
fn zone_puzzle(st: &Stalker, phase: Phase, no_alife: bool) -> Result<(), String> {
    if phase == Phase::Initialize {
        st.start_targets(Some(GAME_PATH), MENTAL_FREE)?;
        if no_alife {
            st.request(|r| r.desired_position_set = false)?;
        }
    }
    Ok(())
}

/// 3.3, `action_gather_items`. Starting: no desired direction, level
/// path, detail line, standing, walking, mental state danger; the sounds
/// stopped (TODO(#1217)) and the item handler's request (TODO(#1132)).
/// Each run: with a chosen item, its level vertex and position as the
/// targets; the sight updated (TODO(#1129)). Finishing: the desired
/// position dropped; the sounds reset if alive (TODO(#1217)).
fn gather_items(st: &Stalker, phase: Phase) -> Result<(), String> {
    match phase {
        Phase::Initialize => st.start_targets(Some(LEVEL_PATH), MENTAL_DANGER),
        Phase::Execute => {
            let Some(item) = st.choices()?.item else {
                return Ok(());
            };
            let target = {
                let objects = st.objects.borrow();
                objects.get(item).map(|o| {
                    let p = o.placement[3];
                    (o.level_vertex, [p[0], p[1], p[2]])
                })
            };
            match target {
                Some((vertex, position)) => st.request(|r| {
                    r.dest_level_vertex = Some(vertex);
                    r.desired_position = position;
                    r.desired_position_set = true;
                }),
                None => Ok(()),
            }
        }
        Phase::Finalize => st.request(|r| r.desired_position_set = false),
    }
}

/// 2.3: building stores anomaly = false in its own set and the root's,
/// then adds its parts.
fn build_anomaly_planner(
    b: &mut Builder,
    anomaly: EnginePart,
    stalker: &Stalker,
    root: &PropertyStorage,
) -> Result<(), Error> {
    let own = b.storage(anomaly)?;
    own.set(ANOMALY, false);
    root.set(ANOMALY, false);
    let st = stalker.clone();
    b.evaluator(
        anomaly,
        INSIDE_ANOMALY,
        engine(move || {
            if st.touches_anomaly()? {
                st.anomaly_condition("inside_anomaly")
            } else {
                Ok(false)
            }
        }),
    )?;
    // The anomaly rule over this planner's set; the review has it read the
    // root's, so it decides only where the two agree.
    let st = stalker.clone();
    let (stored, in_root) = (own.clone(), root.clone());
    b.evaluator(
        anomaly,
        ANOMALY,
        engine(move || {
            let (mine, roots) = (stored.get(ANOMALY), in_root.get(ANOMALY));
            if mine != roots {
                return Err(format!(
                    "the anomaly planner's anomaly evaluator: which set it reads is disputed (TODO({REVIEW}))"
                ));
            }
            if st.touches_anomaly()? || mine == Some(true) {
                st.anomaly_condition("anomaly")
            } else {
                Ok(false)
            }
        }),
    )?;
    let st = stalker.clone();
    let stored = own.clone();
    b.action(
        anomaly,
        GET_OUT_OF_ANOMALY,
        &[(INSIDE_ANOMALY, true)],
        (INSIDE_ANOMALY, false),
        engine_action(move |phase| get_out_of_anomaly(&st, &stored, phase)),
    )?;
    b.action(
        anomaly,
        DETECT_ANOMALY,
        &[(INSIDE_ANOMALY, false), (ANOMALY, true)],
        (ANOMALY, false),
        engine_action(|phase| match phase {
            Phase::Initialize => Err(format!(
                "detect_anomaly: the random part of its duration and its further request are unestablished (TODO({OPEN}))"
            )),
            _ => Ok(()),
        }),
    )
}

/// 3.5, `get_out_of_anomaly`. Starting: the sounds stopped
/// (TODO(#1217)); level path, detail line, standing, walking, mental state
/// danger; the item handler's request (TODO(#1132)); anomaly = true stored
/// in the anomaly planner's set. Each run: the same targets and a position
/// clear of the zones it touches, CH-9's.
fn get_out_of_anomaly(st: &Stalker, own: &PropertyStorage, phase: Phase) -> Result<(), String> {
    match phase {
        Phase::Initialize => {
            st.start_targets(Some(LEVEL_PATH), MENTAL_DANGER)?;
            own.set(ANOMALY, true);
            Ok(())
        }
        Phase::Execute => {
            st.start_targets(Some(LEVEL_PATH), MENTAL_DANGER)?;
            Err(format!(
                "get_out_of_anomaly: the position clear of the zones is CH-9's (TODO({PATHS}))"
            ))
        }
        Phase::Finalize => Ok(()),
    }
}
