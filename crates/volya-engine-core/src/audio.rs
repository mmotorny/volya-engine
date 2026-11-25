// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Audio mixing: decoded sources mixed into one stereo stream at the output's
//! sample rate. Platform-free; platforms pull mixed blocks with
//! [`Mixer::mix`]. The engine drives a mixer through [`MixerControl`]: a
//! [`Mixer`] it mixes itself, or a [`remote::Remote`] whose commands a
//! [`remote::Host`] applies to a mixer mixing on the output's own schedule
//! (the web platform's audio worker).
//!
//! Each voice has a stereo pan and a volume category. Panning is a linear
//! balance: a pan `p` in [-1, 1] scales the left channel by `min(1, 1 - p)`
//! and the right by `min(1, 1 + p)`. Centred voices (`p = 0`) play exactly as
//! without panning (a mono source at full gain in both channels), and the
//! extremes send a source to one channel only. That is what the stereo pairs
//! of the game need: the title menu's music is two mono files, the left and
//! right channels of one track, played as listener-relative sounds on either
//! side of the listener (docs/behavior/title-menu.md section 6). How the
//! original sound system pans by position is not specified, so we model no
//! 3D positioning: callers choose the pan (the music pair uses -1 and +1, full
//! separation, reproducing the two channels as recorded).

pub mod remote;

use std::io::{self, Cursor, Read, Seek};

use volya_engine_formats::audio::VorbisStream;

/// Decoded audio the mixer can play.
pub trait Source {
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
    /// Appends interleaved samples in [-1, 1]; `false` at the end.
    fn read(&mut self, out: &mut Vec<f32>) -> io::Result<bool>;
    /// Restarts from the beginning.
    fn rewind(&mut self) -> io::Result<()>;
}

impl<R: Read + Seek> Source for VorbisStream<R> {
    fn sample_rate(&self) -> u32 {
        VorbisStream::sample_rate(self)
    }

    fn channels(&self) -> u16 {
        VorbisStream::channels(self)
    }

    fn read(&mut self, out: &mut Vec<f32>) -> io::Result<bool> {
        self.read_block(out)
    }

    fn rewind(&mut self) -> io::Result<()> {
        VorbisStream::rewind(self)
    }
}

/// A compressed file a voice decodes from.
pub trait File: Read + Seek {}

impl<T: Read + Seek> File for T {}

/// Opens compressed Ogg Vorbis `bytes` for a voice, `skip` frames in.
pub fn open_vorbis(bytes: Vec<u8>, skip: u64) -> io::Result<Box<dyn Source>> {
    open_vorbis_file(Box::new(Cursor::new(bytes)), skip)
}

/// [`open_vorbis`] on a file of any kind.
pub fn open_vorbis_file(file: Box<dyn File>, skip: u64) -> io::Result<Box<dyn Source>> {
    let mut stream = VorbisStream::open(file)?;
    stream.skip(skip)?;
    Ok(Box::new(stream))
}

/// How the engine drives a mixer: the [`Mixer`] itself, or a
/// [`remote::Remote`] standing for one that mixes elsewhere. Changes made
/// between two mixed blocks apply together, from the next block.
pub trait MixerControl {
    /// Starts the compressed Ogg Vorbis file `bytes`, `skip` frames in;
    /// `None` when it is not a stream that can play.
    fn play_file(&mut self, bytes: Vec<u8>, skip: u64, playback: Playback) -> Option<VoiceId>;
    /// Changes a playing voice's gain, pan and pitch ([`Mixer::set_voice`]).
    fn set_voice(&mut self, id: VoiceId, gain: f32, pan: f32, pitch: f32);
    fn stop(&mut self, id: VoiceId);
    fn stop_all(&mut self);
    /// Whether the voice plays: started, not stopped and not known to have
    /// ended.
    fn is_playing(&self, id: VoiceId) -> bool;
    fn volume(&self, category: Category) -> f32;
    fn set_volume(&mut self, category: Category, volume: f32);
    fn master_gain(&self) -> f32;
    fn set_master_gain(&mut self, gain: f32);
}

