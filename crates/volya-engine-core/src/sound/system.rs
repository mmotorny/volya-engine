// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Positional sound playback (docs/behavior/sound-positional.md): sounds and
//! their emitters, the once-per-frame update (timeline, volume step, fades),
//! the limited voices, and what is sent to the audio device.
//!
//! The device is a trait shaped like the OpenAL 1.1 interface the spec
//! describes (section 7): voices are started from a sound's data at a
//! sample offset, stopped, and given their source properties after the
//! listener every update. [`heard`] applies OpenAL's inverse distance
//! clamped model and a stereo pan by direction, for devices that mix
//! themselves.
//!
//! Section numbers below are the spec's.

use std::rc::Rc;

use super::occlusion::Occlusion;
use super::random::SharedRandom;
use super::{Fatal, Sound, Sounds};
use crate::audio::Category;
use crate::config::PathAliases;
use crate::vfs::Vfs;

pub type Vec3 = [f32; 3];

/// The rolloff factor (7.4).
pub const ROLLOFF: f32 = 0.75;
/// An emitter is audible at an output volume of at least this (5.3).
pub const AUDIBLE: f32 = 0.01;
/// Below this expected level an emitter fades out (5.3).
const FADE_OUT_LEVEL: f32 = 0.01;
/// The fade's rate per second (5.3).
const FADE_RATE: f32 = 10.0;
/// A stopping emitter ends once its fade is below this (4.4).
const FADE_END: f32 = 1e-7;
/// The occlusion's rate per second (5.3).
const OCCLUSION_RATE: f32 = 1.0;
/// The smoothing of the output volume per update (5.3 step 5).
const SMOOTHING: f32 = 0.1;
/// The priority factor of a 2D emitter (3.3).
const PRIORITY_2D: f32 = 100.0;
/// A delay shorter than this starts at once (3.2).
const NO_DELAY: f32 = 0.001;
/// Samples per millisecond when resuming (4.2: the program's 88 bytes).
const SAMPLES_PER_MS: u64 = 44;

/// The number of voices `snd_targets` asks for, kept to its console range
/// 4 … 32 (2): the engine's initial value is 16.
pub const DEFAULT_VOICES: usize = 16;
pub const MIN_VOICES: usize = 4;
pub const MAX_VOICES: usize = 32;

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(v: Vec3) -> f32 {
    dot(v, v).sqrt()
}

/// World coordinates to OpenAL's: z negated (7.2).
fn to_device(v: Vec3) -> Vec3 {
    [v[0], v[1], -v[2]]
}

/// The engine-side distance factor *a* = clamp(*m* / (0.75 · *δ*), 0, 1)
/// (5.3 step 2, 6); 1 at the emitter's position.
pub fn distance_factor(min: f32, distance: f32) -> f32 {
    let a = min / (ROLLOFF * distance);
    if a.is_nan() { 1.0 } else { a.clamp(0.0, 1.0) }
}

/// The device's distance gain *g* of the inverse distance clamped model
/// (7.1, 7.4; OpenAL 1.1 section 3.4.2) for a source at `distance`.
pub fn distance_gain(reference: f32, max: f32, rolloff: f32, distance: f32) -> f32 {
    let d = distance.max(reference).min(max.max(reference));
    let g = reference / (reference + rolloff * (d - reference));
    if g.is_nan() { 1.0 } else { g.clamp(0.0, 1.0) }
}

/// The listener (7.2), in world coordinates: the camera's eye position,
/// view direction and up vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub direction: Vec3,
    pub up: Vec3,
}

impl Default for Listener {
    /// At the origin, looking along +z, up +y.
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            direction: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
        }
    }
}

/// The listener as sent to the device (7.2): OpenAL coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceListener {
    pub position: Vec3,
    pub at: Vec3,
    pub up: Vec3,
}

/// A voice's source properties as sent to the device (7.1): OpenAL
/// coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceSource {
    pub reference_distance: f32,
    pub max_distance: f32,
    pub rolloff: f32,
    pub position: Vec3,
    /// Relative to the listener (a 2D emitter).
    pub relative: bool,
    pub gain: f32,
    pub pitch: f32,
}

