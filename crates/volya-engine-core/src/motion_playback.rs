// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Motion playback (docs/behavior/motion-playback.md, TC1): the playbacks
//! of an animated skeleton model's motions, their start on partitions
//! (section 3) and as fx motions (6), the tracks update that advances them
//! (4, 5), and the pose they give each bone (7) as the evaluation of
//! docs/formats/skeletal-motions.md section 5 blends it; the render
//! matrices *K* = *B*⁻¹ · *T* of docs/behavior/model-shading.md 2.
//!
//! A notification (5.2) is a value of the starting code's choosing,
//! handed back by [`AnimatedModel::take_notifications`] in the order the
//! ends were reached, after the update that reached them.
// TODO(#996): the spec leaves open what a playback started or ended by a
// notification during a tracks update does in that update, and a new
// playback's last-advanced frame (here: none, so it can be advanced in the
// frame it starts in).

use std::fmt;
use std::sync::Arc;

use volya_engine_formats::model::Mat4;
use volya_engine_formats::skeletal_motions::{Motion, MotionDefinition, MotionParams, quantize};

/// f32(0.0015259022): speed and power per unit of *q* (2.1).
pub const SPEED_UNIT: f32 = 0.001_525_902_2;
/// f32(0.0022888533): accrue and falloff per unit of *q* (2.1).
pub const RATE_UNIT: f32 = 0.002_288_853_3;
/// f32(1/30): one key's time (skeletal-motions 5.1).
pub const KEY_TIME: f32 = 1.0 / 30.0;
/// The weight a mixing start and an fx motion start at, f32(1e−7) (2.2, 6).
pub const START_WEIGHT: f32 = 1e-7;
/// The longest step of a tracks update, milliseconds (4.1).
pub const MAX_STEP_MS: u32 = 66;
/// A cycle with fade at end falls off at this rate, unscaled (5.1).
pub const END_FALLOFF: f32 = 2.0;
/// Playbacks of a model in all (8), fx playbacks (6), playbacks of a bone
/// (8).
pub const MAX_PLAYBACKS: usize = 256;
pub const MAX_FX: usize = 16;
pub const MAX_BONE_PLAYBACKS: usize = 16;
/// A definition's "bone or partition" for every partition (cycles) or the
/// root bone (fx) (3.2, 6).
pub const ALL_PARTITIONS: u16 = 0xFFFF;
/// Partitions and channels a model has at most (3.1, 1).
pub const PARTITIONS: usize = 4;
pub const CHANNELS: usize = 4;
/// Flags bit 1: stop at end (5.3).
pub const STOP_AT_END: u16 = 0x2;
/// The key count is kept in 24 bits (player-cycle-lengths.md 3).
const KEY_COUNT_MASK: u32 = 0x00FF_FFFF;
/// f32(1e−5): the interpolation's ε (skeletal-motions 5.2) and the scale's
/// limits (7.2).
const EPSILON: f32 = 1e-5;
/// f32(1e−7): a weight sum below it gives weight 0 (7.1).
const ZERO_SUM: f32 = 1e-7;
/// The arccos polynomial of skeletal-motions 5.2.
const HALF_PI: f32 = 1.570_796_4;
const ACOS_A: f32 = 2.838_933;
const ACOS_B: f32 = 3.853_735;
const ACOS_C: f32 = 1.693_204;
const ACOS_D: f32 = 0.892_399;

/// A fatal error of playback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fatal {
    /// Section 8: a 257th playback.
    TooManyPlaybacks,
    /// Section 6: no fx motion of that name, a failed check.
    NoFx(String),
}

impl Fatal {
    /// The failed check's expression and description (6), if it is one.
    pub fn check(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Fatal::NoFx(_) => Some(("motion_ID.valid()", "! MODEL: can't find FX: ")),
            Fatal::TooManyPlaybacks => None,
        }
    }
}

impl fmt::Display for Fatal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fatal::TooManyPlaybacks => f.write_str("Too many blended motions requisted"),
            Fatal::NoFx(name) => write!(f, "! MODEL: can't find FX: {name}"),
        }
    }
}

/// A motion of a model: its motion set (the model's motion file, 0 for
/// embedded motions) and its definition there (skeletal-motions 4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MotionId {
    pub set: u16,
    pub definition: u16,
}

