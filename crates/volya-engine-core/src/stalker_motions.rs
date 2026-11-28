// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Stalker motions (docs/behavior/stalker-motions.md, CH-11; section
//! numbers are its): the motion lists built from a stalker model's motions
//! (2), the stalker's motion update and its five slots (3), the choice of
//! the global (4), head (5), torso (6) and legs (7) motions with the speed
//! hand-over (7.4), and the queue of script animations (8).
//!
//! The slots start their cycles on the stalker's [`AnimatedModel`]
//! (docs/behavior/motion-playback.md, TC1) with their own end
//! notifications ([`Slot`]'s values), which the caller hands back by
//! leaving them on the model: [`StalkerMotions::take_script_animation_end`]
//! takes them at the start of each update. The states the choice reads
//! (the body, movement and mental states, the yaws, the item in hand, the
//! sounds) are the caller's ([`StalkerState`]); what the update writes
//! back to the movement is its [`Outcome`].

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6, PI};
use std::sync::Arc;

use crate::motion_playback::{
    AnimatedModel, Clock, Fatal, MotionId, Motions, Notified, PARTITIONS, PlaybackId, Start,
};

/// The body prefixes, for body states 0, 1, 2 (1, 2.1).
pub const BODY_PREFIXES: [&str; 3] = ["cr_", "norm_", "dmg_norm_"];
/// The item slots `0_` … `10_` (2.1).
pub const ITEM_SLOTS: usize = 11;
/// The torso actions (2.1), in order.
pub const TORSO_ACTIONS: [&str; 15] = [
    "draw_",
    "attack_",
    "drop_",
    "holster_",
    "reload_",
    "pick_",
    "aim_",
    "walk_",
    "run_",
    "idle_",
    "prepare_",
    "strap_",
    "unstrap_",
    "look_beack_ls_",
    "look_beack_rs_",
];
/// The movement types and directions (2.1).
pub const MOVEMENT_TYPES: [&str; 2] = ["walk_", "run_"];
pub const DIRECTIONS: [&str; 4] = ["fwd_", "back_", "ls_", "rs_"];
/// The in-place names (2.1), each one motion.
pub const IN_PLACE: [&str; 10] = [
    "idle_0",
    "idle_1",
    "turn_right_0",
    "turn_left_0",
    "turn_right_1",
    "turn_left_1",
    "jump_begin",
    "jump_idle",
    "jump_end",
    "jump_end_1",
];
/// The head motions (2.1).
pub const HEAD: [&str; 2] = ["head_idle_0", "head_talk_0"];

/// The torso actions' indices in [`TORSO_ACTIONS`].
const DRAW: usize = 0;
const ATTACK: usize = 1;
const HOLSTER: usize = 3;
const RELOAD: usize = 4;
const AIM: usize = 6;
const TORSO_WALK: usize = 7;
const TORSO_RUN: usize = 8;
const IDLE: usize = 9;
const STRAP: usize = 11;
const UNSTRAP: usize = 12;
const LOOK_BACK_LS: usize = 13;

/// The global list of `escape_` (2.1).
const ESCAPE: usize = 1;
/// How many global lists there are (2.1).
const GLOBAL_LISTS: usize = 23;

/// The in-place names' indices.
const IDLE_0: usize = 0;
const IDLE_1: usize = 1;
const TURN_RIGHT_0: usize = 2;
const TURN_LEFT_0: usize = 3;
const TURN_RIGHT_1: usize = 4;
const TURN_LEFT_1: usize = 5;

/// The body states, movement types and mental states (1).
pub const CROUCHED: u8 = 0;
pub const STANDING: u8 = 1;
pub const WALK: u8 = 0;
pub const RUN: u8 = 1;
pub const STAND: u8 = 2;
pub const DANGER: u8 = 0;
pub const FREE: u8 = 1;
pub const PANIC: u8 = 2;

/// The legs directions (7.2).
const FORWARD: u8 = 0;
const BACK: u8 = 1;

/// The speed table's entries (7): 19 speeds.
pub const SPEEDS: usize = 19;

/// A motion's flags bit 3 (TC1 5.3), which the time carry reads (3.6).
const TIME_CARRY: u16 = 0x8;

/// The look-back's pause after a torso motion ends, ms (6.6).
const LOOK_BACK_PAUSE: u32 = 2_000;
/// The body direction's wait, ms (7.2).
const DIRECTION_WAIT: u32 = 500;

/// The global pseudo-random generator's largest draw (conventions).
const DRAW_MAX: f32 = 32_767.0;