impl MixerControl for Mixer {
    fn play_file(&mut self, bytes: Vec<u8>, skip: u64, playback: Playback) -> Option<VoiceId> {
        Some(self.play(open_vorbis(bytes, skip).ok()?, playback))
    }
    fn set_voice(&mut self, id: VoiceId, gain: f32, pan: f32, pitch: f32) {
        Mixer::set_voice(self, id, gain, pan, pitch);
    }
    fn stop(&mut self, id: VoiceId) {
        Mixer::stop(self, id);
    }
    fn stop_all(&mut self) {
        Mixer::stop_all(self);
    }
    fn is_playing(&self, id: VoiceId) -> bool {
        Mixer::is_playing(self, id)
    }
    fn volume(&self, category: Category) -> f32 {
        Mixer::volume(self, category)
    }
    fn set_volume(&mut self, category: Category, volume: f32) {
        Mixer::set_volume(self, category, volume);
    }
    fn master_gain(&self) -> f32 {
        Mixer::master_gain(self)
    }
    fn set_master_gain(&mut self, gain: f32) {
        Mixer::set_master_gain(self, gain);
    }
}

/// Volume category of a sound; each has its own volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Effects,
    Music,
    /// Gains that already hold their category's volume: the voices of the
    /// sound system (`crate::sound::system`, docs/behavior/sound-positional.md
    /// section 5.4). Only the master gain applies.
    Scaled,
}

impl Category {
    const COUNT: usize = 3;

    fn index(self) -> usize {
        match self {
            Category::Effects => 0,
            Category::Music => 1,
            Category::Scaled => 2,
        }
    }
}

/// How a sound plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playback {
    pub looping: bool,
    pub gain: f32,
    /// Stereo balance in [-1, 1]: -1 left only, 0 centred, 1 right only (see
    /// the module docs). Values outside are clamped.
    pub pan: f32,
    pub category: Category,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            looping: false,
            gain: 1.0,
            pan: 0.0,
            category: Category::Effects,
        }
    }
}

/// The two mono files of music track `track`, its left and right channels,
/// relative to the game data directory (docs/behavior/title-menu.md section 6).
pub fn music_files(track: &str) -> [String; 2] {
    [
        format!("sounds\\{track}_l.ogg"),
        format!("sounds\\{track}_r.ogg"),
    ]
}

/// How the two channels of a music track play: started together, not looped,
/// in the music category. The spec places them at x = -0.5 and +0.5 relative
/// to the listener; we play them fully left and right (see the module docs).
pub const MUSIC_PLAYBACK: [Playback; 2] = [
    Playback {
        looping: false,
        gain: 1.0,
        pan: -1.0,
        category: Category::Music,
    },
    Playback {
        looping: false,
        gain: 1.0,
        pan: 1.0,
        category: Category::Music,
    },
];

/// Channel gains for `pan` (the linear balance of the module docs).
fn balance(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0))
}

/// Left and right gains for `gain` at `pan`.
fn gains(gain: f32, pan: f32) -> (f32, f32) {
    let (l, r) = balance(pan);
    (l * gain, r * gain)
}

/// Identifies a playing sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceId(u32);

impl VoiceId {
    /// A number identifying the voice, unique while it plays.
    pub fn index(self) -> u32 {
        self.0
    }
}

/// A playing voice's state ([`Mixer::probe`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceProbe {
    /// Input frames advanced since the voice started, loops included.
    pub advanced: f64,
    /// The left and right gains the last mixed block ended at.
    pub gains: (f32, f32),
    /// The gains the next block ramps to.
    pub target: (f32, f32),
    /// Input frames per output frame.
    pub step: f64,
}

struct Voice {
    id: VoiceId,
    source: Box<dyn Source>,
    channels: usize,
    looping: bool,
    category: Category,
    /// Gains of the left and right channels: playback gain and pan.
    gains: (f32, f32),
    /// The gains [`Mixer::set_voice`] asked for, reached by a linear ramp
    /// over the next mixed block (so changes don't click).
    target: (f32, f32),
    /// Decoded input not yet consumed, interleaved.
    input: Vec<f32>,
    /// Position in input frames (fractional for resampling).
    pos: f64,
    /// Input frames per output frame.
    step: f64,
    /// The step at a pitch of 1.
    base_step: f64,
    /// Input frames advanced since the voice started, loops included.
    advanced: f64,
    ended: bool,
}

impl Voice {
    fn frames(&self) -> usize {
        self.input.len() / self.channels
    }

    /// Ensures input frames up to `needed` (exclusive) are decoded, unless the
    /// source ends; returns whether they are.
    fn fill(&mut self, needed: usize) -> bool {
        while self.frames() < needed {
            if self.ended {
                return false;
            }
            let read = tracing::info_span!("decode").in_scope(|| self.source.read(&mut self.input));
            match read {
                Ok(true) => {}
                Ok(false) if self.looping => {
                    if self.source.rewind().is_err() {
                        self.ended = true;
                    }
                }
                // Errors end the sound rather than the mix.
                Ok(false) | Err(_) => self.ended = true,
            }
        }
        true
    }

