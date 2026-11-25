// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A mixer that mixes on the output's own schedule, apart from the engine
//! (docs/architecture.md, Audio): the engine drives a [`Remote`], which
//! records what it is asked as commands; a [`Host`] beside the output
//! applies them to its [`Mixer`] as they arrive and mixes whenever the
//! output asks, as an audio device mixes apart from the game's frames.
//!
//! The commands are a byte stream (the web platform passes it through a
//! ring buffer in shared memory); those taken together apply together
//! before the next block, so a [`Host`] mixes exactly what a [`Mixer`]
//! driven directly does when the same changes come between the same
//! blocks. Voices are numbered by the [`Remote`]; it learns that one ended
//! from the [`Host`]'s reports ([`Remote::ended`]). Compressed files travel
//! apart from the commands, numbered ([`Host::add_file`]), and are decoded
//! by the host, block by block; commands playing a file not yet arrived
//! wait for it, with every command taken with them.

use std::io::{self, Cursor};

use volya_engine_formats::audio::VorbisStream;

use super::{Category, Mixer, MixerControl, Playback, Source, VoiceId, open_vorbis};

const INIT: u8 = 0;
const PLAY: u8 = 1;
const STOP: u8 = 2;
const STOP_ALL: u8 = 3;
const SET_VOICE: u8 = 4;
const SET_VOLUME: u8 = 5;
const SET_MASTER: u8 = 6;
const WATCH: u8 = 7;

const CATEGORIES: [Category; Category::COUNT] =
    [Category::Effects, Category::Music, Category::Scaled];

/// The engine's side: a [`MixerControl`] recording commands for a [`Host`].
pub struct Remote {
    next_id: u32,
    /// The number of the next file sent.
    next_file: u32,
    /// Voices started and not known to have ended.
    playing: Vec<VoiceId>,
    volumes: [f32; Category::COUNT],
    master: f32,
    commands: Vec<u8>,
    files: Vec<(u32, Vec<u8>)>,
}

/// What a [`Host`] is sent at once: commands, and the files they play with
/// their numbers.
pub struct Message {
    pub commands: Vec<u8>,
    pub files: Vec<(u32, Vec<u8>)>,
}

impl Remote {
    /// A mixer at `rate` Hz, all volumes 1.
    pub fn new(rate: u32) -> Self {
        let mut remote = Self {
            next_id: 0,
            next_file: 0,
            playing: Vec::new(),
            volumes: [1.0; Category::COUNT],
            master: 1.0,
            commands: Vec::new(),
            files: Vec::new(),
        };
        remote.connect(rate);
        remote
    }

    /// A new host at `rate` Hz: commands not yet taken are dropped, every
    /// voice is gone, and the volumes and the master gain carry over. Voice
    /// and file numbers go on, so reports of the old host's voices change
    /// nothing.
    pub fn connect(&mut self, rate: u32) {
        self.commands.clear();
        self.files.clear();
        self.playing.clear();
        self.commands.push(INIT);
        self.u32(rate);
        for category in CATEGORIES {
            self.push_volume(category);
        }
        self.commands.push(SET_MASTER);
        self.f32(self.master);
    }

    /// Starts `bytes` without checking that they are a stream that plays;
    /// one that is not ends at once, which a report tells.
    pub fn play_unchecked(&mut self, bytes: Vec<u8>, skip: u64, playback: Playback) -> VoiceId {
        let id = VoiceId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.commands.push(PLAY);
        self.u32(id.0);
        let file = self.next_file;
        self.next_file = self.next_file.wrapping_add(1);
        self.u32(file);
        self.commands.extend_from_slice(&skip.to_le_bytes());
        self.commands.push(u8::from(playback.looping));
        self.f32(playback.gain);
        self.f32(playback.pan);
        self.commands.push(playback.category.index() as u8);
        self.files.push((file, bytes));
        self.playing.push(id);
        id
    }

    /// The host reports that these voices ended.
    pub fn ended(&mut self, ids: &[u32]) {
        self.playing.retain(|v| !ids.contains(&v.0));
    }

    /// For measurements: the host records every block with the state of
    /// `voice` (`Some(None)`: no voice), or nothing (`None`).
    pub fn watch(&mut self, voice: Option<Option<VoiceId>>) {
        self.commands.push(WATCH);
        let code = match voice {
            None => -2,
            Some(None) => -1,
            Some(Some(id)) => i64::from(id.0),
        };
        self.commands.extend_from_slice(&code.to_le_bytes());
    }