/// The global global-name list (2.1): `damage_` (0), `escape_` (1),
/// `dead_stop_` (2), `hello_` (3), the critical hits of groups 1 – 3
/// (4 – 21), `panic_stand_` (22).
pub fn global_names() -> Vec<String> {
    let mut names: Vec<String> = ["damage_", "escape_", "dead_stop_", "hello_"]
        .map(String::from)
        .to_vec();
    let parts = [
        "head_",
        "torso_",
        "hend_left_",
        "hend_right_",
        "legs_left_",
        "legs_right_",
    ];
    for group in 1..=3 {
        for part in parts {
            names.push(format!("{group}_critical_hit_{part}"));
        }
    }
    names.push("panic_stand_".into());
    names
}

/// The list of base name `base` (2.2): `base` + *i* for *i* = 0, 1, …,
/// each a cycle or else an fx motion; a missing one skipped below 10, the
/// end from 10 on.
fn list(motions: &Motions, base: &str) -> Vec<MotionId> {
    let mut found = Vec::new();
    for i in 0.. {
        let name = format!("{base}{i}");
        match motions.cycle(&name).or_else(|| motions.fx(&name)) {
            Some(m) => found.push(m),
            None if i > 9 => break,
            None => {}
        }
    }
    found
}

/// The motion lists of a stalker model (2.1, 2.2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MotionLists {
    /// By body prefix, then global name.
    pub global: Vec<Vec<Vec<MotionId>>>,
    /// By body prefix, item slot, torso action.
    pub torso: Vec<Vec<Vec<Vec<MotionId>>>>,
    /// By body prefix, movement type, direction.
    pub movement: Vec<Vec<Vec<Vec<MotionId>>>>,
    /// By body prefix, in-place name.
    pub in_place: Vec<Vec<Option<MotionId>>>,
    pub head: [Option<MotionId>; 2],
    /// `item_` + slot + action, by slot and action (built, not read: 2.2).
    pub item: Vec<Vec<Vec<MotionId>>>,
}

impl MotionLists {
    /// The lists of a model with `motions` (2.1, 2.2).
    pub fn build(motions: &Motions) -> Self {
        let globals = global_names();
        let cycle = |name: &str| motions.cycle(name);
        let per_prefix = |p: &str| {
            let global = globals.iter().map(|g| list(motions, &format!("{p}{g}")));
            let torso = (0..ITEM_SLOTS).map(|s| {
                (TORSO_ACTIONS.iter())
                    .map(|a| list(motions, &format!("{p}torso_{s}_{a}")))
                    .collect()
            });
            let movement = MOVEMENT_TYPES.iter().map(|t| {
                (DIRECTIONS.iter())
                    .map(|d| list(motions, &format!("{p}{t}{d}")))
                    .collect()
            });
            let in_place = IN_PLACE.iter().map(|n| cycle(&format!("{p}{n}")));
            (
                global.collect(),
                torso.collect(),
                movement.collect(),
                in_place.collect(),
            )
        };
        let mut lists = MotionLists {
            head: HEAD.map(cycle),
            item: (0..ITEM_SLOTS)
                .map(|s| {
                    (TORSO_ACTIONS.iter())
                        .map(|a| list(motions, &format!("item_{s}_{a}")))
                        .collect()
                })
                .collect(),
            ..Default::default()
        };
        for p in BODY_PREFIXES {
            let (global, torso, movement, in_place) = per_prefix(p);
            lists.global.push(global);
            lists.torso.push(torso);
            lists.movement.push(movement);
            lists.in_place.push(in_place);
        }
        lists
    }
}

/// A stalker's model as its motion state takes it at the reload (3.1):
/// its motions and their lists. Two are equal when they share both.
#[derive(Clone, Debug)]
pub struct StalkerModel {
    pub motions: Arc<Motions>,
    pub lists: Arc<MotionLists>,
}

impl PartialEq for StalkerModel {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.motions, &other.motions) && Arc::ptr_eq(&self.lists, &other.lists)
    }
}

impl StalkerModel {
    /// The model of `motions` with its lists built. Sharing the lists of
    /// models with the same motion files (2.2) is the caller's, with
    /// [`StalkerModel::with_lists`].
    pub fn new(motions: Arc<Motions>) -> Self {
        let lists = Arc::new(MotionLists::build(&motions));
        StalkerModel { motions, lists }
    }

    pub fn with_lists(motions: Arc<Motions>, lists: Arc<MotionLists>) -> Self {
        StalkerModel { motions, lists }
    }
}

/// The five slots (1, 3), in their notification values' order: the end
/// notification of a slot's playbacks is its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Global = 0,
    Head = 1,
    Torso = 2,
    Legs = 3,
    Script = 4,
}

impl Slot {
    const ALL: [Slot; 5] = [
        Slot::Global,
        Slot::Head,
        Slot::Torso,
        Slot::Legs,
        Slot::Script,
    ];

    /// The slot of end notification `value`.
    pub fn of_notification(value: u32) -> Option<Slot> {
        Slot::ALL.get(value as usize).copied()
    }

