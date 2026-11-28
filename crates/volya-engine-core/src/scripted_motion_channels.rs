// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Scripted motion channels (docs/behavior/scripted-motion-channels.md,
//! TC2; section numbers are its): a trader's **channel record**, its
//! global and head channels and its sound (1), the five calls scripts make
//! on it (2), its clearing at spawning (3) and the per-frame **check** that
//! raises the events 32, 30 and 31 (4).
//!
//! The channels start their cycles on the trader's [`AnimatedModel`]
//! (docs/behavior/motion-playback.md, TC1) with the global or the head
//! notification ([`GLOBAL_NOTIFICATION`], [`HEAD_NOTIFICATION`]), which
//! the caller hands back with [`MotionChannels::notified`]. The sound is
//! the caller's sound system's ([`ChannelSounds`]; [`SystemSounds`] over
//! sound-positional.md's).

use std::fmt;

use crate::audio::Category;
use crate::config::PathAliases;
use crate::motion_playback::{self, AnimatedModel, Clock, MotionId, Start};
use crate::sound::Fatal as SoundFatal;
use crate::sound::system::{PlayFlags, SoundId, SoundSystem};
use crate::vfs::Vfs;

/// The class id of the trader class, the only one with motion channels.
pub const TRADER_CLASS: &str = "AI_TRD_S";

/// The notification values a channel's cycles are started with (2.2, 2.3;
/// our values).
pub const GLOBAL_NOTIFICATION: u32 = 0;
pub const HEAD_NOTIFICATION: u32 = 1;

/// The events the check raises (4.2), by their callback ids (spec O 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelEvent {
    /// 30, `trader_global_anim_request`.
    GlobalAnimRequest,
    /// 31, `trader_head_anim_request`.
    HeadAnimRequest,
    /// 32, `trader_sound_end`.
    SoundEnd,
}

impl ChannelEvent {
    /// The event's callback id (spec O 2).
    pub fn id(self) -> u8 {
        match self {
            ChannelEvent::GlobalAnimRequest => 30,
            ChannelEvent::HeadAnimRequest => 31,
            ChannelEvent::SoundEnd => 32,
        }
    }
}

/// A fatal error of a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fatal {
    /// 2.2 step 2: no cycle of that name, a failed check.
    NoCycle(String),
    /// A playback limit (TC1 8).
    Playback(motion_playback::Fatal),
    /// The sound's file (S1).
    Sound(String),
}

impl Fatal {
    /// The failed check's expression and description (2.2), if it is one.
    pub fn check(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Fatal::NoCycle(_) => Some(("motion_ID.valid()", "! MODEL: can't find cycle: ")),
            _ => None,
        }
    }
}

impl fmt::Display for Fatal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fatal::NoCycle(name) => write!(f, "! MODEL: can't find cycle: {name}"),
            Fatal::Playback(e) => e.fmt(f),
            Fatal::Sound(e) => f.write_str(e),
        }
    }
}

impl From<motion_playback::Fatal> for Fatal {
    fn from(e: motion_playback::Fatal) -> Self {
        Fatal::Playback(e)
    }
}

impl From<SoundFatal> for Fatal {
    fn from(e: SoundFatal) -> Self {
        Fatal::Sound(e.0)
    }
}

/// A channel (1): the name last given, if any, and its motion; no motion
/// is **ended**.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Channel {
    pub name: Option<Vec<u8>>,
    pub motion: Option<MotionId>,
}

impl Channel {
    pub fn ended(&self) -> bool {
        self.motion.is_none()
    }
}

/// The trader's sound (1): a sound of the sound system, or one created
/// without sound data (2.4: the sound system disabled), which has no
/// emitter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelSound(pub Option<SoundId>);

/// What the channels need of the sound system (S2).
pub trait ChannelSounds {
    /// 2.4 steps 3 and 4: a sound created from `name` (resolved per S1) in
    /// the effects category and played 2D, not looped, delay 0; `None`
    /// when creating makes no sound data.
    fn start(&mut self, name: &str) -> Result<Option<SoundId>, SoundFatal>;
    /// Whether `sound` has an emitter (S2 4.4).
    fn has_emitter(&self, sound: SoundId) -> bool;
    /// 2.7: the emitter stopped at once and the sound discarded.
    fn stop(&mut self, sound: SoundId);
}

