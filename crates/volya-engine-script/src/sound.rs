// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Sound as scripts reach it (script API T,
//! docs/behavior/script-api-sound.md; section numbers are its): the class
//! `sound_object` (2), the constant holder `snd_type` (3), the records
//! `sound_params` (4) and `SoundInfo` (7.4), the `level` functions (5) and
//! the creature members of `game_object` ([`creatures`], 6 and 7).
//!
//! Scripts run while the engine's sound system (docs/behavior/
//! sound-positional.md, S2) is out of their reach, so [`ScriptSounds`]
//! keeps the script side's view of each sound and its emitter (what the
//! members read at once) and queues what they ask of the sound system
//! ([`Request`]); the engine carries the requests out each frame and the
//! emitters that ended are forgotten ([`ScriptSounds::frame`]). See
//! docs/architecture/scripts.md, Sound.

pub mod creatures;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use volya_engine_core::audio::Category;
use volya_engine_core::config::{LEVEL, PathAliases};
use volya_engine_core::sound::system::{PlayFlags, Settings, SoundId, SoundSystem};
use volya_engine_core::sound::{Fatal as SoundFatal, GAME_SOUNDS, Sounds};
use volya_engine_core::vfs::Vfs;
use volya_engine_formats::cp1251;

use crate::classes::{EngineClass, ObjectMaker, Objects, Operand};
use crate::dialogs::DialogsAndTrade;
use crate::files::FileSystem;
use crate::game_object::GameObjects;
use crate::game_object::members::Handle;
use crate::loader::{Scripts, Trees};
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::{Vector, lua_number, to_f32, to_flags32};
use crate::{Error, ErrorKind, Value};

/// `sound_object`'s constants (2.2): the bits of a play's flags.
pub const S3D: i32 = 0;
pub const S2D: i32 = 2;
pub const LOOPED: i32 = 1;

/// `snd_type`'s 46 constants (3): the AI sound types of
/// docs/formats/sound-ogg-params.md 7.2, as signed 32-bit integers.
pub const SND_TYPES: [(&str, u32); 46] = [
    ("no_sound", 0),
    ("ambient", 0x0000_0080),
    ("object_explode", 0x0000_0100),
    ("object_collide", 0x0000_0200),
    ("object_break", 0x0000_0400),
    ("idle", 0x0000_0800),
    ("eat", 0x0000_1000),
    ("attack", 0x0000_2000),
    ("talk", 0x0000_4000),
    ("step", 0x0000_8000),
    ("injure", 0x0001_0000),
    ("die", 0x0002_0000),
    ("reload", 0x0004_0000),
    ("bullet_hit", 0x0008_0000),
    ("empty", 0x0010_0000),
    ("shoot", 0x0020_0000),
    ("use", 0x0040_0000),
    ("take", 0x0080_0000),
    ("hide", 0x0100_0000),
    ("drop", 0x0200_0000),
    ("pick_up", 0x0400_0000),
    ("world", 0x0800_0000),
    ("anomaly", 0x1000_0000),
    ("monster", 0x2000_0000),
    ("item", 0x4000_0000),
    ("weapon", 0x8000_0000),
    ("world_ambient", 0x0800_0080),
    ("world_object_explode", 0x0800_0100),
    ("world_object_collide", 0x0800_0200),
    ("world_object_break", 0x0800_0400),
    ("anomaly_idle", 0x1000_0800),
    ("monster_eat", 0x2000_1000),
    ("monster_attack", 0x2000_2000),
    ("monster_talk", 0x2000_4000),
    ("monster_step", 0x2000_8000),
    ("monster_injure", 0x2001_0000),
    ("monster_die", 0x2002_0000),
    ("item_use", 0x4040_0000),
    ("item_take", 0x4080_0000),
    ("item_hide", 0x4100_0000),
    ("item_drop", 0x4200_0000),
    ("item_pick_up", 0x4400_0000),
    ("weapon_reload", 0x8004_0000),
    ("weapon_bullet_hit", 0x8008_0000),
    ("weapon_empty", 0x8010_0000),
    ("weapon_shoot", 0x8020_0000),
];

/// The AI sound type "the file's own" (S1 7.2).
pub const FILE_TYPE: i32 = -1;

/// Identifies a sound the scripts made, for the life of [`ScriptSounds`].
pub type SoundKey = u32;

/// What the script side needs of a sound's file (S1 2, 3; S2 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundFile {
    /// The duration *D*, ms.
    pub duration_ms: u64,
    pub min_distance: f32,
    pub max_distance: f32,
}

/// The game's sound files as the members find and load them.
pub trait SoundFiles {
    /// Whether `<name>.ogg` exists among the game's sound files
    /// (`$game_sounds$`), `name` taken as given (2.1).
    fn exists(&self, name: &[u8]) -> bool;
    /// The sound `name` loaded as S1 2 resolves it, the level's directory
    /// `level` searched first; the error is S1 5's fatal one.
    fn load(&mut self, name: &[u8], level: Option<&str>) -> Result<SoundFile, String>;
}

