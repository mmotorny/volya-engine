// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Scripted actions (script API V,
//! docs/behavior/script-api-scripted-actions.md; section numbers are
//! its): the class `entity_action` (3) and its eight part classes `move`
//! (4), `look` (5), `anim` (6), `sound` (7), `particle` (8), `object` (9),
//! `cond` (3.3) and `act` (10); the members of `game_object` that put an
//! object under script control and queue actions for it (2.2, 2.3); and
//! the engine's runs of an object's queue (2.4, 2.5).
//!
//! A scripted object (2.1) keeps its [`ScriptControl`] in its
//! [`GameObject`]. Queued actions are shared ([`EntityAction`]), so
//! `action_by_index` gives the queued action itself. Of applying the
//! parts (sections 4 to 10), what needs nothing beyond the scripts' own
//! state is done here (the completeness test, the sound part over
//! [`ScriptSounds`], the particle part's done flag, the parts that never
//! apply to a class); what reaches the creatures' movement, sight,
//! animation, object handling and behaviours, the car's driving and the
//! particles waits for those (TODO(#1136) … TODO(#1142)). See
//! docs/architecture/scripts.md, Scripted actions.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use volya_engine_core::sound::system::PlayFlags;

use crate::classes::{EngineClass, ObjectMaker, Objects, Operand};
use crate::game_object::callbacks::{ActionPart, ObjectEvent, raise_inside};
use crate::game_object::members::Handle;
use crate::game_object::{CREATURE_CLASSES, GameObject, GameObjects, ObjectClass};
use crate::loader::Scripts;
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::sound::creatures::bone_position;
use crate::sound::{ScriptSounds, SoundKey, SoundObject};
use crate::values::{Vector, to_f32, to_flags32};
use crate::{Error, ErrorKind, Value};

/// `move`'s 28 constants (1.2).
pub const MOVE_CONSTANTS: [(&str, i32); 28] = [
    ("crouch", 0),
    ("standing", 1),
    ("walk", 0),
    ("run", 1),
    ("stand", 2),
    ("line", 0),
    ("dodge", 1),
    ("criteria", 2),
    ("curve", 0),
    ("curve_criteria", 2),
    ("none", 1),
    ("fwd", 2),
    ("back", 4),
    ("left", 8),
    ("right", 16),
    ("up", 32),
    ("down", 64),
    ("handbrake", 128),
    ("on", 256),
    ("off", 512),
    ("walk_fwd", 0),
    ("walk_bkwd", 1),
    ("run_fwd", 2),
    ("drag", 3),
    ("jump", 4),
    ("steal", 5),
    ("default", 0),
    ("force", 1),
];

/// `look`'s 7 constants (1.2).
pub const LOOK_CONSTANTS: [(&str, i32); 7] = [
    ("cur_dir", 0),
    ("path_dir", 1),
    ("direction", 2),
    ("point", 3),
    ("danger", 5),
    ("search", 6),
    ("fire_point", 10),
];

/// `anim`'s 12 constants (1.2).
pub const ANIM_CONSTANTS: [(&str, i32); 12] = [
    ("danger", 0),
    ("free", 1),
    ("panic", 2),
    ("stand_idle", 0),
    ("sit_idle", 1),
    ("lie_idle", 2),
    ("eat", 3),
    ("sleep", 4),
    ("rest", 5),
    ("attack", 6),
    ("look_around", 7),
    ("turn", 8),
];

/// `sound`'s 9 constants (1.2).
pub const SOUND_CONSTANTS: [(&str, i32); 9] = [
    ("idle", 1),
    ("eat", 2),
    ("attack", 3),
    ("attack_hit", 4),
    ("take_damage", 5),
    ("die", 7),
    ("threaten", 9),
    ("steal", 10),
    ("panic", 11),
];

/// `object`'s 21 constants (1.2).
pub const OBJECT_CONSTANTS: [(&str, i32); 21] = [
    ("switch1", 0),
    ("switch2", 1),
    ("reload", 2),
    ("reload1", 2),
    ("reload2", 3),
    ("aim1", 4),
    ("aim2", 5),
    ("fire1", 6),
    ("fire2", 7),
    ("idle", 8),
    ("strap", 9),
    ("drop", 10),
    ("activate", 15),
    ("deactivate", 16),
    ("use", 17),
    ("turn_on", 18),
    ("turn_off", 19),
    ("show", 20),
    ("hide", 21),
    ("take", 22),
    ("dummy", -1),
];

/// `cond`'s 7 constants (1.2), the bits of a condition set (3.3).
pub const COND_CONSTANTS: [(&str, i32); 7] = [
    ("move_end", 1),
    ("look_end", 2),
    ("anim_end", 4),
    ("sound_end", 8),
    ("object_end", 32),
    ("time_end", 64),
    ("act_end", 128),
];

/// `act`'s 4 constants (1.2).
pub const ACT_CONSTANTS: [(&str, i32); 4] = [("rest", 0), ("eat", 1), ("attack", 2), ("panic", 3)];

/// The condition bit no `cond` constant names: the particle part done
/// (3.3).
pub const PARTICLE_END: u32 = 16;

const STANDING: i32 = 1;
const STAND: i32 = 2;
const LINE: i32 = 0;
const CUR_DIR: i32 = 0;
const IDLE: i32 = 8;
/// "No monster sound" and "the monster's own delay" (7.1, 7.3).
const NONE: i32 = -1;

/// What a movement part moves to (4.1).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MoveTarget {
    #[default]
    None,
    /// An object (none for a nil handle).
    Object(Option<u16>),
    Position([f32; 3]),
    /// A position with a level vertex.
    LevelVertex([f32; 3], u32),
    /// A direct position.
    Direct([f32; 3]),
    /// Input keys (a set of `move`'s key bits).
    Input(u32),
}

/// A `move` part (4.1).
#[derive(Clone, Debug, PartialEq)]
pub struct MovePart {
    pub body: i32,
    pub movement: i32,
    pub path: i32,
    pub target: MoveTarget,
    /// "The number" (4.4: its effect unestablished).
    pub number: f32,
    /// The monster's move, speed and distance where a form gives them
    /// (their defaults unstated: none, TODO(#1144)).
    pub monster_move: Option<i32>,
    pub monster_speed: Option<i32>,
    pub monster_distance: Option<f32>,
    pub done: bool,
}

impl Default for MovePart {
    fn default() -> Self {
        MovePart {
            body: STANDING,
            movement: STAND,
            path: LINE,
            target: MoveTarget::None,
            number: 0.0,
            monster_move: None,
            monster_speed: None,
            monster_distance: None,
            done: true,
        }
    }
}

/// What a look part looks at (5.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LookTarget {
    #[default]
    None,
    /// [`LookPart::object`] with [`LookPart::bone`].
    Object,
    /// [`LookPart::direction`].
    Direction,
}