    /// The step slots (3.4).
    fn steps(self) -> bool {
        matches!(self, Slot::Global | Slot::Legs | Slot::Script)
    }

    /// Whether its motion starts on the partition mask (3.4).
    fn on_mask(self) -> bool {
        matches!(self, Slot::Global | Slot::Script)
    }
}

/// A list a slot chooses from (4: "choosing from a list").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListKey {
    Global {
        prefix: usize,
        index: usize,
    },
    Torso {
        prefix: usize,
        slot: usize,
        action: usize,
    },
}

/// A slot's motion, its playback and whether it is current (3.4), and the
/// list it last chose from with the motion chosen (4).
#[derive(Clone, Debug, Default, PartialEq)]
struct SlotState {
    motion: Option<MotionId>,
    playback: Option<PlaybackId>,
    current: bool,
    memory: Option<(ListKey, MotionId)>,
}

impl SlotState {
    /// Emptied: no motion, no playback. The disputed mark (3.1) on an
    /// emptied slot is left out: either way the next motion chosen starts.
    fn empty(&mut self) {
        self.motion = None;
        self.playback = None;
        self.current = false;
    }

    /// `motion` chosen (3.4): current only if it was and holds it.
    fn choose(&mut self, motion: Option<MotionId>) {
        self.current = self.current && self.motion == motion;
        self.motion = motion;
    }
}

/// An entry of the script queue (8.1): its motion (`None` for an object
/// the engine gave no model, whose names are kept unplayed) and flags.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptAnimation {
    pub name: Vec<u8>,
    pub motion: Option<MotionId>,
    pub a: bool,
    pub b: bool,
}

/// The item in the stalker's active inventory slot (6.2; its states are
/// CH-20's).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ActiveItem {
    /// No active slot, or nothing in it.
    #[default]
    None,
    Weapon {
        /// As `weapon_strapped()` reports it (X 10.2).
        strapped: bool,
        /// Its animation slot (`animation_slot()`, X 10.3).
        slot: usize,
        state: u32,
        /// Its reload kind (6.3).
        reload_kind: u32,
    },
    Missile {
        slot: usize,
        state: u32,
    },
    Other,
}

/// A critical wound (4, CH-21): the wounded body-part value *k* and the
/// stalker's critical wound weights.
#[derive(Clone, Debug, PartialEq)]
pub struct CriticalWound {
    pub value: usize,
    pub weights: Vec<f32>,
}

/// What the update reads of the stalker (1, 4 – 7): its states as its
/// movement keeps them (CH-10), the item in hand and the object handler
/// (CH-20), a critical wound (CH-21), its sounds (CH-25).
#[derive(Clone, Debug, PartialEq)]
pub struct StalkerState {
    /// *b*, *t*, *m* (1).
    pub body_state: u8,
    pub movement_type: u8,
    pub mental_state: u8,
    /// *v*.
    pub speed: f32,
    /// θ_b, θ_b*, θ_h, θ_h*, ψ.
    pub body_yaw: f32,
    pub body_target_yaw: f32,
    pub head_yaw: f32,
    pub head_target_yaw: f32,
    pub path_yaw: f32,
    /// The sight's turning flag (7.3).
    pub sight_turning: bool,
    pub item: ActiveItem,
    /// The object handler's state number (X 10.2's numbers).
    pub object_handler_state: u32,
    pub critical_wound: Option<CriticalWound>,
    /// At least one of its sounds plays and none of
    /// `sound_running_in_danger` plays or is due (5).
    pub talking: bool,
    /// The movement's speed table (CH-10).
    pub speeds: [f32; SPEEDS],
}

impl Default for StalkerState {
    fn default() -> Self {
        StalkerState {
            body_state: STANDING,
            movement_type: STAND,
            mental_state: FREE,
            speed: 0.0,
            body_yaw: 0.0,
            body_target_yaw: 0.0,
            head_yaw: 0.0,
            head_target_yaw: 0.0,
            path_yaw: 0.0,
            sight_turning: false,
            item: ActiveItem::None,
            object_handler_state: 0,
            critical_wound: None,
            talking: false,
            speeds: [0.0; SPEEDS],
        }
    }
}

/// What an update gives the movement and the steps back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// θ_b* as the legs set it (7.2, 7.3), when they did.
    pub body_target_yaw: Option<f32>,
    /// The movement's speed *v*′ (7.4 step 4), when written.
    pub speed: Option<f32>,
    /// The motions step slots started, in order: the steps start with each
    /// (3.4, 9; the last one counts).
    pub steps: Vec<MotionId>,
}

/// wrap(*x*): *x* in [−π, π] (conventions).
pub fn wrap(x: f32) -> f32 {
    let mut x = x % (2.0 * PI);
    if x > PI {
        x -= 2.0 * PI;
    } else if x < -PI {
        x += 2.0 * PI;
    }
    x
}