/// [`SoundFiles`] over the game's files: existence by the file index,
/// loading by the sound files' own loader (headers only, S1 2).
struct GameSoundFiles {
    trees: Rc<dyn Trees>,
    files: Rc<RefCell<FileSystem>>,
    loaded: Sounds,
}

impl SoundFiles for GameSoundFiles {
    fn exists(&self, name: &[u8]) -> bool {
        let files = self.files.borrow();
        let file = format!("{}.ogg", cp1251::decode(name));
        (files.aliases.path(GAME_SOUNDS, &file))
            .is_ok_and(|path| files.index.entry(&path).is_some())
    }

    fn load(&mut self, name: &[u8], level: Option<&str>) -> Result<SoundFile, String> {
        let mut aliases = self.files.borrow().aliases.clone();
        if let Some(level) = level {
            aliases.set_dir(LEVEL, level);
        }
        let name = cp1251::decode(name);
        let mut loaded = Err(SoundFatal(format!(
            "sound {name}: the game's files are unavailable"
        )));
        let lent = self.trees.with_trees(&mut |tree, _| {
            loaded = self.loaded.get(tree, &aliases, &name);
        });
        lent.map_err(|e| format!("sound {name}: {e}"))?;
        let sound = loaded.map_err(|e| e.0)?;
        Ok(SoundFile {
            duration_ms: sound.duration_ms(),
            min_distance: sound.params.min_distance,
            max_distance: sound.params.max_distance,
        })
    }
}

/// What the members ask of the sound system, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// A sound created from `name` in the effects category (2.1, 6.3).
    Create {
        key: SoundKey,
        name: String,
    },
    /// S2's play, or play at position `position` (2.3), with `owner` the
    /// sound's owner (S1 7.1).
    Play {
        key: SoundKey,
        flags: PlayFlags,
        position: Option<[f32; 3]>,
        owner: u16,
    },
    /// S2's play independently (2.4).
    PlayIndependent {
        key: SoundKey,
        flags: PlayFlags,
        position: [f32; 3],
        volume: f32,
        owner: u16,
    },
    /// Stop at once, stop with fade (2.5; S2 4.4).
    Stop(SoundKey),
    StopWithFade(SoundKey),
    /// The emitter's controls (2.7, 2.8; S2 3.3).
    SetPosition(SoundKey, [f32; 3]),
    SetVolume(SoundKey, f32),
    SetFrequency(SoundKey, f32),
    SetRange(SoundKey, f32, f32),
    /// The sound discarded (2.9; S2 3.1).
    Discard(SoundKey),
    /// The effects factor *F* set, kept to 0 … 1 (5.1).
    SetEffectsFactor(f32),
    /// A sound of `name` created and kept by the level (5.2).
    Prefetch(String),
}

/// The script side's view of a sound's emitter (S2 3.3): what the members
/// read back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emitter {
    pub position: [f32; 3],
    pub volume: f32,
    pub frequency: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub looped: bool,
    pub two_d: bool,
}

/// A sound the scripts made.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptSound {
    pub name: Vec<u8>,
    /// Its AI sound type (S1 7.2): −1 for the file's own.
    pub ai_type: i32,
    pub file: SoundFile,
    /// Its emitter while it has one (2.6).
    pub emitter: Option<Emitter>,
}

/// The flags of a play (2.2): the bits `looped` and `s2d` of `flags`, and
/// `delay` seconds.
pub fn play_flags(flags: u32, delay: f32) -> PlayFlags {
    PlayFlags {
        looped: flags & LOOPED as u32 != 0,
        two_d: flags & S2D as u32 != 0,
        delay,
    }
}

/// The sounds the scripts made, their requests to the sound system and the
/// creatures' sound state.
pub struct ScriptSounds {
    /// How the members find and load sound files; the game's files unless
    /// set before registering (tests).
    pub files: Option<Box<dyn SoundFiles>>,
    /// The loaded level's directory, searched first for a sound's file
    /// (S1 2); the engine sets it.
    pub level_dir: Option<String>,
    sounds: HashMap<SoundKey, ScriptSound>,
    next: SoundKey,
    requests: Vec<Request>,
    /// Sound objects the runtime collected, whose sounds go (2.9).
    collected: Rc<RefCell<Vec<SoundKey>>>,
    /// The effects factor *F* as last known (5.1).
    effects_factor: f32,
    /// The names the running level prefetched (5.2).
    prefetched: HashSet<String>,
    /// The creatures' sound players, hearing and voice prefixes (6, 7).
    pub creatures: creatures::Creatures,
    /// The engine's sounds of the scripts' keys, and of the prefetched
    /// names.
    system_ids: HashMap<SoundKey, SoundId>,
    prefetched_ids: Vec<SoundId>,
}