/// A motion set as played: its parameters, its motions and, for each bone
/// of the model, the track of that bone (skeletal-motions 4.1: by name).
#[derive(Debug)]
struct PlayedSet {
    params: MotionParams,
    motions: Vec<Motion>,
    track_of_bone: Vec<Option<u16>>,
}

/// What an animated skeleton model plays from, shared by its instances:
/// its bones (names, parents and inverse bind matrices), its motion sets
/// and its partitions' bones.
#[derive(Debug)]
pub struct Motions {
    parents: Vec<Option<usize>>,
    /// The bones with every parent before its children.
    order: Vec<usize>,
    children: Vec<Vec<usize>>,
    root: usize,
    inverse_binds: Vec<Mat4>,
    sets: Vec<PlayedSet>,
    /// The model bones of each partition of motion file 0, in the order of
    /// its bone entries (skeletal-motions 4.3); a name the model lacks is
    /// left out (our choice: every shipped set's names are its model's).
    partitions: Vec<Vec<usize>>,
}

impl Motions {
    /// The motions of a model whose bones are `names` with `parents` and
    /// bind matrices `binds` (skeleton-ogf 4), from its motion sets in the
    /// order of its motion files, each with its motions (one per
    /// definition, decoded with this model's bone count).
    pub fn new(
        names: &[&str],
        parents: &[Option<usize>],
        binds: &[Mat4],
        sets: Vec<(MotionParams, Vec<Motion>)>,
    ) -> Self {
        let n = names.len();
        let mut children = vec![Vec::new(); n];
        let mut root = 0;
        for (i, p) in parents.iter().enumerate() {
            match p {
                Some(p) => children[*p].push(i),
                None => root = i,
            }
        }
        let mut order = Vec::with_capacity(n);
        let mut placed = vec![false; n];
        for start in 0..n {
            let mut chain = vec![start];
            while let Some(p) = parents[*chain.last().unwrap()] {
                if placed[p] || chain.len() > n {
                    break;
                }
                chain.push(p);
            }
            for &i in chain.iter().rev() {
                if !placed[i] {
                    placed[i] = true;
                    order.push(i);
                }
            }
        }
        let bone = |name: &str| names.iter().position(|b| *b == name);
        let sets: Vec<PlayedSet> = sets
            .into_iter()
            .map(|(params, motions)| {
                let mut track_of_bone = vec![None; n];
                for partition in &params.partitions {
                    for (name, track) in &partition.bones {
                        if let Some(b) = bone(name) {
                            track_of_bone[b] = Some(*track);
                        }
                    }
                }
                PlayedSet {
                    params,
                    motions,
                    track_of_bone,
                }
            })
            .collect();
        let partitions = sets.first().map_or(Vec::new(), |s| {
            (s.params.partitions.iter())
                .map(|p| p.bones.iter().filter_map(|(name, _)| bone(name)).collect())
                .collect()
        });
        Self {
            parents: parents.to_vec(),
            order,
            children,
            root,
            inverse_binds: binds.iter().map(inverse_affine).collect(),
            sets,
            partitions,
        }
    }

    /// How many bones the model has.
    pub fn bone_count(&self) -> usize {
        self.parents.len()
    }

    /// The definition of `motion`.
    pub fn definition(&self, motion: MotionId) -> &MotionDefinition {
        &self.sets[usize::from(motion.set)].params.definitions[usize::from(motion.definition)]
    }

    /// The motion named `name` of the last motion file holding one of that
    /// kind (skeletal-motions 4.2; fx or cycle), names compared byte for
    /// byte; within a file its first.
    fn find(&self, name: &str, fx: bool) -> Option<MotionId> {
        self.sets.iter().enumerate().rev().find_map(|(s, set)| {
            (set.params.definitions.iter())
                .position(|d| d.is_fx() == fx && d.name == name)
                .map(|d| MotionId {
                    set: s as u16,
                    definition: d as u16,
                })
        })
    }

    /// Every definition with its motion, in the order of the motion sets
    /// and of their definitions.
    pub fn definitions(&self) -> impl Iterator<Item = (MotionId, &MotionDefinition)> {
        self.sets.iter().enumerate().flat_map(|(s, set)| {
            (set.params.definitions.iter().enumerate()).map(move |(d, def)| {
                let id = MotionId {
                    set: s as u16,
                    definition: d as u16,
                };
                (id, def)
            })
        })
    }