/// The direction of δ = wrap(`a` − `b`) (7.2): 0 forward, 1 back, 2 and 3
/// the sides.
fn direction(a: f32, b: f32) -> u8 {
    let delta = wrap(a - b);
    if delta.abs() <= FRAC_PI_4 {
        FORWARD
    } else if delta.abs() > 3.0 * FRAC_PI_4 {
        BACK
    } else if (a - b).sin() <= 0.0 {
        2
    } else {
        3
    }
}

/// Whether directions `x` and `y` are opposite (7.2).
fn opposite(x: u8, y: u8) -> bool {
    matches!((x.min(y), x.max(y)), (0, 1) | (2, 3))
}

/// A stalker's motion state (3): its model's lists, its five slots, the
/// script queue and what the choice keeps between updates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StalkerMotions {
    model: Option<StalkerModel>,
    /// The partitions global and script motions start on (3.1).
    mask: Vec<u16>,
    /// `crouch_type` of its specific character (10).
    crouch_preference: i32,
    slots: [SlotState; 5],
    queue: Vec<ScriptAnimation>,
    /// A script animation ended since the last update (8.3).
    script_ended: bool,
    /// The look-back side kept (6.6: 0 `ls`, 1 `rs`) and time.
    look_back: Option<usize>,
    look_back_time: u32,
    /// The body direction *e*, the pending direction *p* and their time
    /// (7.2).
    body_direction: u8,
    pending_direction: u8,
    direction_time: u32,
    /// The previous legs direction (7.2).
    legs_direction: u8,
    /// The crouch type chosen since the stalker last moved (7.3).
    crouch: Option<i32>,
    /// *s*₀ and *s*₁ (7).
    kept_speed: f32,
    target_speed: f32,
}

impl StalkerMotions {
    pub fn new() -> Self {
        Self::default()
    }

    /// The reload at the stalker's spawning (3.1): `model`'s lists taken,
    /// the crouch type preference copied, and, when `alive` (health above
    /// 0), the partition mask made: every partition but `head_idle_0`'s.
    /// The slots start empty.
    // The reload's further condition of the stalker is
    // unestablished (3.1); none here.
    pub fn reload(&mut self, model: Option<StalkerModel>, alive: bool, crouch_preference: i32) {
        self.mask.clear();
        if alive && let Some(m) = &model {
            let motions = &m.motions;
            let head = m.lists.head[0].map(|h| motions.definition(h).bone_or_partition);
            let count = motions.partition_count().min(PARTITIONS) as u16;
            self.mask = (0..count).filter(|&p| Some(p) != head).collect();
        }
        self.model = model;
        self.crouch_preference = crouch_preference;
        self.slots = Default::default();
    }

    /// The model taken at the reload.
    pub fn model(&self) -> Option<&StalkerModel> {
        self.model.as_ref()
    }

    /// The partition mask (3.1).
    pub fn mask(&self) -> &[u16] {
        &self.mask
    }

    /// `add_animation(name, a, b)` (8.1): `name` looked up as a cycle of
    /// the model and appended; `false` (the diagnostic, nothing appended)
    /// when the model lacks it. Without a model the name is kept unplayed.
    pub fn add_animation(&mut self, name: &[u8], a: bool, b: bool) -> bool {
        let motion = match &self.model {
            Some(m) => {
                let found = std::str::from_utf8(name)
                    .ok()
                    .and_then(|n| m.motions.cycle(n));
                if found.is_none() {
                    return false;
                }
                found
            }
            None => None,
        };
        self.queue.push(ScriptAnimation {
            name: name.to_vec(),
            motion,
            a,
            b,
        });
        true
    }

    /// `clear_animations()` (8.1): the queue and the script slot emptied.
    pub fn clear_animations(&mut self) {
        self.queue.clear();
        self.slots[Slot::Script as usize].empty();
    }

    /// `animation_count()` (8.1).
    pub fn animation_count(&self) -> usize {
        self.queue.len()
    }

    /// The queue (8.1), front first.
    pub fn queue(&self) -> &[ScriptAnimation] {
        &self.queue
    }

    /// The motion slot `slot` holds and its playback.
    pub fn slot(&self, slot: Slot) -> (Option<MotionId>, Option<PlaybackId>) {
        let s = &self.slots[slot as usize];
        (s.motion, s.playback)
    }

    /// The end notification of `slot` (3.5): the slot no longer current;
    /// the torso's ends a look-back (6.6), the script slot's is 8.3.
    pub fn slot_ended(&mut self, slot: Slot, now: u32) {
        let s = &mut self.slots[slot as usize];
        s.current = false;
        match slot {
            Slot::Torso if self.look_back.is_some() => {
                self.look_back = None;
                self.look_back_time = now.wrapping_add(LOOK_BACK_PAUSE);
            }
            Slot::Script => {
                let front = self.queue.first().and_then(|e| e.motion);
                if s.motion.is_some() && front == s.motion {
                    self.queue.remove(0);
                    self.slots[Slot::Script as usize].empty();
                }
                self.script_ended = true;
            }
            _ => {}
        }
    }