impl Default for ScriptSounds {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptSounds {
    pub fn new() -> Self {
        ScriptSounds {
            files: None,
            level_dir: None,
            sounds: HashMap::new(),
            next: 0,
            requests: Vec::new(),
            collected: Rc::default(),
            effects_factor: 1.0,
            prefetched: HashSet::new(),
            creatures: creatures::Creatures::default(),
            system_ids: HashMap::new(),
            prefetched_ids: Vec::new(),
        }
    }

    /// The sounds that exist, by key.
    pub fn sounds(&self) -> impl Iterator<Item = (SoundKey, &ScriptSound)> + '_ {
        self.sounds.iter().map(|(&k, s)| (k, s))
    }

    /// The sound of `key`, while it exists.
    pub fn sound(&self, key: SoundKey) -> Option<&ScriptSound> {
        self.sounds.get(&key)
    }

    /// The requests since the last call, oldest first (tests; the engine
    /// takes them with [`ScriptSounds::frame`]).
    pub fn take_requests(&mut self) -> Vec<Request> {
        self.collect();
        std::mem::take(&mut self.requests)
    }

    /// The effects factor *F* as the scripts read it (5.1).
    pub fn effects_factor(&self) -> f32 {
        self.effects_factor
    }

    fn files(&mut self) -> Result<&mut Box<dyn SoundFiles>, String> {
        self.files
            .as_mut()
            .ok_or_else(|| "the sound files aren't set".to_owned())
    }

    /// Whether `<name>.ogg` exists among the game's sound files.
    pub fn exists(&mut self, name: &[u8]) -> Result<bool, String> {
        Ok(self.files()?.exists(name))
    }

    /// Loads `name`'s file (S1 2): the error is fatal (S1 5).
    pub fn load(&mut self, name: &[u8]) -> Result<SoundFile, String> {
        let level = self.level_dir.clone();
        self.files()?.load(name, level.as_deref())
    }

    /// A sound made from `name` with AI sound type `ai_type`, in the
    /// effects category (2.1, 6.3), its file loaded; the error is fatal.
    pub fn create(&mut self, name: &[u8], ai_type: i32) -> Result<SoundKey, String> {
        let file = self.load(name)?;
        self.next += 1;
        let key = self.next;
        self.sounds.insert(
            key,
            ScriptSound {
                name: name.to_vec(),
                ai_type,
                file,
                emitter: None,
            },
        );
        self.requests.push(Request::Create {
            key,
            name: cp1251::decode(name),
        });
        Ok(key)
    }

    /// The sound of `key` discarded, its emitter stopped at once (S2 3.1).
    pub fn discard(&mut self, key: SoundKey) {
        if self.sounds.remove(&key).is_some() {
            self.requests.push(Request::Discard(key));
        }
    }

    /// The sounds of collected sound objects discarded (2.9).
    fn collect(&mut self) {
        let collected = std::mem::take(&mut *self.collected.borrow_mut());
        for key in collected {
            self.discard(key);
        }
    }

    /// S2's play or play at position (2.3; S2 3.2): a new emitter, or the
    /// sound's emitter restarted keeping its looped flag, delay and
    /// position; a position then set; the 2D flag switching it to 2D.
    pub fn play(
        &mut self,
        key: SoundKey,
        flags: PlayFlags,
        position: Option<[f32; 3]>,
        owner: u16,
    ) {
        let Some(sound) = self.sounds.get_mut(&key) else {
            return;
        };
        let file = sound.file;
        let emitter = sound.emitter.get_or_insert(Emitter {
            position: [0.0; 3],
            volume: 1.0,
            frequency: 1.0,
            min_distance: file.min_distance,
            max_distance: file.max_distance,
            looped: flags.looped,
            two_d: false,
        });
        if let Some(p) = position {
            emitter.position = p;
        }
        if flags.two_d {
            emitter.two_d = true;
        }
        self.requests.push(Request::Play {
            key,
            flags,
            position,
            owner,
        });
    }

    /// The emitter of `key`, if the sound has one.
    pub fn emitter(&self, key: SoundKey) -> Option<Emitter> {
        self.sounds.get(&key).and_then(|s| s.emitter)
    }

    fn emitter_mut(&mut self, key: SoundKey) -> Option<&mut Emitter> {
        self.sounds.get_mut(&key).and_then(|s| s.emitter.as_mut())
    }

    fn push(&mut self, request: Request) {
        self.requests.push(request);
    }

    /// The emitter stopped at once (2.5).
    pub fn stop(&mut self, key: SoundKey) {
        if let Some(sound) = self.sounds.get_mut(&key)
            && sound.emitter.take().is_some()
        {
            self.requests.push(Request::Stop(key));
        }
    }

