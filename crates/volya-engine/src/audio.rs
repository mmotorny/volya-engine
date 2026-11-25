// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! How screens play sounds: [`AudioOut`] starts, stops and watches voices;
//! [`MixerOut`] implements it on the core mixer, decoding files of the game
//! tree. Screens take the trait so their timing is testable without audio.
//!
//! The positional sound system (docs/behavior/sound-positional.md) runs on
//! the same mixer: [`SoundHost`] owns it with its clock, and [`MixerDevice`]
//! is its audio device, each voice a mixer voice whose gain and pan follow
//! the device rules ([`heard`]).

use std::io::Cursor;

use crate::core::audio::{Category, Mixer, Playback, VoiceId};
use crate::core::sound::Sound;
use crate::core::sound::system::{
    DEFAULT_VOICES, Device, DeviceListener, DeviceSource, Listener, NoDevice, SoundSystem, heard,
};
use crate::core::vfs::{self, Vfs};
use volya_engine_formats::audio::VorbisStream;

/// A voice as screens know it.
pub type Voice = u32;

pub trait AudioOut {
    /// Starts the Ogg Vorbis file at `path` of the game tree; `None` when it
    /// cannot play (no audio output, or the file fails to open).
    fn play(&mut self, path: &str, playback: Playback) -> Option<Voice>;
    fn stop(&mut self, voice: Voice);
    fn is_playing(&self, voice: Voice) -> bool;
    /// Sets the master (listener) gain: 0 mutes everything, 1 is normal.
    fn set_master_gain(&mut self, gain: f32);
    /// Sets the volume of a sound category (`snd_volume_eff`,
    /// `snd_volume_music`, docs/behavior/options-screens.md section 6.4).
    fn set_volume(&mut self, category: Category, volume: f32);
    /// The positional sound system, if there is audio output.
    fn sounds(&mut self) -> Option<&mut SoundSystem> {
        None
    }
}

/// No audio output: nothing plays.
pub struct Silent;

impl AudioOut for Silent {
    fn play(&mut self, _: &str, _: Playback) -> Option<Voice> {
        None
    }
    fn stop(&mut self, _: Voice) {}
    fn is_playing(&self, _: Voice) -> bool {
        false
    }
    fn set_master_gain(&mut self, _: f32) {}
    fn set_volume(&mut self, _: Category, _: f32) {}
}

/// [`AudioOut`] on a mixer, reading files from `tree`.
pub struct MixerOut<'a> {
    pub mixer: &'a mut Mixer,
    pub tree: &'a dyn Vfs,
    /// Voices started through this, by their index.
    pub voices: &'a mut Vec<VoiceId>,
    /// The positional sound system on the same mixer.
    pub sounds: Option<&'a mut SoundSystem>,
}

impl AudioOut for MixerOut<'_> {
    fn play(&mut self, path: &str, playback: Playback) -> Option<Voice> {
        let mixer = &*self.mixer;
        self.voices.retain(|v| mixer.is_playing(*v));
        let bytes = vfs::read(self.tree, path).ok()?;
        let stream = VorbisStream::open(Cursor::new(bytes)).ok()?;
        let id = self.mixer.play(Box::new(stream), playback);
        self.voices.push(id);
        Some(id.index())
    }

    fn stop(&mut self, voice: Voice) {
        if let Some(i) = self.voices.iter().position(|v| v.index() == voice) {
            self.mixer.stop(self.voices.swap_remove(i));
        }
    }

    fn is_playing(&self, voice: Voice) -> bool {
        self.voices
            .iter()
            .any(|v| v.index() == voice && self.mixer.is_playing(*v))
    }

    fn set_master_gain(&mut self, gain: f32) {
        self.mixer.set_master_gain(gain);
    }

    fn set_volume(&mut self, category: Category, volume: f32) {
        self.mixer.set_volume(category, volume);
        if let Some(sounds) = self.sounds.as_deref_mut() {
            sounds.set_volume(category, volume);
        }
    }

    fn sounds(&mut self) -> Option<&mut SoundSystem> {
        self.sounds.as_deref_mut()
    }
}

/// The sound system's audio device on a mixer (sound-positional.md section
/// 7): each voice a mixer voice in [`Category::Scaled`] (its gain already
/// holds the category volume), reading the sound's file from `tree` when it
/// starts. The listener's overall gain is the mixer's master gain.
pub struct MixerDevice<'a> {
    pub mixer: &'a mut Mixer,
    /// `None` when no game is mounted: nothing starts.
    pub tree: Option<&'a dyn Vfs>,
    /// The mixer voice of each device voice.
    pub voices: &'a mut Vec<Option<VoiceId>>,
    pub listener: DeviceListener,
}