    /// Hands the ends `model` reached to their slots (3.5).
    fn take_notifications(&mut self, model: &mut AnimatedModel, now: u32) {
        for Notified { value, .. } in model.take_notifications() {
            if let Some(slot) = Slot::of_notification(value) {
                self.slot_ended(slot, now);
            }
        }
    }

    /// 3.3 step 1: the ends reached on `model` since the last update are
    /// handed to their slots; whether a script animation ended, for the
    /// caller to call callback 29 (`script_animation`) before
    /// [`StalkerMotions::update`]. The mark is cleared.
    pub fn take_script_animation_end(&mut self, model: &mut AnimatedModel, now: u32) -> bool {
        self.take_notifications(model, now);
        std::mem::take(&mut self.script_ended)
    }

    /// 3.3 steps 2 – 4 of a stalker whose health is above 0 (3.2): the
    /// tracks update when the queue is not empty and `callback_set`
    /// (callback 29 has a function; no functions are registered on the
    /// slots, 3.5), then the script mode or the normal selection. `draw`
    /// is the global generator (0 … 32,767).
    pub fn update(
        &mut self,
        model: &mut AnimatedModel,
        state: &StalkerState,
        callback_set: bool,
        clock: Clock,
        draw: &mut dyn FnMut() -> u32,
    ) -> Result<Outcome, Fatal> {
        let mut out = Outcome::default();
        let now = clock.time_global;
        if !self.queue.is_empty() && callback_set {
            model.update_tracks(clock);
            self.take_notifications(model, now);
        }
        if !self.queue.is_empty() {
            // 3: script mode.
            for s in [Slot::Global, Slot::Torso, Slot::Legs] {
                self.slots[s as usize].empty();
            }
            let front = self.queue[0].motion;
            self.slots[Slot::Script as usize].choose(front);
            self.play(Slot::Script, model, clock, &mut out)?;
            self.head(model, state, clock, &mut out)?;
            return Ok(out);
        }
        // 4: the normal selection.
        self.slots[Slot::Script as usize].empty();
        let global = self.global(state, draw);
        if global.is_some() {
            for s in [Slot::Torso, Slot::Legs] {
                self.slots[s as usize].empty();
            }
            self.slots[Slot::Global as usize].choose(global);
            self.play(Slot::Global, model, clock, &mut out)?;
            self.head(model, state, clock, &mut out)?;
            return Ok(out);
        }
        self.slots[Slot::Global as usize].empty();
        self.head(model, state, clock, &mut out)?;
        let torso = self.torso(state, now, draw);
        self.slots[Slot::Torso as usize].choose(torso);
        self.play(Slot::Torso, model, clock, &mut out)?;
        self.legs(model, state, clock, draw, &mut out)?;
        self.time_carry(model);
        Ok(out)
    }

    /// Plays `slot`'s motion (3.4): a slot not current starts it and
    /// becomes current.
    fn play(
        &mut self,
        slot: Slot,
        model: &mut AnimatedModel,
        clock: Clock,
        out: &mut Outcome,
    ) -> Result<(), Fatal> {
        let s = &self.slots[slot as usize];
        if s.current {
            return Ok(());
        }
        let Some(motion) = s.motion else {
            return Ok(());
        };
        let notification = Some(slot as u32);
        let playback = if slot.on_mask() {
            let mut kept = None;
            for &p in &self.mask {
                let start = Start {
                    partition: Some(p),
                    notification: if kept.is_none() { notification } else { None },
                    ..Start::mixing(None)
                };
                let started = model.play_cycle(motion, start, clock)?;
                if kept.is_none() {
                    kept = started;
                }
            }
            kept
        } else {
            model.play_cycle(motion, Start::mixing(notification), clock)?
        };
        let s = &mut self.slots[slot as usize];
        s.playback = playback;
        s.current = true;
        if s.memory.is_some_and(|(_, m)| m != motion) {
            s.memory = None;
        }
        if slot.steps() {
            out.steps.push(motion);
        }
        Ok(())
    }

    /// The prefix index of body state *b* (1).
    fn prefix(state: &StalkerState) -> usize {
        usize::from(state.body_state).min(BODY_PREFIXES.len() - 1)
    }

    fn lists(&self) -> Option<&MotionLists> {
        self.model.as_ref().map(|m| &*m.lists)
    }