/// A `look` part (5.1).
#[derive(Clone, Debug, PartialEq)]
pub struct LookPart {
    /// The sight type, a `look` constant.
    pub sight: i32,
    pub target: LookTarget,
    pub object: Option<u16>,
    /// Empty for none.
    pub bone: Vec<u8>,
    pub direction: [f32; 3],
    pub numbers: (f32, f32),
    pub done: bool,
}

impl Default for LookPart {
    fn default() -> Self {
        LookPart {
            sight: CUR_DIR,
            target: LookTarget::None,
            object: None,
            bone: Vec::new(),
            direction: [0.0; 3],
            numbers: (0.0, 0.0),
            done: true,
        }
    }
}

/// An `anim` part (6.1).
#[derive(Clone, Debug, PartialEq)]
pub struct AnimPart {
    /// Empty for none.
    pub name: Vec<u8>,
    /// The mental state and the monster animation where given (their
    /// defaults unstated: none, TODO(#1144)).
    pub mental_state: Option<i32>,
    pub monster_animation: Option<i32>,
    pub number: f32,
    /// What it does is unestablished (14).
    pub flag: bool,
    pub done: bool,
}

impl Default for AnimPart {
    fn default() -> Self {
        AnimPart {
            name: Vec::new(),
            mental_state: None,
            monster_animation: None,
            number: 0.0,
            flag: false,
            done: true,
        }
    }
}

/// A `sound` part (7.1).
#[derive(Clone, Debug, PartialEq)]
pub struct SoundPart {
    /// Empty for none.
    pub name: Vec<u8>,
    /// Empty for none.
    pub bone: Vec<u8>,
    pub position: [f32; 3],
    pub angles: [f32; 3],
    pub looped: bool,
    /// T's AI sound type where set (its default unstated: none, made with
    /// type 0 as `sound_object(name)` is, TODO(#1144)).
    pub sound_type: Option<i32>,
    /// −1 for none (7.3).
    pub monster_sound: i32,
    /// −1 for the monster's own (7.1).
    pub delay: i32,
    pub started: bool,
    pub done: bool,
}

impl Default for SoundPart {
    fn default() -> Self {
        SoundPart {
            name: Vec::new(),
            bone: Vec::new(),
            position: [0.0; 3],
            angles: [0.0; 3],
            looped: false,
            sound_type: None,
            monster_sound: NONE,
            delay: NONE,
            started: false,
            done: true,
        }
    }
}

/// A `particle` part (8).
#[derive(Clone, Debug, PartialEq)]
pub struct ParticlePart {
    /// The particles object, by its effect's name: none for `particle()`
    /// (making and playing them, TODO(#1140)).
    pub particles: Option<Vec<u8>>,
    pub auto_removal: bool,
    /// Empty for none.
    pub bone: Vec<u8>,
    pub position: [f32; 3],
    pub angles: [f32; 3],
    pub velocity: [f32; 3],
    pub started: bool,
    pub done: bool,
}

impl Default for ParticlePart {
    fn default() -> Self {
        ParticlePart {
            particles: None,
            auto_removal: true,
            bone: Vec::new(),
            position: [0.0; 3],
            angles: [0.0; 3],
            velocity: [0.0; 3],
            started: false,
            done: false,
        }
    }
}

/// An `object` part (9.1).
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectPart {
    pub item: Option<u16>,
    /// Empty for none.
    pub name: Vec<u8>,
    /// An `object` constant.
    pub action: i32,
    /// −1 unless given (the forms without one: our choice, TODO(#1144)).
    pub number: f32,
    pub done: bool,
}

impl Default for ObjectPart {
    fn default() -> Self {
        ObjectPart {
            item: None,
            name: Vec::new(),
            action: IDLE,
            number: -1.0,
            done: true,
        }
    }
}

/// A `cond` part (3.3): its condition set and time limit, ms.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CondPart {
    pub flags: u32,
    /// `None`: no time limit.
    pub limit: Option<u64>,
}

/// An `act` part (10).
#[derive(Clone, Debug, PartialEq)]
pub struct ActPart {
    /// An `act` constant.
    pub action: i32,
    pub object: Option<u16>,
    pub done: bool,
}

impl Default for ActPart {
    fn default() -> Self {
        ActPart {
            action: 0,
            object: None,
            done: true,
        }
    }
}

/// One scripted action (3): its parts, its start time and its "checked"
/// flag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Action {
    pub movement: MovePart,
    pub look: LookPart,
    pub anim: AnimPart,
    pub sound: SoundPart,
    pub particle: ParticlePart,
    pub object: ObjectPart,
    pub cond: CondPart,
    pub act: ActPart,
    /// Now when it last became current (2.4); `None` before (what the
    /// time test reads then is disputed in review: false here,
    /// TODO(#1144)).
    pub start: Option<u32>,
    pub checked: bool,
}

impl Action {
    /// `time()` (3.2): the start time plus the limit, as 64-bit unsigned
    /// integers, below 2^32 and below `now`; false with no limit.
    pub fn time(&self, now: u32) -> bool {
        match (self.start, self.cond.limit) {
            (Some(start), Some(limit)) => {
                let end = u64::from(start).wrapping_add(limit);
                end < 1 << 32 && end < u64::from(now)
            }
            _ => false,
        }
    }

    /// The completeness test (3.3): the first test after the "checked"
    /// flag was cleared answers "not complete"; then each condition that
    /// holds is cleared from the condition set (the part's own, our
    /// reading of "the condition set"), complete when none remains.
    pub fn complete(&mut self, now: u32) -> bool {
        if !self.checked {
            self.checked = true;
            return false;
        }
        let holds = [
            (1, self.movement.done),
            (2, self.look.done),
            (4, self.anim.done),
            (8, self.sound.done),
            (PARTICLE_END, self.particle.done),
            (32, self.object.done),
            (64, self.time(now)),
            (128, self.act.done),
        ];
        for (bit, held) in holds {
            if held {
                self.cond.flags &= !bit;
            }
        }
        self.cond.flags == 0
    }
}

/// An `entity_action`'s part: the action, shared with the queue it is in
/// (the queue holds copies of what scripts give it, 2.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityAction(pub Rc<RefCell<Action>>);

impl EntityAction {
    /// A new action copying `action`'s state.
    pub fn copy_of(action: &Action) -> Self {
        EntityAction(Rc::new(RefCell::new(action.clone())))
    }
}

/// A scripted object's script control (2.1): the control flag and name,
/// the capture flag, the action queue (first to last) and its sound for
/// actions. TODO(#1127): whether it is saved, or kept while the object is
/// offline, is a review gap: it lives with the online object.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptControl {
    pub controlled: bool,
    /// Empty at first.
    pub name: Vec<u8>,
    pub capture: bool,
    pub queue: VecDeque<Rc<RefCell<Action>>>,
    /// The current action seen last time (2.4).
    pub last: Option<Rc<RefCell<Action>>>,
    /// Its sound for actions (7.2), while it has one.
    pub sound: Option<SoundKey>,
}

