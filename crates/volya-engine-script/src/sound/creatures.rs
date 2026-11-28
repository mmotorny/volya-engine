// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The creature members of script API T (docs/behavior/script-api-sound.md
//! sections 6 and 7): each fighter's sound player (its collections, mask,
//! queued sounds and sound prefix), its hearing thresholds, the
//! characters' voice prefixes, the trader's external sound and the
//! remembered sounds. What these do over time is the creatures' own
//! (CH-25, CH-14, CH-15); here is what each call does at once.

use std::collections::{BTreeMap, HashMap};

use volya_engine_formats::model::transform_point;

use super::{Context, FILE_TYPE, SoundInfo, SoundKey, arg, integer, items, number, numbered, text};
use crate::Value;
use crate::classes::ObjectMaker;
use crate::dialogs::Event as DialogEvent;
use crate::game_object::members::Handle;
use crate::game_object::{GameObject, Kinds, ObjectClass};
use crate::values::to_f32;

/// The bone a collection plays at when `add_sound` names none (6.2).
pub const DEFAULT_BONE: &[u8] = b"bip01_head";

/// The base hearing threshold when the section has no `sound_threshold`
/// (7.1).
pub const DEFAULT_THRESHOLD: f32 = 0.05;

/// The 32-bit linear congruential draw of 6.3 step 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Draw(pub u32);

impl Draw {
    /// An integer drawn uniformly from 0 … `range` − 1: the state
    /// advanced (multiplier 134775813, increment 1), scaled by the high 32
    /// bits of its product with `range`.
    pub fn below(&mut self, range: u32) -> u32 {
        self.0 = self.0.wrapping_mul(134_775_813).wrapping_add(1);
        ((u64::from(self.0) * u64::from(range)) >> 32) as u32
    }

    /// A time drawn from `min` and `max` (6.3 step 6): 0 when `max` is 0;
    /// `max` when it is not greater than `min`; otherwise `min` plus an
    /// integer below `max − min`.
    pub fn between(&mut self, min: u32, max: u32) -> u32 {
        if max == 0 {
            0
        } else if max <= min {
            max
        } else {
            min + self.below(max - min)
        }
    }
}

/// A collection of a sound player (6.2).
#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    /// The AI sound type its sounds are created with (6.2).
    pub ai_type: i32,
    pub priority: i32,
    pub mask: u32,
    pub bone: Vec<u8>,
    /// Its sounds' names.
    pub sounds: Vec<Vec<u8>>,
    /// The sound the random pick gave last (6.3 step 4).
    pub last: Option<usize>,
    /// TODO(#1101): where the random state starts and whether the pick
    /// and the time draws share it are review gaps: one state per
    /// collection, from 0, for both (our choice).
    pub draw: Draw,
}

/// A queued sound (6.3).
#[derive(Clone, Debug, PartialEq)]
pub struct QueuedSound {
    pub priority: i32,
    pub mask: u32,
    pub bone: Vec<u8>,
    /// Its own sound.
    pub key: SoundKey,
    /// Start and stop time, ms of `time_global()`.
    pub start: u32,
    pub stop: u32,
}

/// A fighter's sound player (6).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundPlayer {
    /// By internal type.
    pub collections: BTreeMap<u32, Collection>,
    /// Believed 0 before any call (6.4).
    pub mask: u32,
    pub queued: Vec<QueuedSound>,
    /// The sound prefix (6.5).
    pub prefix: Option<Vec<u8>>,
}

/// A fighter's hearing thresholds (7.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hearing {
    pub current: f32,
    pub base: f32,
}

/// The creatures' sound state, by object id.
#[derive(Default)]
pub struct Creatures {
    pub players: HashMap<u16, SoundPlayer>,
    pub hearing: HashMap<u16, Hearing>,
    /// The characters' `snd_config` texts (6.5), which the engine gives
    /// as they spawn.
    pub voice_prefixes: HashMap<u16, Vec<u8>>,
}