    fn list(&self, key: ListKey) -> &[MotionId] {
        let Some(lists) = self.lists() else {
            return &[];
        };
        let found = match key {
            ListKey::Global { prefix, index } => {
                lists.global.get(prefix).and_then(|g| g.get(index))
            }
            ListKey::Torso {
                prefix,
                slot,
                action,
            } => (lists.torso.get(prefix))
                .and_then(|t| t.get(slot))
                .and_then(|t| t.get(action)),
        };
        found.map_or(&[], Vec::as_slice)
    }

    /// A motion of list `key` for `slot` (4): the remembered one when the
    /// list is the remembered one, otherwise one drawn (with `weights`,
    /// the first element whose running sum reaches *r* · Σ*w* / 32,767).
    /// An empty list gives none (what plays is unestablished, 2.3).
    fn choose_from(
        &mut self,
        slot: Slot,
        key: ListKey,
        weights: &[f32],
        draw: &mut dyn FnMut() -> u32,
    ) -> Option<MotionId> {
        if let Some((k, m)) = self.slots[slot as usize].memory
            && k == key
        {
            return Some(m);
        }
        let list = self.list(key);
        if list.is_empty() {
            return None;
        }
        let r = draw();
        let n = weights.len().min(list.len());
        let chosen = if weights.is_empty() {
            list[r as usize % list.len()]
        } else {
            let total: f32 = weights[..n].iter().sum();
            let goal = r as f32 * total / DRAW_MAX;
            let mut sum = 0.0;
            let k = (weights[..n].iter())
                .position(|w| {
                    sum += w;
                    sum >= goal
                })
                .unwrap_or(n - 1);
            list[k]
        };
        self.slots[slot as usize].memory = Some((key, chosen));
        Some(chosen)
    }

    /// 4: the global slot's motion.
    fn global(&mut self, state: &StalkerState, draw: &mut dyn FnMut() -> u32) -> Option<MotionId> {
        if state.mental_state == PANIC {
            if state.speed < 1e-7 {
                return None;
            }
            let key = ListKey::Global {
                prefix: Self::prefix(state),
                index: ESCAPE,
            };
            return self.choose_from(Slot::Global, key, &[], draw);
        }
        if let Some(wound) = &state.critical_wound {
            if let Some(m) = self.slots[Slot::Global as usize].motion {
                return Some(m);
            }
            // With no active item *w* is unestablished (4): no motion.
            let w = match state.item {
                ActiveItem::Weapon { slot, .. } | ActiveItem::Missile { slot, .. } => slot,
                _ => return None,
            };
            let index = (wound.value + 6 * w).checked_sub(6)?;
            let key = ListKey::Global {
                prefix: usize::from(STANDING),
                index,
            };
            if index >= GLOBAL_LISTS {
                return None;
            }
            let weights = wound.weights.clone();
            return self.choose_from(Slot::Global, key, &weights, draw);
        }
        None
    }

    /// 5: the head slot chosen and played.
    fn head(
        &mut self,
        model: &mut AnimatedModel,
        state: &StalkerState,
        clock: Clock,
        out: &mut Outcome,
    ) -> Result<(), Fatal> {
        let head = self
            .lists()
            .and_then(|l| l.head[usize::from(state.talking)]);
        self.slots[Slot::Head as usize].choose(head);
        self.play(Slot::Head, model, clock, out)
    }

    /// T(*s*, action, *k*) of `prefix` (2.1); none past a list's end
    /// (unestablished, 2.3).
    fn element(&self, prefix: usize, slot: usize, action: usize, k: usize) -> Option<MotionId> {
        let key = ListKey::Torso {
            prefix,
            slot,
            action,
        };
        self.list(key).get(k).copied()
    }

    fn torso_key(state: &StalkerState, slot: usize, action: usize) -> ListKey {
        ListKey::Torso {
            prefix: Self::prefix(state),
            slot,
            action,
        }
    }

    /// Whether the stalker is still (1).
    fn still(state: &StalkerState) -> bool {
        state.speed < 0.001 || state.movement_type == STAND
    }

    /// The default rule R(*s*) (6.1).
    fn default_rule(&self, state: &StalkerState, s: usize) -> Option<MotionId> {
        let p = Self::prefix(state);
        let still = Self::still(state);
        if state.mental_state == FREE {
            let action = match (still, state.movement_type) {
                (true, _) => IDLE,
                (false, WALK) => TORSO_WALK,
                _ => TORSO_RUN,
            };
            self.element(p, s, action, 1)
        } else {
            let k = match (still, state.movement_type) {
                (true, _) => 0,
                (false, WALK) => 2,
                _ => 3,
            };
            self.element(p, s, AIM, k)
        }
    }

