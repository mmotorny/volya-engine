// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use std::io::Read;

/// A source of a known signal: frame `i` of channel `c` is
/// `sin(i · (c + 1) · 0.01) · 0.5`, in blocks of 37 frames.
struct Tone {
    rate: u32,
    channels: u16,
    frames: u64,
    pos: u64,
}

impl Source for Tone {
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
        let end = (self.pos + 37).min(self.frames);
        for i in self.pos..end {
            for c in 0..self.channels {
                out.push((i as f32 * f32::from(c + 1) * 0.01).sin() * 0.5);
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

/// A "file" naming a tone: rate, channels and length; empty fails to open.
fn tone_file(rate: u32, channels: u16, frames: u64) -> Vec<u8> {
    let mut b = rate.to_le_bytes().to_vec();
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&frames.to_le_bytes());
    b
}

fn open_tone(mut file: Box<dyn File>, skip: u64) -> io::Result<Box<dyn Source>> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if bytes.len() != 14 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a tone"));
    }
    let rate = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let channels = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    let frames = u64::from_le_bytes(bytes[6..14].try_into().unwrap());
    Ok(Box::new(Tone {
        rate,
        channels,
        frames,
        pos: skip.min(frames),
    }))
}

/// Hands `m` to `host`: its files, then its commands, all applied.
fn deliver(host: &mut Host, m: Message) {
    for (n, f) in m.files {
        host.add_file(n, Box::new(io::Cursor::new(f)));
    }
    assert_eq!(host.apply(&m.commands).unwrap(), m.commands.len());
}

/// A mixer driven directly and one driven through a remote and its host.
struct Pair {
    direct: Mixer,
    /// The direct mixer's voice of each remote voice, by index.
    ids: Vec<(VoiceId, VoiceId)>,
    remote: Remote,
    host: Host,
}

impl Pair {
    fn new(rate: u32) -> Self {
        Self {
            direct: Mixer::new(rate),
            ids: Vec::new(),
            remote: Remote::new(rate),
            host: Host::new(open_tone),
        }
    }

    fn play(&mut self, file: Vec<u8>, skip: u64, playback: Playback) -> VoiceId {
        let direct = self.direct.play(
            open_tone(Box::new(io::Cursor::new(file.clone())), skip).unwrap(),
            playback,
        );
        let remote = self.remote.play_unchecked(file, skip, playback);
        self.ids.push((remote, direct));
        remote
    }

    fn direct_id(&self, id: VoiceId) -> VoiceId {
        self.ids.iter().find(|(r, _)| *r == id).unwrap().1
    }

    fn set_voice(&mut self, id: VoiceId, gain: f32, pan: f32, pitch: f32) {
        let d = self.direct_id(id);
        self.direct.set_voice(d, gain, pan, pitch);
        MixerControl::set_voice(&mut self.remote, id, gain, pan, pitch);
    }

    fn stop(&mut self, id: VoiceId) {
        let d = self.direct_id(id);
        self.direct.stop(d);
        MixerControl::stop(&mut self.remote, id);
    }

    /// Mixes a block on both; they must agree exactly.
    fn mix(&mut self, frames: usize) -> Vec<f32> {
        if let Some(m) = self.remote.take() {
            deliver(&mut self.host, m);
        }
        let direct = self.direct.mix(frames);
        let hosted = self.host.mix(frames);
        assert_eq!(direct.len(), hosted.len());
        for (i, (a, b)) in direct.iter().zip(&hosted).enumerate() {
            assert_eq!(a.to_bits(), b.to_bits(), "sample {i}: {a} vs {b}");
        }
        let ended = self.host.take_ended();
        self.remote.ended(&ended);
        // Both agree on which voices still play.
        for &(r, d) in &self.ids {
            assert_eq!(self.remote.is_playing(r), self.direct.is_playing(d));
        }
        hosted
    }
}

