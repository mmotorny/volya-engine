// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the level plays (docs/behavior/level-sound-scope.md): the static
//! sources (S3, docs/behavior/sound-static-sources.md), the weather's
//! ambient sounds (S4, sound-weather-ambient.md), the level music (S5,
//! sound-level-music.md), the rain and thunder (S8, sound-rain-thunder.md),
//! what occludes them (S6, sound-occlusion.md) and the material pairs'
//! sounds (S9, material-pairs.md; the player's footsteps play them,
//! crate::footsteps), on the positional sound system (S2).
//!
//! [`LevelSoundData`] is read while the level loads; [`LevelSounds`] are
//! the sounds created from it in the sound system once the level runs;
//! [`SoundRun`] is the state the specs keep for the whole run, across
//! levels (S4 4.2, S8 3.2), with the clock they share.
//!
//! The clock (S3 4.1, S4 3.1, S5 4.1, S8 3.1) is the engine's global
//! clock of S4 3.1 (`volya-engine-core::pause`), which the app keeps; the
//! rules run on the level's frames while the pause is off.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use crate::core::config::{self, LEVEL, PathAliases};
use crate::core::detector::DetectorSection;
use crate::core::sound::Fatal;
use crate::core::sound::ambient::{self, AmbientParticles, AmbientSet, AmbientState, SetData};
use crate::core::sound::music::{self, LevelMusic, TrackParams};
use crate::core::sound::occlusion::{self, SoundOcclusionMesh};
use crate::core::sound::rain_thunder::{
    self, BoltCommon, Collection, Rain, ThunderState, ThunderWeather,
};
use crate::core::sound::static_sources::{self, SourceRecord, StaticSources};
use crate::core::sound::system::{PlayFlags, SoundId, SoundSystem, Vec3};
use crate::core::torch::NightVisionSection;
use crate::core::vfs::{self, Vfs};
use crate::core::weather::Weather;
use crate::core::weather::wind::Wind;
use crate::core::zones::{self, SoundEvent};
use crate::data::GameData;
use crate::detector::{DetectorCommand, DetectorSound, DetectorSounds};
use crate::footsteps::{MaterialPairs, PairSounds};
use crate::torch::{TorchSound, TorchSounds};

/// Reads `name` in the level directory `dir`, `None` when it is missing.
fn optional(tree: &dyn Vfs, dir: &str, name: &str) -> io::Result<Option<Vec<u8>>> {
    match vfs::read(tree, &format!("{dir}\\{name}")) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io::Error::new(e.kind(), format!("{dir}\\{name}: {e}"))),
    }
}

fn context(path: &str, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{path}: {e}"))
}

/// The level's sound data, read with the level.
#[derive(Clone, Debug, Default)]
pub struct LevelSoundData {
    /// The level directory (`$level$`).
    pub dir: String,
    /// `level.snd_static`'s sources (S3); none without the file.
    pub statics: Vec<SourceRecord>,
    /// `level.som`'s mesh (S6), if the level has one.
    pub occlusion_mesh: Option<SoundOcclusionMesh>,
    /// The occlusion scale *k* (S6 section 3).
    pub occlusion_scale: f32,
    /// The track list (S5 2.1).
    pub music: Vec<(String, TrackParams)>,
    /// The ambient sets the weather's sections name, by name (S4 2.2).
    pub sets: HashMap<String, SetData>,
    /// The thunderbolt collections the weather's sections name: their
    /// descriptions' sounds (S8 2.2).
    pub collections: HashMap<String, Vec<Option<String>>>,
    pub bolt_common: Option<BoltCommon>,
    /// The material pairs, whose sounds are created with the level's (S9
    /// section 4); read with the materials.
    pub pairs: Option<Arc<MaterialPairs>>,
    /// The night vision of each of the player's torches with it, by item
    /// id: its sounds are loaded with its section (torch.md 14).
    pub torches: Vec<(u16, NightVisionSection)>,
    /// The player's detectors' sections, by item id: their sound lists are
    /// loaded with them (sound-detector.md 2.2).
    pub detectors: Vec<(u16, DetectorSection)>,
    /// The sounds of each zone object (S15 2.1), by zone, created with the
    /// level's.
    pub zones: Vec<ZoneSoundNames>,
}

