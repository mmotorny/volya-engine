// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! How screens play sounds: [`AudioOut`] starts, stops and watches voices;
//! [`MixerOut`] implements it on the core mixer, decoding files of the game
//! tree. Screens take the trait so their timing is testable without audio.

use std::io::Cursor;

use crate::core::audio::{Mixer, Playback, VoiceId};
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
}

/// [`AudioOut`] on a mixer, reading files from `tree`.
pub struct MixerOut<'a> {
    pub mixer: &'a mut Mixer,
    pub tree: &'a dyn Vfs,
    /// Voices started through this, by their index.
    pub voices: &'a mut Vec<VoiceId>,
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
}