    /// The engine's part of a frame: the requests carried out on `system`
    /// (sounds created from `tree` with `aliases`, `$level$` at the level's
    /// directory), then each sound whose emitter ended forgotten as without
    /// one, and *F* read back (the shock effect writes it too, S2 5.4).
    /// A file that cannot be loaded is fatal (S1 5).
    pub fn frame(
        &mut self,
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
    ) -> Result<(), SoundFatal> {
        self.collect();
        let mut aliases = aliases.clone();
        if let Some(dir) = &self.level_dir {
            aliases.set_dir(LEVEL, dir);
        }
        for request in std::mem::take(&mut self.requests) {
            self.apply(system, tree, &aliases, request)?;
        }
        self.sync(system);
        Ok(())
    }

    fn apply(
        &mut self,
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        request: Request,
    ) -> Result<(), SoundFatal> {
        let id = |key: &SoundKey| self.system_ids.get(key).copied();
        match request {
            Request::Create { key, name } => {
                let made = system.create(tree, aliases, &name, Category::Effects)?;
                self.system_ids.insert(key, made);
            }
            Request::Play {
                key,
                flags,
                position,
                ..
            } => {
                // TODO(S24, docs/behavior/sound-positional.md 5.5): the
                // owner and the AI sound type are for the reports to the
                // AI, which nothing hears yet.
                if let Some(id) = id(&key) {
                    match position {
                        Some(p) => system.play_at(id, p, flags),
                        None => system.play(id, flags),
                    }
                }
            }
            Request::PlayIndependent {
                key,
                flags,
                position,
                volume,
                ..
            } => {
                if let Some(id) = id(&key) {
                    let settings = Settings {
                        position: Some(position),
                        volume: Some(volume),
                        ..Settings::default()
                    };
                    system.play_independent(id, flags, settings);
                }
            }
            Request::Stop(key) => id(&key).into_iter().for_each(|id| system.stop(id)),
            Request::StopWithFade(key) => {
                id(&key)
                    .into_iter()
                    .for_each(|id| system.stop_with_fade(id));
            }
            Request::SetPosition(key, p) => {
                id(&key)
                    .into_iter()
                    .for_each(|id| system.set_position(id, p));
            }
            Request::SetVolume(key, v) => {
                id(&key)
                    .into_iter()
                    .for_each(|id| system.set_play_volume(id, v));
            }
            Request::SetFrequency(key, f) => {
                id(&key)
                    .into_iter()
                    .for_each(|id| system.set_frequency(id, f));
            }
            Request::SetRange(key, min, max) => {
                id(&key)
                    .into_iter()
                    .for_each(|id| system.set_range(id, min, max));
            }
            Request::Discard(key) => {
                if let Some(id) = self.system_ids.remove(&key) {
                    system.discard(id);
                }
            }
            Request::SetEffectsFactor(f) => system.set_effects_factor(f),
            Request::Prefetch(name) => {
                let made = system.create(tree, aliases, &name, Category::Effects)?;
                self.prefetched_ids.push(made);
            }
        }
        Ok(())
    }

    /// The sounds whose emitter ended are without one; *F* read back.
    fn sync(&mut self, system: &SoundSystem) {
        for (key, sound) in &mut self.sounds {
            let live = (self.system_ids.get(key)).is_some_and(|&id| system.has_emitter(id));
            if !live {
                sound.emitter = None;
            }
        }
        self.effects_factor = system.effects_factor();
    }

    /// A frame without a sound system (no audio output): the requests are
    /// dropped and no sound has an emitter, as with the sound system
    /// disabled (S2 2).
    pub fn frame_without_system(&mut self) {
        self.collect();
        self.requests.clear();
        for sound in self.sounds.values_mut() {
            sound.emitter = None;
        }
    }

    /// The level was left (its emitters stopped): its prefetched sounds go
    /// (5.2, believed kept until the level ends), and the emitters that
    /// ended are forgotten.
    pub fn level_left(&mut self, system: &mut SoundSystem) {
        for id in self.prefetched_ids.drain(..) {
            system.discard(id);
        }
        self.prefetched.clear();
        self.sync(system);
    }

    /// The game ends: every sound of the scripts leaves `system`, which
    /// outlives them.
    pub fn discard_all(&mut self, system: &mut SoundSystem) {
        for (_, id) in self.system_ids.drain() {
            system.discard(id);
        }
        for id in self.prefetched_ids.drain(..) {
            system.discard(id);
        }
        self.requests.clear();
        for sound in self.sounds.values_mut() {
            sound.emitter = None;
        }
    }

    /// `level.prefetch_sound(name)` (5.2): the name lowered and cut at its
    /// last `.`; a name the running level has not prefetched yet is loaded
    /// (fatal when its file is missing, S1 5) and kept.
    pub fn prefetch(&mut self, name: &[u8]) -> Result<(), String> {
        let lower = cp1251::decode(name).to_lowercase();
        let cut = lower.rfind('.').map_or(lower.as_str(), |dot| &lower[..dot]);
        if self.prefetched.contains(cut) {
            return Ok(());
        }
        let cut = cut.to_owned();
        self.load(&cp1251::encode(&cut).unwrap_or_else(|| cut.as_bytes().to_vec()))?;
        self.prefetched.insert(cut.clone());
        self.requests.push(Request::Prefetch(cut));
        Ok(())
    }
}