impl ScriptControl {
    /// As an object comes into the level: not under control, the capture
    /// flag set (2.2, believed), the queue empty.
    pub fn new() -> Self {
        ScriptControl {
            controlled: false,
            name: Vec::new(),
            capture: true,
            queue: VecDeque::new(),
            last: None,
            sound: None,
        }
    }
}

impl Default for ScriptControl {
    fn default() -> Self {
        Self::new()
    }
}

/// What applies a scripted object's parts (2.1, sections 4 to 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    /// The 17 monster classes.
    Monster,
    Stalker,
    Trader,
    Car,
    /// Objects of class id `O_SEARCH`.
    Searchlight,
    /// Objects of class id `SCRPTOBJ`.
    ScriptObject,
}

fn body(class: ObjectClass) -> Option<Body> {
    match class {
        ObjectClass::Named("CAI_Stalker") => Some(Body::Stalker),
        ObjectClass::Named("CAI_Trader") => Some(Body::Trader),
        ObjectClass::Named("CCar") => Some(Body::Car),
        ObjectClass::Named(name) if CREATURE_CLASSES.contains(&name) => Some(Body::Monster),
        ObjectClass::Unnamed(id) if id == class_id(b"O_SEARCH") => Some(Body::Searchlight),
        ObjectClass::Unnamed(id) if id == class_id(b"SCRPTOBJ") => Some(Body::ScriptObject),
        _ => None,
    }
}

/// Whether objects of `class` are scripted (2.1): the 19 creature
/// classes, `CCar`, and the objects of class ids `O_SEARCH` and
/// `SCRPTOBJ`.
pub fn is_scripted(class: ObjectClass) -> bool {
    body(class).is_some()
}

/// What the members share.
#[derive(Clone)]
pub(crate) struct Context {
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub sounds: Rc<RefCell<ScriptSounds>>,
    pub fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// Now (conventions): the device's `time_global()`.
    fn now(&self) -> u32 {
        self.services.borrow().device.time_global()
    }

    /// Whether a sound part's name finds `<name>.ogg` among the game's
    /// sounds (7.1).
    fn sound_exists(&self, name: &[u8]) -> Result<bool, String> {
        self.sounds.borrow_mut().exists(name)
    }

    /// A fatal error standing for a read the spec believes invalid (our
    /// wording).
    fn crash(&self, function: &str, text: &str) -> String {
        crate::services::report_fatal(
            &self.services,
            &self.fatal,
            crate::services::Fatal {
                function,
                file: file!(),
                line: line!(),
                expression: "fatal error",
                description: "<no expression>",
                arguments: &[text],
            },
        )
    }
}

type Results = Result<Vec<Value>, String>;

/// An argument's kind, for matching a form (the listing's overloads).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum K {
    /// A number (constants, flags, numbers).
    N,
    /// A text.
    S,
    B,
    /// A `vector`.
    V,
    /// A handle or nil (`game_object*`).
    H,
    /// A `sound_object`.
    So,
}

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

fn fits(m: &ObjectMaker, args: &[Value], kinds: &[K]) -> bool {
    args.len() == kinds.len()
        && args.iter().zip(kinds).all(|(v, k)| match k {
            K::N => matches!(v, Value::Number(_)),
            K::S => matches!(v, Value::String(_)),
            K::B => matches!(v, Value::Boolean(_)),
            K::V => m.get::<Vector>(v).is_some(),
            K::H => matches!(v, Value::Nil) || m.get::<Handle>(v).is_some(),
            K::So => m.is::<SoundObject>(v),
        })
}

/// Adds the constructor of the form `kinds`, its part made by `make` once
/// the arguments match.
fn form<T: 'static>(
    class: EngineClass,
    kinds: &'static [K],
    make: impl Fn(&ObjectMaker, &[Value]) -> Result<T, String> + 'static,
) -> EngineClass {
    class.constructor_with_objects(
        kinds.len(),
        move |m, args| fits(m, args, kinds).then_some(0),
        move |_, m, args| make(m, args),
    )
}

/// A matched number argument.
fn num(args: &[Value], i: usize) -> f64 {
    match arg(args, i) {
        Value::Number(n) => *n,
        _ => 0.0,
    }
}

/// A matched number argument made an integer (conventions).
fn int(args: &[Value], i: usize) -> i32 {
    to_flags32(num(args, i)) as i32
}

fn float(args: &[Value], i: usize) -> f32 {
    to_f32(num(args, i))
}

fn text(args: &[Value], i: usize) -> Vec<u8> {
    match arg(args, i) {
        Value::String(s) => s.clone(),
        _ => Vec::new(),
    }
}

fn boolean(args: &[Value], i: usize) -> bool {
    matches!(arg(args, i), Value::Boolean(true))
}

fn vector(m: &ObjectMaker, args: &[Value], i: usize) -> [f32; 3] {
    m.get::<Vector>(arg(args, i))
        .map_or([0.0; 3], |v| [v.x, v.y, v.z])
}

/// A matched handle argument's object; none for nil.
fn handle(m: &ObjectMaker, args: &[Value], i: usize) -> Option<u16> {
    m.get::<Handle>(arg(args, i)).map(|h| h.id)
}

/// A method's number argument made an integer (our wording for a
/// mismatch, A).
fn m_int(args: &[Value], i: usize, what: &str) -> Result<i32, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(to_flags32(*n) as i32),
        _ => Err(format!("{what}: number expected")),
    }
}

fn m_text(args: &[Value], i: usize, what: &str) -> Result<Vec<u8>, String> {
    match arg(args, i) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{what}: text expected")),
    }
}

fn m_boolean(args: &[Value], i: usize, what: &str) -> Result<bool, String> {
    match arg(args, i) {
        Value::Boolean(b) => Ok(*b),
        _ => Err(format!("{what}: boolean expected")),
    }
}

fn m_vector(o: &Objects, args: &[Value], i: usize, what: &str) -> Result<[f32; 3], String> {
    match o.get::<Vector>(arg(args, i)) {
        Some(Operand::Other(v)) => Ok([v.x, v.y, v.z]),
        _ => Err(format!("{what}: vector expected")),
    }
}

/// A method's handle argument: its object, none for nil.
fn m_handle(o: &Objects, args: &[Value], i: usize, what: &str) -> Result<Option<u16>, String> {
    match arg(args, i) {
        Value::Nil => Ok(None),
        v => match o.get::<Handle>(v) {
            Some(Operand::Other(h)) => Ok(Some(h.id)),
            _ => Err(format!("{what}: game object expected")),
        },
    }
}