    /// The cycle named `name` (skeletal-motions 4.2).
    pub fn cycle(&self, name: &str) -> Option<MotionId> {
        self.find(name, false)
    }

    /// The fx motion named `name`, the last motion set first (6).
    pub fn fx(&self, name: &str) -> Option<MotionId> {
        self.find(name, true)
    }

    /// How many partitions the model has (those of motion file 0).
    pub fn partition_count(&self) -> usize {
        self.partitions.len()
    }

    /// The model bones partition `i` applies to.
    pub fn partition_bones(&self, i: usize) -> &[usize] {
        &self.partitions[i]
    }

    /// The motion of `motion` and its track on `bone`, if the set has one.
    fn track(&self, motion: MotionId, bone: usize) -> Option<(&Motion, usize)> {
        let set = &self.sets[usize::from(motion.set)];
        let m = set.motions.get(usize::from(motion.definition))?;
        let t = usize::from(set.track_of_bone[bone]?);
        (t < m.tracks.len()).then_some((m, t))
    }

    /// The motion's length *L* = f32(*N* · f32(1/30)).
    pub fn length(&self, motion: MotionId) -> f32 {
        let set = &self.sets[usize::from(motion.set)];
        let keys = set
            .motions
            .get(usize::from(motion.definition))
            .map_or(0, |m| m.keys);
        (keys & KEY_COUNT_MASK) as f32 * KEY_TIME
    }
}

/// A playback's value from a quantized number *q* (2.1): speed and power.
pub fn speed_value(q: u16) -> f32 {
    f32::from(q) * SPEED_UNIT
}

/// Accrue and falloff (2.1).
pub fn rate_value(q: u16) -> f32 {
    f32::from(q) * RATE_UNIT
}

/// Whether a playback is fading in or out (1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fade {
    In,
    Out,
}

/// One motion being played on a model (1).
#[derive(Clone, Debug, PartialEq)]
pub struct Playback {
    pub motion: MotionId,
    /// A cycle's partition; `None` for an fx motion.
    pub partition: Option<u8>,
    /// *t*, seconds.
    pub time: f32,
    /// *a* and the target *P*.
    pub weight: f32,
    pub target: f32,
    pub accrue: f32,
    /// Its own falloff rate: 0 until it is faded out (2.2, 3.3).
    pub falloff: f32,
    pub speed: f32,
    /// The definition's falloff (2.1, lowered for a cycle): the rate the
    /// playbacks this cycle ends fade out at (3.3).
    pub cycle_falloff: f32,
    pub fade: Fade,
    pub advancing: bool,
    pub stop_at_end: bool,
    pub fade_at_end: bool,
    pub channel: u8,
    pub notification: Option<u32>,
    /// The motion's length *L* (4.2).
    pub length: f32,
    /// The frame it was last advanced in (4.1).
    last_frame: Option<u32>,
    /// The bones it is applied to.
    bones: Vec<u16>,
}

/// A playback of a model, valid while it plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaybackId(u16);

/// How a cycle is started (2.2, 3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    /// The partition the starting code names; `None`: the definition's.
    pub partition: Option<u16>,
    /// Fades in (and fades out what it ends), else replaces at once.
    pub mixing: bool,
    pub notification: Option<u32>,
    pub channel: u8,
}

impl Start {
    /// A script's `PlayCycle` and the game's own starts (3.4): the
    /// definition's partition, mixing, channel 0.
    pub fn mixing(notification: Option<u32>) -> Self {
        Self {
            partition: None,
            mixing: true,
            notification,
            channel: 0,
        }
    }
}

/// The device's clock a tracks update reads (4.1): `time_global()` in
/// milliseconds and the frame number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    pub time_global: u32,
    pub frame: u32,
}

/// A playback that reached its end with a notification (5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notified {
    pub playback: PlaybackId,
    pub motion: MotionId,
    pub partition: Option<u8>,
    pub value: u32,
}

/// A bone's rotation **q** and translation **p** (skeletal-motions 5).
type Value = ([f32; 4], [f32; 3]);