    /// The look-back (6.6) of slot *s*, element `k`: none unless *b* = 1
    /// and *s* = 2, or when no side is kept.
    fn look_back(
        &mut self,
        state: &StalkerState,
        s: usize,
        k: usize,
        now: u32,
        draw: &mut dyn FnMut() -> u32,
    ) -> Option<Option<MotionId>> {
        if state.body_state != STANDING || s != 2 {
            return None;
        }
        if self.look_back.is_none() && self.legs_direction == BACK && now >= self.look_back_time {
            self.look_back = Some(draw() as usize % 2);
        }
        let side = self.look_back?;
        Some(self.element(Self::prefix(state), s, LOOK_BACK_LS + side, k))
    }

    /// The rows of 6.3 (states 1, 2) and 6.5 (9, 10, …) of `action`:
    /// still element 0; walking the look-back's element 1, else element 0;
    /// running the look-back's element 0, else element 3.
    fn moving_rule(
        &mut self,
        state: &StalkerState,
        s: usize,
        action: usize,
        now: u32,
        draw: &mut dyn FnMut() -> u32,
    ) -> Option<MotionId> {
        let p = Self::prefix(state);
        if Self::still(state) {
            return self.element(p, s, action, 0);
        }
        let (k, fallback) = if state.movement_type == WALK {
            (1, 0)
        } else {
            (0, 3)
        };
        match self.look_back(state, s, k, now, draw) {
            Some(m) => m,
            None => self.element(p, s, action, fallback),
        }
    }

    /// The object-handler rule O(*s*) (6.5).
    fn object_handler_rule(
        &mut self,
        state: &StalkerState,
        s: usize,
        now: u32,
        draw: &mut dyn FnMut() -> u32,
    ) -> Option<MotionId> {
        let norm = usize::from(STANDING);
        match state.object_handler_state {
            n @ (3 | 4) => self.element(norm, s, STRAP, (n - 3) as usize),
            n @ (5 | 6) => self.element(norm, s, UNSTRAP, (n - 5) as usize),
            9 | 10 | 17 | 18 | 21 | 22 | 23 | 24 => self.moving_rule(state, s, AIM, now, draw),
            _ => self.default_rule(state, s),
        }
    }

    /// 6: the torso slot's motion.
    fn torso(
        &mut self,
        state: &StalkerState,
        now: u32,
        draw: &mut dyn FnMut() -> u32,
    ) -> Option<MotionId> {
        let p = Self::prefix(state);
        match state.item {
            ActiveItem::None => self.default_rule(state, 0),
            ActiveItem::Weapon { strapped: true, .. } => self.default_rule(state, 0),
            ActiveItem::Weapon {
                slot: s,
                state: n,
                reload_kind,
                ..
            } => match n {
                1 | 2 => self.moving_rule(state, s, ATTACK, now, draw),
                3 => self.element(p, s, RELOAD, reload_kind.min(2) as usize),
                4 => self.choose_from(Slot::Torso, Self::torso_key(state, s, DRAW), &[], draw),
                5 => self.choose_from(Slot::Torso, Self::torso_key(state, s, HOLSTER), &[], draw),
                6 => self.default_rule(state, 0),
                _ => self.object_handler_rule(state, s, now, draw),
            },
            ActiveItem::Missile { slot, state: n } => {
                let s = if state.body_state == CROUCHED {
                    0
                } else {
                    slot
                };
                let mut choose = |this: &mut Self, action| {
                    this.choose_from(Slot::Torso, Self::torso_key(state, s, action), &[], draw)
                };
                match n {
                    1 => choose(self, DRAW),
                    3 => self.element(p, s, ATTACK, 0),
                    4 => self.element(p, s, ATTACK, 1),
                    5 | 6 | 9 => self.element(p, s, ATTACK, 2),
                    8 => choose(self, HOLSTER),
                    _ => choose(self, AIM),
                }
            }
            ActiveItem::Other => self.object_handler_rule(state, 0, now, draw),
        }
    }

    /// The speed table's entry `i` (7).
    fn speed(state: &StalkerState, i: usize) -> f32 {
        state.speeds.get(i).copied().unwrap_or(0.0)
    }