fn done_result(done: bool) -> Results {
    Ok(vec![Value::Boolean(done)])
}

fn constants(class: EngineClass, list: &[(&str, i32)]) -> EngineClass {
    list.iter()
        .fold(class, |class, &(name, value)| class.constant(name, value))
}

/// `move` (4): the 21 forms, the setters and `completed`.
fn move_class() -> EngineClass {
    use K::{H, N, V};
    let class = EngineClass::new("move").constructible();
    let class = form(class, &[], |_, _| Ok(MovePart::default()));
    // A form with arguments is not done (4.2).
    let made = |f: fn(&ObjectMaker, &[Value], &mut MovePart)| {
        move |m: &ObjectMaker, a: &[Value]| {
            let mut p = MovePart {
                done: false,
                ..MovePart::default()
            };
            f(m, a, &mut p);
            Ok(p)
        }
    };
    let class = form(
        class,
        &[N],
        made(|_, a, p| {
            p.target = MoveTarget::Input(int(a, 0) as u32);
        }),
    );
    let class = form(
        class,
        &[N, N],
        made(|_, a, p| {
            p.target = MoveTarget::Input(int(a, 0) as u32);
            p.number = float(a, 1);
        }),
    );
    // The stalkers' forms, with the number or without (0, our choice).
    fn stalker(p: &mut MovePart, a: &[Value]) {
        p.body = int(a, 0);
        p.movement = int(a, 1);
        p.path = int(a, 2);
        p.number = float(a, 4);
    }
    let class = form(
        class,
        &[N, N, N, H],
        made(|m, a, p| {
            stalker(p, a);
            p.target = MoveTarget::Object(handle(m, a, 3));
        }),
    );
    let class = form(
        class,
        &[N, N, N, H, N],
        made(|m, a, p| {
            stalker(p, a);
            p.target = MoveTarget::Object(handle(m, a, 3));
        }),
    );
    let class = form(
        class,
        &[N, N, N, V],
        made(|m, a, p| {
            stalker(p, a);
            p.target = MoveTarget::Position(vector(m, a, 3));
        }),
    );
    let class = form(
        class,
        &[N, N, N, V, N],
        made(|m, a, p| {
            stalker(p, a);
            p.target = MoveTarget::Position(vector(m, a, 3));
        }),
    );
    let class = form(
        class,
        &[V, N],
        made(|m, a, p| {
            p.target = MoveTarget::Direct(vector(m, a, 0));
            p.number = float(a, 1);
        }),
    );
    // The monsters' forms: move, target, distance (absent: none), speed.
    let class = form(
        class,
        &[N, V],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::Position(vector(m, a, 1));
        }),
    );
    let class = form(
        class,
        &[N, H],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::Object(handle(m, a, 1));
        }),
    );
    let class = form(
        class,
        &[N, V, N],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::Position(vector(m, a, 1));
            p.monster_distance = Some(float(a, 2));
        }),
    );
    let class = form(
        class,
        &[N, H, N],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::Object(handle(m, a, 1));
            p.monster_distance = Some(float(a, 2));
        }),
    );
    let class = form(
        class,
        &[N, N, V],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::LevelVertex(vector(m, a, 2), int(a, 1) as u32);
        }),
    );
    let class = form(
        class,
        &[N, N, V, N],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::LevelVertex(vector(m, a, 2), int(a, 1) as u32);
            p.monster_distance = Some(float(a, 3));
        }),
    );
    let class = form(
        class,
        &[N, V, N, N],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::Position(vector(m, a, 1));
            p.monster_distance = Some(float(a, 2));
            p.monster_speed = Some(int(a, 3));
        }),
    );
    let mut class = form(
        class,
        &[N, H, N, N],
        made(|m, a, p| {
            p.monster_move = Some(int(a, 0));
            p.target = MoveTarget::Object(handle(m, a, 1));
            p.monster_distance = Some(float(a, 2));
            p.monster_speed = Some(int(a, 3));
        }),
    );
    // TODO(#1143): the six forms taking a `patrol` (spec N) match nothing
    // until it is registered.

    // 4.3: each setter clears the done flag.
    type Set = fn(&mut MovePart, i32);
    let setters: [(&str, Set); 4] = [
        ("body", |p, v| p.body = v),
        ("move", |p, v| p.movement = v),
        ("path", |p, v| p.path = v),
        ("input", |p, v| p.target = MoveTarget::Input(v as u32)),
    ];
    for (name, set) in setters {
        let what = format!("move:{name}");
        class = class.method(name, move |p: &mut MovePart, args| {
            set(p, m_int(args, 0, &what)?);
            p.done = false;
            Ok(Vec::new())
        });
    }
    class
        .method_with_objects("object", |p: &mut MovePart, o, args| {
            p.target = MoveTarget::Object(m_handle(o, args, 0, "move:object")?);
            p.done = false;
            Ok(Vec::new())
        })
        .method_with_objects("position", |p: &mut MovePart, o, args| {
            p.target = MoveTarget::Position(m_vector(o, args, 0, "move:position")?);
            p.done = false;
            Ok(Vec::new())
        })
        .method("patrol", |_: &mut MovePart, _| {
            // TODO(#1143): spec N's patrol path.
            Err("move:patrol: patrol paths aren't available".into())
        })
        .method("completed", |p: &mut MovePart, _| done_result(p.done))
}

/// `look` (5.1).
fn look_class() -> EngineClass {
    use K::{H, N, S, V};
    let class = EngineClass::new("look").constructible();
    let class = form(class, &[], |_, _| Ok(LookPart::default()));
    let made = |f: fn(&ObjectMaker, &[Value], &mut LookPart)| {
        move |m: &ObjectMaker, a: &[Value]| {
            let mut p = LookPart {
                done: false,
                ..LookPart::default()
            };
            f(m, a, &mut p);
            Ok(p)
        }
    };
    let class = form(class, &[N], made(|_, a, p| p.sight = int(a, 0)));
    let class = form(
        class,
        &[N, V],
        made(|m, a, p| {
            p.sight = int(a, 0);
            p.target = LookTarget::Direction;
            p.direction = vector(m, a, 1);
        }),
    );
    let class = form(
        class,
        &[N, H],
        made(|m, a, p| {
            p.sight = int(a, 0);
            p.target = LookTarget::Object;
            p.object = handle(m, a, 1);
        }),
    );
    let class = form(
        class,
        &[N, H, S],
        made(|m, a, p| {
            p.sight = int(a, 0);
            p.target = LookTarget::Object;
            p.object = handle(m, a, 1);
            p.bone = text(a, 2);
        }),
    );
    let class = form(
        class,
        &[V, N, N],
        made(|m, a, p| {
            p.target = LookTarget::Direction;
            p.direction = vector(m, a, 0);
            p.numbers = (float(a, 1), float(a, 2));
        }),
    );
    let class = form(
        class,
        &[H, N, N],
        made(|m, a, p| {
            p.target = LookTarget::Object;
            p.object = handle(m, a, 0);
            p.numbers = (float(a, 1), float(a, 2));
        }),
    );
    class
        .method("type", |p: &mut LookPart, args| {
            p.sight = m_int(args, 0, "look:type")?;
            p.done = false;
            Ok(Vec::new())
        })
        .method_with_objects("object", |p: &mut LookPart, o, args| {
            p.object = m_handle(o, args, 0, "look:object")?;
            p.target = LookTarget::Object;
            p.done = false;
            Ok(Vec::new())
        })
        .method("bone", |p: &mut LookPart, args| {
            p.bone = m_text(args, 0, "look:bone")?;
            p.done = false;
            Ok(Vec::new())
        })
        .method_with_objects("direct", |p: &mut LookPart, o, args| {
            p.direction = m_vector(o, args, 0, "look:direct")?;
            p.target = LookTarget::Direction;
            p.done = false;
            Ok(Vec::new())
        })
        .method("completed", |p: &mut LookPart, _| done_result(p.done))
}