/// The playbacks of one animated model and their state.
#[derive(Debug)]
pub struct AnimatedModel {
    motions: Arc<Motions>,
    slots: Vec<Option<Playback>>,
    live: usize,
    by_bone: Vec<Vec<u16>>,
    by_partition: [Vec<u16>; PARTITIONS],
    fx: Vec<u16>,
    /// The channels' factors (7.2), 1 at the start; channel 0's is not
    /// used.
    pub factors: [f32; CHANNELS],
    /// `time_global()` at the previous tracks update: before the first,
    /// believed 0 (4.1).
    previous: u32,
    notified: Vec<Notified>,
}

impl AnimatedModel {
    pub fn new(motions: Arc<Motions>) -> Self {
        let bones = motions.bone_count();
        Self {
            motions,
            slots: Vec::new(),
            live: 0,
            by_bone: vec![Vec::new(); bones],
            by_partition: Default::default(),
            fx: Vec::new(),
            factors: [1.0; CHANNELS],
            previous: 0,
            notified: Vec::new(),
        }
    }

    pub fn motions(&self) -> &Arc<Motions> {
        &self.motions
    }

    /// The playback `id`, while it plays.
    pub fn playback(&self, id: PlaybackId) -> Option<&Playback> {
        self.slots.get(usize::from(id.0))?.as_ref()
    }

    /// The playback `id` to change, while it plays: game code may set its
    /// values (section 10).
    pub fn playback_mut(&mut self, id: PlaybackId) -> Option<&mut Playback> {
        self.slots.get_mut(usize::from(id.0))?.as_mut()
    }

    /// Every playback, with its id, in slot order.
    pub fn playbacks(&self) -> impl Iterator<Item = (PlaybackId, &Playback)> {
        (self.slots.iter().enumerate())
            .filter_map(|(i, p)| Some((PlaybackId(i as u16), p.as_ref()?)))
    }

    /// The playbacks of partition `i`, in the order they were started.
    pub fn partition_playbacks(&self, i: usize) -> impl Iterator<Item = &Playback> {
        (self.by_partition[i].iter()).filter_map(|&s| self.slots[usize::from(s)].as_ref())
    }

    /// The fx playbacks, in the order they were started.
    pub fn fx_playbacks(&self) -> impl Iterator<Item = &Playback> {
        (self.fx.iter()).filter_map(|&s| self.slots[usize::from(s)].as_ref())
    }