    /// Stereo frame at input frame `i`.
    fn frame(&self, i: usize) -> (f32, f32) {
        let s = &self.input[i * self.channels..];
        if self.channels == 1 {
            (s[0], s[0])
        } else {
            (s[0], s[1])
        }
    }

    /// Mixes into `out` (stereo, interleaved) with the voice's gains scaled
    /// by `volume`; returns false when finished.
    fn mix_into(&mut self, out: &mut [f32], volume: f32) -> bool {
        let frames = out.len() / 2;
        let (from, to) = (self.gains, self.target);
        self.gains = to;
        let ramp = |a: f32, b: f32, k: usize| a + (b - a) * (k + 1) as f32 / frames.max(1) as f32;
        for (k, frame) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let (gl, gr) = if from == to {
                (to.0 * volume, to.1 * volume)
            } else {
                (
                    ramp(from.0, to.0, k) * volume,
                    ramp(from.1, to.1, k) * volume,
                )
            };
            let i = self.pos as usize;
            if !self.fill(i + 2) && i + 1 >= self.frames() {
                if i < self.frames() {
                    let (l, r) = self.frame(i);
                    frame[0] += l * gl;
                    frame[1] += r * gr;
                }
                return false;
            }
            let t = (self.pos - i as f64) as f32;
            let (l0, r0) = self.frame(i);
            let (l1, r1) = self.frame(i + 1);
            frame[0] += (l0 + (l1 - l0) * t) * gl;
            frame[1] += (r0 + (r1 - r0) * t) * gr;
            self.pos += self.step;
            self.advanced += self.step;
        }
        // Drop consumed input, keeping the frame interpolation still needs.
        let consumed = (self.pos as usize).min(self.frames());
        if consumed > 0 {
            self.input.drain(..consumed * self.channels);
            self.pos -= consumed as f64;
        }
        true
    }
}

/// Mixes playing sounds into stereo output at a fixed sample rate.
pub struct Mixer {
    rate: u32,
    voices: Vec<Voice>,
    next_id: u32,
    volumes: [f32; Category::COUNT],
    /// The master (listener) gain over every category.
    master: f32,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            voices: Vec::new(),
            next_id: 0,
            volumes: [1.0; Category::COUNT],
            master: 1.0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Volume of `category` (initially 1).
    pub fn volume(&self, category: Category) -> f32 {
        self.volumes[category.index()]
    }

    /// Sets the volume of `category`, applying to its playing sounds too.
    pub fn set_volume(&mut self, category: Category, volume: f32) {
        self.volumes[category.index()] = volume;
    }

    /// The master (listener) gain (initially 1).
    pub fn master_gain(&self) -> f32 {
        self.master
    }