#[test]
fn a_hosted_mixer_mixes_what_a_direct_one_does() {
    // Per-frame changes as a sound system sends them (gain, pan, pitch,
    // volumes, master gain, starts and stops) between blocks of varying
    // sizes, at a rate other than the sources'.
    let mut pair = Pair::new(48_000);
    let looped = pair.play(
        tone_file(44_100, 1, 3_000),
        0,
        Playback {
            looping: true,
            gain: 0.0,
            pan: 0.0,
            category: Category::Scaled,
        },
    );
    let music = pair.play(
        tone_file(44_100, 2, 20_000),
        0,
        Playback {
            category: Category::Music,
            ..Playback::default()
        },
    );
    let mut heard = 0.0f32;
    for frame in 0..200u32 {
        let t = frame as f32 * 0.05;
        pair.set_voice(
            looped,
            0.5 + 0.4 * t.sin(),
            t.cos(),
            1.0 + 0.2 * (t * 0.7).sin(),
        );
        match frame {
            20 => pair.remote.set_volume(Category::Music, 0.3),
            40 => {
                MixerControl::set_master_gain(&mut pair.remote, 0.5);
                pair.direct.set_master_gain(0.5);
            }
            60 => {
                pair.play(tone_file(22_050, 1, 500), 100, Playback::default());
            }
            90 => pair.stop(music),
            _ => {}
        }
        if frame == 20 {
            pair.direct.set_volume(Category::Music, 0.3);
        }
        let out = pair.mix([128, 735, 2_400, 50][frame as usize % 4]);
        heard = heard.max(out.iter().fold(0.0, |m, s| m.max(s.abs())));
    }
    assert!(heard > 0.1, "something played: {heard}");
    assert!(pair.remote.is_playing(looped));
    assert!(!pair.remote.is_playing(music));
}

#[test]
fn ended_voices_are_reported() {
    let mut pair = Pair::new(100);
    let short = pair.play(tone_file(100, 1, 10), 0, Playback::default());
    pair.mix(5);
    assert!(pair.remote.is_playing(short));
    pair.mix(10);
    assert!(!pair.remote.is_playing(short), "ended in the host");
    // A file that does not open ends at once.
    let bad = pair
        .remote
        .play_unchecked(Vec::new(), 0, Playback::default());
    assert!(pair.remote.is_playing(bad));
    let m = pair.remote.take().unwrap();
    deliver(&mut pair.host, m);
    pair.remote.ended(&pair.host.take_ended());
    assert!(!pair.remote.is_playing(bad));
}

#[test]
fn a_new_host_keeps_volumes_and_drops_voices() {
    let mut remote = Remote::new(100);
    let mut host = Host::new(open_tone);
    remote.set_volume(Category::Effects, 0.5);
    let id = remote.play_unchecked(tone_file(100, 1, 1_000), 0, Playback::default());
    let m = remote.take().unwrap();
    deliver(&mut host, m);
    assert_eq!(host.mix(4)[2], (0.01f32).sin() * 0.5 * 0.5);
    // Reconnected: a fresh host from the first message on.
    remote.connect(100);
    assert!(!remote.is_playing(id));
    let next = remote.play_unchecked(tone_file(100, 1, 1_000), 0, Playback::default());
    assert_ne!(next, id, "numbers go on");
    let mut fresh = Host::new(open_tone);
    let m = remote.take().unwrap();
    deliver(&mut fresh, m);
    assert_eq!(fresh.mix(4)[2], (0.01f32).sin() * 0.5 * 0.5, "volume kept");
    // The old host's voice is reported ended on a new INIT.
    remote.connect(100);
    let m = remote.take().unwrap();
    deliver(&mut host, m);
    assert_eq!(host.take_ended(), [id.index()]);
    assert!(remote.take().is_none(), "nothing more to send");
}

#[test]
fn malformed_messages_are_errors_and_silence_comes_first() {
    let mut host = Host::default();
    assert!(host.mix(3).iter().all(|&s| s == 0.0));
    assert!(host.apply(&[99]).is_err());
    assert!(host.apply(&[INIT, 1, 2]).is_err());
    // A command before INIT has no mixer.
    assert!(host.apply(&[STOP_ALL]).is_err());
}

#[test]
fn commands_wait_for_every_file_they_play_and_apply_together() {
    // A frame starting two voices whose files arrive one at a time: blocks
    // mixed between the arrivals have neither, and both start together.
    let mut pair = Pair::new(100);
    let left = pair.play(tone_file(100, 1, 1_000), 0, Playback::default());
    let right = pair.play(tone_file(100, 1, 1_000), 0, Playback::default());
    MixerControl::set_master_gain(&mut pair.remote, 0.5);
    let m = pair.remote.take().unwrap();
    let mut files = m.files.into_iter();
    let (n, f) = files.next().unwrap();
    pair.host.add_file(n, Box::new(io::Cursor::new(f)));
    assert_eq!(
        pair.host.apply(&m.commands).unwrap(),
        0,
        "a file is missing"
    );
    assert!(pair.host.mix(3).iter().all(|&s| s == 0.0));
    for (n, f) in files {
        pair.host.add_file(n, Box::new(io::Cursor::new(f)));
    }
    assert_eq!(pair.host.apply(&m.commands).unwrap(), m.commands.len());
    let mixer = pair.host.mixer_ref().unwrap();
    assert_eq!(mixer.master_gain(), 0.5);
    let (l, r) = (mixer.probe(left).unwrap(), mixer.probe(right).unwrap());
    assert_eq!(
        (l.advanced, r.advanced),
        (0.0, 0.0),
        "neither started before the other"
    );
}