    /// The playbacks applied to `bone`, in the order they were applied.
    pub fn bone_playbacks(&self, bone: usize) -> impl Iterator<Item = PlaybackId> + '_ {
        self.by_bone[bone].iter().map(|&s| PlaybackId(s))
    }

    /// The notifications of the ends reached so far, in their order (5.2).
    pub fn take_notifications(&mut self) -> Vec<Notified> {
        std::mem::take(&mut self.notified)
    }

    /// Starts the cycle `motion` (3.1, 3.2): on its definition's partition
    /// or the one `start` names; on every partition for 0xFFFF, when no
    /// playback is handed back.
    pub fn play_cycle(
        &mut self,
        motion: MotionId,
        start: Start,
        clock: Clock,
    ) -> Result<Option<PlaybackId>, Fatal> {
        let d = self.motions.definition(motion);
        let partition = start.partition.unwrap_or(d.bone_or_partition);
        if partition == ALL_PARTITIONS {
            for i in 0..PARTITIONS as u16 {
                self.start_on(i, motion, start, clock)?;
            }
            return Ok(None);
        }
        self.start_on(partition, motion, start, clock)
    }

    /// 3.1 on partition `i`.
    fn start_on(
        &mut self,
        i: u16,
        motion: MotionId,
        start: Start,
        clock: Clock,
    ) -> Result<Option<PlaybackId>, Fatal> {
        let i = usize::from(i);
        if i >= PARTITIONS || i >= self.motions.partition_count() {
            return Ok(None);
        }
        let new = self.cycle_playback(motion, i as u8, start);
        if start.channel == 0 {
            let ended: Vec<u16> = (self.by_partition[i].iter().copied())
                .filter(|&s| {
                    self.slots[usize::from(s)]
                        .as_ref()
                        .is_some_and(|p| p.channel == 0)
                })
                .collect();
            for s in ended {
                if start.mixing {
                    self.fade_out(s, new.cycle_falloff);
                } else {
                    self.remove(s);
                }
            }
        }
        self.update_tracks(clock);
        let slot = self.add(new)?;
        let motions = self.motions.clone();
        for &b in motions.partition_bones(i) {
            self.apply(slot, b);
        }
        self.by_partition[i].push(slot);
        Ok(Some(PlaybackId(slot)))
    }

    /// A cycle's playback from its definition (2.2).
    fn cycle_playback(&self, motion: MotionId, partition: u8, start: Start) -> Playback {
        let d = self.motions.definition(motion);
        let accrue = quantize(d.accrue);
        let mut falloff = quantize(d.falloff);
        if falloff >= accrue {
            falloff = accrue.saturating_sub(1);
        }
        let stop_at_end = d.flags & STOP_AT_END != 0;
        Playback {
            motion,
            partition: Some(partition),
            time: 0.0,
            weight: if start.mixing { START_WEIGHT } else { 1.0 },
            target: 1.0,
            accrue: rate_value(accrue),
            falloff: 0.0,
            speed: speed_value(quantize(d.speed)),
            cycle_falloff: rate_value(falloff),
            fade: Fade::In,
            advancing: true,
            stop_at_end,
            fade_at_end: stop_at_end && start.channel != 0,
            channel: start.channel,
            notification: start.notification,
            length: self.motions.length(motion),
            last_frame: None,
            bones: Vec::new(),
        }
    }

    /// Starts the fx motion named `name` with power factor `k` (6); `None`
    /// when 16 fx playbacks already play.
    pub fn play_fx(&mut self, name: &str, k: f32) -> Result<Option<PlaybackId>, Fatal> {
        let motion = (self.motions.fx(name)).ok_or_else(|| Fatal::NoFx(name.to_owned()))?;
        if self.fx.len() >= MAX_FX {
            return Ok(None);
        }
        let d = self.motions.definition(motion);
        let playback = Playback {
            motion,
            partition: None,
            time: 0.0,
            weight: START_WEIGHT,
            target: speed_value(quantize(d.power)) * k,
            accrue: rate_value(quantize(d.accrue)),
            falloff: rate_value(quantize(d.falloff)),
            speed: speed_value(quantize(d.speed)),
            cycle_falloff: rate_value(quantize(d.falloff)),
            fade: Fade::In,
            advancing: true,
            stop_at_end: false,
            fade_at_end: false,
            channel: 0,
            notification: None,
            length: self.motions.length(motion),
            last_frame: None,
            bones: Vec::new(),
        };
        let first = match d.bone_or_partition {
            ALL_PARTITIONS => self.motions.root,
            b => usize::from(b),
        };
        let slot = self.add(playback)?;
        let motions = self.motions.clone();
        let mut stack = vec![first];
        while let Some(b) = stack.pop() {
            if b >= motions.bone_count() {
                continue;
            }
            self.apply(slot, b);
            stack.extend(motions.children[b].iter().rev());
        }
        self.fx.push(slot);
        Ok(Some(PlaybackId(slot)))
    }

    /// Takes a free slot for `playback` (8: 256 in all).
    fn add(&mut self, playback: Playback) -> Result<u16, Fatal> {
        if self.live >= MAX_PLAYBACKS {
            return Err(Fatal::TooManyPlaybacks);
        }
        self.live += 1;
        match self.slots.iter().position(Option::is_none) {
            Some(s) => {
                self.slots[s] = Some(playback);
                Ok(s as u16)
            }
            None => {
                self.slots.push(Some(playback));
                Ok((self.slots.len() - 1) as u16)
            }
        }
    }

    /// Applies playback `slot` to `bone` (8: a bone holds 16).
    fn apply(&mut self, slot: u16, bone: usize) {
        let list = &mut self.by_bone[bone];
        if list.len() < MAX_BONE_PLAYBACKS {
            list.push(slot);
        } else {
            if self.slots[usize::from(slot)]
                .as_ref()
                .is_some_and(|p| p.fade_at_end)
            {
                return;
            }
            let weight = |s: u16| {
                self.slots[usize::from(s)]
                    .as_ref()
                    .map_or(0.0, |p| p.weight)
            };
            let mut smallest = 0;
            for (k, &s) in list.iter().enumerate() {
                if weight(s) < weight(list[smallest]) {
                    smallest = k;
                }
            }
            let replaced = std::mem::replace(&mut list[smallest], slot);
            if let Some(p) = self.slots[usize::from(replaced)].as_mut() {
                p.bones.retain(|&b| usize::from(b) != bone);
            }
        }
        if let Some(p) = self.slots[usize::from(slot)].as_mut() {
            p.bones.push(bone as u16);
        }
    }

    /// Fades playback `slot` out at `falloff` (3.3).
    fn fade_out(&mut self, slot: u16, falloff: f32) {
        if let Some(p) = self.slots[usize::from(slot)].as_mut() {
            p.fade = Fade::Out;
            p.falloff = falloff;
            if p.stop_at_end {
                p.advancing = false;
            }
        }
    }

    /// Removes playback `slot` from its bones and its list at once (3.3).
    fn remove(&mut self, slot: u16) {
        let Some(p) = self.slots[usize::from(slot)].take() else {
            return;
        };
        self.live -= 1;
        for &b in &p.bones {
            self.by_bone[usize::from(b)].retain(|&s| s != slot);
        }
        match p.partition {
            Some(i) => self.by_partition[usize::from(i)].retain(|&s| s != slot),
            None => self.fx.retain(|&s| s != slot),
        }
    }

    /// The tracks update (4): nothing when `time_global()` is the previous
    /// update's; otherwise every cycle of partitions 0 … 3, then every fx
    /// motion, advanced by at most 66 ms.
    pub fn update_tracks(&mut self, clock: Clock) {
        if clock.time_global == self.previous {
            return;
        }
        let ms = clock
            .time_global
            .wrapping_sub(self.previous)
            .min(MAX_STEP_MS);
        self.previous = clock.time_global;
        let dt = ms as f32 * 0.001;
        for i in 0..PARTITIONS.min(self.motions.partition_count()) {
            let mut k = 0;
            while k < self.by_partition[i].len() {
                let slot = self.by_partition[i][k];
                if !self.advance_cycle(slot, dt, clock.frame) {
                    k += 1;
                }
            }
        }
        let mut k = 0;
        while k < self.fx.len() {
            let slot = self.fx[k];
            if !self.advance_fx(slot, dt) {
                k += 1;
            }
        }
    }

    /// 4.2 and 5.1 for cycle `slot`; whether it was removed.
    fn advance_cycle(&mut self, slot: u16, dt: f32, frame: u32) -> bool {
        let Some(p) = self.slots[usize::from(slot)].as_mut() else {
            return false;
        };
        if p.last_frame == Some(frame) {
            return false;
        }
        p.last_frame = Some(frame);
        if p.advancing {
            p.time += p.speed * dt;
        }
        if p.fade == Fade::In {
            p.weight = (p.weight + p.accrue * p.target * dt).min(p.target);
            let end = p.length - KEY_TIME;
            if p.stop_at_end && p.time > end {
                p.time = end;
                if p.advancing
                    && let Some(value) = p.notification
                {
                    self.notified.push(Notified {
                        playback: PlaybackId(slot),
                        motion: p.motion,
                        partition: p.partition,
                        value,
                    });
                }
                p.advancing = false;
                if p.fade_at_end {
                    p.fade = Fade::Out;
                    p.falloff = END_FALLOFF;
                }
            }
        }
        if p.fade == Fade::Out {
            p.weight -= p.target * p.falloff * dt;
            if p.weight <= 0.0 {
                self.remove(slot);
                return true;
            }
        }
        false
    }

    /// 4.3 for fx `slot`; whether it ended.
    fn advance_fx(&mut self, slot: u16, dt: f32) -> bool {
        let Some(p) = self.slots[usize::from(slot)].as_mut() else {
            return false;
        };
        if p.advancing {
            p.time += p.speed * dt;
        }
        match p.fade {
            Fade::In => {
                p.weight += p.accrue * p.target * p.speed * dt;
                if p.weight >= p.target {
                    p.weight = p.target;
                    p.fade = Fade::Out;
                }
            }
            Fade::Out => {
                p.weight -= p.target * p.falloff * p.speed * dt;
                if p.weight <= 0.0 {
                    self.remove(slot);
                    return true;
                }
            }
        }
        false
    }

    /// The value of playback `p`'s motion on `bone` at its time, and that
    /// of key 0 (skeletal-motions 5.1 – 5.3); `None` when its set has no
    /// track of the bone.
    fn evaluate(&self, p: &Playback, bone: usize) -> Option<(Value, Value)> {
        let (motion, t) = self.motions.track(p.motion, bone)?;
        let track = &motion.tracks[t];
        let keys = (motion.keys & KEY_COUNT_MASK).max(1) as u64;
        let u = p.time * 30.0;
        let frame = u.floor();
        let f = u - frame;
        let frame = frame.max(0.0) as u64;
        let (k0, k1) = ((frame % keys) as usize, ((frame + 1) % keys) as usize);
        let q = if track.single_rotation() {
            track.rotation(0)
        } else {
            interp(track.rotation(k0), track.rotation(k1), f.clamp(0.0, 1.0))
        };
        let pos = if track.has_translations() {
            let (a, b) = (track.translation(k0), track.translation(k1));
            std::array::from_fn(|i| (1.0 - f) * a[i] + f * b[i])
        } else {
            track.base
        };
        Some(((q, pos), (track.rotation(0), track.translation(0))))
    }

    /// Bone `bone`'s (**q**, **p**) from the playbacks applied to it (7.1,
    /// 7.2).
    fn bone_value(&self, bone: usize) -> Value {
        let mut result = ([0.0; 4], [0.0; 3]);
        let mut values: Vec<(Value, f32)> = Vec::with_capacity(MAX_BONE_PLAYBACKS);
        for channel in 0..CHANNELS as u8 {
            values.clear();
            for &s in &self.by_bone[bone] {
                let Some(p) = self.slots[usize::from(s)].as_ref() else {
                    continue;
                };
                if p.channel != channel {
                    continue;
                }
                // A set without the bone's track gives it nothing (our
                // choice: skeletal-motions 8 leaves it open; no shipped
                // set lacks one).
                let Some((v, key0)) = self.evaluate(p, bone) else {
                    continue;
                };
                let v = if channel == 0 {
                    v
                } else {
                    (
                        multiply(v.0, conjugate(key0.0)),
                        std::array::from_fn(|i| v.1[i] - key0.1[i]),
                    )
                };
                values.push((v, p.weight));
            }
            if channel == 0 {
                result = combine(&mut values);
            } else if !values.is_empty() {
                let (r, d) = combine(&mut values);
                let f = self.factors[usize::from(channel)];
                result.0 = multiply(scale(r, f), result.0);
                result.1 = std::array::from_fn(|i| result.1[i] + f * d[i]);
            }
        }
        result
    }

    /// Every bone's current model-space matrix *T* = *L* · *T*(parent)
    /// (7.3; skeletal-motions 5.4), into `out`.
    pub fn pose(&self, out: &mut Vec<Mat4>) {
        self.pose_following(out, |_, _| {});
    }

    /// [`Self::pose`] with a bone callback in follow mode
    /// (head-bone-turning.md 1, 4): `follow(bone, T)` changes each bone's
    /// *T* once it is computed from the motions, before its children are
    /// computed from it.
    pub fn pose_following(&self, out: &mut Vec<Mat4>, mut follow: impl FnMut(usize, &mut Mat4)) {
        let m = &self.motions;
        out.clear();
        out.resize(m.bone_count(), IDENTITY);
        for &b in &m.order {
            let (q, p) = self.bone_value(b);
            let l = local_matrix(q, p);
            out[b] = match m.parents[b] {
                Some(parent) => mat_multiply(&l, &out[parent]),
                None => l,
            };
            follow(b, &mut out[b]);
        }
    }

    /// The render matrices *K* = *B*⁻¹ · *T* of `pose` (model-shading.md
    /// 2), into `out`.
    pub fn render_matrices(&self, pose: &[Mat4], out: &mut Vec<Mat4>) {
        out.clear();
        out.extend(
            (self.motions.inverse_binds.iter())
                .zip(pose)
                .map(|(b, t)| mat_multiply(b, t)),
        );
    }
}