    /// The commands since the last call, `None` when there are none.
    pub fn take(&mut self) -> Option<Message> {
        if self.commands.is_empty() {
            return None;
        }
        Some(Message {
            commands: std::mem::take(&mut self.commands),
            files: std::mem::take(&mut self.files),
        })
    }

    fn u32(&mut self, v: u32) {
        self.commands.extend_from_slice(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.commands.extend_from_slice(&v.to_le_bytes());
    }

    fn push_volume(&mut self, category: Category) {
        self.commands.push(SET_VOLUME);
        self.commands.push(category.index() as u8);
        self.f32(self.volumes[category.index()]);
    }
}

impl MixerControl for Remote {
    fn play_file(&mut self, bytes: Vec<u8>, skip: u64, playback: Playback) -> Option<VoiceId> {
        // The headers only; the host decodes.
        VorbisStream::open(Cursor::new(bytes.as_slice())).ok()?;
        Some(self.play_unchecked(bytes, skip, playback))
    }

    fn set_voice(&mut self, id: VoiceId, gain: f32, pan: f32, pitch: f32) {
        if !self.is_playing(id) {
            return;
        }
        self.commands.push(SET_VOICE);
        self.u32(id.0);
        self.f32(gain);
        self.f32(pan);
        self.f32(pitch);
    }

    fn stop(&mut self, id: VoiceId) {
        if !self.is_playing(id) {
            return;
        }
        self.playing.retain(|v| *v != id);
        self.commands.push(STOP);
        self.u32(id.0);
    }

    fn stop_all(&mut self) {
        self.playing.clear();
        self.commands.push(STOP_ALL);
    }

    fn is_playing(&self, id: VoiceId) -> bool {
        self.playing.contains(&id)
    }

    fn volume(&self, category: Category) -> f32 {
        self.volumes[category.index()]
    }

    fn set_volume(&mut self, category: Category, volume: f32) {
        self.volumes[category.index()] = volume;
        self.push_volume(category);
    }

    fn master_gain(&self) -> f32 {
        self.master
    }

    fn set_master_gain(&mut self, gain: f32) {
        self.master = gain;
        self.commands.push(SET_MASTER);
        self.f32(gain);
    }
}

/// Opens a voice's compressed file, so many frames in.
pub type Opener = fn(Vec<u8>, u64) -> io::Result<Box<dyn Source>>;

/// The output's side: applies a [`Remote`]'s commands to its mixer and
/// mixes on the output's schedule.
pub struct Host {
    /// `None` until the first command (`INIT`): silence.
    mixer: Option<Mixer>,
    /// Files arrived and not yet played, by number.
    files: Vec<(u32, Vec<u8>)>,
    open: Opener,
    /// Voices that ended since the last [`Host::take_ended`].
    ended: Vec<u32>,
    watch: Option<Option<VoiceId>>,
}

impl Default for Host {
    fn default() -> Self {
        Self::new(open_vorbis)
    }
}

/// A command message that does not parse.
fn malformed() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "malformed mixer command")
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>().ok_or_else(malformed)?;
        self.0 = rest;
        Ok(*head)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.bytes::<1>()?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn category(&mut self) -> io::Result<Category> {
        CATEGORIES
            .get(usize::from(self.u8()?))
            .copied()
            .ok_or_else(malformed)
    }
}

/// A decoded command.
enum Command {
    Init(u32),
    Play {
        id: VoiceId,
        file: u32,
        skip: u64,
        playback: Playback,
    },
    Stop(VoiceId),
    StopAll,
    SetVoice {
        id: VoiceId,
        gain: f32,
        pan: f32,
        pitch: f32,
    },
    SetVolume(Category, f32),
    SetMaster(f32),
    Watch(Option<Option<VoiceId>>),
}