/// `anim` (6.1).
fn anim_class() -> EngineClass {
    use K::{B, N, S};
    let class = EngineClass::new("anim").constructible();
    let class = form(class, &[], |_, _| Ok(AnimPart::default()));
    let class = form(class, &[S], |_, a| {
        Ok(AnimPart {
            name: text(a, 0),
            done: false,
            ..AnimPart::default()
        })
    });
    let class = form(class, &[S, B], |_, a| {
        Ok(AnimPart {
            name: text(a, 0),
            flag: boolean(a, 1),
            done: false,
            ..AnimPart::default()
        })
    });
    // A part made only to carry a mental state is done.
    let class = form(class, &[N], |_, a| {
        Ok(AnimPart {
            mental_state: Some(int(a, 0)),
            done: true,
            ..AnimPart::default()
        })
    });
    let class = form(class, &[N, N], |_, a| {
        Ok(AnimPart {
            monster_animation: Some(int(a, 0)),
            number: float(a, 1),
            done: false,
            ..AnimPart::default()
        })
    });
    class
        .method("anim", |p: &mut AnimPart, args| {
            p.name = m_text(args, 0, "anim:anim")?;
            p.done = false;
            Ok(Vec::new())
        })
        .method("type", |p: &mut AnimPart, args| {
            p.mental_state = Some(m_int(args, 0, "anim:type")?);
            p.done = true;
            Ok(Vec::new())
        })
        .method("completed", |p: &mut AnimPart, _| done_result(p.done))
}

/// A sound part's name set (7.1): made done at once when `<name>.ogg`
/// doesn't exist.
fn set_sound_name(ctx: &Context, p: &mut SoundPart, name: Vec<u8>) -> Result<(), String> {
    if !ctx.sound_exists(&name)? {
        p.done = true;
    }
    p.name = name;
    Ok(())
}

/// The `sound_object` forms: what they take is unestablished (7.1), so
/// they raise (our wording, TODO(#1144)).
fn sound_object_form() -> String {
    "sound: the forms taking a sound_object aren't available".into()
}

/// `sound` (7.1).
fn sound_class(ctx: &Context) -> EngineClass {
    use K::{B, N, S, So, V};
    let class = EngineClass::new("sound").constructible();
    let class = form(class, &[], |_, _| Ok(SoundPart::default()));
    type Fill = fn(&ObjectMaker, &[Value], &mut SoundPart);
    // The named forms: the name looked for, the rest as `fill` reads it.
    let named = |kinds: &'static [K], fill: Fill| {
        let c = ctx.clone();
        move |class: EngineClass| {
            form(class, kinds, move |m, a| {
                let mut p = SoundPart {
                    done: false,
                    ..SoundPart::default()
                };
                fill(m, a, &mut p);
                set_sound_name(&c, &mut p, text(a, 0))?;
                Ok(p)
            })
        }
    };
    let with_bone: Fill = |m, a, p| {
        p.bone = text(a, 1);
        if a.len() > 2 {
            p.position = vector(m, a, 2);
        }
        if a.len() > 3 {
            p.angles = vector(m, a, 3);
        }
        p.looped = a.len() > 4 && boolean(a, 4);
    };
    let at_position: Fill = |m, a, p| {
        p.position = vector(m, a, 1);
        if a.len() > 2 {
            p.angles = vector(m, a, 2);
        }
        p.looped = a.len() > 3 && boolean(a, 3);
    };
    let mut class = class;
    for kinds in [&[S, S][..], &[S, S, V], &[S, S, V, V], &[S, S, V, V, B]] {
        class = named(kinds, with_bone)(class);
    }
    for kinds in [&[S, V][..], &[S, V, V], &[S, V, V, B]] {
        class = named(kinds, at_position)(class);
    }
    // The head animation's effect is unestablished (14): not kept.
    class = named(&[S, S, N], |_, a, p| p.bone = text(a, 1))(class);
    for kinds in [
        &[So, S, V][..],
        &[So, S, V, V],
        &[So, S, V, V, B],
        &[So, V],
        &[So, V, V],
        &[So, V, V, B],
    ] {
        class = form(class, kinds, |_, _| -> Result<SoundPart, String> {
            Err(sound_object_form())
        });
    }
    let class = form(class, &[N], |_, a| {
        Ok(SoundPart {
            monster_sound: int(a, 0),
            done: false,
            ..SoundPart::default()
        })
    });
    let class = form(class, &[N, N], |_, a| {
        Ok(SoundPart {
            monster_sound: int(a, 0),
            delay: int(a, 1),
            done: false,
            ..SoundPart::default()
        })
    });
    let c = ctx.clone();
    class
        .method("set_sound", move |p: &mut SoundPart, args| {
            match arg(args, 0) {
                Value::String(name) => {
                    set_sound_name(&c, p, name.clone())?;
                    Ok(Vec::new())
                }
                Value::Object(_) => Err(sound_object_form()),
                _ => Err("sound:set_sound: text expected".into()),
            }
        })
        .method_with_objects("set_position", |p: &mut SoundPart, o, args| {
            p.position = m_vector(o, args, 0, "sound:set_position")?;
            p.started = false;
            Ok(Vec::new())
        })
        .method("set_bone", |p: &mut SoundPart, args| {
            p.bone = m_text(args, 0, "sound:set_bone")?;
            p.started = false;
            Ok(Vec::new())
        })
        .method_with_objects("set_angles", |p: &mut SoundPart, o, args| {
            p.angles = m_vector(o, args, 0, "sound:set_angles")?;
            p.started = false;
            Ok(Vec::new())
        })
        .method("set_sound_type", |p: &mut SoundPart, args| {
            p.sound_type = Some(m_int(args, 0, "sound:set_sound_type")?);
            p.started = false;
            Ok(Vec::new())
        })
        .method("completed", |p: &mut SoundPart, _| done_result(p.done))
}