/// Combines one channel's values by their weights (7.1).
fn combine(values: &mut [(Value, f32)]) -> Value {
    match values.len() {
        0 => ([0.0; 4], [0.0; 3]),
        1 => values[0].0,
        2 => {
            let (a0, a1) = (values[0].1, values[1].1);
            let sum = a0 + a1;
            let w = if sum.abs() < ZERO_SUM { 0.0 } else { a1 / sum };
            blend(values[0].0, values[1].0, w.clamp(0.0, 1.0))
        }
        _ => {
            // Stable: equal weights keep the bone's order.
            values.sort_by(|a, b| b.1.total_cmp(&a.1));
            let mut result = values[0].0;
            let mut s = values[0].1;
            for &(v, a) in &values[1..] {
                let next = s + a;
                let w = if next.abs() < ZERO_SUM { 0.0 } else { a / next };
                result = blend(result, v, w);
                s = next;
            }
            result
        }
    }
}

/// **interp** (7.1): the rotations by skeletal-motions 5.2 with weight
/// `w`, the translations mixed linearly.
fn blend(a: Value, b: Value, w: f32) -> Value {
    (
        interp(a.0, b.0, w),
        std::array::from_fn(|i| (1.0 - w) * a.1[i] + w * b.1[i]),
    )
}