/// The commands of `bytes`, which must be whole.
fn decode(bytes: &[u8]) -> io::Result<Vec<Command>> {
    let mut r = Reader(bytes);
    let mut out = Vec::new();
    while !r.0.is_empty() {
        out.push(match r.u8()? {
            INIT => Command::Init(r.u32()?),
            PLAY => Command::Play {
                id: VoiceId(r.u32()?),
                file: r.u32()?,
                skip: r.u64()?,
                playback: Playback {
                    looping: r.u8()? != 0,
                    gain: r.f32()?,
                    pan: r.f32()?,
                    category: r.category()?,
                },
            },
            STOP => Command::Stop(VoiceId(r.u32()?)),
            STOP_ALL => Command::StopAll,
            SET_VOICE => Command::SetVoice {
                id: VoiceId(r.u32()?),
                gain: r.f32()?,
                pan: r.f32()?,
                pitch: r.f32()?,
            },
            SET_VOLUME => Command::SetVolume(r.category()?, r.f32()?),
            SET_MASTER => Command::SetMaster(r.f32()?),
            WATCH => Command::Watch(match r.u64()? as i64 {
                -2 => None,
                -1 => Some(None),
                id => Some(Some(VoiceId(id as u32))),
            }),
            _ => return Err(malformed()),
        });
    }
    Ok(out)
}

impl Host {
    /// A host opening files with `open` ([`open_vorbis`] for the game's).
    pub fn new(open: Opener) -> Self {
        Self {
            mixer: None,
            files: Vec::new(),
            open,
            ended: Vec::new(),
            watch: None,
        }
    }

    /// File `number` arrived.
    pub fn add_file(&mut self, number: u32, bytes: Vec<u8>) {
        self.files.push((number, bytes));
    }

    /// Applies `commands` (whole ones) once every file they play has
    /// arrived, all of them together, so none of them is mixed apart from
    /// the others; returns how many bytes it applied: all, or 0 while a
    /// file is missing. A file that does not open ends its voice at once
    /// (reported as ended); malformed commands are an error.
    pub fn apply(&mut self, commands: &[u8]) -> io::Result<usize> {
        let decoded = decode(commands)?;
        let missing = decoded.iter().any(|c| match c {
            Command::Play { file, .. } => !self.files.iter().any(|(n, _)| n == file),
            _ => false,
        });
        if missing {
            return Ok(0);
        }
        for command in decoded {
            match command {
                Command::Init(rate) => {
                    if let Some(old) = self.mixer.take() {
                        self.ended.extend(old.voices().map(VoiceId::index));
                    }
                    self.mixer = Some(Mixer::new(rate));
                }
                Command::Play {
                    id,
                    file,
                    skip,
                    playback,
                } => {
                    let i = self.files.iter().position(|(n, _)| *n == file);
                    let (_, bytes) = self.files.swap_remove(i.ok_or_else(malformed)?);
                    match (self.open)(bytes, skip) {
                        Ok(source) => self.mixer()?.play_as(id, source, playback),
                        // Errors end the sound rather than the mix.
                        Err(_) => self.ended.push(id.0),
                    }
                }
                Command::Stop(id) => self.mixer()?.stop(id),
                Command::StopAll => self.mixer()?.stop_all(),
                Command::SetVoice {
                    id,
                    gain,
                    pan,
                    pitch,
                } => self.mixer()?.set_voice(id, gain, pan, pitch),
                Command::SetVolume(category, volume) => self.mixer()?.set_volume(category, volume),
                Command::SetMaster(gain) => self.mixer()?.set_master_gain(gain),
                Command::Watch(watch) => self.watch = watch,
            }
        }
        Ok(commands.len())
    }

    fn mixer(&mut self) -> io::Result<&mut Mixer> {
        self.mixer.as_mut().ok_or_else(malformed)
    }

    /// Mixes the next `frames` frames ([`Mixer::mix`]); silence before the
    /// first message.
    pub fn mix(&mut self, frames: usize) -> Vec<f32> {
        let Some(mixer) = self.mixer.as_mut() else {
            return vec![0.0; frames * 2];
        };
        let before: Vec<VoiceId> = mixer.voices().collect();
        let out = mixer.mix(frames);
        for id in before {
            if !mixer.is_playing(id) {
                self.ended.push(id.0);
            }
        }
        out
    }

    /// The voices that ended since the last call.
    pub fn take_ended(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.ended)
    }

    /// The mixer, once a message made one.
    pub fn mixer_ref(&self) -> Option<&Mixer> {
        self.mixer.as_ref()
    }

    /// What [`Remote::watch`] asked to record.
    pub fn watched(&self) -> Option<Option<VoiceId>> {
        self.watch
    }
}

#[cfg(test)]
mod tests;