/// A sound object's part (2): its sound, if it has one. Collecting the
/// object discards the sound (2.9).
pub struct SoundObject {
    key: Option<SoundKey>,
    collected: Rc<RefCell<Vec<SoundKey>>>,
}

impl Drop for SoundObject {
    fn drop(&mut self) {
        if let Some(key) = self.key {
            self.collected.borrow_mut().push(key);
        }
    }
}

/// A `sound_params` record (4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoundParams {
    pub position: Vector,
    pub volume: f32,
    pub frequency: f32,
    pub min_distance: f32,
    pub max_distance: f32,
}

/// A `SoundInfo` record (7.4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoundInfo {
    pub who: Option<u16>,
    pub position: Vector,
    pub power: f32,
    pub time: f64,
    pub danger: f64,
}

/// What the members share.
#[derive(Clone)]
pub(crate) struct Context {
    pub state: Rc<RefCell<ScriptSounds>>,
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub dialogs: Rc<RefCell<DialogsAndTrade>>,
    pub fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    pub fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error: S1 5's, or one standing for a read through no
    /// object (our wording). The error to raise.
    pub fn fatal(&self, function: &str, line: u32, text: &str) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description: "<no expression>",
                arguments: &[text],
            },
        )
    }

    /// The device's `time_global()`, ms (B 3.3).
    pub fn now(&self) -> u32 {
        self.services.borrow().device.time_global()
    }

    /// Runs `f` on the sounds; its error is fatal.
    fn with_sounds<T>(
        &self,
        function: &str,
        line: u32,
        f: impl FnOnce(&mut ScriptSounds) -> Result<T, String>,
    ) -> Result<T, String> {
        let done = f(&mut self.state.borrow_mut());
        done.map_err(|e| self.fatal(function, line, &e))
    }
}

type Results = Result<Vec<Value>, String>;

pub(crate) fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A number argument (our wording for a mismatch; depends on A).
pub(crate) fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// A number argument made an integer (conventions): its low 32 bits.
pub(crate) fn integer(args: &[Value], i: usize, what: &str) -> Result<u32, String> {
    number(args, i, what).map(to_flags32)
}

pub(crate) fn text(args: &[Value], i: usize, what: &str) -> Result<Vec<u8>, String> {
    match arg(args, i) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{what}: text expected")),
    }
}

fn handle_arg(objects: &Objects, args: &[Value], i: usize, what: &str) -> Result<u16, String> {
    match objects.get::<Handle>(arg(args, i)) {
        Some(Operand::Other(h)) => Ok(h.id),
        _ => Err(format!("{what}: game object expected")),
    }
}

fn vector_arg(objects: &Objects, args: &[Value], i: usize, what: &str) -> Result<[f32; 3], String> {
    match objects.get::<Vector>(arg(args, i)) {
        Some(Operand::Other(v)) => Ok([v.x, v.y, v.z]),
        _ => Err(format!("{what}: vector expected")),
    }
}

fn new_vector(objects: &Objects, p: [f32; 3]) -> Value {
    objects.new_object("vector", Vector::new(p[0], p[1], p[2]))
}

/// The items of a list text split at `,`, each with its surrounding
/// spaces removed (5.3, believed).
pub(crate) fn items(list: &[u8]) -> Vec<Vec<u8>> {
    list.split(|&b| b == b',')
        .map(|item| item.trim_ascii().to_vec())
        .collect()
}

/// `<item><i>`, `i` in decimal (5.3, 6.2).
pub(crate) fn numbered(item: &[u8], i: u32) -> Vec<u8> {
    let mut name = item.to_vec();
    name.extend_from_slice(i.to_string().as_bytes());
    name
}