/// [`ChannelSounds`] over sound-positional.md's system; without one (no
/// audio output) creating makes no sound data.
pub struct SystemSounds<'a> {
    pub system: Option<&'a mut SoundSystem>,
    pub tree: &'a dyn Vfs,
    pub aliases: &'a PathAliases,
}

impl ChannelSounds for SystemSounds<'_> {
    fn start(&mut self, name: &str) -> Result<Option<SoundId>, SoundFatal> {
        let Some(system) = self.system.as_deref_mut() else {
            return Ok(None);
        };
        // TODO(#1080): what the value 2²⁷ passed at creation means (2.4
        // step 3) is unestablished; it is not passed.
        let id = system.create(self.tree, self.aliases, name, Category::Effects)?;
        let flags = PlayFlags {
            looped: false,
            two_d: true,
            delay: 0.0,
        };
        system.play(id, flags);
        Ok(Some(id))
    }

    fn has_emitter(&self, sound: SoundId) -> bool {
        self.system.as_deref().is_some_and(|s| s.has_emitter(sound))
    }

    fn stop(&mut self, sound: SoundId) {
        if let Some(system) = self.system.as_deref_mut() {
            system.discard(sound);
        }
    }
}

/// A call of 2 on a trader, as scripts make it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelCall {
    /// `set_trader_global_anim(name)` (2.2).
    SetGlobalAnim(Vec<u8>),
    /// `set_trader_head_anim(name)` (2.3).
    SetHeadAnim(Vec<u8>),
    /// `set_trader_sound(sound, head_anim)` (2.4).
    SetSound { sound: Vec<u8>, head_anim: Vec<u8> },
    /// `external_sound_start(sound)` (2.5).
    ExternalSoundStart(Vec<u8>),
    /// `external_sound_stop()` (2.6).
    ExternalSoundStop,
}

/// What a call acts on: the trader's model, the sound system and the
/// device's clock.
pub struct TraderParts<'a> {
    pub model: &'a mut AnimatedModel,
    pub sounds: &'a mut dyn ChannelSounds,
    pub clock: Clock,
}

/// What the check reaches (4.2): the sound system, and the events' calls.
pub trait CheckParts {
    type Error: From<Fatal>;
    fn sounds(&mut self) -> &mut dyn ChannelSounds;
    /// Raises `event` on the trader (spec O 3.2, no arguments) and makes
    /// the calls its function made on `channels`, before the check goes on.
    fn raise(
        &mut self,
        event: ChannelEvent,
        channels: &mut MotionChannels,
    ) -> Result<(), Self::Error>;
}

/// A trader's channel record (1): made at construction with both channels
/// ended, no names and no sound (3), and so again at every spawning.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MotionChannels {
    pub global: Channel,
    pub head: Channel,
    pub sound: Option<ChannelSound>,
}

impl MotionChannels {
    pub fn new() -> Self {
        Self::default()
    }

    /// The reinitialisation at a spawning (3): the record cleared; a sound
    /// it held is dropped without being stopped.
    pub fn reinitialise(&mut self) {
        *self = Self::new();
    }

    /// Makes `call` (2).
    pub fn call(&mut self, call: &ChannelCall, parts: &mut TraderParts<'_>) -> Result<(), Fatal> {
        match call {
            ChannelCall::SetGlobalAnim(name) => self.set_global_anim(name, parts),
            ChannelCall::SetHeadAnim(name) => self.set_head_anim(name, parts),
            ChannelCall::SetSound { sound, head_anim } => self.set_sound(sound, head_anim, parts),
            ChannelCall::ExternalSoundStart(sound) => {
                self.external_sound_start(sound, parts.sounds)
            }
            ChannelCall::ExternalSoundStop => {
                self.external_sound_stop(parts.sounds);
                Ok(())
            }
        }
    }

    /// `set_trader_global_anim` (2.2).
    pub fn set_global_anim(
        &mut self,
        name: &[u8],
        parts: &mut TraderParts<'_>,
    ) -> Result<(), Fatal> {
        self.global.name = Some(name.to_vec());
        self.global.motion = Some(find(name, parts)?);
        self.start(name, GLOBAL_NOTIFICATION, parts)
    }

    /// `set_trader_head_anim` (2.3).
    pub fn set_head_anim(&mut self, name: &[u8], parts: &mut TraderParts<'_>) -> Result<(), Fatal> {
        self.head.name = Some(name.to_vec());
        self.head.motion = Some(find(name, parts)?);
        self.start(name, HEAD_NOTIFICATION, parts)
    }

