// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the level plays (docs/behavior/level-sound-scope.md): the static
//! sources (S3, docs/behavior/sound-static-sources.md), the weather's
//! ambient sounds (S4, sound-weather-ambient.md), the level music (S5,
//! sound-level-music.md), the rain and thunder (S8, sound-rain-thunder.md),
//! and what occludes them (S6, sound-occlusion.md), on the positional
//! sound system (S2).
//!
//! [`LevelSoundData`] is read while the level loads; [`LevelSounds`] are
//! the sounds created from it in the sound system once the level runs;
//! [`SoundRun`] is the state the specs keep for the whole run, across
//! levels (S4 4.2, S8 3.2), with the clock they share.
//!
//! The clock (S3 4.1, S4 3.1, S5 4.1, S8 3.1): the engine clock's real
//! time counted only on frames that advance the level's game time, so it
//! stops while the game is paused (the menu, the intro) and while the page
//! is hidden (#93), and never counts the time outside a level (our choice:
//! the specs do not say whether it runs during the title menu and the
//! loading). Its rate is 1 and it has no fixed-step mode: the spec's
//! timer speed factor (1 unless changed) and the device flag 0x100 (20 ms
//! per frame, off initially) are at their initial values, and nothing of
//! Volya Engine changes them (S3's review gap; S4 3.1).

use std::collections::HashMap;
use std::io;

use crate::core::config::{self, LEVEL, PathAliases};
use crate::core::sound::Fatal;
use crate::core::sound::ambient::{self, AmbientSet, AmbientState, SetData};
use crate::core::sound::music::{self, LevelMusic, TrackParams};
use crate::core::sound::occlusion::{self, SoundOcclusionMesh};
use crate::core::sound::rain_thunder::{
    self, BoltCommon, Collection, Rain, ThunderState, ThunderWeather,
};
use crate::core::sound::static_sources::{self, SourceRecord, StaticSources};
use crate::core::sound::system::{SoundId, SoundSystem, Vec3};
use crate::core::vfs::{self, Vfs};
use crate::core::weather::Weather;
use crate::data::GameData;

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
}

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
        })
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
}

impl LevelSounds {
    /// Creates every sound of `data` in `system` (each file resolved with
    /// `$level$` at the level's directory, S1 section 2): fatal when a file
    /// cannot be opened.
    pub fn create(
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
        })
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

/// The state kept for the whole run (S4 4.2, S8 3.2) and the clock.
#[derive(Debug, Default)]
pub struct SoundRun {
    /// The global clock (ms; module docs).
    pub clock: u64,
    pub ambient: AmbientState,
    pub rain: Rain,
    pub thunder: ThunderState,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

impl SoundRun {
    /// Advances the clock by a level frame of `elapsed` ms of game-time
    /// advance.
    pub fn advance(&mut self, elapsed: u64) {
        self.clock += elapsed;
    }

    /// The level was left (S4 4.2).
    pub fn level_left(&mut self) {
        self.ambient.level_left();
    }

    /// The rules of one running frame `elapsed_s` seconds after the last,
    /// in the original's order: the static sources, then the music (S5
    /// 4.1), then the environment's thunder and rain (S8 3.2), then the
    /// ambient sounds after the engine's frame (S4 4.1).
    pub fn frame(
        &mut self,
        system: &mut SoundSystem,
        sounds: &mut LevelSounds,
        inputs: &SoundInputs,
        elapsed_s: f32,
    ) {
        let now = self.clock as u32;
        sounds.statics.frame(system, now, inputs.tau);
        sounds.music.frame(system, now, inputs.tau);
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
        let dt = self.thunder.smooth_frame_time(elapsed_s);
        if let Some(common) = &sounds.bolt_common {
            let seconds = self.clock as f32 / 1000.0;
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
        self.ambient
            .frame(system, now, (set(a), set(b), t), outdoors, inputs.eye);
    }
}