/// The class `sound_object` (2).
fn sound_object(ctx: &Context) -> EngineClass {
    let construct = |ctx: &Context| {
        let c = ctx.clone();
        move |_, args: &[Value]| -> Result<SoundObject, String> {
            c.stopped()?;
            let name = text(args, 0, "sound_object")?;
            // 2.1: the type made an integer (truncated); one argument gives
            // type 0, so no report to the AI.
            let ai_type = match args.get(1) {
                Some(_) => integer(args, 1, "sound_object")? as i32,
                None => 0,
            };
            let key = c.with_sounds("sound_object", line!(), |s| {
                if s.exists(&name)? {
                    s.create(&name, ai_type).map(Some)
                } else {
                    Ok(None)
                }
            })?;
            let collected = Rc::clone(&c.state.borrow().collected);
            Ok(SoundObject { key, collected })
        }
    };
    let text_score = |args: &[Value]| matches!(args.first(), Some(Value::String(_))).then_some(0);
    let mut class = EngineClass::new("sound_object")
        .constructor(1, text_score, construct(ctx))
        .constructor(
            2,
            move |args: &[Value]| matches!(args, [Value::String(_), Value::Number(_)]).then_some(0),
            construct(ctx),
        )
        .constructible()
        .constant("s3d", S3D)
        .constant("s2d", S2D)
        .constant("looped", LOOPED);

    // 2.3.
    let c = ctx.clone();
    class = class.method_with_objects("play", move |o: &mut SoundObject, objects, args| {
        c.stopped()?;
        let what = "sound_object:play";
        let owner = handle_arg(objects, args, 0, what)?;
        let delay = match args.get(1) {
            Some(_) => to_f32(number(args, 1, what)?),
            None => 0.0,
        };
        let flags = match args.get(2) {
            Some(_) => integer(args, 2, what)?,
            None => 0,
        };
        if let Some(key) = o.key {
            (c.state.borrow_mut()).play(key, play_flags(flags, delay), None, owner);
        }
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method_with_objects("play_at_pos", move |o: &mut SoundObject, objects, args| {
        c.stopped()?;
        let what = "sound_object:play_at_pos";
        let owner = handle_arg(objects, args, 0, what)?;
        let position = vector_arg(objects, args, 1, what)?;
        let delay = match args.get(2) {
            Some(_) => to_f32(number(args, 2, what)?),
            None => 0.0,
        };
        let flags = match args.get(3) {
            Some(_) => integer(args, 3, what)?,
            None => 0,
        };
        if let Some(key) = o.key {
            let flags = play_flags(flags, delay);
            (c.state.borrow_mut()).play(key, flags, Some(position), owner);
        }
        Ok(Vec::new())
    });
    // 2.4.
    let c = ctx.clone();
    class = class.method_with_objects(
        "play_no_feedback",
        move |o: &mut SoundObject, objects, args| {
            c.stopped()?;
            let what = "sound_object:play_no_feedback";
            let owner = handle_arg(objects, args, 0, what)?;
            let flags = integer(args, 1, what)?;
            let delay = to_f32(number(args, 2, what)?);
            let position = vector_arg(objects, args, 3, what)?;
            let volume = to_f32(number(args, 4, what)?);
            if let Some(key) = o.key {
                (c.state.borrow_mut()).push(Request::PlayIndependent {
                    key,
                    flags: play_flags(flags, delay),
                    position,
                    volume,
                    owner,
                });
            }
            Ok(Vec::new())
        },
    );
    // 2.5.
    let c = ctx.clone();
    class = class.method("stop", move |o: &mut SoundObject, _| {
        c.stopped()?;
        if let Some(key) = o.key {
            c.state.borrow_mut().stop(key);
        }
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method("stop_deffered", move |o: &mut SoundObject, _| {
        c.stopped()?;
        let mut s = c.state.borrow_mut();
        if let Some(key) = o.key
            && s.emitter(key).is_some()
        {
            s.push(Request::StopWithFade(key));
        }
        Ok(Vec::new())
    });
    // 2.6.
    let c = ctx.clone();
    class = class.method("playing", move |o: &mut SoundObject, _| {
        c.stopped()?;
        let playing = o.key.is_some_and(|k| c.state.borrow().emitter(k).is_some());
        Ok(vec![Value::Boolean(playing)])
    });
    let c = ctx.clone();
    class = class.method("length", move |o: &mut SoundObject, _| {
        c.stopped()?;
        let length = o
            .key
            .and_then(|k| c.state.borrow().sound(k).map(|s| s.file.duration_ms));
        match length {
            Some(ms) => Ok(vec![Value::Number(ms as f64)]),
            // Read through no object (2.6): fatal here.
            None => Err(c.fatal(
                "sound_object:length",
                line!(),
                "the sound object has no sound",
            )),
        }
    });
    // 2.7.
    let c = ctx.clone();
    class = class.method_with_objects("get_position", move |o: &mut SoundObject, objects, _| {
        c.stopped()?;
        let emitter = o.key.and_then(|k| c.state.borrow().emitter(k));
        Ok(vec![new_vector(
            objects,
            emitter.map_or([0.0; 3], |e| e.position),
        )])
    });
    let c = ctx.clone();
    class = class.method_with_objects("set_position", move |o: &mut SoundObject, objects, args| {
        c.stopped()?;
        let position = vector_arg(objects, args, 0, "sound_object:set_position")?;
        let mut s = c.state.borrow_mut();
        let key = o.key.filter(|&k| s.emitter(k).is_some());
        let Some(key) = key else {
            drop(s);
            // Read through no object (2.7): fatal here.
            return Err(c.fatal(
                "sound_object:set_position",
                line!(),
                "the sound object has no emitter",
            ));
        };
        if let Some(e) = s.emitter_mut(key) {
            e.position = position;
        }
        s.push(Request::SetPosition(key, position));
        Ok(Vec::new())
    });
    // 2.8.
    for (name, read, write) in emitter_properties() {
        let c = ctx.clone();
        let set_c = ctx.clone();
        class = class.property_with_objects(
            name,
            move |o: &SoundObject, _| {
                c.stopped()?;
                match o.key.and_then(|k| c.state.borrow().emitter(k)) {
                    Some(e) => Ok(Value::Number(f64::from(read(&e)))),
                    // Read through no object (2.8): fatal here.
                    None => Err(c.fatal(
                        &format!("sound_object.{name}"),
                        line!(),
                        "the sound object has no emitter",
                    )),
                }
            },
            Some(Box::new(move |o: &mut SoundObject, _, value: &Value| {
                set_c.stopped()?;
                let v = to_f32(lua_number(value));
                let mut s = set_c.state.borrow_mut();
                if let Some(key) = o.key
                    && let Some(e) = s.emitter_mut(key)
                {
                    let request = write(e, key, v);
                    s.push(request);
                }
                Ok(())
            })),
        );
    }
    class
}

type Read = fn(&Emitter) -> f32;
type Write = fn(&mut Emitter, SoundKey, f32) -> Request;

/// The emitter's controls as properties (2.8): what each reads, and the
/// request assigning it makes.
fn emitter_properties() -> [(&'static str, Read, Write); 4] {
    [
        (
            "volume",
            |e| e.volume,
            |e, k, v| {
                e.volume = v;
                Request::SetVolume(k, v)
            },
        ),
        (
            "frequency",
            |e| e.frequency,
            |e, k, v| {
                e.frequency = v;
                Request::SetFrequency(k, v)
            },
        ),
        (
            "min_distance",
            |e| e.min_distance,
            |e, k, v| {
                e.min_distance = v;
                Request::SetRange(k, v, e.max_distance)
            },
        ),
        (
            "max_distance",
            |e| e.max_distance,
            |e, k, v| {
                e.max_distance = v;
                Request::SetRange(k, e.min_distance, v)
            },
        ),
    ]
}

/// A number property of a record (4, 7.4): read exactly, assigned by
/// Lua's number conversion.
fn record_number<T: 'static>(
    class: EngineClass,
    name: &str,
    get: fn(&T) -> f64,
    set: fn(&mut T, f64),
) -> EngineClass {
    class.writable_property(
        name,
        move |r: &T| Value::Number(get(r)),
        move |r: &mut T, v| {
            set(r, lua_number(v));
            Ok(())
        },
    )
}

/// A vector property of a record.
fn record_vector<T: 'static>(
    class: EngineClass,
    name: &str,
    get: fn(&T) -> Vector,
    set: fn(&mut T, Vector),
) -> EngineClass {
    let what = name.to_owned();
    class.property_with_objects(
        name,
        move |r: &T, objects| Ok(objects.new_object("vector", get(r))),
        Some(Box::new(
            move |r: &mut T, objects, value: &Value| match objects.get::<Vector>(value) {
                Some(Operand::Other(v)) => {
                    set(r, v);
                    Ok(())
                }
                _ => Err(format!("{what}: vector expected")),
            },
        )),
    )
}

/// `sound_params` (4): five fields, no constructor.
fn sound_params() -> EngineClass {
    let class = EngineClass::new("sound_params");
    let class = record_vector(
        class,
        "position",
        |r: &SoundParams| r.position,
        |r, v| r.position = v,
    );
    let class = record_number(
        class,
        "volume",
        |r: &SoundParams| f64::from(r.volume),
        |r, v| r.volume = to_f32(v),
    );
    let class = record_number(
        class,
        "frequency",
        |r: &SoundParams| f64::from(r.frequency),
        |r, v| r.frequency = to_f32(v),
    );
    let class = record_number(
        class,
        "min_distance",
        |r: &SoundParams| f64::from(r.min_distance),
        |r, v| r.min_distance = to_f32(v),
    );
    record_number(
        class,
        "max_distance",
        |r: &SoundParams| f64::from(r.max_distance),
        |r, v| r.max_distance = to_f32(v),
    )
}

/// `SoundInfo` (7.4): five fields, no constructor.
fn sound_info(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    let class = EngineClass::new("SoundInfo").property_with_objects(
        "who",
        move |r: &SoundInfo, _| Ok(c.objects.borrow().handle_value(r.who)),
        Some(Box::new(|r: &mut SoundInfo, objects, value: &Value| {
            r.who = match objects.get::<Handle>(value) {
                Some(Operand::Other(h)) => Some(h.id),
                _ => None,
            };
            Ok(())
        })),
    );
    let class = record_vector(
        class,
        "position",
        |r: &SoundInfo| r.position,
        |r, v| r.position = v,
    );
    let class = record_number(
        class,
        "power",
        |r: &SoundInfo| f64::from(r.power),
        |r, v| r.power = to_f32(v),
    );
    let class = record_number(class, "time", |r: &SoundInfo| r.time, |r, v| r.time = v);
    record_number(
        class,
        "danger",
        |r: &SoundInfo| r.danger,
        |r, v| r.danger = v,
    )
}

/// `snd_type` (3): the 46 constants.
fn snd_type() -> EngineClass {
    SND_TYPES
        .into_iter()
        .fold(EngineClass::new("snd_type"), |c, (name, value)| {
            c.constant(name, value as i32)
        })
}

/// The `level` functions' and members' engine part: `args[0]` names the
/// operation.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("sound: operation expected".into()),
    };
    let args = args.get(1..).unwrap_or(&[]);
    match op.as_str() {
        // Fatal as D 1.2: the state stops with the binding library's error.
        "fatal" => {
            let message = format!(
                "LUA error: {}",
                String::from_utf8_lossy(&text(args, 0, "fatal")?)
            );
            *ctx.fatal.borrow_mut() = Some(message.clone());
            return Ok(vec![Value::string(message)]);
        }
        _ => ctx.stopped()?,
    }
    match op.as_str() {
        // 5.1.
        "set_snd_volume" => {
            let f = to_f32(number(args, 0, "level.set_snd_volume")?).clamp(0.0, 1.0);
            let mut s = ctx.state.borrow_mut();
            s.effects_factor = f;
            s.push(Request::SetEffectsFactor(f));
            Ok(Vec::new())
        }
        "get_snd_volume" => Ok(vec![Value::Number(f64::from(
            ctx.state.borrow().effects_factor,
        ))]),
        // 5.2.
        "prefetch_sound" => {
            let name = text(args, 0, "level.prefetch_sound")?;
            ctx.with_sounds("level.prefetch_sound", line!(), |s| s.prefetch(&name))?;
            Ok(Vec::new())
        }
        // 5.3: the texts `f` is called with, in order.
        "iterate_sounds" => {
            let what = "level.iterate_sounds";
            let list = text(args, 0, what)?;
            let n = integer(args, 1, what)?;
            let names = ctx.with_sounds(what, line!(), |s| {
                let mut names = Vec::new();
                for item in items(&list) {
                    if s.exists(&item)? {
                        names.push(list.clone());
                    }
                    for i in 0..n {
                        let name = numbered(&item, i);
                        if s.exists(&name)? {
                            names.push(name);
                        }
                    }
                }
                Ok(names)
            })?;
            Ok(vec![Value::Table(
                (names.into_iter().enumerate())
                    .map(|(i, n)| (Value::Number(i as f64 + 1.0), Value::String(n)))
                    .collect(),
            )])
        }
        _ => creatures::member(ctx, maker, &op, args),
    }
}

/// Registers the classes, the raw function and the Lua side.
fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    scripts.register_class(sound_object(ctx))?;
    scripts.register_class(snd_type())?;
    scripts.register_class(sound_params())?;
    scripts.register_class(sound_info(ctx))?;
    let lua = scripts.lua_mut();
    let c = ctx.clone();
    lua.register_with_objects("__volya_sound", move |maker, args| raw(&c, maker, args))?;
    lua.exec("=sound", include_bytes!("sound.lua")).map(drop)
}