/// A zone's sound names, by the index of S15 2.1's keys.
pub type ZoneSoundNames = [Option<String>; 8];

impl LevelSoundData {
    /// Reads the sound data of level `name` in `dir` under `weather`:
    /// `level.snd_static` and `level.som` (both optional), the track list of
    /// `config\game.ltx`, the weather's ambient sets and thunderbolt
    /// collections and `[thunderbolt_common]` of the system configuration,
    /// and `[sound] occlusion_scale`.
    pub fn read(
        tree: &dyn Vfs,
        data: &GameData,
        dir: &str,
        name: &str,
        weather: &Weather,
    ) -> io::Result<Self> {
        let statics = match optional(tree, dir, static_sources::FILE)? {
            Some(bytes) => static_sources::parse(&bytes)
                .map_err(|e| context(&format!("{dir}\\{}", static_sources::FILE), e))?,
            None => Vec::new(),
        };
        let occlusion_mesh = match optional(tree, dir, occlusion::FILE)? {
            Some(bytes) => {
                let path = format!("{dir}\\{}", occlusion::FILE);
                let polygons = occlusion::parse(&bytes).map_err(|e| context(&path, e))?;
                Some(SoundOcclusionMesh::build(&polygons).map_err(|e| context(&path, e))?)
            }
            None => None,
        };
        let game_path = data.aliases.path("$game_config$", "game.ltx")?;
        let game = config::load(tree, &game_path)?;
        let music = music::track_list(&game, name).map_err(|e| context(&game_path, e))?;
        let system = &data.system;
        let mut sets = HashMap::new();
        let mut collections = HashMap::new();
        for (_, section) in &weather.sections {
            if let Some(set) = &section.env_ambient
                && !sets.contains_key(set)
            {
                sets.insert(set.clone(), SetData::read(system, set)?);
            }
            let c = &section.thunderbolt;
            if !c.is_empty() && !collections.contains_key(c) {
                collections.insert(c.clone(), rain_thunder::collection_sounds(system, c)?);
            }
        }
        let bolt_common = if collections.is_empty() {
            None
        } else {
            Some(BoltCommon::read(system)?)
        };
        Ok(Self {
            dir: dir.to_owned(),
            statics,
            occlusion_mesh,
            occlusion_scale: occlusion::scale(system)?,
            music,
            sets,
            collections,
            bolt_common,
            pairs: None,
            torches: Vec::new(),
            detectors: Vec::new(),
            zones: Vec::new(),
        })
    }

    /// Checks every sound file of the level as creating its sounds does
    /// (S1 section 5: a file that cannot be opened is fatal at level load),
    /// whether or not a sound system exists to create them in.
    pub fn check(&self, tree: &dyn Vfs, aliases: &PathAliases) -> Result<(), Fatal> {
        let mut scratch = SoundSystem::new(0);
        LevelSounds::create(&mut scratch, tree, aliases, self).map(|_| ())
    }
}

/// The level's sounds in the sound system.
#[derive(Debug)]
pub struct LevelSounds {
    pub statics: StaticSources,
    pub music: LevelMusic,
    pub sets: HashMap<String, AmbientSet>,
    pub collections: HashMap<String, Collection>,
    pub bolt_common: Option<BoltCommon>,
    /// `ambient\rain` (S8 4.1).
    pub rain: SoundId,
    pub pairs: PairSounds,
    /// The torches' night-vision sounds, by item id (torch.md 14).
    pub torches: HashMap<u16, TorchSounds>,
    /// The detectors' sound lists, by item id (sound-detector.md 2.2).
    pub detectors: HashMap<u16, DetectorSounds>,
    /// The level directory, for the sounds of torches that come later.
    dir: String,
    /// Each zone's sounds (S15 2.1), by zone.
    pub zones: Vec<[Option<SoundId>; 8]>,
}