/// The key interpolation of skeletal-motions 5.2 at weight `g`; not
/// normalised.
fn interp(a: [f32; 4], b: [f32; 4], g: f32) -> [f32; 4] {
    let mut c = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let sign = if c < 0.0 {
        c = -c;
        -1.0
    } else {
        1.0
    };
    let (wa, wb) = if 1.0 - c <= EPSILON {
        (1.0 - g, g)
    } else {
        let c2 = c * c;
        let theta = HALF_PI - c * (((ACOS_A * c2 - ACOS_B) * c2 + ACOS_C) * c2 + ACOS_D);
        let s = theta.sin();
        ((theta - theta * g).sin() / s, (theta * g).sin() / s)
    };
    std::array::from_fn(|i| wa * a[i] + sign * wb * b[i])
}

/// conj(*x*, *y*, *z*, *w*) = (−*x*, −*y*, −*z*, *w*) (7.2).
fn conjugate(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

/// The product *l* ⊗ *b* (7.2).
fn multiply(l: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [lx, ly, lz, lw] = l;
    let [bx, by, bz, bw] = b;
    [
        lw * bx + lx * bw + ly * bz - lz * by,
        lw * by + ly * bw + lz * bx - lx * bz,
        lw * bz + lz * bw + lx * by - ly * bx,
        lw * bw - lx * bx - ly * by - lz * bz,
    ]
}

/// scale(**r**, *f*) (7.2).
fn scale(r: [f32; 4], f: f32) -> [f32; 4] {
    // A component past ±1 by rounding is taken as ±1 (our choice).
    let h = r[3].clamp(-1.0, 1.0).acos();
    if h <= EPSILON {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let (s, hf) = (h.sin(), h * f);
    let k = hf.sin() / s;
    let q = [r[0] * k, r[1] * k, r[2] * k, hf.cos()];
    let len = (q.iter().map(|c| c * c).sum::<f32>()).sqrt();
    if len < EPSILON { q } else { q.map(|c| c / len) }
}

const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// The local matrix *L* of **q** and **p** (skeletal-motions 5.4; **q**
/// not normalised).
pub fn local_matrix(q: [f32; 4], p: [f32; 3]) -> Mat4 {
    let [x, y, z, w] = q;
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
            0.0,
        ],
        [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
            0.0,
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
        ],
        [p[0], p[1], p[2], 1.0],
    ]
}