/// What a state with the sound registered keeps.
pub(crate) struct Registered {
    pub ctx: Context,
}

impl Scripts {
    /// Registers script API T's names (1.1) over `state`: after the game
    /// object basics and the dialogs and trade (whose objects, services,
    /// files and events it shares), before [`Scripts::load_globals`].
    /// Sound files are found and loaded over `trees` unless `state` has
    /// its own [`SoundFiles`].
    pub fn register_sound(
        &mut self,
        state: Rc<RefCell<ScriptSounds>>,
        trees: Rc<dyn Trees>,
    ) -> Result<(), Error> {
        let api = |message: &str| Error {
            kind: ErrorKind::Api,
            message: message.into(),
        };
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let dialogs =
            (self.registered_dialogs()).ok_or_else(|| api("the dialogs aren't registered"))?;
        let ctx = Context {
            state,
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            dialogs,
            fatal: self.fatal_flag(),
        };
        {
            let mut s = ctx.state.borrow_mut();
            if s.files.is_none() {
                s.files = Some(Box::new(GameSoundFiles {
                    trees,
                    files: Rc::clone(&ctx.files),
                    loaded: Sounds::new(),
                }));
            }
        }
        register(self, &ctx)?;
        self.sound_state = Some(Registered { ctx });
        Ok(())
    }

    /// The sounds the scripts made, once registered.
    pub fn script_sounds(&self) -> Option<Rc<RefCell<ScriptSounds>>> {
        self.sound_state.as_ref().map(|r| Rc::clone(&r.ctx.state))
    }
}
