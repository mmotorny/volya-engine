// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's traders' scripted motion channels
//! (docs/behavior/scripted-motion-channels.md, TC2): a channel record for
//! each trader instance while it is online, made when it spawns (3) and
//! dropped when it goes offline or is removed; the calls scripts make on
//! it (2) on its model's playbacks ([`crate::level_motions`]); its
//! model's notifications after each frame's tracks updates; and the check
//! (4), once after each level update.
// TODO(#974): no trader is ever under script control here (CH-24), so the
// check always runs (4.1).

use std::collections::BTreeMap;
use std::io;

use crate::core::motion_playback::{Clock, Notified};
use crate::core::scripted_motion_channels::{
    ChannelCall, ChannelEvent, ChannelSounds, CheckParts, Fatal, MotionChannels, TRADER_CLASS,
    TraderParts,
};
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::level_motions::{LevelMotions, model_motions};
use crate::objects::LevelObjects;

/// The channel records of the level's online traders, by instance.
#[derive(Debug, Default)]
pub struct LevelMotionChannels {
    records: BTreeMap<usize, MotionChannels>,
    /// A level update ran since the last check.
    due: bool,
}

impl LevelMotionChannels {
    /// The records of the traders online at the start.
    pub fn new(objects: &LevelObjects) -> Self {
        let mut channels = Self::default();
        for (k, i) in objects.instances.iter().enumerate() {
            if i.online {
                channels.spawned(k, &i.class_id);
            }
        }
        channels
    }

    /// Instance `k` of class `class_id` spawned (3): a trader's record made
    /// (or cleared by the reinitialisation).
    pub fn spawned(&mut self, k: usize, class_id: &str) {
        if class_id == TRADER_CLASS {
            self.records.entry(k).or_default().reinitialise();
        }
    }

    /// Instance `k` went offline or was removed: its record freed without
    /// stopping its sound (3).
    pub fn destroyed(&mut self, k: usize) {
        self.records.remove(&k);
    }

    /// Instance `k`'s record, while it is an online trader.
    pub fn record(&self, k: usize) -> Option<&MotionChannels> {
        self.records.get(&k)
    }

    /// The frame's tracks updates ran: their notifications reach the
    /// traders' records (2.2), and a check is due.
    pub fn tracks_updated(&mut self, notified: Vec<(usize, Notified)>) {
        for (k, n) in notified {
            if let Some(r) = self.records.get_mut(&k) {
                r.notified(n.value);
            }
        }
        self.due = true;
    }

    /// Whether a check is due, which it then no longer is.
    pub fn take_due(&mut self) -> bool {
        std::mem::take(&mut self.due)
    }

    /// The online traders' instances, in order.
    pub fn traders(&self) -> Vec<usize> {
        self.records.keys().copied().collect()
    }

    /// Instance `k`'s record, while it is an online trader.
    pub fn record_mut(&mut self, k: usize) -> Option<&mut MotionChannels> {
        self.records.get_mut(&k)
    }
}

/// What a call on a trader acts on in the level: the instances' models
/// (each made the first time it plays), the game's files and the clock.
pub struct LevelTraders<'a> {
    pub motions: &'a mut LevelMotions,
    pub objects: &'a LevelObjects,
    pub dir: &'a str,
    pub tree: &'a dyn Vfs,
    pub data: &'a GameData,
    pub clock: Clock,
}

/// A call's failure: a fatal error of the call, or the model's motions
/// unread.
#[derive(Debug)]
pub enum CallError {
    Fatal(Fatal),
    Io(io::Error),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Fatal(e) => e.fmt(f),
            CallError::Io(e) => e.fmt(f),
        }
    }
}

impl From<Fatal> for CallError {
    fn from(e: Fatal) -> Self {
        CallError::Fatal(e)
    }
}

impl From<io::Error> for CallError {
    fn from(e: io::Error) -> Self {
        CallError::Io(e)
    }
}

impl LevelTraders<'_> {
    /// Makes `call` on instance `k`'s `record` (2).
    pub fn call(
        &mut self,
        k: usize,
        record: &mut MotionChannels,
        call: &ChannelCall,
        sounds: &mut dyn ChannelSounds,
    ) -> Result<(), CallError> {
        let objects = self.objects;
        let model = objects.instances[k].model;
        let (loaded, names, dir) = (&objects.models[model], &objects.names, self.dir);
        let (tree, data) = (self.tree, self.data);
        let animated = self
            .motions
            .animate(k, model, || model_motions(tree, data, dir, loaded, names))?;
        let mut parts = TraderParts {
            model: animated,
            sounds,
            clock: self.clock,
        };
        Ok(record.call(call, &mut parts)?)
    }
}

/// The check of instance `k` (4.2): `raise` raises an event on the trader
/// and gives the calls its function made, which are made at once.
pub struct Check<'a, 'b, E> {
    pub k: usize,
    pub traders: &'a mut LevelTraders<'b>,
    pub sounds: &'a mut dyn ChannelSounds,
    pub raise: &'a mut dyn FnMut(ChannelEvent) -> Result<Vec<ChannelCall>, E>,
}

/// A check's failure: the scripts' error raising an event, or a call's.
#[derive(Debug)]
pub enum CheckError<E> {
    Script(E),
    Call(CallError),
}

impl<E> From<Fatal> for CheckError<E> {
    fn from(e: Fatal) -> Self {
        CheckError::Call(CallError::Fatal(e))
    }
}

impl<E> From<CallError> for CheckError<E> {
    fn from(e: CallError) -> Self {
        CheckError::Call(e)
    }
}

impl<E> CheckParts for Check<'_, '_, E> {
    type Error = CheckError<E>;

    fn sounds(&mut self) -> &mut dyn ChannelSounds {
        self.sounds
    }

    fn raise(
        &mut self,
        event: ChannelEvent,
        channels: &mut MotionChannels,
    ) -> Result<(), CheckError<E>> {
        for call in (self.raise)(event).map_err(CheckError::Script)? {
            self.traders.call(self.k, channels, &call, self.sounds)?;
        }
        Ok(())
    }
}