    /// `set_trader_sound` (2.4): the sound stopped, the head motion
    /// started, the new sound made and played.
    pub fn set_sound(
        &mut self,
        sound: &[u8],
        head_anim: &[u8],
        parts: &mut TraderParts<'_>,
    ) -> Result<(), Fatal> {
        self.stop_sound(parts.sounds);
        self.set_head_anim(head_anim, parts)?;
        self.play_sound(sound, parts.sounds)
    }

    /// `external_sound_start` (2.5): the sound stopped, the new one made
    /// and played, the head channel marked ended.
    pub fn external_sound_start(
        &mut self,
        sound: &[u8],
        sounds: &mut dyn ChannelSounds,
    ) -> Result<(), Fatal> {
        self.stop_sound(sounds);
        self.play_sound(sound, sounds)?;
        self.head.motion = None;
        Ok(())
    }

    /// `external_sound_stop` (2.6).
    pub fn external_sound_stop(&mut self, sounds: &mut dyn ChannelSounds) {
        self.stop_sound(sounds);
    }

    /// 2.4 steps 3 and 4.
    fn play_sound(&mut self, name: &[u8], sounds: &mut dyn ChannelSounds) -> Result<(), Fatal> {
        let name = volya_engine_formats::cp1251::decode(name);
        self.sound = Some(ChannelSound(sounds.start(&name)?));
        Ok(())
    }

    /// Stopping the sound (2.7), if the trader has one.
    fn stop_sound(&mut self, sounds: &mut dyn ChannelSounds) {
        if let Some(ChannelSound(Some(id))) = self.sound.take() {
            sounds.stop(id);
        }
    }

    /// 2.2 step 4: the cycle `name` started on its definition's partition,
    /// mixing, playback channel 0, with `notification`. The ends the
    /// start's tracks update reaches (TC1 3.1) notify at once, after the
    /// channel was given its motion (step 3).
    fn start(
        &mut self,
        name: &[u8],
        notification: u32,
        parts: &mut TraderParts<'_>,
    ) -> Result<(), Fatal> {
        let motion = find(name, parts)?;
        let start = Start::mixing(Some(notification));
        parts.model.play_cycle(motion, start, parts.clock)?;
        for n in parts.model.take_notifications() {
            self.notified(n.value);
        }
        Ok(())
    }

    /// A notification of the trader's model (TC1 5.2): the global one
    /// marks the global channel ended, the head one the head channel.
    pub fn notified(&mut self, value: u32) {
        match value {
            GLOBAL_NOTIFICATION => self.global.motion = None,
            HEAD_NOTIFICATION => self.head.motion = None,
            _ => {}
        }
    }

    /// Whether the trader's sound has an emitter.
    fn sound_playing(&self, sounds: &dyn ChannelSounds) -> bool {
        matches!(self.sound, Some(ChannelSound(Some(id))) if sounds.has_emitter(id))
    }

    /// The check (4.2), outside script control: events 32, 30 and 31 in
    /// that order, each followed at once by the calls its function made.
    pub fn check<P: CheckParts>(&mut self, parts: &mut P) -> Result<(), P::Error> {
        // 1: the sound without an emitter, then stopped and discarded.
        if self.sound.is_some() && !self.sound_playing(parts.sounds()) {
            parts.raise(ChannelEvent::SoundEnd, self)?;
            // TODO(#1080): the function leaving the trader without a sound
            // (`external_sound_stop`) is believed a crash; here the stop
            // after the call does nothing then.
            self.stop_sound(parts.sounds());
        }
        // 2: the global channel ended; then, once a global name has been
        // given since the spawning, the head channel marked ended.
        if self.global.ended() {
            parts.raise(ChannelEvent::GlobalAnimRequest, self)?;
            if self.global.name.is_some() {
                self.head.motion = None;
            }
        }
        // 3: the head channel ended while the sound plays.
        if self.head.ended() && self.sound_playing(parts.sounds()) {
            parts.raise(ChannelEvent::HeadAnimRequest, self)?;
        }
        Ok(())
    }
}

/// 2.2 step 2: the cycle `name` looked up as `PlayCycle` looks it up (B
/// 5.2); a name the model lacks is fatal.
fn find(name: &[u8], parts: &TraderParts<'_>) -> Result<MotionId, Fatal> {
    let text = volya_engine_formats::cp1251::decode(name);
    (parts.model.motions().cycle(&text)).ok_or(Fatal::NoCycle(text))
}

#[cfg(test)]
mod tests;