/// `particle` (8).
fn particle_class() -> EngineClass {
    use K::S;
    let class = EngineClass::new("particle").constructible();
    let class = form(class, &[], |_, _| Ok(ParticlePart::default()));
    let class = form(class, &[S, S], |_, a| {
        Ok(ParticlePart {
            particles: Some(text(a, 0)),
            bone: text(a, 1),
            ..ParticlePart::default()
        })
    });
    // TODO(#1140): the four forms taking a `particle_params` (spec U)
    // match nothing until it is registered.
    let mut class = class.method("set_particle", |p: &mut ParticlePart, args| {
        p.particles = Some(m_text(args, 0, "particle:set_particle")?);
        p.auto_removal = m_boolean(args, 1, "particle:set_particle")?;
        p.done = false;
        Ok(Vec::new())
    });
    class = class.method("set_bone", |p: &mut ParticlePart, args| {
        p.bone = m_text(args, 0, "particle:set_bone")?;
        p.done = false;
        Ok(Vec::new())
    });
    type Set = fn(&mut ParticlePart, [f32; 3]);
    let setters: [(&str, Set); 3] = [
        ("set_position", |p, v| p.position = v),
        ("set_angles", |p, v| p.angles = v),
        ("set_velocity", |p, v| p.velocity = v),
    ];
    for (name, set) in setters {
        let what = format!("particle:{name}");
        class = class.method_with_objects(name, move |p: &mut ParticlePart, o, args| {
            set(p, m_vector(o, args, 0, &what)?);
            p.done = false;
            Ok(Vec::new())
        });
    }
    class.method("completed", |p: &mut ParticlePart, _| done_result(p.done))
}

/// `object` (9.1).
fn object_class() -> EngineClass {
    use K::{H, N, S};
    let class = EngineClass::new("object").constructible();
    let class = form(class, &[], |_, _| Ok(ObjectPart::default()));
    let class = form(class, &[H, N], |m, a| {
        Ok(ObjectPart {
            item: handle(m, a, 0),
            action: int(a, 1),
            done: false,
            ..ObjectPart::default()
        })
    });
    let class = form(class, &[H, N, N], |m, a| {
        Ok(ObjectPart {
            item: handle(m, a, 0),
            action: int(a, 1),
            number: float(a, 2),
            done: false,
            ..ObjectPart::default()
        })
    });
    let class = form(class, &[N], |_, a| {
        Ok(ObjectPart {
            action: int(a, 0),
            done: false,
            ..ObjectPart::default()
        })
    });
    let class = form(class, &[S, N], |_, a| {
        Ok(ObjectPart {
            name: text(a, 0),
            action: int(a, 1),
            done: false,
            ..ObjectPart::default()
        })
    });
    class
        .method_with_objects("object", |p: &mut ObjectPart, o, args| {
            match arg(args, 0) {
                Value::String(name) => p.name = name.clone(),
                _ => p.item = m_handle(o, args, 0, "object:object")?,
            }
            p.done = false;
            Ok(Vec::new())
        })
        .method("action", |p: &mut ObjectPart, args| {
            p.action = m_int(args, 0, "object:action")?;
            p.done = false;
            Ok(Vec::new())
        })
        .method("completed", |p: &mut ObjectPart, _| done_result(p.done))
}

/// `cond` (3.3): no member but its constructors.
fn cond_class() -> EngineClass {
    use K::N;
    let class = EngineClass::new("cond").constructible();
    let class = form(class, &[], |_, _| Ok(CondPart::default()));
    let class = form(class, &[N], |_, a| {
        Ok(CondPart {
            flags: int(a, 0) as u32,
            limit: None,
        })
    });
    // The time made a 64-bit integer: truncated, saturating (our choice).
    form(class, &[N, N], |_, a| {
        Ok(CondPart {
            flags: int(a, 0) as u32,
            limit: Some(num(a, 1) as i64 as u64),
        })
    })
}

/// `act` (10).
fn act_class() -> EngineClass {
    use K::{H, N};
    let class = EngineClass::new("act").constructible();
    let class = form(class, &[], |_, _| Ok(ActPart::default()));
    let class = form(class, &[N], |_, a| {
        Ok(ActPart {
            action: int(a, 0),
            object: None,
            done: false,
        })
    });
    form(class, &[N, H], |m, a| {
        Ok(ActPart {
            action: int(a, 0),
            object: handle(m, a, 1),
            done: false,
        })
    })
}

/// `entity_action` (3).
fn entity_action_class(ctx: &Context) -> EngineClass {
    let class = EngineClass::new("entity_action")
        .constructible()
        .constructor_with_objects(0, |_, _| Some(0), |_, _, _| Ok(EntityAction::default()))
        // A copy, or a default one for nil (believed, 3.1).
        .constructor_with_objects(
            1,
            |m, a| {
                (matches!(arg(a, 0), Value::Nil) || m.get::<EntityAction>(arg(a, 0)).is_some())
                    .then_some(0)
            },
            |_, m, a| {
                Ok(match m.get::<EntityAction>(arg(a, 0)) {
                    Some(other) => EntityAction::copy_of(&other.0.borrow()),
                    None => EntityAction::default(),
                })
            },
        );
    // 3.2: one form per part class.
    let class = class.method_with_objects("set_action", |e: &mut EntityAction, o, args| {
        let v = arg(args, 0);
        let mut a = e.0.borrow_mut();
        if let Some(Operand::Other(p)) = o.get::<MovePart>(v) {
            a.movement = p;
        } else if let Some(Operand::Other(p)) = o.get::<LookPart>(v) {
            a.look = p;
        } else if let Some(Operand::Other(p)) = o.get::<AnimPart>(v) {
            a.anim = p;
        } else if let Some(Operand::Other(p)) = o.get::<SoundPart>(v) {
            a.sound = p;
        } else if let Some(Operand::Other(p)) = o.get::<ParticlePart>(v) {
            a.particle = p;
        } else if let Some(Operand::Other(p)) = o.get::<ObjectPart>(v) {
            a.object = p;
        } else if let Some(Operand::Other(p)) = o.get::<CondPart>(v) {
            a.cond = p;
        } else if let Some(Operand::Other(p)) = o.get::<ActPart>(v) {
            a.act = p;
        } else {
            return Err("entity_action:set_action: no form of it takes these arguments".into());
        }
        Ok(Vec::new())
    });
    type Done = fn(&Action) -> bool;
    let parts: [(&str, Done); 6] = [
        ("move", |a| a.movement.done),
        ("look", |a| a.look.done),
        ("anim", |a| a.anim.done),
        ("sound", |a| a.sound.done),
        ("particle", |a| a.particle.done),
        ("object", |a| a.object.done),
    ];
    let mut class = class;
    for (name, done) in parts {
        class = class.method(name, move |e: &mut EntityAction, _| {
            done_result(done(&e.0.borrow()))
        });
    }
    let c = ctx.clone();
    class = class.method("time", move |e: &mut EntityAction, _| {
        done_result(e.0.borrow().time(c.now()))
    });
    for name in ["all", "completed"] {
        let c = ctx.clone();
        class = class.method(name, move |e: &mut EntityAction, _| {
            let now = c.now();
            done_result(e.0.borrow_mut().complete(now))
        });
    }
    class
}