    /// 7: the legs slot chosen and played, with the speed hand-over (7.4).
    fn legs(
        &mut self,
        model: &mut AnimatedModel,
        state: &StalkerState,
        clock: Clock,
        draw: &mut dyn FnMut() -> u32,
        out: &mut Outcome,
    ) -> Result<(), Fatal> {
        let now = clock.time_global;
        let p = Self::prefix(state);
        let lists = self.model.as_ref().map(|m| Arc::clone(&m.lists));
        let movement = |t: u8, d: u8, k: usize| {
            (lists.as_ref())
                .and_then(|l| l.movement.get(p))
                .and_then(|l| l.get(usize::from(t)))
                .and_then(|l| l.get(usize::from(d)))
                .and_then(|l| l.get(k).copied())
        };
        let in_place = |i: usize| {
            (lists.as_ref())
                .and_then(|l| l.in_place.get(p))
                .and_then(|l| l.get(i).copied().flatten())
        };
        let t = state.movement_type;
        let chosen = if !Self::still(state) {
            self.crouch = None;
            if state.mental_state != DANGER {
                // 7.1.
                self.target_speed = if state.mental_state == FREE {
                    Self::speed(state, 16 + usize::from(t))
                } else {
                    Self::speed(state, 18)
                };
                movement(t, FORWARD, 1)
            } else {
                // 7.2.
                let (e0, p0) = (self.body_direction, self.pending_direction);
                let c = direction(state.path_yaw, state.head_yaw);
                if c == self.body_direction {
                    self.direction_time = now;
                } else if c != self.pending_direction {
                    self.pending_direction = c;
                    self.direction_time = now;
                } else {
                    let k = if opposite(p0, e0) { 0 } else { 1 };
                    if now.wrapping_sub(self.direction_time) > DIRECTION_WAIT * k {
                        self.body_direction = c;
                        self.direction_time = now;
                    }
                }
                let offset = [0.0, PI, FRAC_PI_2, -FRAC_PI_2][usize::from(self.body_direction)];
                out.body_target_yaw = Some(state.path_yaw + offset);
                let d = direction(state.path_yaw, state.body_yaw);
                if d != self.legs_direction {
                    self.look_back_time = self.look_back_time.max(now);
                    if opposite(self.pending_direction, self.body_direction) {
                        self.kept_speed = 0.0;
                        self.target_speed = 0.0;
                    }
                    self.legs_direction = d;
                }
                let i = (2 * usize::from(state.body_state) + usize::from(t)) * 4 + usize::from(d);
                self.target_speed = Self::speed(state, i);
                movement(t, d, 0)
            }
        } else {
            // 7.3.
            self.kept_speed = 0.0;
            self.target_speed = 0.0;
            // The legs are in place: the look-back time is now (6.6).
            self.look_back_time = now;
            let c = *self.crouch.get_or_insert_with(|| {
                if self.crouch_preference == -1 {
                    (draw() % 2) as i32
                } else {
                    self.crouch_preference
                }
            });
            let mut target = state.body_target_yaw;
            let free = state.mental_state == FREE;
            let reached = wrap(target - state.body_yaw).abs() < 0.001;
            let name = if reached && !free {
                if state.body_state == CROUCHED {
                    usize::try_from(c).ok()
                } else {
                    Some(IDLE_0)
                }
            } else if reached
                && !state.sight_turning
                && wrap(state.body_yaw - state.head_yaw).abs() <= FRAC_PI_6
            {
                Some(IDLE_1)
            } else {
                if reached {
                    target = state.head_target_yaw;
                    out.body_target_yaw = Some(target);
                }
                let right = (state.body_yaw - target).sin() <= 0.0;
                Some(match (right, free) {
                    (true, true) => TURN_RIGHT_1,
                    (true, false) => TURN_RIGHT_0,
                    (false, true) => TURN_LEFT_1,
                    (false, false) => TURN_LEFT_0,
                })
            };
            name.and_then(in_place)
        };
        // 7.4.
        let legs = Slot::Legs as usize;
        let previous = (self.slots[legs].motion, self.slots[legs].playback);
        let changed = chosen != previous.0;
        let weight = |model: &AnimatedModel, motion: Option<MotionId>, id: Option<PlaybackId>| {
            let p = model.playback(id?)?;
            (Some(p.motion) == motion).then_some(p.weight)
        };
        if changed
            && previous.0.is_some()
            && let Some(a) = weight(model, previous.0, previous.1)
        {
            self.kept_speed += (self.target_speed - self.kept_speed) * a;
        }
        self.slots[legs].choose(chosen);
        self.play(Slot::Legs, model, clock, out)?;
        let now_weight = weight(model, self.slots[legs].motion, self.slots[legs].playback);
        let v = match now_weight {
            Some(a) if !changed => self.kept_speed + (self.target_speed - self.kept_speed) * a,
            _ => 0.0,
        };
        if v.abs() >= 1e-7 && now_weight.is_some() {
            out.speed = Some(v);
        }
        Ok(())
    }

    /// 3.6: the torso playback's time set to the legs playback's when both
    /// motions have flags bit 3.
    fn time_carry(&self, model: &mut AnimatedModel) {
        let (Some(torso), Some(legs)) = (
            self.slots[Slot::Torso as usize].playback,
            self.slots[Slot::Legs as usize].playback,
        ) else {
            return;
        };
        let carried = |m: &AnimatedModel, id| {
            let p = m.playback(id)?;
            (m.motions().definition(p.motion).flags & TIME_CARRY != 0).then_some(p.time)
        };
        if carried(model, torso).is_some()
            && let Some(time) = carried(model, legs)
            && let Some(p) = model.playback_mut(torso)
        {
            p.time = time;
        }
    }
}

#[cfg(test)]
mod tests;