/// What a mixing device makes of `source` heard by `listener`: its gain
/// (the source gain times the distance gain) and a stereo pan in -1 … 1
/// (left … right). How the device renders a direction is unestablished
/// (7.2); the pan is the direction's component along the listener's right
/// vector (at × up in OpenAL's right-handed coordinates), an ordinary
/// stereo panning.
pub fn heard(source: &DeviceSource, listener: &DeviceListener) -> (f32, f32) {
    // A relative source is in the listener's frame: x right, y up, -z ahead.
    let (offset, right) = if source.relative {
        (source.position, [1.0, 0.0, 0.0])
    } else {
        (
            sub(source.position, listener.position),
            cross(listener.at, listener.up),
        )
    };
    let distance = length(offset);
    let gain = source.gain
        * distance_gain(
            source.reference_distance,
            source.max_distance,
            source.rolloff,
            distance,
        );
    let right_len = length(right);
    let pan = if distance > 1e-6 && right_len > 1e-6 {
        (dot(offset, right) / (distance * right_len)).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (gain, pan)
}

/// The audio device (7.1): voices by index, each an OpenAL-like source
/// streaming a sound's data. Data of a looped voice repeats without a gap;
/// a non-looped voice's data is followed by silence.
pub trait Device {
    /// Starts `voice` with `sound`'s data from sample `offset` (replacing
    /// what it played).
    fn start(&mut self, voice: usize, sound: &Sound, looped: bool, offset: u64);
    /// Stops `voice`.
    fn stop(&mut self, voice: usize);
    /// Sets the listener; sent every update before the sources.
    fn set_listener(&mut self, listener: &DeviceListener);
    /// Sets `voice`'s source properties.
    fn set_source(&mut self, voice: usize, source: &DeviceSource);
}

/// A device that renders nothing.
pub struct NoDevice;

impl Device for NoDevice {
    fn start(&mut self, _: usize, _: &Sound, _: bool, _: u64) {}
    fn stop(&mut self, _: usize) {}
    fn set_listener(&mut self, _: &DeviceListener) {}
    fn set_source(&mut self, _: usize, _: &DeviceSource) {}
}

/// Identifies a sound (3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(usize);

/// Identifies an emitter, unique for the system's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmitterId(u64);

/// The phases of an emitter (4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Waiting,
    Starting,
    Playing,
    Silent,
    Ended,
}

/// How a sound is played (3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayFlags {
    pub looped: bool,
    pub two_d: bool,
    /// The delay *t*_d in seconds.
    pub delay: f32,
}

/// Settings an independent play may set at once (3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Settings {
    pub position: Option<Vec3>,
    pub frequency: Option<f32>,
    /// Min and max distance.
    pub range: Option<(f32, f32)>,
    pub volume: Option<f32>,
}

struct SoundSlot {
    sound: Rc<Sound>,
    category: Category,
    emitter: Option<EmitterId>,
}

struct Emitter {
    id: EmitterId,
    sound: Rc<Sound>,
    category: Category,
    /// The sound it is attached to.
    owner: Option<SoundId>,
    phase: Phase,
    looped: bool,
    two_d: bool,
    delay: f32,
    position: Vec3,
    /// Play volume *v*.
    volume: f32,
    /// Frequency *f*.
    frequency: f32,
    min: f32,
    max: f32,
    /// Base volume *b*.
    base: f32,
    /// Priority factor *k*.
    priority_factor: f32,
    stopping: bool,
    /// The pause level that paused it; 0 when not paused (4.5).
    paused: u32,
    /// Fade *φ*.
    fade: f32,
    /// Occlusion *o*.
    occlusion: f32,
    /// Output volume *V*.
    out: f32,
    /// Start and end time *T*_s, *T*_e (ms, fractional: pauses move them by
    /// the time steps).
    start: f64,
    end: f64,
    voice: Option<usize>,
    /// Restarted while it had a voice: the voice starts its data again at
    /// the next update (4.3).
    rewind: bool,
    /// The next AI report time (ms; 5.5).
    next_report: f64,
    /// Priority *P* at its last update (6).
    priority: f32,
    updated: bool,
}

impl Emitter {
    fn started(&self) -> bool {
        matches!(self.phase, Phase::Playing | Phase::Silent)
    }
}