/// Finishing an action (2.5): the object's sound for actions stopped and
/// released. TODO(#1140): particles without auto-removal stopped and
/// released.
fn finish(control: &mut ScriptControl, sounds: &RefCell<ScriptSounds>) {
    if let Some(key) = control.sound.take() {
        sounds.borrow_mut().discard(key);
    }
}

fn alive(o: &GameObject) -> bool {
    o.entity.as_ref().is_some_and(|e| e.health > 0.0)
}

/// Runs object `id`'s queue once (2.4) if it is under control: the parts
/// that became done, in order, whose callbacks the caller raises.
fn run_queue(ctx: &Context, id: u16) -> Result<Vec<ActionPart>, String> {
    let now = ctx.now();
    let mut objects = ctx.objects.borrow_mut();
    let Some(o) = objects.get_mut(id) else {
        return Ok(Vec::new());
    };
    let Some(body) = body(o.class) else {
        return Ok(Vec::new());
    };
    // Monsters run it only while alive (2.4).
    if body == Body::Monster && !alive(o) {
        return Ok(Vec::new());
    }
    let current = {
        let Some(control) = o.scripted.as_mut().filter(|c| c.controlled) else {
            return Ok(Vec::new());
        };
        loop {
            let Some(current) = control.queue.front().cloned() else {
                break None;
            };
            let seen = control
                .last
                .as_ref()
                .is_some_and(|l| Rc::ptr_eq(l, &current));
            if !seen {
                control.last = Some(Rc::clone(&current));
                let mut a = current.borrow_mut();
                a.start = Some(now);
                a.checked = false;
                a.sound.started = false;
            }
            if !current.borrow_mut().complete(now) {
                break Some(current);
            }
            finish(control, &ctx.sounds);
            control.queue.pop_front();
        }
    };
    let Some(current) = current else {
        return Ok(Vec::new());
    };
    let mut done = Vec::new();
    apply(ctx, o, body, &mut current.borrow_mut(), &mut done)?;
    Ok(done)
}

/// Applies the current action's parts (2.4 step 2) in their order: look,
/// animation, sound, particle, object, movement, `act`.
fn apply(
    ctx: &Context,
    o: &mut GameObject,
    body: Body,
    a: &mut Action,
    done: &mut Vec<ActionPart>,
) -> Result<(), String> {
    // 5.2. TODO(#1137): monsters', stalkers' and searchlights' turning;
    // a stalker's part without a target (whether it becomes done
    // conflicts in 5.2: unestablished, so left as it is). Other objects:
    // not applied.

    // 6.2. TODO(#1138): the named animation selected, done at its end.

    // 7.2, 7.3.
    if !a.sound.done {
        if body == Body::Monster && a.sound.monster_sound != NONE {
            // TODO(#1139): the monster's own sound; not done by this.
        } else {
            apply_sound(ctx, o, &mut a.sound, done)?;
        }
    }

    // 8. TODO(#1140): the particles played.
    if !a.particle.done {
        if a.particle.particles.is_none() || a.particle.started {
            a.particle.done = true;
            done.push(ActionPart::Particle);
        } else {
            a.particle.started = true;
        }
    }

    // 9.2. TODO(#1141): stalkers' object handling, the car's doors and
    // bones, searchlights' light. An empty name makes the car's done.
    if !a.object.done && body == Body::Car && a.object.name.is_empty() {
        a.object.done = true;
        done.push(ActionPart::Object);
    }

    // 4.4. TODO(#1136): the monsters' and stalkers' movement; callback 18
    // for a movement part that became done (its numbers unestablished,
    // TODO(#1144)). TODO(#1142): the car's controls.
    if !a.movement.done {
        match body {
            Body::Monster if !alive(o) => a.movement.done = true,
            Body::Stalker
                if !alive(o)
                    || matches!(a.movement.target, MoveTarget::Input(_) | MoveTarget::None) =>
            {
                a.movement.done = true;
            }
            // The trader, searchlights and `SCRPTOBJ` objects: the
            // diagnostic goes to M 2.3's routine, which returns at once;
            // the part never becomes done.
            _ => {}
        }
    }

    // 10. TODO(#1142): the monsters' scripted behaviour; never done by
    // applying it. Other objects ignore it.
    Ok(())
}

/// The sound part's base behaviour (7.2) over the object's sound for
/// actions.
fn apply_sound(
    ctx: &Context,
    o: &mut GameObject,
    p: &mut SoundPart,
    done: &mut Vec<ActionPart>,
) -> Result<(), String> {
    let id = o.id;
    let key = o.scripted.as_ref().and_then(|c| c.sound);
    let Some(key) = key else {
        if p.name.is_empty() {
            p.done = true;
            done.push(ActionPart::Sound);
            return Ok(());
        }
        let made = ctx
            .sounds
            .borrow_mut()
            .create(&p.name, p.sound_type.unwrap_or(0));
        let key = made.map_err(|e| ctx.crash("sound", &e))?;
        if let Some(control) = o.scripted.as_mut() {
            control.sound = Some(key);
        }
        return Ok(());
    };
    let mut sounds = ctx.sounds.borrow_mut();
    if sounds.emitter(key).is_some() {
        return Ok(());
    }
    if !p.started {
        let position = if p.bone.is_empty() {
            p.position
        } else {
            bone_position(o, &p.bone)
        };
        let flags = PlayFlags {
            looped: p.looped,
            two_d: false,
            delay: 0.0,
        };
        sounds.play(key, flags, Some(position), id);
        p.started = true;
    } else {
        p.done = true;
        done.push(ActionPart::Sound);
    }
    Ok(())
}