/// Creates a zone's sounds in the effects category (S15 2.1): each key a
/// sound of its own.
fn zone_sounds(
    system: &mut SoundSystem,
    tree: &dyn Vfs,
    aliases: &PathAliases,
    names: &ZoneSoundNames,
) -> Result<[Option<SoundId>; 8], Fatal> {
    let mut ids = [None; 8];
    for (id, name) in ids.iter_mut().zip(names) {
        if let Some(name) = name {
            *id =
                Some(system.create(tree, aliases, name, crate::core::audio::Category::Effects)?);
        }
    }
    Ok(ids)
}

impl LevelSounds {
    /// Creates every sound of `data` in `system` (each file resolved with
    /// `$level$` at the level's directory, S1 section 2): fatal when a file
    /// cannot be opened (S1 section 5), and then none of them is kept.
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        data: &LevelSoundData,
    ) -> Result<Self, Fatal> {
        system.all_or_none(|system| Self::create_all(system, tree, aliases, data))
    }

    fn create_all(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        data: &LevelSoundData,
    ) -> Result<Self, Fatal> {
        let mut aliases = aliases.clone();
        aliases.set_dir(LEVEL, &data.dir);
        let aliases = &aliases;
        let statics = StaticSources::create(system, tree, aliases, &data.statics)?;
        let music = LevelMusic::create(system, tree, aliases, &data.music)?;
        let mut sets = HashMap::new();
        for (name, set) in &data.sets {
            sets.insert(
                name.clone(),
                AmbientSet::create(system, tree, aliases, set.clone())?,
            );
        }
        let mut collections = HashMap::new();
        for (name, sounds) in &data.collections {
            collections.insert(
                name.clone(),
                Collection::create(system, tree, aliases, name, sounds)?,
            );
        }
        let rain = system.create(
            tree,
            aliases,
            rain_thunder::RAIN,
            crate::core::audio::Category::Effects,
        )?;
        // S9 section 4: when the library loads, here with the level (the
        // library is read with it, crate::level).
        let pairs = match &data.pairs {
            Some(p) => PairSounds::create(system, tree, aliases, p)?,
            None => PairSounds::default(),
        };
        let mut torches = HashMap::new();
        for (id, section) in &data.torches {
            torches.insert(*id, TorchSounds::create(system, tree, aliases, section)?);
        }
        let mut detectors = HashMap::new();
        for (id, section) in &data.detectors {
            detectors.insert(*id, DetectorSounds::create(system, tree, aliases, section)?);
        }
        let zones = (data.zones.iter())
            .map(|names| zone_sounds(system, tree, aliases, names))
            .collect::<Result<_, _>>()?;
        let occlusion = system.occlusion_mut();
        occlusion.scale = data.occlusion_scale;
        if let Some(mesh) = &data.occlusion_mesh {
            // S6 section 3: replaces the previous; kept when a level without
            // one is loaded.
            occlusion.sound = Some(mesh.clone());
        }
        Ok(Self {
            statics,
            music,
            sets,
            collections,
            bolt_common: data.bolt_common.clone(),
            rain,
            pairs,
            torches,
            detectors,
            dir: data.dir.clone(),
            zones,
        })
    }

    /// Carries out a torch's sound command (torch.md 14), its sounds created
    /// from `section` the first time (a torch that came after the level's
    /// sounds): fatal when a file cannot be opened (S1 section 5).
    pub fn torch(
        &mut self,
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        sound: TorchSound,
        section: Option<&NightVisionSection>,
    ) -> Result<(), Fatal> {
        if !self.torches.contains_key(&sound.torch) {
            let Some(section) = section else {
                return Ok(());
            };
            let mut aliases = aliases.clone();
            aliases.set_dir(LEVEL, &self.dir);
            let created = system
                .all_or_none(|system| TorchSounds::create(system, tree, &aliases, section))?;
            self.torches.insert(sound.torch, created);
        }
        if let Some(t) = self.torches.get_mut(&sound.torch) {
            t.apply(system, sound.command);
        }
        Ok(())
    }

    /// Carries out a detector's sound command (sound-detector.md 3, 6), its
    /// sound lists created from `section` the first time (a detector that
    /// came after the level's sounds): fatal when a file cannot be opened
    /// (S1 section 5). Discarding discards them.
    pub fn detector(
        &mut self,
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        sound: DetectorSound,
        section: Option<&DetectorSection>,
    ) -> Result<(), Fatal> {
        let command = match sound.command {
            DetectorCommand::Discard => {
                if let Some(d) = self.detectors.remove(&sound.detector) {
                    d.discard(system);
                }
                return Ok(());
            }
            DetectorCommand::Sound(c) => c,
        };
        if !self.detectors.contains_key(&sound.detector) {
            let Some(section) = section else {
                return Ok(());
            };
            let mut aliases = aliases.clone();
            aliases.set_dir(LEVEL, &self.dir);
            let created = system
                .all_or_none(|system| DetectorSounds::create(system, tree, &aliases, section))?;
            self.detectors.insert(sound.detector, created);
        }
        if let Some(d) = self.detectors.get(&sound.detector) {
            d.apply(system, command);
        }
        Ok(())
    }

    /// Creates the sounds of zones that came online after the level's
    /// sounds were made (`names` from the first zone on), each zone's as
    /// its object is loaded (S15 2.1); fatal as creating any is.
    pub fn add_zones(
        &mut self,
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        dir: &str,
        names: &[ZoneSoundNames],
    ) -> Result<(), Fatal> {
        if names.len() <= self.zones.len() {
            return Ok(());
        }
        let mut aliases = aliases.clone();
        aliases.set_dir(LEVEL, dir);
        for n in &names[self.zones.len()..] {
            let ids = system.all_or_none(|system| zone_sounds(system, tree, &aliases, n))?;
            self.zones.push(ids);
        }
        Ok(())
    }

    /// The zones' idle sounds start and stop (P9 4.1, 4.2; S15 3): played
    /// at position, looped, not 2D, delay 0, without an owner; stopped at
    /// once.
    pub fn zone_events(&self, system: &mut SoundSystem, events: &[SoundEvent]) {
        for e in events {
            match *e {
                SoundEvent::Play { zone, position } => {
                    if let Some(Some(id)) = self.zones.get(zone).map(|z| z[zones::IDLE]) {
                        let looped = PlayFlags {
                            looped: true,
                            ..PlayFlags::default()
                        };
                        system.play_at(id, position, looped);
                    }
                }
                SoundEvent::Stop { zone } => {
                    if let Some(Some(id)) = self.zones.get(zone).map(|z| z[zones::IDLE]) {
                        system.stop(id);
                    }
                }
            }
        }
    }

    /// The level was left: every sound is discarded (S5 section 6; S3's
    /// own sources' fate is unestablished, S3 section 7, with no audible
    /// difference), and the sound system loses the level mesh (S6 3).
    pub fn discard(mut self, system: &mut SoundSystem) {
        self.statics.discard(system);
        self.music.discard(system);
        for set in self.sets.values() {
            set.discard(system);
        }
        for c in self.collections.values() {
            c.discard(system);
        }
        system.discard(self.rain);
        self.pairs.discard(system);
        for t in self.torches.values() {
            t.discard(system);
        }
        for d in self.detectors.values() {
            d.discard(system);
        }
        for &id in self.zones.iter().flatten().flatten() {
            system.discard(id);
        }
        system.occlusion_mut().level = None;
    }
}