#[derive(Clone, Copy)]
struct Voice {
    emitter: Option<EmitterId>,
    /// *π*: −1 when free (6).
    priority: f32,
}

const FREE: Voice = Voice {
    emitter: None,
    priority: -1.0,
};

/// A snapshot of an emitter, for tests and diagnostics.
#[derive(Clone, Debug, PartialEq)]
pub struct EmitterState {
    pub id: EmitterId,
    pub name: String,
    pub phase: Phase,
    pub position: Vec3,
    pub two_d: bool,
    pub looped: bool,
    pub stopping: bool,
    pub paused: bool,
    pub fade: f32,
    pub out: f32,
    pub priority: f32,
    pub voice: Option<usize>,
    /// Play volume *v*, frequency *f*, range, occlusion *o*.
    pub volume: f32,
    pub frequency: f32,
    pub range: (f32, f32),
    pub occlusion: f32,
}

/// Emitters the system has room for when made.
const EMITTERS: usize = 256;

/// The sound system (sections 2 – 7).
pub struct SoundSystem {
    files: Sounds,
    slots: Vec<Option<SoundSlot>>,
    /// In creation order.
    emitters: Vec<Emitter>,
    next_emitter: u64,
    voices: Vec<Voice>,
    /// The sound clock at the last update (ms; 5.1).
    now: u64,
    last: Option<u64>,
    pause_level: u32,
    effects_volume: f32,
    music_volume: f32,
    effects_factor: f32,
    listener: Listener,
    random: u32,
    /// The generator the level's sound rules and the occlusion share
    /// (sound-weather-ambient.md 3.2).
    shared_random: SharedRandom,
    /// What occludes sound (S6, docs/behavior/sound-occlusion.md).
    occlusion: Occlusion,
    /// The voice count a restart asked for, taken at the next update.
    restart_voices: Option<usize>,
}

impl SoundSystem {
    /// A sound system with `voices` voices (`snd_targets`, kept to 4 … 32).
    pub fn new(voices: usize) -> Self {
        Self {
            files: Sounds::new(),
            slots: Vec::new(),
            // Made with the first sound ([`Self::create`]): without a device
            // no sound is created and the system holds no sound memory
            // (console-diagnostics.md 2.1.1).
            emitters: Vec::new(),
            next_emitter: 0,
            voices: vec![FREE; voices.clamp(MIN_VOICES, MAX_VOICES)],
            now: 0,
            last: None,
            pause_level: 0,
            // The engine's initial values (8).
            effects_volume: 1.0,
            music_volume: 0.7,
            effects_factor: 1.0,
            listener: Listener::default(),
            random: 1,
            shared_random: SharedRandom::default(),
            occlusion: Occlusion::default(),
            restart_voices: None,
        }
    }

    /// The shared generator (sound-weather-ambient.md 3.2).
    pub fn random(&mut self) -> &mut SharedRandom {
        &mut self.shared_random
    }

    /// The meshes and scale that occlude sound (S6).
    pub fn occlusion_mut(&mut self) -> &mut Occlusion {
        &mut self.occlusion
    }

    pub fn occlusion(&self) -> &Occlusion {
        &self.occlusion
    }

    /// The duration *D* of sound `id`'s file (ms; section 2).
    pub fn duration_ms(&self, id: SoundId) -> Option<u64> {
        self.slot(id).map(|s| s.sound.duration_ms())
    }

    /// Keeps `sound` as loaded, so creating it needs no file (tests).
    #[cfg(test)]
    pub(crate) fn preload(&mut self, sound: Sound) {
        self.files.loaded.insert(sound.name.clone(), Rc::new(sound));
    }

    /// The loaded sound files (S1), with their warnings.
    pub fn files(&mut self) -> &mut Sounds {
        &mut self.files
    }

    pub fn voice_count(&self) -> usize {
        self.restart_voices.unwrap_or(self.voices.len())
    }

    /// A sound restart (2): every emitter stops, and the next update
    /// starts with `voices` voices (`snd_targets`, kept to 4 … 32).
    pub fn restart(&mut self, voices: usize) {
        self.stop_all();
        self.restart_voices = Some(voices.clamp(MIN_VOICES, MAX_VOICES));
    }