/// The members of 2.2 and 2.3 (`args[0]` names the member, `args[1]` is
/// the handle): a member on an object that isn't scripted does nothing
/// and gives its fallback (2.1).
fn member(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    ctx.stopped()?;
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("scripted actions: member expected".into()),
    };
    let what = format!("game_object:{op}");
    let Some(handle) = maker.get::<Handle>(arg(args, 1)) else {
        return Err(format!("{what}: game object expected"));
    };
    let id = handle.id;
    let args = args.get(2..).unwrap_or(&[]);
    let fallback = || -> Results {
        Ok(match op.as_str() {
            "get_script" | "can_script_capture" => vec![Value::Boolean(false)],
            "get_script_name" => vec![Value::String(Vec::new())],
            "action" | "action_by_index" => vec![Value::Nil],
            "action_count" => vec![Value::Number(0.0)],
            _ => Vec::new(),
        })
    };
    let mut objects = ctx.objects.borrow_mut();
    let Some(o) = objects.get_mut(id) else {
        return Err(format!("game object {id} no longer exists"));
    };
    let spawned = o.handle().is_some();
    let Some(control) = o.scripted.as_mut() else {
        return fallback();
    };
    match op.as_str() {
        // 2.2.
        "script" => {
            let on = m_boolean(args, 0, &what)?;
            let name = m_text(args, 1, &what)?;
            if !control.controlled && on {
                if control.capture {
                    control.controlled = true;
                    control.name = name;
                }
            } else if control.controlled && !on && !control.name.is_empty() && control.name == name
            {
                control.controlled = false;
                control.name.clear();
                reset_queue(control, &ctx.sounds);
            }
            // Otherwise "Invalid sequence of taking an entity under script
            // control" goes to M 2.3's routine, which returns at once.
            Ok(Vec::new())
        }
        "get_script" => Ok(vec![Value::Boolean(control.controlled)]),
        "get_script_name" => Ok(vec![Value::String(control.name.clone())]),
        "can_script_capture" => Ok(vec![Value::Boolean(control.capture)]),
        // 2.3.
        "command" => {
            let Some(act) = maker.get::<EntityAction>(arg(args, 0)) else {
                return Err(format!("{what}: entity_action expected"));
            };
            let first = m_boolean(args, 1, &what)?;
            let copy = Rc::new(RefCell::new(act.0.borrow().clone()));
            let was_empty = control.queue.is_empty();
            if !first || was_empty {
                control.queue.push_back(copy);
            } else {
                finish(control, &ctx.sounds);
                if let Some(current) = control.queue.pop_front() {
                    let again = Rc::new(RefCell::new(current.borrow().clone()));
                    control.queue.push_front(again);
                }
                control.queue.push_front(copy);
            }
            let run = was_empty && spawned;
            drop(objects);
            if run {
                // Our reading: only an object under control runs it (2.4).
                let done = run_queue(ctx, id)?;
                raise_done(ctx, id, &done)?;
            }
            Ok(Vec::new())
        }
        "action" => Ok(vec![match control.queue.front() {
            Some(a) => maker.new_object("entity_action", EntityAction::copy_of(&a.borrow())),
            None => Value::Nil,
        }]),
        "action_count" => Ok(vec![Value::Number(control.queue.len() as f64)]),
        "action_by_index" => {
            let i = m_int(args, 0, &what)?;
            match usize::try_from(i).ok().and_then(|i| control.queue.get(i)) {
                Some(a) => Ok(vec![
                    maker.new_object("entity_action", EntityAction(Rc::clone(a))),
                ]),
                // A read past the queue, believed invalid (2.3): fatal
                // here (our wording).
                None => {
                    let count = control.queue.len();
                    drop(objects);
                    Err(ctx.crash(
                        &what,
                        &format!("action index {i} is outside the queue of {count}"),
                    ))
                }
            }
        }
        "reset_action_queue" => {
            reset_queue(control, &ctx.sounds);
            Ok(Vec::new())
        }
        _ => Err(format!("{what}: no such member")),
    }
}

/// `reset_action_queue` (2.3): the current action finished, every queued
/// action removed.
fn reset_queue(control: &mut ScriptControl, sounds: &RefCell<ScriptSounds>) {
    if !control.queue.is_empty() {
        finish(control, sounds);
    }
    control.queue.clear();
}

/// The callbacks of the parts that became done (2.4), from inside a member.
fn raise_done(ctx: &Context, id: u16, done: &[ActionPart]) -> Result<(), String> {
    for &part in done {
        let event = ObjectEvent::ActionDone {
            object: Some(id),
            part,
        };
        raise_inside(&ctx.objects, &ctx.services, &ctx.fatal, id, &event)?;
    }
    Ok(())
}

/// What a state with the scripted actions registered keeps.
pub(crate) struct Registered {
    ctx: Context,
}

impl Scripts {
    /// Registers script API V's names (1.1) over the game object basics'
    /// objects and services and the sound's sounds: after both, before
    /// [`Scripts::load_globals`].
    pub fn register_scripted_actions(&mut self) -> Result<(), Error> {
        let api = |message: &str| Error {
            kind: ErrorKind::Api,
            message: message.into(),
        };
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let sounds = (self.script_sounds()).ok_or_else(|| api("the sound isn't registered"))?;
        let ctx = Context {
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            sounds,
            fatal: self.fatal_flag(),
        };
        self.register_class(constants(move_class(), &MOVE_CONSTANTS))?;
        self.register_class(constants(look_class(), &LOOK_CONSTANTS))?;
        self.register_class(constants(anim_class(), &ANIM_CONSTANTS))?;
        self.register_class(constants(sound_class(&ctx), &SOUND_CONSTANTS))?;
        self.register_class(particle_class())?;
        self.register_class(constants(object_class(), &OBJECT_CONSTANTS))?;
        self.register_class(constants(cond_class(), &COND_CONSTANTS))?;
        self.register_class(constants(act_class(), &ACT_CONSTANTS))?;
        self.register_class(entity_action_class(&ctx))?;
        let c = ctx.clone();
        let lua = self.lua_mut();
        lua.register_with_objects("__volya_scripted_actions", move |maker, args| {
            member(&c, maker, args)
        })?;
        lua.exec("=scripted_actions", include_bytes!("scripted_actions.lua"))?;
        self.scripted_actions_state = Some(Registered { ctx });
        Ok(())
    }

    /// The engine runs object `id`'s queue in its scheduled update (2.4,
    /// believed), if it is under control, and raises the callbacks of the
    /// parts that became done after applying them (our placement).
    pub fn run_action_queue(&mut self, id: u16) -> Result<(), Error> {
        let Some(state) = self.scripted_actions_state.as_ref() else {
            return Ok(());
        };
        let ctx = state.ctx.clone();
        let done = run_queue(&ctx, id).map_err(|message| {
            let kind = if ctx.fatal.borrow().is_some() {
                ErrorKind::Fatal
            } else {
                ErrorKind::Runtime
            };
            Error { kind, message }
        })?;
        for part in done {
            let event = ObjectEvent::ActionDone {
                object: Some(id),
                part,
            };
            self.raise(id, &event)?;
        }
        Ok(())
    }
}