#[test]
fn watch_is_carried() {
    let mut remote = Remote::new(100);
    let mut host = Host::new(open_tone);
    remote.watch(Some(Some(VoiceId(7))));
    let m = remote.take().unwrap();
    deliver(&mut host, m);
    assert_eq!(host.watched(), Some(Some(VoiceId(7))));
    remote.watch(Some(None));
    remote.watch(None);
    let m = remote.take().unwrap();
    deliver(&mut host, m);
    assert_eq!(host.watched(), None);
}

#[test]
fn checked_plays_need_a_vorbis_stream() {
    let mut remote = Remote::new(100);
    assert!(
        remote
            .play_file(vec![1, 2, 3], 0, Playback::default())
            .is_none()
    );
}

/// A mono source of 16-bit samples (as a Vorbis stream decodes), its
/// length known: frame `i` is `(i · 13) mod 65,536`, in blocks of 37.
struct Exact {
    frames: u64,
    pos: u64,
}

impl Source for Exact {
    fn sample_rate(&self) -> u32 {
        44_100
    }
    fn channels(&self) -> u16 {
        1
    }
    fn read(&mut self, out: &mut Vec<f32>) -> io::Result<bool> {
        if self.pos >= self.frames {
            return Ok(false);
        }
        let end = (self.pos + 37).min(self.frames);
        out.extend(
            (self.pos..end).map(|i| f32::from(((i * 13) % 65_536) as u16 as i16) / 32_768.0),
        );
        self.pos = end;
        Ok(true)
    }
    fn rewind(&mut self) -> io::Result<()> {
        self.pos = 0;
        Ok(())
    }
    fn frames(&self) -> Option<u64> {
        Some(self.frames)
    }
}

fn open_exact(mut file: Box<dyn File>, skip: u64) -> io::Result<Box<dyn Source>> {
    let mut b = [0; 8];
    file.read_exact(&mut b)?;
    let frames = u64::from_le_bytes(b);
    Ok(Box::new(Exact {
        frames,
        pos: skip.min(frames),
    }))
}

#[test]
fn files_played_by_key_mix_through_the_cache_as_decoded() {
    let mut remote = Remote::new(48_000);
    let mut host = Host::new(open_exact);
    let mut direct = Mixer::new(48_000);
    // Before the cache is sized, and after (twice: the second time from
    // its lines): the same output.
    for (sized, looping) in [(false, false), (true, true), (true, false)] {
        if sized {
            MixerControl::set_sound_cache(&mut remote, 4);
        }
        let file = 20_000u64.to_le_bytes().to_vec();
        let playback = Playback {
            looping,
            ..Playback::default()
        };
        for skip in [0, 9_000] {
            let key = remote.key("a.ogg");
            let (_, n) = remote.play_sent(key, skip, playback);
            host.add_file(n, Box::new(io::Cursor::new(file.clone())));
            let source = open_exact(Box::new(io::Cursor::new(file.clone())), skip).unwrap();
            direct.play(source, playback);
        }
        let m = remote.take().unwrap();
        assert_eq!(host.apply(&m.commands).unwrap(), m.commands.len());
        if !sized {
            assert_eq!(host.cache_bytes(), 0);
        }
        for _ in 0..30 {
            let (a, b) = (direct.mix(1_000), host.mix(1_000));
            for (i, (x, y)) in a.iter().zip(&b).enumerate() {
                assert_eq!(x.to_bits(), y.to_bits(), "sample {i}");
            }
        }
        direct.stop_all();
        MixerControl::stop_all(&mut remote);
    }
    // The code of the file's 5 lines of 4,410 samples (allocated as first
    // used, at most their samples' bytes), every line's record, the file's
    // table, 2 bytes a line, and the coder's buffers (a line's code and
    // residuals).
    let lines = super::super::cache::lines_for(4);
    assert!(host.cache_bytes() >= lines * 16 + 10);
    assert!(host.cache_bytes() < 5 * 8_821 + lines * 40 + 4 * 8_820);
    // A new host is sized again with the connection: its records only.
    remote.connect(48_000);
    let mut fresh = Host::new(open_exact);
    let m = remote.take().unwrap();
    fresh.apply(&m.commands).unwrap();
    assert!(fresh.cache_bytes() >= lines * 16);
    assert!(fresh.cache_bytes() < lines * 40);
}