    /// The sound clock at the last update (ms).
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Sets a category's volume setting: `snd_volume_eff` or
    /// `snd_volume_music` (5.4).
    pub fn set_volume(&mut self, category: Category, volume: f32) {
        match category {
            Category::Effects => self.effects_volume = volume,
            Category::Music => self.music_volume = volume,
            Category::Scaled => {}
        }
    }

    /// Sets the effects factor *F*, clamped to 0 … 1 (5.4).
    pub fn set_effects_factor(&mut self, factor: f32) {
        self.effects_factor = factor.clamp(0.0, 1.0);
    }

    /// Sets *F* as the shock effect does (5.4; sound-player-body.md 5),
    /// unclamped.
    pub fn set_shock_factor(&mut self, factor: f32) {
        self.effects_factor = factor;
    }

    pub fn effects_factor(&self) -> f32 {
        self.effects_factor
    }

    /// Sets the listener for the next update (7.2).
    pub fn set_listener(&mut self, listener: Listener) {
        self.listener = listener;
    }

    pub fn listener(&self) -> Listener {
        self.listener
    }

    /// The category volume *c* (5.4).
    fn category_volume(&self, category: Category) -> f32 {
        match category {
            Category::Effects => self.effects_volume * self.effects_factor,
            Category::Music => self.music_volume,
            Category::Scaled => 1.0,
        }
    }

    /// Creates a sound from `name` (resolved and loaded per S1) in
    /// `category` (3.1).
    pub fn create(
        &mut self,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        name: &str,
        category: Category,
    ) -> Result<SoundId, Fatal> {
        let sound = self.files.get(tree, aliases, name)?;
        // Room for a level's looped sounds (the first level's zones play
        // 111), made once, not grown amid a level's heap.
        if self.emitters.capacity() == 0 {
            self.emitters.reserve_exact(EMITTERS);
        }
        let slot = SoundSlot {
            sound,
            category,
            emitter: None,
        };
        let index = match self.slots.iter().position(Option::is_none) {
            Some(i) => {
                self.slots[i] = Some(slot);
                i
            }
            None => {
                self.slots.push(Some(slot));
                self.slots.len() - 1
            }
        };
        Ok(SoundId(index))
    }

    /// Discards a sound: its emitter stops at once (3.1).
    pub fn discard(&mut self, id: SoundId) {
        self.stop(id);
        if let Some(slot) = self.slots.get_mut(id.0) {
            *slot = None;
        }
    }