/// The weather and view a frame's rules read.
#[derive(Clone, Debug)]
pub struct SoundInputs<'a> {
    /// τ, the time of day (ms).
    pub tau: u32,
    /// The current sections A and B: their ambient sets, rain densities,
    /// collections, bolt periods and durations; *t*.
    pub sections: [&'a crate::core::weather::WeatherSection; 2],
    pub t: f32,
    /// The blended far plane and L_sun_dir (S8 2.3).
    pub far_plane: f32,
    pub sun_dir: Vec3,
    /// The camera's eye.
    pub eye: Vec3,
    /// The view object's sky visibility *h* (S4 4.3, S8 4.3).
    pub sky_visibility: f32,
}

/// The state kept for the whole run (S4 4.2, S8 3.2).
#[derive(Debug, Default)]
pub struct SoundRun {
    pub ambient: AmbientState,
    pub rain: Rain,
    pub thunder: ThunderState,
    /// The environment's wind (weather-ambient-effects.md 4).
    pub wind: Wind,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

impl SoundRun {
    /// The level was left (S4 4.2).
    pub fn level_left(&mut self) {
        self.ambient.level_left();
    }

    /// The engine's frame time *dt* of a frame `real_s` seconds of real
    /// time after the previous one (S8 3.1): smoothed, or 0 while the game
    /// is `paused`. Every frame, in a level or not.
    pub fn frame_time(&mut self, real_s: f32, paused: bool) {
        if paused {
            self.thunder.paused_frame();
        } else {
            self.thunder.smooth_frame_time(real_s);
        }
    }