    /// Sets the master (listener) gain, scaling every sound, playing or not.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master = gain;
    }

    /// Starts playing `source`. Sounds started between two [`Mixer::mix`]
    /// calls start on the same output frame.
    pub fn play(&mut self, source: Box<dyn Source>, playback: Playback) -> VoiceId {
        let id = VoiceId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.play_as(id, source, playback);
        id
    }

    /// [`Mixer::play`] with an id chosen by the caller ([`remote::Host`]:
    /// the engine's), not one of a voice playing.
    fn play_as(&mut self, id: VoiceId, source: Box<dyn Source>, playback: Playback) {
        let channels = usize::from(source.channels().max(1));
        let step = f64::from(source.sample_rate()) / f64::from(self.rate);
        self.voices.push(Voice {
            id,
            source,
            channels,
            looping: playback.looping,
            category: playback.category,
            gains: gains(playback.gain, playback.pan),
            target: gains(playback.gain, playback.pan),
            input: Vec::new(),
            pos: 0.0,
            step,
            base_step: step,
            advanced: 0.0,
            ended: false,
        });
    }

    /// The voices playing.
    pub fn voices(&self) -> impl Iterator<Item = VoiceId> + '_ {
        self.voices.iter().map(|v| v.id)
    }

    /// Changes a playing voice's gain, pan and pitch (a playback-rate
    /// factor, which also changes its pitch); nothing if it has ended.
    pub fn set_voice(&mut self, id: VoiceId, gain: f32, pan: f32, pitch: f32) {
        if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
            v.target = gains(gain, pan);
            v.step = v.base_step * f64::from(pitch);
        }
    }

    pub fn stop(&mut self, id: VoiceId) {
        self.voices.retain(|v| v.id != id);
    }

    pub fn stop_all(&mut self) {
        self.voices.clear();
    }

    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.voices.iter().any(|v| v.id == id)
    }

    /// Where a playing voice is: for tests and measurements.
    pub fn probe(&self, id: VoiceId) -> Option<VoiceProbe> {
        self.voices.iter().find(|v| v.id == id).map(|v| VoiceProbe {
            advanced: v.advanced,
            gains: v.gains,
            target: v.target,
            step: v.step,
        })
    }

    /// Mixes the next `frames` output frames: stereo, interleaved. Finished
    /// sounds are removed.
    pub fn mix(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        let (volumes, master) = (self.volumes, self.master);
        self.voices
            .retain_mut(|voice| voice.mix_into(&mut out, master * volumes[voice.category.index()]));
        for sample in &mut out {
            *sample = sample.clamp(-1.0, 1.0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source producing a known ramp in blocks.
    struct Ramp {
        rate: u32,
        channels: u16,
        frames: usize,
        pos: usize,
    }

    impl Source for Ramp {
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn channels(&self) -> u16 {
            self.channels
        }
        fn read(&mut self, out: &mut Vec<f32>) -> io::Result<bool> {
            if self.pos >= self.frames {
                return Ok(false);
            }
            let end = (self.pos + 7).min(self.frames);
            for i in self.pos..end {
                for c in 0..self.channels {
                    out.push(i as f32 / 100.0 * if c == 0 { 1.0 } else { -1.0 });
                }
            }
            self.pos = end;
            Ok(true)
        }
        fn rewind(&mut self) -> io::Result<()> {
            self.pos = 0;
            Ok(())
        }
    }

    fn ramp(rate: u32, channels: u16, frames: usize) -> Box<dyn Source> {
        Box::new(Ramp {
            rate,
            channels,
            frames,
            pos: 0,
        })
    }

    fn gain(gain: f32) -> Playback {
        Playback {
            gain,
            ..Playback::default()
        }
    }

    fn channels(out: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let left = out.iter().step_by(2).copied().collect();
        let right = out.iter().skip(1).step_by(2).copied().collect();
        (left, right)
    }

    #[test]
    fn set_voice_ramps_gain_and_pan_and_changes_pitch() {
        let mut mixer = Mixer::new(100);
        mixer.set_volume(Category::Effects, 0.0);
        let id = mixer.play(
            ramp(100, 1, 1000),
            Playback {
                gain: 0.0,
                category: Category::Scaled,
                ..Playback::default()
            },
        );
        mixer.set_voice(id, 0.5, 1.0, 1.0);
        let (left, right) = channels(&mixer.mix(10));
        assert!(left.iter().all(|&s| s == 0.0));
        // Frame k of the block at gain 0.5 · (k + 1) / 10: the ramp's value
        // k / 100 times that.
        for (k, s) in right.iter().enumerate() {
            let want = k as f32 / 100.0 * 0.5 * (k + 1) as f32 / 10.0;
            assert!((s - want).abs() < 1e-6, "{k}: {s} vs {want}");
        }
        // At pitch 2 the voice steps two input frames per output frame.
        mixer.set_voice(id, 0.5, 1.0, 2.0);
        let (_, right) = channels(&mixer.mix(3));
        for (k, s) in right.iter().enumerate() {
            let want = (10 + 2 * k) as f32 / 100.0 * 0.5;
            assert!((s - want).abs() < 1e-6, "{k}: {s} vs {want}");
        }
        // Category volumes don't scale a Scaled voice; the master gain does.
        mixer.set_master_gain(0.0);
        assert!(mixer.mix(3).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn plays_at_the_same_rate_exactly() {
        let mut mixer = Mixer::new(100);
        let id = mixer.play(ramp(100, 2, 10), Playback::default());
        let out = mixer.mix(12);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let right: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
        let expected: Vec<f32> = (0..10)
            .map(|i| i as f32 / 100.0)
            .chain([0.0, 0.0])
            .collect();
        assert_eq!(left, expected);
        assert_eq!(right, expected.iter().map(|v| -v).collect::<Vec<_>>());
        assert!(!mixer.is_playing(id), "finished sounds are removed");
    }

    #[test]
    fn mono_goes_to_both_channels_and_gain_applies() {
        let mut mixer = Mixer::new(100);
        mixer.play(ramp(100, 1, 5), gain(0.5));
        let out = mixer.mix(5);
        assert_eq!(out[6], 0.015);
        assert_eq!(out[7], 0.015);
    }

    #[test]
    fn resamples_linearly() {
        // 50 Hz source at 100 Hz output: every other frame interpolates.
        let mut mixer = Mixer::new(100);
        mixer.play(ramp(50, 1, 10), Playback::default());
        let out = mixer.mix(6);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        for (got, want) in left.iter().zip([0.0, 0.005, 0.01, 0.015, 0.02, 0.025]) {
            assert!((got - want).abs() < 1e-6, "{left:?}");
        }
    }

    #[test]
    fn loops_and_stops() {
        let mut mixer = Mixer::new(100);
        let id = mixer.play(
            ramp(100, 1, 4),
            Playback {
                looping: true,
                ..Playback::default()
            },
        );
        let out = mixer.mix(9);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert_eq!(left, [0.0, 0.01, 0.02, 0.03, 0.0, 0.01, 0.02, 0.03, 0.0]);
        assert!(mixer.is_playing(id));
        mixer.stop(id);
        assert!(mixer.mix(3).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn master_gain_mutes_and_restores_playing_sounds() {
        let mut mixer = Mixer::new(100);
        mixer.play(ramp(100, 1, 10), Playback::default());
        mixer.set_master_gain(0.0);
        assert!(mixer.mix(3).iter().all(|&s| s == 0.0));
        mixer.set_master_gain(1.0);
        assert_eq!(mixer.mix(1)[0], 0.03, "kept playing while muted");
    }

    #[test]
    fn mixes_voices_and_clamps() {
        let mut mixer = Mixer::new(100);
        mixer.play(ramp(100, 1, 10), gain(100.0));
        mixer.play(ramp(100, 1, 10), gain(100.0));
        let out = mixer.mix(3);
        assert_eq!(out[2], 1.0, "0.01 * 100 * 2 clamps to 1");
    }

    #[test]
    fn music_tracks_are_pairs_of_files() {
        assert_eq!(
            music_files("music\\wasteland2"),
            [
                "sounds\\music\\wasteland2_l.ogg",
                "sounds\\music\\wasteland2_r.ogg"
            ]
        );
        assert_eq!(MUSIC_PLAYBACK.map(|p| p.pan), [-1.0, 1.0]);
    }

    #[test]
    fn pans_a_stereo_pair_into_separate_channels() {
        // Two mono files as the left and right channels of one track, as the
        // menu music plays them.
        let mut mixer = Mixer::new(100);
        mixer.play(ramp(100, 1, 10), MUSIC_PLAYBACK[0]);
        mixer.play(ramp(100, 1, 5), MUSIC_PLAYBACK[1]);
        let (left, right) = channels(&mixer.mix(10));
        let expected: Vec<f32> = (0..10).map(|i| i as f32 / 100.0).collect();
        assert_eq!(left, expected);
        assert_eq!(right[..5], expected[..5]);
        assert!(right[5..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn partial_pan_attenuates_the_far_channel_linearly() {
        let mut mixer = Mixer::new(100);
        mixer.play(
            ramp(100, 2, 3),
            Playback {
                pan: 0.25,
                ..Playback::default()
            },
        );
        let out = mixer.mix(3);
        assert_eq!(out[4], 0.02 * 0.75, "left scaled by 1 - pan");
        assert_eq!(out[5], -0.02, "right at full gain");
        assert_eq!(balance(-7.0), (1.0, 0.0), "pan clamps");
    }

    #[test]
    fn category_volumes_scale_their_voices() {
        let mut mixer = Mixer::new(100);
        assert_eq!(mixer.volume(Category::Music), 1.0);
        mixer.set_volume(Category::Music, 0.5);
        mixer.set_volume(Category::Effects, 0.0);
        mixer.play(
            ramp(100, 1, 10),
            Playback {
                category: Category::Music,
                ..Playback::default()
            },
        );
        mixer.play(ramp(100, 1, 10), gain(3.0));
        let (left, right) = channels(&mixer.mix(3));
        assert_eq!(left, [0.0, 0.005, 0.01]);
        assert_eq!(left, right);
        mixer.set_volume(Category::Effects, 1.0);
        let out = mixer.mix(1);
        assert!(
            (out[0] - (0.015 + 0.09)).abs() < 1e-6,
            "changes apply to playing voices: {out:?}"
        );
    }
}