    /// The sounds that exist (created and not discarded).
    pub fn sound_ids(&self) -> impl Iterator<Item = SoundId> + '_ {
        (0..self.slots.len())
            .filter(|&i| self.slots[i].is_some())
            .map(SoundId)
    }

    /// Runs `create`, which creates sounds; when it fails, every sound it
    /// created is discarded, so a fatal error leaves none behind.
    pub fn all_or_none<T>(
        &mut self,
        create: impl FnOnce(&mut Self) -> Result<T, Fatal>,
    ) -> Result<T, Fatal> {
        let before: Vec<bool> = self.slots.iter().map(Option::is_some).collect();
        let created = create(self);
        if created.is_err() {
            let made: Vec<SoundId> = self
                .sound_ids()
                .filter(|id| !before.get(id.0).copied().unwrap_or(false))
                .collect();
            for id in made {
                self.discard(id);
            }
        }
        created
    }

    fn slot(&self, id: SoundId) -> Option<&SoundSlot> {
        self.slots.get(id.0)?.as_ref()
    }

    fn index(&self, id: EmitterId) -> Option<usize> {
        self.emitters.iter().position(|e| e.id == id)
    }

    /// The live emitter of sound `id`.
    fn attached(&self, id: SoundId) -> Option<usize> {
        let emitter = self.slot(id)?.emitter?;
        self.index(emitter)
            .filter(|&i| self.emitters[i].phase != Phase::Ended)
    }

    fn new_emitter(
        &mut self,
        sound: Rc<Sound>,
        category: Category,
        owner: Option<SoundId>,
        flags: PlayFlags,
    ) -> usize {
        let id = EmitterId(self.next_emitter);
        self.next_emitter += 1;
        let p = sound.params;
        self.emitters.push(Emitter {
            id,
            sound,
            category,
            owner,
            phase: if flags.delay.abs() < NO_DELAY {
                Phase::Starting
            } else {
                Phase::Waiting
            },
            looped: flags.looped,
            two_d: false,
            delay: flags.delay,
            position: [0.0; 3],
            volume: 1.0,
            frequency: 1.0,
            min: p.min_distance,
            max: p.max_distance,
            base: p.base_volume,
            priority_factor: 1.0,
            stopping: false,
            paused: 0,
            fade: 1.0,
            occlusion: 1.0,
            out: 1.0,
            start: 0.0,
            end: 0.0,
            voice: None,
            rewind: false,
            // The time of the play request (5.5).
            next_report: self.now as f64,
            priority: 0.0,
            updated: false,
        });
        self.emitters.len() - 1
    }

    /// **play** (3.2): a new emitter, or the sound's emitter restarted.
    pub fn play(&mut self, id: SoundId, flags: PlayFlags) {
        let i = match self.attached(id) {
            Some(i) => {
                self.restart_emitter(i);
                i
            }
            None => {
                let Some(slot) = self.slot(id) else {
                    return;
                };
                let (sound, category) = (slot.sound.clone(), slot.category);
                let i = self.new_emitter(sound, category, Some(id), flags);
                let emitter = self.emitters[i].id;
                if let Some(slot) = self.slots[id.0].as_mut() {
                    slot.emitter = Some(emitter);
                }
                i
            }
        };
        if flags.two_d {
            switch_2d(&mut self.emitters[i]);
        }
    }

    /// **play at position** (3.2).
    pub fn play_at(&mut self, id: SoundId, position: Vec3, flags: PlayFlags) {
        self.play(id, flags);
        self.set_position(id, position);
    }

    /// **play independently** (3.2): a new emitter attached to no sound.
    pub fn play_independent(&mut self, id: SoundId, flags: PlayFlags, settings: Settings) {
        let Some(slot) = self.slot(id) else {
            return;
        };
        let (sound, category) = (slot.sound.clone(), slot.category);
        let i = self.new_emitter(sound, category, None, flags);
        let e = &mut self.emitters[i];
        if let Some(p) = settings.position {
            e.position = p;
        }
        if let Some(f) = settings.frequency {
            e.frequency = f;
        }
        if let Some((min, max)) = settings.range {
            (e.min, e.max) = (min, max);
        }
        if let Some(v) = settings.volume {
            e.volume = v;
        }
        if flags.two_d {
            switch_2d(e);
        }
    }

    /// Restart (4.3).
    fn restart_emitter(&mut self, i: usize) {
        let now = self.now as f64;
        let e = &mut self.emitters[i];
        e.stopping = false;
        if e.started() {
            e.end += now - e.start;
            e.start = now;
            e.rewind = e.voice.is_some();
        }
    }

    fn with_emitter(&mut self, id: SoundId, f: impl FnOnce(&mut Emitter)) {
        if let Some(i) = self.attached(id) {
            f(&mut self.emitters[i]);
        }
    }

    /// Controls of a sound's emitter (3.3).
    pub fn set_position(&mut self, id: SoundId, position: Vec3) {
        self.with_emitter(id, |e| e.position = position);
    }

    pub fn set_frequency(&mut self, id: SoundId, frequency: f32) {
        self.with_emitter(id, |e| e.frequency = frequency);
    }

    pub fn set_range(&mut self, id: SoundId, min: f32, max: f32) {
        self.with_emitter(id, |e| (e.min, e.max) = (min, max));
    }

    pub fn set_play_volume(&mut self, id: SoundId, volume: f32) {
        self.with_emitter(id, |e| e.volume = volume);
    }

    pub fn set_priority_factor(&mut self, id: SoundId, factor: f32) {
        self.with_emitter(id, |e| e.priority_factor = factor);
    }

    /// Switches to 2D (*k* becomes 100) or to 3D (*k* kept).
    pub fn set_2d(&mut self, id: SoundId, two_d: bool) {
        self.with_emitter(id, |e| {
            if two_d {
                switch_2d(e);
            } else {
                e.two_d = false;
            }
        });
    }

    /// Stop at once (4.4): the emitter ends; its voice is released at the
    /// next update.
    pub fn stop(&mut self, id: SoundId) {
        if let Some(i) = self.attached(id) {
            self.emitters[i].phase = Phase::Ended;
        }
        if let Some(Some(slot)) = self.slots.get_mut(id.0) {
            slot.emitter = None;
        }
    }

    /// Stop with fade (4.4).
    pub fn stop_with_fade(&mut self, id: SoundId) {
        self.with_emitter(id, |e| e.stopping = true);
    }

    /// Stop all (4.4): every emitter stops at once.
    pub fn stop_all(&mut self) {
        for e in &mut self.emitters {
            e.phase = Phase::Ended;
        }
        for slot in self.slots.iter_mut().flatten() {
            slot.emitter = None;
        }
    }

    /// Whether sound `id` has an emitter.
    pub fn has_emitter(&self, id: SoundId) -> bool {
        self.attached(id).is_some()
    }

    /// The play time (3.3) in ms: now − *T*_s while the sound's emitter is
    /// playing or silent, else 0.
    pub fn play_time(&self, id: SoundId) -> f64 {
        // TODO(docs/behavior/sound-positional.md section 3.3): what the play
        // time is once the emitter has ended is disputed, so unestablished;
        // we give 0 (the simplest: an ended emitter is gone at the end of
        // its update).
        self.attached(id)
            .map(|i| &self.emitters[i])
            .filter(|e| e.started())
            .map_or(0.0, |e| self.now as f64 - e.start)
    }

    /// Pauses every emitter not yet paused (4.5).
    pub fn pause(&mut self) {
        self.pause_level += 1;
        let level = self.pause_level;
        for e in &mut self.emitters {
            if e.paused == 0 {
                e.paused = level;
            }
        }
    }

    /// Resumes the emitters the last [`SoundSystem::pause`] paused (4.5).
    pub fn resume(&mut self) {
        if self.pause_level == 0 {
            return;
        }
        let level = self.pause_level;
        for e in &mut self.emitters {
            if e.paused == level {
                e.paused = 0;
            }
        }
        self.pause_level -= 1;
    }

    pub fn pause_level(&self) -> u32 {
        self.pause_level
    }

    /// The emitters in creation order.
    pub fn emitters(&self) -> Vec<EmitterState> {
        self.emitters
            .iter()
            .map(|e| EmitterState {
                id: e.id,
                name: e.sound.name.clone(),
                phase: e.phase,
                position: e.position,
                two_d: e.two_d,
                looped: e.looped,
                stopping: e.stopping,
                paused: e.paused != 0,
                fade: e.fade,
                out: e.out,
                priority: e.priority,
                voice: e.voice,
                volume: e.volume,
                frequency: e.frequency,
                range: (e.min, e.max),
                occlusion: e.occlusion,
            })
            .collect()
    }

    /// The update (5.1) at sound clock `now` (ms).
    pub fn update(&mut self, now: u64, device: &mut dyn Device) {
        let step_ms = self.last.map_or(0, |last| now.saturating_sub(last));
        self.last = Some(now);
        self.now = now;
        let dt = step_ms as f32 * 0.001;
        // Emitters stopped since the last update release their voices.
        for i in 0..self.emitters.len() {
            if self.emitters[i].phase == Phase::Ended {
                self.release(i, device);
            }
        }
        if let Some(voices) = self.restart_voices.take() {
            self.voices = vec![FREE; voices];
        }
        for e in &mut self.emitters {
            e.updated = e.phase == Phase::Ended;
        }
        for v in 0..self.voices.len() {
            if let Some(i) = self.voices[v].emitter.and_then(|id| self.index(id))
                && !self.emitters[i].updated
            {
                self.update_emitter(i, dt, step_ms as f64, device);
            }
            self.voices[v].priority = match self.voices[v].emitter.and_then(|id| self.index(id)) {
                Some(i) => self.emitters[i].priority,
                None => -1.0,
            };
        }
        for i in 0..self.emitters.len() {
            if !self.emitters[i].updated {
                self.update_emitter(i, dt, step_ms as f64, device);
            }
        }
        for i in 0..self.emitters.len() {
            if self.emitters[i].phase == Phase::Ended {
                self.release(i, device);
            }
        }
        let slots = &mut self.slots;
        self.emitters.retain(|e| {
            let keep = e.phase != Phase::Ended;
            if !keep
                && let Some(slot) = e
                    .owner
                    .and_then(|o| slots.get_mut(o.0))
                    .and_then(Option::as_mut)
                && slot.emitter == Some(e.id)
            {
                slot.emitter = None;
            }
            keep
        });
        let l = self.listener;
        device.set_listener(&DeviceListener {
            position: to_device(l.position),
            at: to_device(l.direction),
            up: to_device(l.up),
        });
        for v in 0..self.voices.len() {
            if let Some(i) = self.voices[v].emitter.and_then(|id| self.index(id)) {
                let e = &self.emitters[i];
                device.set_source(
                    v,
                    &DeviceSource {
                        reference_distance: e.min,
                        max_distance: e.max,
                        rolloff: ROLLOFF,
                        position: to_device(e.position),
                        relative: e.two_d,
                        gain: e.out.clamp(1e-7, 1.0),
                        pitch: e.frequency.clamp(0.001, 2.0),
                    },
                );
            }
        }
    }

    /// The per-emitter update (5.2).
    fn update_emitter(&mut self, i: usize, dt: f32, step_ms: f64, device: &mut dyn Device) {
        self.emitters[i].updated = true;
        let now = self.now as f64;
        let paused = self.emitters[i].paused != 0;
        match self.emitters[i].phase {
            Phase::Waiting => {
                if !paused {
                    let e = &mut self.emitters[i];
                    e.delay -= dt;
                    if e.delay <= 0.0 {
                        e.phase = Phase::Starting;
                    }
                }
            }
            Phase::Starting => {
                if !paused {
                    let c = self.category_volume(self.emitters[i].category);
                    let e = &mut self.emitters[i];
                    e.start = now;
                    if !e.looped {
                        e.end = now + e.sound.duration_ms() as f64;
                    }
                    e.fade = 1.0;
                    // S6 4.1: the target at the start of every emitter, from
                    // p; a 2D emitter's has no effect.
                    let (listener, position) = (self.listener.position, e.position);
                    let target = self
                        .occlusion
                        .target(listener, position, &mut self.shared_random);
                    if !self.emitters[i].two_d {
                        self.emitters[i].occlusion = target;
                    }
                    let e = &mut self.emitters[i];
                    let o = if e.two_d { 1.0 } else { e.occlusion };
                    e.out = e.base * e.volume * c * o;
                    e.phase = Phase::Silent;
                    let audible = self.volume_step(i, dt);
                    self.decide(i, audible, device);
                }
            }
            Phase::Playing | Phase::Silent => {
                if paused {
                    if self.emitters[i].voice.is_some() {
                        self.release(i, device);
                    }
                    let e = &mut self.emitters[i];
                    e.phase = Phase::Silent;
                    e.start += step_ms;
                    e.end += step_ms;
                } else if !self.emitters[i].looped && now >= self.emitters[i].end {
                    self.end(i, device);
                } else {
                    let audible = self.volume_step(i, dt);
                    self.decide(i, audible, device);
                }
            }
            Phase::Ended => {}
        }
        let e = &self.emitters[i];
        if e.phase != Phase::Ended && e.stopping && e.fade < FADE_END {
            self.end(i, device);
        }
        if self.emitters[i].phase != Phase::Ended && now >= self.emitters[i].next_report {
            let r = self.next_random() % 60;
            let e = &mut self.emitters[i];
            e.next_report += f64::from(470 + r);
            // TODO(S24, docs/behavior/sound-positional.md 5.5): a report is
            // made only for a sound the game has associated with an object;
            // nothing associates sounds with objects yet, so none is made.
        }
    }

    /// A pseudo-random number for the report times (5.5). The generator is
    /// unestablished; ours is the C standard's example `rand`.
    fn next_random(&mut self) -> u32 {
        self.random = self.random.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.random / 65_536) % 32_768
    }

    /// The volume step (5.3); returns whether the emitter is audible. Also
    /// computes its priority (6).
    fn volume_step(&mut self, i: usize, dt: f32) -> bool {
        let c = self.category_volume(self.emitters[i].category);
        let listener = self.listener.position;
        let e = &self.emitters[i];
        // S6 4.1: computed at every volume step of a 3D emitter within its
        // max distance.
        let target = if !e.two_d && length(sub(listener, e.position)) <= e.max {
            self.occlusion
                .target(listener, e.position, &mut self.shared_random)
        } else {
            1.0
        };
        let e = &mut self.emitters[i];
        let delta = length(sub(listener, e.position));
        let fade_step = FADE_RATE * dt;
        if e.two_d {
            e.occlusion = 1.0;
            e.fade = if e.stopping {
                e.fade - fade_step
            } else {
                e.fade + fade_step
            }
            .clamp(0.0, 1.0);
        } else {
            if delta > e.max {
                e.out = 0.0;
                e.priority = 0.0;
                return false;
            }
            let a = distance_factor(e.min, delta);
            let down = e.stopping || e.base * e.volume * c * a < FADE_OUT_LEVEL;
            e.fade = if down {
                e.fade - fade_step
            } else {
                e.fade + fade_step
            }
            .clamp(0.0, 1.0);
            let gap = target - e.occlusion;
            if gap.abs() >= 1e-7 {
                e.occlusion += gap.clamp(-OCCLUSION_RATE * dt, OCCLUSION_RATE * dt);
            }
            e.occlusion = e.occlusion.clamp(0.0, 1.0);
        }
        let target = e.base * e.volume * c * e.fade * e.occlusion;
        e.out = SMOOTHING * target + (1.0 - SMOOTHING) * e.out;
        e.priority = e.out * e.priority_factor * distance_factor(e.min, delta);
        e.out >= AUDIBLE
    }

    /// Playing or silent after the volume step (5.3, 6).
    fn decide(&mut self, i: usize, audible: bool, device: &mut dyn Device) {
        if !audible {
            self.release(i, device);
            self.emitters[i].phase = Phase::Silent;
            self.emitters[i].rewind = false;
            return;
        }
        if let Some(v) = self.emitters[i].voice {
            if std::mem::take(&mut self.emitters[i].rewind) {
                let e = &self.emitters[i];
                device.start(v, &e.sound, e.looped, 0);
            }
            self.emitters[i].phase = Phase::Playing;
            return;
        }
        self.emitters[i].rewind = false;
        let priority = self.emitters[i].priority;
        // The lowest π, the first among equal ones.
        let lowest = self.voices.iter().enumerate().fold(
            None::<(usize, f32)>,
            |best, (v, voice)| match best {
                Some((_, p)) if p <= voice.priority => best,
                _ => Some((v, voice.priority)),
            },
        );
        let Some((v, _)) = lowest.filter(|&(_, p)| p < priority) else {
            self.emitters[i].phase = Phase::Silent;
            return;
        };
        if let Some(other) = self.voices[v].emitter.and_then(|id| self.index(id)) {
            self.emitters[other].voice = None;
            if self.emitters[other].phase == Phase::Playing {
                self.emitters[other].phase = Phase::Silent;
            }
        }
        let now = self.now as f64;
        let e = &mut self.emitters[i];
        let duration = e.sound.duration_ms();
        let offset = if duration == 0 {
            0
        } else {
            SAMPLES_PER_MS * (((now - e.start).max(0.0) as u64) % duration)
        };
        device.start(v, &e.sound, e.looped, offset);
        e.voice = Some(v);
        e.phase = Phase::Playing;
        self.voices[v] = Voice {
            emitter: Some(e.id),
            priority,
        };
    }

    /// Releases the emitter's voice, if it has one.
    fn release(&mut self, i: usize, device: &mut dyn Device) {
        if let Some(v) = self.emitters[i].voice.take() {
            device.stop(v);
            self.voices[v] = FREE;
        }
    }

    /// The emitter ends (4.2, 4.4).
    fn end(&mut self, i: usize, device: &mut dyn Device) {
        self.release(i, device);
        self.emitters[i].phase = Phase::Ended;
    }
}

fn switch_2d(e: &mut Emitter) {
    e.two_d = true;
    e.priority_factor = PRIORITY_2D;
}

#[cfg(test)]
mod tests;
