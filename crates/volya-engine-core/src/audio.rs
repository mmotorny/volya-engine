// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Audio mixing: decoded sources mixed into one stereo stream at the output's
//! sample rate. Platform-free; platforms pull mixed blocks with
//! [`Mixer::mix`].

use std::io::{self, Read, Seek};

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

/// Identifies a playing sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceId(u32);

impl VoiceId {
    /// A number identifying the voice, unique while it plays.
    pub fn index(self) -> u32 {
        self.0
    }
}

struct Voice {
    id: VoiceId,
    source: Box<dyn Source>,
    channels: usize,
    looping: bool,
    gain: f32,
    /// Decoded input not yet consumed, interleaved.
    input: Vec<f32>,
    /// Position in input frames (fractional for resampling).
    pos: f64,
    /// Input frames per output frame.
    step: f64,
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
            match self.source.read(&mut self.input) {
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

    /// Mixes into `out` (stereo, interleaved); returns false when finished.
    fn mix_into(&mut self, out: &mut [f32]) -> bool {
        for frame in out.as_chunks_mut::<2>().0 {
            let i = self.pos as usize;
            if !self.fill(i + 2) && i + 1 >= self.frames() {
                if i < self.frames() {
                    let (l, r) = self.frame(i);
                    frame[0] += l * self.gain;
                    frame[1] += r * self.gain;
                }
                return false;
            }
            let t = (self.pos - i as f64) as f32;
            let (l0, r0) = self.frame(i);
            let (l1, r1) = self.frame(i + 1);
            frame[0] += (l0 + (l1 - l0) * t) * self.gain;
            frame[1] += (r0 + (r1 - r0) * t) * self.gain;
            self.pos += self.step;
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
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            voices: Vec::new(),
            next_id: 0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Starts playing `source`.
    pub fn play(&mut self, source: Box<dyn Source>, looping: bool, gain: f32) -> VoiceId {
        let id = VoiceId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        let channels = usize::from(source.channels().max(1));
        let step = f64::from(source.sample_rate()) / f64::from(self.rate);
        self.voices.push(Voice {
            id,
            source,
            channels,
            looping,
            gain,
            input: Vec::new(),
            pos: 0.0,
            step,
            ended: false,
        });
        id
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

    /// Mixes the next `frames` output frames: stereo, interleaved. Finished
    /// sounds are removed.
    pub fn mix(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        self.voices.retain_mut(|voice| voice.mix_into(&mut out));
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

    #[test]
    fn plays_at_the_same_rate_exactly() {
        let mut mixer = Mixer::new(100);
        let id = mixer.play(ramp(100, 2, 10), false, 1.0);
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
        mixer.play(ramp(100, 1, 5), false, 0.5);
        let out = mixer.mix(5);
        assert_eq!(out[6], 0.015);
        assert_eq!(out[7], 0.015);
    }

    #[test]
    fn resamples_linearly() {
        // 50 Hz source at 100 Hz output: every other frame interpolates.
        let mut mixer = Mixer::new(100);
        mixer.play(ramp(50, 1, 10), false, 1.0);
        let out = mixer.mix(6);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        for (got, want) in left.iter().zip([0.0, 0.005, 0.01, 0.015, 0.02, 0.025]) {
            assert!((got - want).abs() < 1e-6, "{left:?}");
        }
    }

    #[test]
    fn loops_and_stops() {
        let mut mixer = Mixer::new(100);
        let id = mixer.play(ramp(100, 1, 4), true, 1.0);
        let out = mixer.mix(9);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert_eq!(left, [0.0, 0.01, 0.02, 0.03, 0.0, 0.01, 0.02, 0.03, 0.0]);
        assert!(mixer.is_playing(id));
        mixer.stop(id);
        assert!(mixer.mix(3).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn mixes_voices_and_clamps() {
        let mut mixer = Mixer::new(100);
        mixer.play(ramp(100, 1, 10), false, 100.0);
        mixer.play(ramp(100, 1, 10), false, 100.0);
        let out = mixer.mix(3);
        assert_eq!(out[2], 1.0, "0.01 * 100 * 2 clamps to 1");
    }
}