impl Creatures {
    /// The object is gone: its sound state goes.
    pub fn forget(&mut self, id: u16) {
        self.players.remove(&id);
        self.hearing.remove(&id);
        self.voice_prefixes.remove(&id);
    }
}

fn is_trader(o: &GameObject) -> bool {
    matches!(o.class, ObjectClass::Named("CAI_Trader"))
}

/// The world position of bone `bone` on `o`'s model; the object's own
/// position when its visual lacks the bone (unestablished which bone, 6.3:
/// our choice).
fn bone_position(o: &GameObject, bone: &[u8]) -> [f32; 3] {
    let local = (o.visual.as_ref())
        .and_then(|v| v.bone(bone).and_then(|b| v.bones.get(b)))
        .map(|(_, t)| [t[3][0], t[3][1], t[3][2]]);
    match local {
        Some(p) => transform_point(&o.placement, p),
        None => {
            let p = o.placement[3];
            [p[0], p[1], p[2]]
        }
    }
}

/// Whether queued sound `q` goes when the mask `mask` is applied (6.3
/// step 3): its mask shares a bit, or it has no emitter and its stop time
/// has come.
fn goes(s: &super::ScriptSounds, q: &QueuedSound, mask: u32, now: u32) -> bool {
    q.mask & mask != 0 || (s.emitter(q.key).is_none() && now >= q.stop)
}

/// Removes the queued sounds that go under `mask`, each stopped at once
/// (believed: its sound is discarded).
fn remove_queued(s: &mut super::ScriptSounds, id: u16, mask: u32, now: u32) {
    let Some(player) = s.creatures.players.get_mut(&id) else {
        return;
    };
    let queued = std::mem::take(&mut player.queued);
    let (gone, kept): (Vec<_>, Vec<_>) = queued.into_iter().partition(|q| goes(s, q, mask, now));
    if let Some(player) = s.creatures.players.get_mut(&id) {
        player.queued = kept;
    }
    for q in gone {
        s.discard(q.key);
    }
}

type Results = Result<Vec<Value>, String>;

fn none() -> Results {
    Ok(Vec::new())
}

fn one(v: impl Into<Value>) -> Results {
    Ok(vec![v.into()])
}