impl MixerDevice<'_> {
    fn slot(&mut self, voice: usize) -> &mut Option<VoiceId> {
        if self.voices.len() <= voice {
            self.voices.resize(voice + 1, None);
        }
        &mut self.voices[voice]
    }
}

impl Device for MixerDevice<'_> {
    fn start(&mut self, voice: usize, sound: &Sound, looped: bool, offset: u64) {
        if let Some(id) = self.slot(voice).take() {
            self.mixer.stop(id);
        }
        let Some(tree) = self.tree else {
            return;
        };
        // Errors leave the voice silent: the file was checked when loaded.
        let Ok(mut stream) = sound.open(tree) else {
            return;
        };
        // TODO(#309): resuming far into a long sound decodes up to the
        // offset in this update; seek by pages if it shows as a hitch.
        if stream.skip(offset).is_err() {
            return;
        }
        let playback = Playback {
            looping: looped,
            // Ramped to the source's gain from the first block mixed.
            gain: 0.0,
            pan: 0.0,
            category: Category::Scaled,
        };
        let id = self.mixer.play(Box::new(stream), playback);
        *self.slot(voice) = Some(id);
    }

    fn stop(&mut self, voice: usize) {
        if let Some(id) = self.slot(voice).take() {
            self.mixer.stop(id);
        }
    }

    fn set_listener(&mut self, listener: &DeviceListener) {
        self.listener = *listener;
    }

    fn set_source(&mut self, voice: usize, source: &DeviceSource) {
        if let Some(id) = *self.slot(voice) {
            let (gain, pan) = heard(source, &self.listener);
            self.mixer.set_voice(id, gain, pan, source.pitch);
        }
    }
}

/// The sound system with its clock and its voices on a mixer.
pub struct SoundHost {
    pub system: SoundSystem,
    voices: Vec<Option<VoiceId>>,
    /// Platform time of the sound clock's 0 (ms).
    origin: Option<f64>,
    clock: u64,
    /// The page was hidden since the last update.
    rebase: bool,
}

impl Default for SoundHost {
    fn default() -> Self {
        Self::new(DEFAULT_VOICES)
    }
}

impl SoundHost {
    /// A sound system with `voices` voices (`snd_targets`).
    pub fn new(voices: usize) -> Self {
        Self {
            system: SoundSystem::new(voices),
            voices: Vec::new(),
            origin: None,
            clock: 0,
            rebase: false,
        }
    }

    /// The page was hidden or shown again (#93): the sound clock skips the
    /// hidden time, as the engine clock does.
    pub fn set_hidden(&mut self, hidden: bool) {
        self.rebase |= hidden;
    }

    /// The sound clock at platform time `time_ms` (section 5.1): real time
    /// since the first frame, without hidden time.
    fn tick(&mut self, time_ms: f64) -> u64 {
        if std::mem::take(&mut self.rebase) || self.origin.is_none() {
            self.origin = Some(time_ms - self.clock as f64);
        }
        let origin = self.origin.unwrap_or(time_ms);
        self.clock = self.clock.max((time_ms - origin).max(0.0) as u64);
        self.clock
    }

    /// The sound update of the frame at platform time `time_ms`, on `mixer`
    /// (or no device without audio output).
    pub fn update(&mut self, time_ms: f64, mixer: Option<&mut Mixer>, tree: Option<&dyn Vfs>) {
        let now = self.tick(time_ms);
        match mixer {
            Some(mixer) => {
                let mut device = MixerDevice {
                    mixer,
                    tree,
                    voices: &mut self.voices,
                    listener: DeviceListener {
                        position: [0.0; 3],
                        at: [0.0, 0.0, -1.0],
                        up: [0.0, 1.0, 0.0],
                    },
                };
                self.system.update(now, &mut device);
            }
            None => self.system.update(now, &mut NoDevice),
        }
    }

    /// The mixer was replaced: its voices are gone, so every emitter stops.
    pub fn mixer_replaced(&mut self) {
        self.system.stop_all();
        self.voices.clear();
    }

    /// Sets the listener (section 7.2).
    pub fn set_listener(&mut self, listener: Listener) {
        self.system.set_listener(listener);
    }
}