/// `a · b`, row-vector convention.
fn mat_multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

/// The inverse of an affine matrix (rows 0 – 2 its linear part, row 3 its
/// translation).
fn inverse_affine(m: &Mat4) -> Mat4 {
    let a = |r: usize, c: usize| f64::from(m[r][c]);
    let cof = |r: usize, c: usize| {
        let (r0, r1) = ((r + 1) % 3, (r + 2) % 3);
        let (c0, c1) = ((c + 1) % 3, (c + 2) % 3);
        a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0)
    };
    let det = a(0, 0) * cof(0, 0) + a(0, 1) * cof(0, 1) + a(0, 2) * cof(0, 2);
    let det = if det == 0.0 { 1.0 } else { det };
    // inv[r][c] = cof(c, r) / det.
    let inv: [[f64; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| cof(c, r) / det));
    let t: [f64; 3] = std::array::from_fn(|c| -(0..3).map(|k| a(3, k) * inv[k][c]).sum::<f64>());
    [
        [inv[0][0] as f32, inv[0][1] as f32, inv[0][2] as f32, 0.0],
        [inv[1][0] as f32, inv[1][1] as f32, inv[1][2] as f32, 0.0],
        [inv[2][0] as f32, inv[2][1] as f32, inv[2][2] as f32, 0.0],
        [t[0] as f32, t[1] as f32, t[2] as f32, 1.0],
    ]
}

#[cfg(test)]
mod tests;