    /// The rules of one frame of the running level, the pause off, at
    /// global clock `now` (ms), in the original's order: the static sources, then the music (S5
    /// 4.1), then the environment's thunder and rain (S8 3.2), then the
    /// ambient sounds after the engine's frame (S4 4.1). Under the loading
    /// image (`loading_image`) the static sources and the music do not run
    /// (S3 4.1, S5 4.1); the specs of the others make no such exception.
    /// Fatal when a rule draws from a range with equal bounds (S3 4.2, S4
    /// 4.7).
    pub fn frame(
        &mut self,
        system: &mut SoundSystem,
        sounds: &mut LevelSounds,
        inputs: &SoundInputs,
        now: u64,
        loading_image: bool,
        particles: &mut dyn AmbientParticles,
    ) -> Result<(), Fatal> {
        // Unsigned 32-bit milliseconds (S3 4.1, S4 3.1).
        let seconds = now as f32 / 1000.0;
        let now = now as u32;
        if !loading_image {
            sounds.statics.frame(system, now, inputs.tau)?;
            sounds.music.frame(system, now, inputs.tau)?;
        }
        let [a, b] = inputs.sections;
        let t = inputs.t;
        // S8 2.3.
        let in_effect = if t >= 0.5 { b } else { a };
        let collection = sounds.collections.get(&in_effect.thunderbolt);
        let weather = ThunderWeather {
            collection,
            period: lerp(a.bolt_period, b.bolt_period, t),
            duration: lerp(a.bolt_duration, b.bolt_duration, t),
            far_plane: inputs.far_plane,
            sun_dir: inputs.sun_dir,
        };
        // The environment's wind with the gust the previous frame's rule
        // left (P11 3.6, 4.3).
        self.wind.update(self.ambient.wind_gust_factor, seconds);
        let dt = self.thunder.frame_time;
        if let Some(common) = &sounds.bolt_common {
            let f = self
                .thunder
                .frame(system.random(), seconds, dt, &weather, common, inputs.eye);
            if let (Some(Some(thunder)), Some(c)) = (f.bolt, collection) {
                rain_thunder::play_thunder(system, c, &thunder);
            }
        }
        let density = lerp(a.rain_density, b.rain_density, t);
        let k = rain_thunder::sky_factor(inputs.sky_visibility);
        self.rain.frame(system, sounds.rain, density, k, inputs.eye);
        let set = |s: &crate::core::weather::WeatherSection| {
            s.env_ambient.as_ref().and_then(|n| sounds.sets.get(n))
        };
        let outdoors = inputs.sky_visibility >= ambient::OUTDOORS;
        self.ambient.frame(
            system,
            now,
            (set(a), set(b), t),
            outdoors,
            inputs.eye,
            particles,
        )?;
        Ok(())
    }
}