/// The `game_object` members of 6 and 7: `args[0]` is the object's handle.
pub(crate) fn member(ctx: &Context, maker: &ObjectMaker, op: &str, args: &[Value]) -> Results {
    let id = maker
        .get::<Handle>(arg(args, 0))
        .map(|h| h.id)
        .ok_or_else(|| format!("{op}: game object expected"))?;
    let args = args.get(1..).unwrap_or(&[]);
    let (kinds, trader, section) = match ctx.objects.borrow().get(id) {
        Some(o) => (o.kinds, is_trader(o), o.section.clone()),
        None => return Err(format!("game object {id} no longer exists")),
    };
    let fighter = kinds.has(Kinds::FIGHTER);
    let now = ctx.now();
    match op {
        // 6.2.
        "add_sound" => {
            let prefix = text(args, 0, op)?;
            let n = integer(args, 1, op)?;
            let ai_type = integer(args, 2, op)? as i32;
            let priority = integer(args, 3, op)? as i32;
            let mask = integer(args, 4, op)?;
            let internal = integer(args, 5, op)?;
            let bone = match args.get(6) {
                Some(_) => text(args, 6, op)?,
                None => DEFAULT_BONE.to_vec(),
            };
            if !fighter {
                return one(0.0);
            }
            ctx.with_sounds(op, line!(), |s| {
                let player = s.creatures.players.entry(id).or_default();
                if player.collections.contains_key(&internal) {
                    return one(0.0);
                }
                // TODO(#1101): what an object without a sound prefix does
                // is a review gap: the empty prefix (our choice).
                let p = player.prefix.clone().unwrap_or_default();
                let mut sounds = Vec::new();
                for item in items(&prefix) {
                    let mut base = p.clone();
                    base.extend_from_slice(&item);
                    let names =
                        std::iter::once(base.clone()).chain((0..n).map(|i| numbered(&base, i)));
                    for name in names {
                        if s.exists(&name)? {
                            // Created (6.2): its file loaded now, fatal when
                            // it can't be (S1 5).
                            s.load(&name)?;
                            sounds.push(name);
                        }
                    }
                }
                let count = sounds.len();
                let collection = Collection {
                    ai_type,
                    priority,
                    mask,
                    bone,
                    sounds,
                    last: None,
                    draw: Draw::default(),
                };
                (s.creatures.players.entry(id).or_default().collections)
                    .insert(internal, collection);
                one(count as f64)
            })
        }
        // 6.3.
        "play_sound" => {
            let internal = integer(args, 0, op)?;
            let time = |i: usize| match args.get(i) {
                Some(_) => integer(args, i, op),
                None => Ok(0),
            };
            let (max_start, min_start, max_stop, min_stop) =
                (time(1)?, time(2)?, time(3)?, time(4)?);
            let index = match args.get(5) {
                Some(_) => integer(args, 5, op)? as i32,
                None => -1,
            };
            if !fighter {
                return none();
            }
            let mut s = ctx.state.borrow_mut();
            let Some(player) = s.creatures.players.get(&id) else {
                return none();
            };
            // Step 1.
            let Some(c) = player.collections.get(&internal) else {
                return none();
            };
            let blocked = player.mask & c.mask != 0
                || (player.queued.iter()).any(|q| q.mask & c.mask != 0 && q.priority <= c.priority);
            // Step 2.
            if blocked || c.sounds.is_empty() {
                return none();
            }
            let (mask, count) = (c.mask, c.sounds.len());
            // Step 3.
            remove_queued(&mut s, id, mask, now);
            let player = s.creatures.players.get_mut(&id).expect("kept above");
            let c = player.collections.get_mut(&internal).expect("kept above");
            // Step 4.
            let chosen = if index != -1 {
                index as usize
            } else if count >= 3 {
                // TODO(#1101): how the pick scales its draw is a review gap:
                // uniform over the others (our choice).
                let i = c.draw.below(count as u32 - 1) as usize;
                let i = match c.last {
                    Some(last) if i >= last => i + 1,
                    _ => i,
                };
                c.last = Some(i);
                i
            } else {
                let i = c.draw.below(count as u32) as usize;
                c.last = Some(i);
                i
            };
            let Some(name) = c.sounds.get(chosen).cloned() else {
                drop(s);
                // An index past the end is unestablished (6.3): fatal here.
                return Err(ctx.fatal(op, line!(), "play_sound: no sound of that index"));
            };
            // Step 6's draws.
            let d1 = c.draw.between(min_start, max_start);
            let d2 = c.draw.between(min_stop, max_stop);
            let (priority, bone) = (c.priority, c.bone.clone());
            drop(s);
            // Step 5: the file's own AI sound type, the object its owner.
            let key = ctx.with_sounds(op, line!(), |s| s.create(&name, FILE_TYPE))?;
            let mut s = ctx.state.borrow_mut();
            let duration = s.sound(key).map_or(0, |x| x.file.duration_ms) as u32;
            let start = now.wrapping_add(d1);
            let stop = start.wrapping_add(duration).wrapping_add(d2);
            // Step 7.
            if let Some(player) = s.creatures.players.get_mut(&id) {
                player.queued.push(QueuedSound {
                    priority,
                    mask,
                    bone: bone.clone(),
                    key,
                    start,
                    stop,
                });
            }
            // TODO(#975): a queued sound whose start time comes later is
            // started, follows its bone and ends by the creature's own
            // sound update (CH-25).
            if d1 == 0 {
                let objects = ctx.objects.borrow();
                let position = objects
                    .get(id)
                    .map_or([0.0; 3], |o| bone_position(o, &bone));
                s.play(key, super::play_flags(0, 0.0), Some(position), id);
            }
            none()
        }
        // 6.4.
        "remove_sound" => {
            let internal = integer(args, 0, op)?;
            if fighter && let Some(player) = ctx.state.borrow_mut().creatures.players.get_mut(&id) {
                player.collections.remove(&internal);
            }
            none()
        }
        "set_sound_mask" => {
            let mask = integer(args, 0, op)?;
            if fighter {
                let mut s = ctx.state.borrow_mut();
                s.creatures.players.entry(id).or_default().mask = mask;
                remove_queued(&mut s, id, mask, now);
            }
            none()
        }
        "active_sound_count" => {
            let only_playing = matches!(args.first(), Some(Value::Boolean(true)));
            if !fighter {
                return one(-1.0);
            }
            let s = ctx.state.borrow();
            let count = (s.creatures.players.get(&id)).map_or(0, |p| {
                (p.queued.iter())
                    .filter(|q| s.emitter(q.key).is_some() || (!only_playing && now >= q.start))
                    .count()
            });
            one(count as f64)
        }
        // 6.5.
        "sound_prefix" => {
            if !args.is_empty() {
                let prefix = text(args, 0, op)?;
                if fighter {
                    let mut s = ctx.state.borrow_mut();
                    s.creatures.players.entry(id).or_default().prefix = Some(prefix);
                }
                return none();
            }
            let s = ctx.state.borrow();
            let prefix = fighter
                .then(|| s.creatures.players.get(&id).and_then(|p| p.prefix.clone()))
                .flatten();
            one(prefix.map_or(Value::Nil, Value::String))
        }
        "sound_voice_prefix" => {
            if !kinds.has(Kinds::INVENTORY_OWNER) {
                return one(Value::Nil);
            }
            let s = ctx.state.borrow();
            let prefix = s
                .creatures
                .voice_prefixes
                .get(&id)
                .cloned()
                .unwrap_or_default();
            one(Value::String(prefix))
        }
        // 6.6: TC2 2.5 and 2.6, carried out by the trader's motion channels
        // (dialogs' events).
        "external_sound_start" => {
            let sound = text(args, 0, op)?;
            if trader {
                let event = DialogEvent::ExternalSoundStart { trader: id, sound };
                ctx.dialogs.borrow_mut().push_event(event);
            }
            none()
        }
        "external_sound_stop" => {
            if trader {
                let event = DialogEvent::ExternalSoundStop { trader: id };
                ctx.dialogs.borrow_mut().push_event(event);
            }
            none()
        }
        // 7.1.
        "set_sound_threshold" | "restore_sound_threshold" => {
            let value = match op {
                "set_sound_threshold" => Some(to_f32(number(args, 0, op)?)),
                _ => None,
            };
            if !fighter {
                return none();
            }
            let base = base_threshold(ctx, section.as_deref());
            let mut s = ctx.state.borrow_mut();
            let hearing = s.creatures.hearing.entry(id).or_insert(Hearing {
                current: base,
                base,
            });
            hearing.current = value.unwrap_or(hearing.base);
            none()
        }
        // 7.2: the Lua side gives the iterator.
        "memory_sound_objects" => {
            if !fighter {
                // Read through no object (7.2): fatal here.
                return Err(ctx.fatal(op, line!(), "memory_sound_objects: not a fighter"));
            }
            // TODO(#964): the remembered sounds are the creature's memory
            // (CH-15); none is remembered yet.
            one(Value::Table(Vec::new()))
        }
        // 7.3.
        "get_sound_info" => {
            // TODO(#964): a monster's remembered sounds are its memory's
            // (CH-15): none is remembered yet, so every field is zero, as
            // for an object that is not a monster.
            one(maker.new_object("SoundInfo", SoundInfo::default()))
        }
        _ => Err(format!("sound: unknown operation {op}")),
    }
}

/// The base threshold (7.1): the object's section's `sound_threshold`, 0.05
/// when the key is absent (read at first use: our choice).
fn base_threshold(ctx: &Context, section: Option<&[u8]>) -> f32 {
    let section = section.map(volya_engine_formats::cp1251::decode);
    let files = ctx.files.borrow();
    section
        .and_then(|s| {
            files
                .system
                .get(&s, "sound_threshold")
                .flatten()
                .map(str::to_owned)
        })
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(DEFAULT_THRESHOLD)
}
