// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game's level (docs/behavior/level-loading.md): which level, its
//! static world, and the view from the player's start
//! (docs/behavior/start-position.md), under the weather of the game clock's
//! time of day (docs/formats/sav.md sections 5 and 12,
//! docs/behavior/static-shading.md section 9), with its collision world
//! (docs/formats/level-cform.md), walked by the player in first person
//! (docs/behavior/player-movement.md) within its character's condition
//! (docs/behavior/player-state.md), its objects switched online and
//! offline by the offline simulation as the player moves
//! (docs/behavior/offline-simulation.md).
//!
//! [`LevelScreen::load`] reads everything synchronously through the VFS
//! but the geometry's buffers: only their layout is read
//! ([`LevelWorld::geometry`]), and the presenter streams them from the file
//! to the GPU on the first frame drawn, so the CPU never holds them whole.

use std::borrow::Cow;
use std::io;
use std::sync::Arc;

use serde::Serialize;

use crate::controls::Controls;
use crate::core::camera::{self, Angles, Mat4, Vec3};
use crate::core::character::condition::Activity;
use crate::core::character::inventory::Place;
use crate::core::character::{self, CharacterState, SpecificCharacter};
use crate::core::climbing::Ladder;
use crate::core::clock::GameClock;
use crate::core::collision::{CollisionWorld, geometry};
use crate::core::config;
use crate::core::config::Config;
use crate::core::detector::DetectorSection;
use crate::core::input::Input;
use crate::core::level::{DrawItem, Level};
use crate::core::lights::{LightSettings, Lights};
use crate::core::movement::{Flags, MovementConfig, Player};
use crate::core::object_light::{self, SkyVisibility};
use crate::core::particles::placed::{Origin, PlacedEffect, PlacedEffects, particle_capacity};
use crate::core::simulation::graph::Graph;
use crate::core::simulation::navigation::Navigation;
use crate::core::simulation::{AlifeConfig, Changes, Simulation};
use crate::core::sound::Fatal;
use crate::core::sound::ambient::AmbientParticles;
use crate::core::vfs::{self, Vfs};
use crate::core::weather::{Environment, Weather, texture_name as weather_texture_name};
use crate::core::zones::{Behaviour, SoundEvent, ZoneRecord, ZoneSection, Zones};
use crate::data::GameData;
use crate::detector::{DetectorReport, DetectorSound, PlayerDetectors};
use crate::footsteps::{self, MaterialPairs, PlayerFootsteps};
use crate::level_lights::{self, GlowShader, LevelLights, LightCamera, LightsData};
use crate::level_sound::ZoneSoundNames;
use crate::level_sound::{LevelSoundData, SoundInputs};
use crate::loading_screen::Stage;
use crate::objects::{self, Characters, LevelObjects, ObjectsReport};
use crate::particle_drawing::{self, ParticleShader};
use crate::render::sky::{SkySection, SkyTextures};
use crate::render::world::Surface;
use crate::scene::{Scene, WorldScene};
use crate::shading::{self, ShaderLibrary};
use crate::sky::{self, Flares, SkyCamera, SunFade};
use crate::torch::{PlayerTorchData, PlayerTorches, TorchReport, TorchSound};
use std::collections::HashMap;
use volya_engine_formats::env_mod::{self, EnvModifier};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::gamemtl;
use volya_engine_formats::lanims::ColourAnimations;
use volya_engine_formats::level::split_shader_entry;
use volya_engine_formats::level_geom::{GeomBuffer, GeomFile, GeomLayout, GeomSource};
use volya_engine_formats::model as model_math;
use volya_engine_formats::object::{self as record, ObjectRecord};
use volya_engine_formats::particles::{self as particle_records, Library};
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_formats::shaders_xr;
use volya_engine_formats::spawn;
use volya_engine_formats::ui::Resolution;

/// The spawn file's name: the first server parameter `all`
/// (level-loading.md section 2.1).
pub const SPAWN_NAME: &str = "all";
/// Files a level directory must hold to be a level (section 2.2).
pub const LEVEL_FILES: [&str; 4] = ["level", "level.ltx", "level.geom", "level.cform"];
/// `rs_vis_distance` of the shipped `user.ltx` (start-position.md 7.1); the
/// user settings are not read for it here (our choice).
pub const VIS_DISTANCE: f32 = 1.0;
/// Longest frame the sun's fade advances by, seconds (our choice).
const MAX_STEP: f32 = 0.1;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The static world shared between the level and the presenter.
#[derive(Debug)]
pub struct LevelWorld {
    /// The level directory name, e.g. `l01_escape`.
    pub name: String,
    /// The level directory in the game tree.
    pub dir: String,
    pub shaders: Vec<String>,
    pub draw_items: Vec<DrawItem>,
    /// The surfaces of the shader entries the draw items use, textures as
    /// file paths (static-shading.md sections 4.2, 7, 8).
    pub surfaces: Vec<Surface<String>>,
    /// The bounding sphere (centre, radius) of each draw item's visual.
    pub spheres: Vec<[f32; 4]>,
    /// The surface of each draw item; `None` where its entry is not drawn.
    pub draw_surfaces: Vec<Option<u32>>,
    /// Shader entries not drawn, with why.
    pub not_drawn: Vec<(String, String)>,
    /// The sky's textures as file paths, per hourly section of the weather
    /// cycle (sky.md sections 3.2, 5–7).
    pub sky: SkyTextures<String>,
    /// The lens-flare section of each hourly section, the sun sprite's
    /// texture as a file path (sky.md 7.2).
    pub flares: Vec<Flares>,
    /// The layout of `level.geom`'s vertex and index buffers, which stay
    /// in the file until the presenter uploads them.
    geometry: GeomFile,
    /// Each library effect's shader with located textures, for the
    /// effects the level's records, its zones and its weather's ambient
    /// effects can show (particle-drawing.md 5.1); `None` for the others and
    /// those not drawn.
    pub particle_shaders: Vec<Option<ParticleShader<String>>>,
    /// The most particles the level's effect objects can hold at once, for
    /// the renderer's buffers, which are sized for them with the world.
    pub particle_capacity: usize,
    /// The spawned objects drawn with the static world
    /// (docs/behavior/level-objects.md).
    pub objects: LevelObjects,
    /// The glows' shaders, textures as file paths (glows.md 6).
    pub glow_shaders: Vec<GlowShader<String>>,
    /// The light passes' attenuation textures as file paths: `att`,
    /// `attclip`, `attpoint` (dynamic-lights.md 6.4).
    pub light_textures: Option<[String; 3]>,
}

/// Every texture file of `surfaces` and `sky`, each once.
fn texture_paths(surfaces: &[Surface<String>], sky: Option<&SkyTextures<String>>) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let surfaces = surfaces
        .iter()
        .flat_map(|s| s.textures.iter().chain([&s.env]).flatten());
    let sky = sky.into_iter().flat_map(|sky| {
        sky.sections
            .iter()
            .flat_map(|s| [Some(&s.cube), Some(&s.clouds), s.sun.as_ref()])
            .flatten()
    });
    for path in surfaces.chain(sky) {
        if seen.insert(path.as_str()) {
            paths.push(path.clone());
        }
    }
    paths
}

/// Adds the glows' and the light passes' textures to `paths`, each once.
fn light_texture_paths(
    paths: &mut Vec<String>,
    glows: &[GlowShader<String>],
    lights: Option<&[String; 3]>,
) {
    let glows = glows.iter().map(|g| &g.texture);
    for t in glows.chain(lights.into_iter().flatten()) {
        if !paths.contains(t) {
            paths.push(t.clone());
        }
    }
}

impl LevelWorld {
    /// Every texture file the level uploads, each once.
    pub fn texture_paths(&self) -> Vec<String> {
        let mut paths = texture_paths(&self.surfaces, Some(&self.sky));
        for s in self.particle_shaders.iter().flatten() {
            for t in s.textures() {
                if !paths.contains(t) {
                    paths.push(t.clone());
                }
            }
        }
        light_texture_paths(&mut paths, &self.glow_shaders, self.light_textures.as_ref());
        paths
    }

    /// The layout of the static geometry's buffers, to be read from
    /// [`LevelWorld::geometry_path`].
    pub fn geometry(&self) -> &GeomFile {
        &self.geometry
    }

    /// The path of `level.geom` in the game tree.
    pub fn geometry_path(&self) -> String {
        format!("{}\\level.geom", self.dir)
    }
}

/// The level shown.
pub struct LevelScreen {
    world: Arc<LevelWorld>,
    /// Shared with the sound system, whose level mesh it is
    /// (sound-occlusion.md section 3).
    collision: Arc<CollisionWorld>,
    /// The level's sound data until the app creates its sounds.
    sound_data: Option<LevelSoundData>,
    /// A fatal error met while the level ran, until the app takes it.
    fatal: Option<String>,
    /// The player's footsteps (S11); `None` only in the unit tests' empty
    /// level.
    footsteps: Option<PlayerFootsteps>,
    /// The player's sky visibility (level-objects.md 5.4,
    /// sound-weather-ambient.md 4.3), for the sounds of the weather; the
    /// player's model's header sphere (ogf.md section 2), centre and radius
    /// in model space, its rays start from; the level's frame count and
    /// the latest frame's time (s), for its once-per-frame steps.
    sky_visibility: SkyVisibility,
    player_sphere: ([f32; 3], f32),
    frames: u64,
    frame_s: f32,
    player: Player,
    /// The player character's state (docs/behavior/player-state.md); `None`
    /// only in the unit tests' empty level.
    character: Option<CharacterState>,
    controls: Controls,
    /// The near plane of the current view (start-position.md section 7).
    near: f32,
    weather: Weather,
    /// The game clock (sav.md sections 5, 12).
    clock: GameClock,
    modifiers: Vec<EnvModifier>,
    environment: Environment,
    /// When the level was shown first (engine clock, ms), later by the
    /// time paused since.
    shown: u64,
    /// The previous update (engine clock, ms).
    last: u64,
    back: bool,
    /// The sun's fade (sky.md 7.3).
    sun_fade: SunFade,
    /// The offline simulation (offline-simulation.md); `None` only in the
    /// unit tests' empty level.
    simulation: Option<Simulation>,
    /// Where each of the objects' instances is drawn; `None` while it is
    /// not online. Shared with the scenes, replaced when it changes.
    placements: Arc<Vec<Option<Mat4>>>,
    /// Instance of each object id with a loaded visual.
    instances: HashMap<u16, usize>,
    /// The level's particle objects: the effects of `level.ps_static`
    /// (level-placed-effects.md), the zones' idle effects
    /// (zone-idle-effects.md) and the weather's ambient effect
    /// (weather-ambient-effects.md).
    effects: Option<PlacedEffects>,
    /// The zone objects (zone-idle-effects.md).
    zones: Zones,
    /// The zone sections read, by name.
    zone_sections: HashMap<String, ZoneSection>,
    /// The weather's ambient effect object, while it exists (P11 3).
    weather_effect: Option<usize>,
    /// The size of the latest frame drawn, for the effects' view test.
    view_size: std::cell::Cell<(u32, u32)>,
    /// Which effect objects are drawn, by origin (tests compare frames
    /// without them): placed, zones', weather's.
    effects_drawn: [bool; 3],
    /// The level's dynamic light records (docs/behavior/dynamic-lights.md):
    /// chunk 6's, the sun's and every source's; its glows and lamps.
    lights: LevelLights,
    /// The global clock (sound-weather-ambient.md 3.1), ms, as the app
    /// last gave it.
    global_ms: u64,
    /// The player's torches (docs/behavior/torch.md).
    torches: PlayerTorches,
    /// The player's detectors (docs/behavior/sound-detector.md).
    detectors: PlayerDetectors,
}

/// What the engine interface reports.
#[derive(Clone, Debug, Serialize)]
pub struct LevelState {
    pub name: String,
    pub draws: usize,
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub far_plane: f32,
    pub near_plane: f32,
    /// The game time, game milliseconds (sav.md section 12).
    pub game_time: u64,
    /// Its time of day, seconds.
    pub time_of_day: f32,
    /// The hourly blend weight t (static-shading.md 9.2).
    pub weather_t: f32,
    /// The bracketing hourly sections A and B.
    pub weather_sections: [String; 2],
    pub player: PlayerReport,
    /// The player character's state (player-state.md).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterReport>,
    pub objects: ObjectsReport,
    /// The lights, glows and lamps (docs/behavior/dynamic-lights.md, glows.md,
    /// hanging-lamps.md).
    pub lights: crate::level_lights::LightsReport,
    /// The player's attached torch (torch.md).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torch: Option<TorchReport>,
    /// The player's detectors (sound-detector.md).
    pub detectors: Vec<DetectorReport>,
}

/// The level's placed effects as the engine interface reports them.
#[derive(Clone, Debug, Default, Serialize)]
pub struct EffectsReport {
    /// The world clock they run on, ms.
    pub clock: u32,
    pub particles: usize,
    pub records: Vec<PlacedReport>,
}

/// One record's instance.
#[derive(Clone, Debug, Serialize)]
pub struct PlacedReport {
    pub name: String,
    pub group: bool,
    pub playing: bool,
    /// The latest scheduled period, ms.
    pub period: u32,
    pub updates: u64,
    /// Bounding sphere: centre and radius.
    pub sphere: [f32; 4],
    pub effects: Vec<EffectReport>,
}

/// One effect instance (a record's, a group member's or a child).
#[derive(Clone, Debug, Serialize)]
pub struct EffectReport {
    pub name: String,
    pub playing: bool,
    pub stopping: bool,
    pub position: [f32; 3],
    pub count: usize,
    /// The first particles: what drawing reads.
    pub particles: Vec<ParticleReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParticleReport {
    pub position: [f32; 3],
    pub size: [f32; 3],
    pub rotation: [f32; 3],
    /// A · 2²⁴ + R · 2¹⁶ + G · 2⁸ + B.
    pub colour: u32,
    pub age: f32,
    pub frame: u16,
}

/// Particles reported per effect at most.
const REPORTED_PARTICLES: usize = 16;

/// The zones and the weather's ambient effect as the engine interface
/// reports them.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ZonesReport {
    pub zones: Vec<ZoneReport>,
    /// The weather's ambient effect object, while it exists.
    pub weather: Option<WeatherEffectReport>,
}

/// One zone object (zone-idle-effects.md).
#[derive(Clone, Debug, Serialize)]
pub struct ZoneReport {
    pub id: u16,
    pub name: String,
    pub section: String,
    /// `idle`, `disabled`, …
    pub state: String,
    pub fast: bool,
    pub position: Vec3,
    /// The idle effect's name.
    pub effect: Option<String>,
    pub playing: bool,
    pub particles: usize,
    /// The idle sound's name.
    pub sound: Option<String>,
    pub sound_on: bool,
    /// The idle light is active.
    pub light: bool,
}

/// The weather's ambient effect object (weather-ambient-effects.md 3).
#[derive(Clone, Debug, Serialize)]
pub struct WeatherEffectReport {
    pub name: String,
    pub playing: bool,
    pub stopping: bool,
    pub sphere: [f32; 4],
    pub particles: usize,
}

/// The level's side of the ambient rule's effect object
/// (weather-ambient-effects.md 3): made among the level's particle objects,
/// placed once at the rule's position in world axes.
pub struct WeatherParticles<'a> {
    effects: &'a mut Option<PlacedEffects>,
    object: &'a mut Option<usize>,
}

impl AmbientParticles for WeatherParticles<'_> {
    fn start(&mut self, particles: &str, position: Vec3) -> Result<(), Fatal> {
        let Some(effects) = self.effects.as_mut() else {
            return Ok(());
        };
        let i = effects.add(particles, Origin::Weather).map_err(Fatal)?;
        // P11 3.2: rows 0 – 2 the identity, row 3 the position.
        let mut m = crate::core::particles::IDENTITY;
        m[3] = [position[0], position[1], position[2], 1.0];
        effects.start(i, &m, [0.0; 3]);
        *self.object = Some(i);
        Ok(())
    }

    fn stop(&mut self) {
        if let (Some(effects), Some(i)) = (self.effects.as_mut(), *self.object) {
            effects.stop_deferred(i);
        }
    }

    fn is_playing(&self) -> bool {
        match (self.effects.as_ref(), *self.object) {
            (Some(effects), Some(i)) => effects.get(i).is_some_and(|p| p.instance.is_playing()),
            _ => false,
        }
    }

    fn remove(&mut self) {
        if let (Some(effects), Some(i)) = (self.effects.as_mut(), self.object.take()) {
            effects.remove(i);
        }
    }
}

/// The report of `effects`' placed records (level-placed-effects.md).
pub fn effects_report(effects: Option<&PlacedEffects>) -> EffectsReport {
    let Some(effects) = effects else {
        return EffectsReport::default();
    };
    let library = effects.library();
    let records = effects
        .effects()
        .filter(|p| p.origin == Origin::Placed)
        .map(|p| {
            let s = p.instance.sphere();
            PlacedReport {
                name: p.name.clone(),
                group: matches!(
                    p.instance,
                    crate::core::particles::placed::Instance::Group(_)
                ),
                playing: p.instance.is_playing(),
                period: p.period(),
                updates: p.updates(),
                sphere: [s.centre[0], s.centre[1], s.centre[2], s.radius],
                effects: p
                    .instance
                    .effects()
                    .map(|e| EffectReport {
                        name: library.effects[e.record()].name.clone(),
                        playing: e.is_playing(),
                        stopping: e.is_stopping(),
                        position: e.position(),
                        count: e.particles().len(),
                        particles: (e.particles().iter().take(REPORTED_PARTICLES))
                            .map(|q| ParticleReport {
                                position: q.position,
                                size: q.size,
                                rotation: q.rotation,
                                colour: q.colour,
                                age: q.age,
                                frame: q.frame,
                            })
                            .collect(),
                    })
                    .collect(),
            }
        })
        .collect();
    EffectsReport {
        clock: effects.clock(),
        particles: (effects.effects())
            .filter(|p| p.origin == Origin::Placed)
            .flat_map(|p| p.instance.effects())
            .map(|e| e.particles().len())
            .sum(),
        records,
    }
}

/// The player character's state as the engine interface reports it
/// (player-state.md sections 2–4).
#[derive(Clone, Debug, Serialize)]
pub struct CharacterReport {
    pub health: f32,
    pub max_health: f32,
    pub power: f32,
    pub max_power: f32,
    pub satiety: f32,
    pub radiation: f32,
    pub psy_health: f32,
    pub morale: f32,
    pub alcohol: f32,
    pub limping: bool,
    pub cant_walk: bool,
    pub cant_sprint: bool,
    pub money: u32,
    pub specific_character: String,
    pub character_profile: String,
    pub community: i32,
    pub rank: i32,
    pub reputation: i32,
    pub character_name: String,
    /// The carried weight (3.4).
    pub weight: f32,
    /// `None` for no active slot.
    pub active_slot: Option<u8>,
    /// In the order the player lists them.
    pub items: Vec<ItemReport>,
}

/// One inventory item as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct ItemReport {
    pub id: u16,
    pub section: String,
    pub class_id: String,
    /// `slot`, `belt`, `backpack` or `none`.
    pub place: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,
    pub condition: f32,
}

impl CharacterReport {
    fn of(s: &CharacterState) -> Self {
        let c = &s.conditions;
        let p = &s.profile;
        Self {
            health: c.health,
            max_health: c.max_health,
            power: c.power,
            max_power: c.max_power,
            satiety: c.satiety,
            radiation: c.radiation,
            psy_health: c.psy_health,
            morale: c.morale,
            alcohol: c.alcohol,
            limping: c.limping,
            cant_walk: c.cant_walk,
            cant_sprint: c.cant_sprint,
            money: p.money,
            specific_character: p.specific_character.clone(),
            character_profile: p.character_profile.clone(),
            community: p.community,
            rank: p.rank,
            reputation: p.reputation,
            character_name: p.character_name.clone(),
            weight: s.inventory.weight(),
            active_slot: s.inventory.active_slot,
            items: s
                .inventory
                .items
                .iter()
                .map(|i| ItemReport {
                    id: i.id,
                    section: i.config.section.clone(),
                    class_id: i.config.class_id.as_str().to_owned(),
                    place: match i.place {
                        Place::None => "none",
                        Place::Slot(_) => "slot",
                        Place::Belt => "belt",
                        Place::Backpack => "backpack",
                    },
                    slot: match i.place {
                        Place::Slot(s) => Some(s),
                        _ => None,
                    },
                    condition: i.condition,
                })
                .collect(),
        }
    }
}

/// The player's movement as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct PlayerReport {
    /// Feet.
    pub position: Vec3,
    pub velocity: Vec3,
    pub airborne: bool,
    /// The collision box: 0 standing, 1 crouch, 2 low crouch.
    pub posture: usize,
    pub moving: bool,
    pub running: bool,
    pub crouch: bool,
    pub sprint: bool,
    pub jump: bool,
    pub falling: bool,
    pub landing: bool,
    /// On a ladder (docs/behavior/climbing.md 3.6).
    pub climbing: bool,
    /// The toggles the touch controls show.
    pub crouch_toggled: bool,
    pub sprint_requested: bool,
    /// The touch stick's sprint lock holds it forward.
    pub sprint_locked: bool,
}

/// The level of a new game and where its actor starts: the actor's object
/// in the spawn file names a game-graph vertex, whose level the game
/// graph's level table names (level-loading.md section 2.2).
pub fn new_game_level(tree: &dyn Vfs, data: &GameData) -> io::Result<(String, spawn::ActorStart)> {
    read_new_game(tree, data).map(|n| (n.level, n.actor))
}

/// What creating a New Game reads: the level, the actor's start, and the
/// spawn file and game graph the objects' switch pass reads again.
struct NewGame {
    level: String,
    actor: spawn::ActorStart,
    spawn_file: Vec<u8>,
    graph: GameGraph,
}

fn read_new_game(tree: &dyn Vfs, data: &GameData) -> io::Result<NewGame> {
    let spawn_path = data
        .aliases
        .path("$game_spawn$", &format!("{SPAWN_NAME}.spawn"))?;
    let spawn_file = vfs::read(tree, &spawn_path)
        .map_err(|e| io::Error::new(e.kind(), format!("Can't find spawn file: {spawn_path}")))?;
    let actor = spawn::actor_start(&spawn_file)?;
    let graph = GameGraph::parse(&vfs::read(
        tree,
        &data.aliases.path("$game_data$", "game.graph")?,
    )?)?;
    let level = graph.vertex_level(actor.game_graph_vertex)?.name.clone();
    Ok(NewGame {
        level,
        actor,
        spawn_file,
        graph,
    })
}

/// The weather name of `level` (`[<level>] weathers` of `game.ltx`'s
/// includes), level-loading.md section 2.6. The weather script is replaced
/// by its result.
fn start_weather(tree: &dyn Vfs, data: &GameData, level: &str) -> io::Result<String> {
    let game = config::load(tree, &data.aliases.path("$game_config$", "game.ltx")?)?;
    Ok(game
        .get(level, "weathers")
        .flatten()
        // TODO(#330): level-loading.md 2.6 has the weather script read the
        // key, but not whether it checks for it first; `default` without
        // it is our choice.
        .unwrap_or("default")
        .to_owned())
}

/// The supply's generator seed (player-state.md 3.1: the game seeds it
/// from a timer and any 15-bit generator serves; a fixed seed is our
/// choice).
const SUPPLY_SEED: u32 = 0;

/// The player's record in `spawn_file`: object `index`, whose class must be
/// the player's (level-objects.md: index 63, the only `O_ACTOR` record).
pub fn player_record(spawn_file: &[u8], index: u32, system: &Config) -> io::Result<ObjectRecord> {
    let mut found = None;
    spawn::for_each_record(spawn_file, |r| {
        if r.index != index {
            return Ok(std::ops::ControlFlow::Continue(()));
        }
        let section = record::section(r.spawn)?;
        let class = system
            .get(&section, "class")
            .flatten()
            .map(str::trim)
            .ok_or_else(|| invalid(format!("section [{section}] has no class")))?;
        found = Some(ObjectRecord::parse(r.spawn, r.update, class, true)?);
        Ok(std::ops::ControlFlow::Break(()))
    })?;
    let found = found.ok_or_else(|| invalid(format!("spawn file: no object {index}")))?;
    if found.kind() != record::Kind::Player {
        return Err(invalid(format!(
            "{}: {} is not the player's class",
            found.header.name,
            found.class_id.as_str()
        )));
    }
    Ok(found)
}

/// The player character at a New Game (player-state.md sections 2, 3)
/// from its record `player`: the profile's specific character, the
/// communities of `[game_relations]` (`config\creatures\game_relations.ltx`
/// unless the system configuration holds the section) and the name's
/// translation. Its items take ids from `first_id` on
/// (offline-simulation.md 8.1).
pub fn new_game_character(
    tree: &dyn Vfs,
    data: &GameData,
    player: &ObjectRecord,
    first_id: u16,
) -> io::Result<CharacterState> {
    let record::Object::Player(p) = &player.object else {
        return Err(invalid(format!("{}: not the player", player.header.name)));
    };
    let mut characters = Characters::new(tree, data);
    let specific = characters
        .specific_character(&p.trader)?
        .cloned()
        .unwrap_or_else(|| SpecificCharacter {
            // Our choice: a profile without a description supplies nothing
            // and fills nothing.
            id: p.trader.specific_character.clone(),
            rank: character::NOT_SET,
            reputation: character::NOT_SET,
            ..SpecificCharacter::default()
        });
    let relations = match data.system.section("game_relations") {
        Some(_) => data.system.clone(),
        None => config::load(
            tree,
            &data
                .aliases
                .path("$game_config$", "creatures\\game_relations.ltx")?,
        )?,
    };
    let communities = character::communities(
        relations
            .get("game_relations", "communities")
            .flatten()
            .unwrap_or(""),
    );
    CharacterState::new_game(&character::NewGame {
        record: player,
        system: &data.system,
        character: &specific,
        communities: &communities,
        character_name: data.strings.get(&specific.name),
        seed: SUPPLY_SEED,
        first_id,
    })
}

/// The New Game's player record and character, read as a New Game reads
/// them (tests and debugging).
pub fn new_game_player(
    tree: &dyn Vfs,
    data: &GameData,
) -> io::Result<(ObjectRecord, CharacterState)> {
    let new = read_new_game(tree, data)?;
    let player = player_record(&new.spawn_file, new.actor.index, &data.system)?;
    let dir = data.aliases.path("$game_levels$", &new.level)?;
    let clock = GameClock::new_game(&*alife(tree, data)?)?;
    let (_, character) = new_game_simulation(tree, data, new.spawn_file, new.graph, &dir, &clock)?;
    Ok((player, character))
}

/// `config\alife.ltx`, or the system configuration where it holds
/// `[alife]` already.
fn alife<'a>(tree: &dyn Vfs, data: &'a GameData) -> io::Result<Cow<'a, Config>> {
    match data.system.section("alife") {
        Some(_) => Ok(Cow::Borrowed(&data.system)),
        None => Ok(Cow::Owned(config::load(
            tree,
            &data.aliases.path("$game_config$", "alife.ltx")?,
        )?)),
    }
}

/// A New Game's offline simulation (offline-simulation.md) and player
/// character: every record of `spawn_file` created and registered (8.1),
/// the player character supplying the player (player-state.md sections 2,
/// 3), the spawn file then dropped, the current level's navigation data
/// read from its directory `dir` (6.1), and the first switch pass at
/// `clock`'s time (6.2, level-objects.md 2.4: all objects evaluated once
/// while the loading image shows), which brings the player online: it then
/// receives its bolt (player-state.md 3.2) with the next id.
fn new_game_simulation(
    tree: &dyn Vfs,
    data: &GameData,
    spawn_file: Vec<u8>,
    graph: GameGraph,
    dir: &str,
    clock: &GameClock,
) -> io::Result<(Simulation, CharacterState)> {
    let alife = alife(tree, data)?;
    let paths = PatrolPaths::parse(&spawn_file)?;
    let mut sim = Simulation::new(
        Graph::new(graph),
        paths,
        AlifeConfig::read(&alife, &data.system)?,
    );
    let mut character = None;
    sim.new_game(&spawn_file, &data.system, &mut |player, first_id| {
        let c = new_game_character(tree, data, player, first_id)?;
        let n = u16::try_from(c.item_count()).map_err(|_| invalid("too many items"))?;
        character = Some(c);
        Ok(n)
    })?;
    drop(spawn_file);
    let mut character = earlier(character, "player character")?;
    let level = sim
        .current_level()
        .ok_or_else(|| invalid("spawn file: no actor"))?;
    sim.set_navigation(Navigation::read(tree, dir, &sim.graph.graph, level)?)?;
    sim.update(
        clock.time(),
        clock.game_time().normal_time_factor,
        hour(clock),
    )?;
    character.receive_bolt(&data.system, sim.take_id()?)?;
    Ok((sim, character))
}

/// The file of texture `name`: `$level$<name>.dds`, then
/// `$game_textures$<name>.dds` (the saved-games folder has no place on this
/// platform); neither is "Can't find texture" (static-shading.md 4.2).
fn locate_texture(tree: &dyn Vfs, data: &GameData, dir: &str, name: &str) -> io::Result<String> {
    let level = format!("{dir}\\{name}.dds");
    if tree.exists(&level) {
        return Ok(level);
    }
    let path = data
        .aliases
        .path("$game_textures$", &format!("{name}.dds"))?;
    if !tree.exists(&path) {
        return Err(invalid(format!("Can't find texture '{name}'")));
    }
    Ok(path)
}

/// The shaders of the effects the level's particle objects and the effects
/// or groups `names` can show (the
/// records' effects, their groups' member effects and the children their
/// member flags use; particle-drawing.md 5.1), by library index, with
/// their textures located by `locate` (a texture found nowhere fails the
/// load, as the level's do); and the effects not drawn, with why.
#[allow(clippy::type_complexity)]
fn particle_shaders(
    tree: &dyn Vfs,
    data: &GameData,
    definitions: &HashMap<String, shaders_xr::Definition>,
    effects: &PlacedEffects,
    names: &[&str],
    locate: &mut dyn FnMut(&str) -> io::Result<String>,
) -> io::Result<(Vec<Option<ParticleShader<String>>>, Vec<(String, String)>)> {
    use volya_engine_formats::particles::member_flags as m;
    let library = effects.library();
    let mut wanted = vec![false; library.effects.len()];
    let mut want = |name: &str| {
        if let Some(i) = library.effect(name) {
            wanted[i] = true;
        }
    };
    let objects = effects.effects().map(|p| p.name.as_str());
    for name in objects.chain(names.iter().copied()) {
        if library.effect(name).is_some() {
            want(name);
        } else if let Some(g) = library.group(name) {
            for member in &library.groups[g].members {
                want(&member.effect);
                for (bit, child) in [
                    (m::PER_PARTICLE_CHILD, &member.per_particle_child),
                    (m::BIRTH_CHILD, &member.birth_child),
                    (m::DEATH_CHILD, &member.death_child),
                ] {
                    if member.flags & bit != 0 {
                        want(child);
                    }
                }
            }
        }
    }
    let shader_dir = data.aliases.path("$game_shaders$", "r1")?;
    let scripted = |module: &str| {
        vfs::read(tree, &format!("{shader_dir}\\{module}.s"))
            .is_ok_and(|text| shading::defines_normal_or_special(&String::from_utf8_lossy(&text)))
    };
    let mut shaders = vec![None; library.effects.len()];
    let mut not_drawn = Vec::new();
    for (i, effect) in library.effects.iter().enumerate() {
        let Some(sprite) = effect.sprite.as_ref().filter(|_| wanted[i]) else {
            continue;
        };
        match particle_drawing::resolve(definitions, &scripted, &sprite.shader, &sprite.texture) {
            Ok(shader) => {
                let mut paths = HashMap::new();
                for t in shader.textures() {
                    paths.insert(t.clone(), locate(t)?);
                }
                shaders[i] = Some(shader.map(|t| paths.get(&t).cloned()));
            }
            Err(why) => not_drawn.push((effect.name.clone(), why)),
        }
    }
    Ok((shaders, not_drawn))
}

/// Every surface the level draws: the static world's and the objects'.
struct Resolved {
    surfaces: Vec<Surface<String>>,
    /// The surface of each draw item; `None` where its entry is not drawn.
    draw_surfaces: Vec<Option<u32>>,
    /// Static shader entries not drawn, with why.
    not_drawn: Vec<(String, String)>,
    /// The surface of each object entry (`<shader>/<texture>`).
    object_surfaces: Vec<Option<u32>>,
    /// Object entries not drawn, with why.
    objects_not_drawn: Vec<(String, String)>,
    /// `shaders.xr`'s definitions, by name.
    definitions: HashMap<String, shaders_xr::Definition>,
}

/// Every entry of the shader table is created at load, in table order,
/// whether or not a draw uses it (level.md 3): what is fatal for one fails
/// the load (static-shading.md 3.4, 4.2, 7), and each texture of a surface
/// is located.
fn create_table(
    library: &ShaderLibrary,
    shaders: &[String],
    locate: &mut dyn FnMut(String) -> io::Result<String>,
) -> io::Result<()> {
    for entry in shaders.iter().filter(|e| !e.is_empty()) {
        match library.resolve(entry) {
            Err(shading::NotDrawn::Fatal(why)) => {
                return Err(invalid(format!("shader {entry:?}: {why}")));
            }
            Ok(surface) => {
                for t in surface.textures.into_iter().flatten().chain(surface.env) {
                    locate(t)?;
                }
            }
            // TODO(#332): which textures a class this engine
            // does not draw loads, and so whether they exist, is unchecked.
            Err(shading::NotDrawn::Unsupported(_)) => {}
        }
    }
    Ok(())
}

/// The surfaces of the shader entries `draw_items` use and of the objects'
/// entries `object_entries` (level-objects.md 5.2: a model mesh's pair
/// resolves like an entry of the shader table; only the model programs
/// draw them). A texture that resolves nowhere fails the load ("Can't find
/// texture", static-shading.md 4.2).
fn resolve_surfaces(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    shaders: &[String],
    draw_items: &[DrawItem],
    object_entries: &[String],
) -> io::Result<Resolved> {
    let aliases = &data.aliases;
    let definitions: HashMap<String, shaders_xr::Definition> = shaders_xr::parse(&vfs::read(
        tree,
        &aliases.path("$game_data$", "shaders.xr")?,
    )?)?
    .into_iter()
    .map(|d| (d.name.clone(), d))
    .collect();
    let shader_dir = aliases.path("$game_shaders$", "r1")?;
    let scripted = |module: &str| {
        vfs::read(tree, &format!("{shader_dir}\\{module}.s"))
            .is_ok_and(|text| shading::defines_normal_or_special(&String::from_utf8_lossy(&text)))
    };
    let textures_ltx = config::load(tree, &aliases.path("$game_textures$", "textures.ltx")?)?;
    let associations = shading::associations(&textures_ltx);
    let library = ShaderLibrary {
        definitions: &definitions,
        scripted: &scripted,
        associations: &associations,
    };
    let mut paths: HashMap<String, String> = HashMap::new();
    let mut locate = |name: String| -> io::Result<String> {
        if let Some(p) = paths.get(&name) {
            return Ok(p.clone());
        }
        let path = locate_texture(tree, data, dir, &name)?;
        paths.insert(name, path.clone());
        Ok(path)
    };
    create_table(&library, shaders, &mut locate)?;
    let mut surfaces = Vec::new();
    let mut by_entry: HashMap<String, Result<u32, String>> = HashMap::new();
    let mut resolve = |entry: &str, model: bool| -> io::Result<Result<u32, String>> {
        if let Some(s) = by_entry.get(entry) {
            return Ok(s.clone());
        }
        let surface = match library.resolve(entry) {
            // static-shading.md 3.4, 7.1, 7.4: the level does not load.
            Err(shading::NotDrawn::Fatal(why)) => {
                return Err(invalid(format!("shader {entry:?}: {why}")));
            }
            Ok(surface) if surface.program.is_model() != model => Err(if model {
                "not a model shader".to_owned()
            } else {
                "a model shader".to_owned()
            }),
            Ok(surface) => {
                let [a, b, c, d] = surface.textures.clone();
                let env = surface.env.clone();
                let mut resolved = surface.map_textures(|_| None::<String>);
                resolved.textures = [
                    a.map(&mut locate).transpose()?,
                    b.map(&mut locate).transpose()?,
                    c.map(&mut locate).transpose()?,
                    d.map(&mut locate).transpose()?,
                ];
                resolved.env = env.map(&mut locate).transpose()?;
                surfaces.push(resolved);
                Ok(surfaces.len() as u32 - 1)
            }
            Err(why) => Err(why.to_string()),
        };
        by_entry.insert(entry.to_owned(), surface.clone());
        Ok(surface)
    };
    let mut not_drawn = Vec::new();
    let mut draw_surfaces = Vec::with_capacity(draw_items.len());
    let mut seen: HashMap<u16, Option<u32>> = HashMap::new();
    for item in draw_items {
        if let Some(s) = seen.get(&item.shader) {
            draw_surfaces.push(*s);
            continue;
        }
        let entry = shaders
            .get(usize::from(item.shader))
            .map_or("", String::as_str);
        let surface = match resolve(entry, false)? {
            Ok(s) => Some(s),
            Err(why) => {
                not_drawn.push((entry.to_owned(), why));
                None
            }
        };
        seen.insert(item.shader, surface);
        draw_surfaces.push(surface);
    }
    let mut objects_not_drawn = Vec::new();
    let mut object_surfaces = Vec::with_capacity(object_entries.len());
    for entry in object_entries {
        object_surfaces.push(match resolve(entry, true)? {
            Ok(s) => Some(s),
            Err(why) => {
                objects_not_drawn.push((entry.clone(), why));
                None
            }
        });
    }
    Ok(Resolved {
        surfaces,
        draw_surfaces,
        not_drawn,
        object_surfaces,
        objects_not_drawn,
        definitions,
    })
}

/// A new game's level being loaded stage by stage
/// (docs/behavior/loading-screen.md section 5): [`NewGameLoad::run`] does
/// the part of each stage's work this engine has, in the stages' order;
/// [`NewGameLoad::finish`] makes the level. Stages whose systems do not
/// exist here (server, life simulation, details, AI) do nothing; the
/// level's objects (docs/behavior/level-objects.md) are switched online,
/// spawned with their models, given surfaces and light inputs in the
/// stages named after those steps (placed there by us).
#[derive(Default)]
pub struct NewGameLoad {
    name: Option<String>,
    actor: Option<spawn::ActorStart>,
    dir: String,
    level: Option<Level>,
    geom: Option<GeomFile>,
    collision: Option<Arc<CollisionWorld>>,
    draw_items: Vec<DrawItem>,
    spheres: Vec<[f32; 4]>,
    modifiers: Vec<EnvModifier>,
    movement: Option<MovementConfig>,
    controls: Option<Controls>,
    weather: Option<Weather>,
    clock: Option<GameClock>,
    character: Option<CharacterState>,
    surfaces: Vec<Surface<String>>,
    draw_surfaces: Vec<Option<u32>>,
    not_drawn: Vec<(String, String)>,
    sky: Option<SkyTextures<String>>,
    flares: Vec<Flares>,
    /// The spawn file and game graph, from creating the game to the
    /// switch pass.
    spawn_file: Vec<u8>,
    graph: Option<GameGraph>,
    simulation: Option<Simulation>,
    /// The objects whose visuals the level loads.
    level_objects: Vec<objects::LevelObject>,
    spawned: Option<objects::Spawned>,
    /// Per model, per mesh: its surface.
    mesh_surfaces: Vec<Vec<Option<u32>>>,
    objects_not_drawn: Vec<(String, String)>,
    dynamic_props: Vec<[f32; 4]>,
    sound: Option<LevelSoundData>,
    /// The engine clock of the frame a stage runs in (ms).
    now: u64,
    particle_shaders: Vec<Option<ParticleShader<String>>>,
    particle_capacity: usize,
    /// The level's particle objects and the engine clock their clock was
    /// last brought to.
    effects: Option<(PlacedEffects, u64)>,
    /// The zone objects online at the start and the sections of every zone
    /// of the level's objects.
    zones: Zones,
    zone_sections: HashMap<String, ZoneSection>,
    /// The section of each zone record of the level's objects.
    zone_record_sections: Vec<String>,
    /// The material pairs, read with the materials (S9).
    pairs: Option<Arc<MaterialPairs>>,
    /// The player's visual, from its record.
    player_visual: Option<String>,
    /// Its model's header sphere (sound-weather-ambient.md 4.3).
    player_sphere: Option<([f32; 3], f32)>,
    footsteps: Option<PlayerFootsteps>,
    /// The level's lights, glows and their shaders
    /// (docs/behavior/dynamic-lights.md, glows.md), and chunk 7's records
    /// until their shaders are resolved.
    lights: LightsData,
    glow_records: Vec<volya_engine_formats::level::Glow>,
    /// The light passes' attenuation textures (dynamic-lights.md 6.4).
    light_textures: Option<[String; 3]>,
    /// The player's torches' data (torch.md 1, 3).
    torches: Option<PlayerTorchData>,
    /// The player's detectors' sections (sound-detector.md 2.1).
    detectors: Option<Vec<DetectorSection>>,
}

/// The zone objects of `level_objects` (zone-idle-effects.md 1.1: the
/// records of the class ids its behaviours cover), each with its section
/// read from `system` once (`sections`, by name): fatal where a required
/// key is missing.
fn zone_objects<'a>(
    level_objects: &'a [objects::LevelObject],
    system: &Config,
    sections: &mut HashMap<String, ZoneSection>,
) -> io::Result<Vec<(&'a objects::LevelObject, ZoneRecord)>> {
    let mut out = Vec::new();
    for o in level_objects {
        let Some(behaviour) = Behaviour::of(o.record.class_id.as_str()) else {
            continue;
        };
        let Some(record) = ZoneRecord::of(o.id, &o.record) else {
            continue;
        };
        let name = &o.record.header.section;
        if !sections.contains_key(name) {
            let section = system
                .section(name)
                .ok_or_else(|| invalid(format!("no section [{name}]")))?;
            sections.insert(name.clone(), ZoneSection::read(section, behaviour)?);
        }
        out.push((o, record));
    }
    Ok(out)
}

/// The game hour of `clock`, 0 … 23 (the scripts' switch rules,
/// level-objects.md 2.5).
fn hour(clock: &GameClock) -> u32 {
    (clock.time_of_day() / 3600.0) as u32
}

/// A geometry source that remembers whether a read failed.
struct ReadFailures<S> {
    source: S,
    failed: bool,
}

impl<S: GeomSource> GeomSource for ReadFailures<S> {
    fn layout(&self) -> &GeomLayout {
        self.source.layout()
    }

    fn read(&mut self, buffer: GeomBuffer, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let result = self.source.read(buffer, offset, out);
        self.failed |= result.is_err();
        result
    }
}

/// What a stage needs from an earlier one, or an error naming it.
fn earlier<T>(value: Option<T>, what: &str) -> io::Result<T> {
    value.ok_or_else(|| invalid(format!("loading stages out of order: no {what}")))
}

impl NewGameLoad {
    /// The level directory name, once the new game is created.
    pub fn level_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Every texture file the level uploads (its surfaces', its objects'
    /// and the sky's), each once, once `st_loading_textures` has run.
    pub fn texture_paths(&self) -> Vec<String> {
        let mut paths = texture_paths(&self.surfaces, self.sky.as_ref());
        for s in self.particle_shaders.iter().flatten() {
            for t in s.textures() {
                if !paths.contains(t) {
                    paths.push(t.clone());
                }
            }
        }
        light_texture_paths(
            &mut paths,
            &self.lights.glow_shaders,
            self.light_textures.as_ref(),
        );
        paths
    }

    /// The engine clock of the frame the next stage runs in.
    pub fn set_clock(&mut self, now: u64) {
        self.now = now;
    }

    /// Does the work of `stage`.
    pub fn run(&mut self, stage: Stage, tree: &dyn Vfs, data: &GameData) -> io::Result<()> {
        let dir = self.dir.clone();
        let file =
            |name: &str, e: io::Error| io::Error::new(e.kind(), format!("{dir}\\{name}: {e}"));
        match stage {
            // The level and its start (level-loading.md section 2.2).
            Stage::CreatingNewGame => {
                let new = read_new_game(tree, data)?;
                self.dir = data.aliases.path("$game_levels$", &new.level)?;
                self.name = Some(new.level);
                self.actor = Some(new.actor);
                self.spawn_file = new.spawn_file;
                self.graph = Some(new.graph);
            }
            // The New Game's clock (sav.md section 5); the offline
            // simulation's New Game (offline-simulation.md 8.1), the player
            // character supplying the player (player-state.md sections 2,
            // 3), the current level's navigation data (6.1); and its first
            // switch pass (6.2, level-objects.md 2.4: all objects
            // evaluated once while the loading image shows), placed under
            // this caption by us (the server has created the game).
            Stage::ServerConnecting => {
                let clock = GameClock::new_game(&*alife(tree, data)?)?;
                let graph = earlier(self.graph.take(), "game graph")?;
                let spawn_file = std::mem::take(&mut self.spawn_file);
                let (sim, character) =
                    new_game_simulation(tree, data, spawn_file, graph, &dir, &clock)?;
                self.player_visual = sim
                    .object(character.owner.id)
                    .and_then(|o| o.record.object.visual())
                    .map(|v| v.model.clone());
                self.character = Some(character);
                self.level_objects = objects::level_objects(&sim);
                self.simulation = Some(sim);
                self.clock = Some(clock);
            }
            // `level.ltx` is read under this caption; a level's files must
            // all exist (level-loading.md section 2.2).
            Stage::ClientConnectingTo => {
                for name in LEVEL_FILES {
                    tree.open(&format!("{dir}\\{name}"))
                        .map_err(|e| file(name, e))?;
                }
            }
            Stage::OpeningStream => {
                let bytes = vfs::read(tree, &format!("{dir}\\level"))?;
                let level = Level::parse(&bytes).map_err(|e| file("level", e))?;
                // Chunks 6 and 7 and the portals, for the lights
                // (dynamic-lights.md 3.1, glows.md 2.1, level.md 9.1).
                self.lights.level_lights = (level.lights.iter())
                    .filter_map(level_lights::level_light)
                    .collect();
                self.lights.portals = (level.portals.iter())
                    .flat_map(|p| level_lights::portal_triangles(&p.vertices))
                    .collect();
                self.glow_records = level.glows.clone();
                self.level = Some(level);
            }
            // The collision mesh with its materials mapped and indexed
            // (level-cform.md sections 2, 3.3). `gamemtl.xr` is read here,
            // not at game start as the game does (our choice; only the
            // level needs it), with its material pairs (S9). Without it
            // there are no materials, so the mapping fails as the game's
            // does (section 1).
            Stage::LoadingCform => {
                let path = data.aliases.path("$game_data$", "gamemtl.xr")?;
                let at = |e: io::Error| io::Error::new(e.kind(), format!("{path}: {e}"));
                let (materials, pairs) = match vfs::read(tree, &path) {
                    Ok(bytes) => {
                        let materials = gamemtl::parse(&bytes).map_err(at)?;
                        let pairs = MaterialPairs::read(&bytes, &materials).map_err(at)?;
                        (materials, pairs)
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        (Vec::new(), MaterialPairs::empty())
                    }
                    Err(e) => return Err(e),
                };
                self.pairs = Some(Arc::new(pairs));
                // Read as a stream into the world's own form, so the file
                // is never held whole.
                let f = tree
                    .open(&format!("{dir}\\level.cform"))
                    .map_err(|e| file("level.cform", e))?;
                self.collision = Some(Arc::new(
                    CollisionWorld::read(f, materials).map_err(|e| file("level.cform", e))?,
                ));
            }
            Stage::LoadingGeometry => {
                let mut f = tree
                    .open(&format!("{dir}\\level.geom"))
                    .map_err(|e| file("level.geom", e))?;
                // The buffers' layout only: the presenter reads the
                // buffers themselves as it uploads them.
                self.geom = Some(GeomFile::read(&mut f).map_err(|e| file("level.geom", e))?);
            }
            // The static visuals' draw list and bounding spheres (placed
            // under this caption by us: the spec names the spatial index).
            Stage::LoadingSpatialDb => {
                let level = earlier(self.level.as_ref(), "level")?;
                let geom = earlier(self.geom.as_ref(), "level.geom")?;
                // The draws' indices are checked as they are read from the
                // file, a piece at a time.
                let mut f = tree
                    .open(&format!("{dir}\\level.geom"))
                    .map_err(|e| file("level.geom", e))?;
                let mut indices = ReadFailures {
                    source: geom.reader(&mut f),
                    failed: false,
                };
                let draw_items = level.static_draw_items_from(&mut indices).map_err(|e| {
                    // A read that failed is the file's; anything else the
                    // level's visuals.
                    file(
                        if indices.failed {
                            "level.geom"
                        } else {
                            "level"
                        },
                        e,
                    )
                })?;
                self.spheres = draw_items
                    .iter()
                    .map(|d| {
                        let h = &level.visuals[d.visual as usize].header;
                        let c = h.sphere_centre;
                        [c[0], c[1], c[2], h.sphere_radius]
                    })
                    .collect();
                self.draw_items = draw_items;
                // Only the shader table is read after this stage; the
                // visuals and the rest go now.
                if let Some(level) = &mut self.level {
                    *level = Level {
                        header: level.header,
                        shaders: std::mem::take(&mut level.shaders),
                        visuals: Vec::new(),
                        portals: Vec::new(),
                        sectors: Vec::new(),
                        lights: Vec::new(),
                        glows: Vec::new(),
                    };
                }
            }
            // `level.env_mod` comes after the sectors (optional, sky.md 9.1).
            Stage::LoadingSectorsPortals => {
                self.modifiers = match vfs::read(tree, &format!("{dir}\\level.env_mod")) {
                    Ok(file) => env_mod::parse(&file)?,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
                    Err(e) => return Err(e),
                };
            }
            // The actor, the start weather (level-loading.md 2.6) and the
            // objects' models (level-objects.md sections 3, 4), placed under
            // this caption by us (the client creates the objects here).
            Stage::ClientSpawning => {
                let name = earlier(self.name.as_deref(), "level")?;
                // player-movement.md section 1: the actor's section is the
                // one its spawn record names.
                self.movement = Some(MovementConfig::read(&data.system, spawn::ACTOR_SECTION)?);
                self.controls = Some(data.controls.clone());
                let weather_name = start_weather(tree, data, name)?;
                let weather = Weather::load(&data.system, &weather_name)?;
                // The level's sound data (docs/behavior/level-sound-scope.md),
                // placed under this caption by us, with the weather it needs.
                // Its files are checked now, with or without audio output
                // (S1 section 5: fatal at level load).
                let mut sound = LevelSoundData::read(tree, data, &dir, name, &weather)?;
                // The player's footsteps (S11) on its cycles (S10), placed
                // under this caption by us, with the player's movement.
                let pairs = earlier(self.pairs.clone(), "gamemtl.xr")?;
                let collision = earlier(self.collision.as_deref(), "level.cform")?;
                let visual = earlier(self.player_visual.as_deref(), "player's visual")?;
                let cycles = footsteps::read_cycles(tree, data, &dir, visual)?;
                // The sky rays' origin (sound-weather-ambient.md 4.3): the
                // player's model's header, read with its skeleton as the
                // torches read it.
                let path = objects::locate_model(tree, data, &dir, visual)?;
                let (header, _) = volya_engine_formats::model::skeleton(&vfs::read(tree, &path)?)
                    .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?;
                self.player_sphere = Some((header.sphere_centre, header.sphere_radius));
                self.footsteps = Some(PlayerFootsteps::new(
                    data,
                    spawn::ACTOR_SECTION,
                    collision.materials(),
                    pairs.clone(),
                    cycles,
                )?);
                sound.pairs = Some(pairs);
                // The player's torches (torch.md 1.2, 3), placed under this
                // caption by us with the objects' models; their night-vision
                // sounds are loaded with their sections (14).
                let character = earlier(self.character.as_ref(), "player character")?;
                let torches =
                    crate::torch::read_player_torches(tree, data, &dir, character, visual)?;
                sound.torches = (character.inventory.items.iter())
                    .filter_map(|i| {
                        let t = torches
                            .torches
                            .iter()
                            .find(|t| t.section == i.config.section)?;
                        Some((i.id, t.item.night_vision.clone()?))
                    })
                    .collect();
                self.torches = Some(torches);
                // The player's detectors' sections (sound-detector.md 2.1),
                // read as their items are made; their sound lists are loaded
                // with them (2.2).
                let detectors = crate::detector::read_sections(&data.system, character)?;
                sound.detectors = (character.inventory.items.iter())
                    .filter_map(|i| {
                        let s = detectors.iter().find(|s| s.name == i.config.section)?;
                        crate::detector::is_detector(i.config.class_id.as_str())
                            .then(|| (i.id, s.clone()))
                    })
                    .collect();
                self.detectors = Some(detectors);
                // The zones online at the start are created as the client
                // creates its objects (zone-idle-effects.md 3.1), on the
                // effects' clock brought to this frame; their sounds are
                // the level's (S15 2.1).
                let (effects, at) = earlier(self.effects.as_mut(), "particles.xr")?;
                effects.advance(self.now.saturating_sub(*at));
                *at = self.now;
                for (o, record) in
                    zone_objects(&self.level_objects, &data.system, &mut self.zone_sections)?
                {
                    self.zone_record_sections
                        .push(o.record.header.section.clone());
                    if o.online {
                        let section = self.zone_sections[&o.record.header.section].clone();
                        self.zones
                            .create(&record, section, effects.clock(), effects)
                            .map_err(invalid)?;
                    }
                }
                sound.zones = (self.zones.zones().iter())
                    .map(|z| z.section.sounds.clone())
                    .collect();
                sound.check(tree, &data.aliases)?;
                self.sound = Some(sound);
                self.weather = Some(weather);
                let level_objects = std::mem::take(&mut self.level_objects);
                // docs/behavior/climbing.md 1.1 – 1.3: every ladder of the
                // level is present from the first frame, its box in the
                // collision world.
                let ladders = (level_objects.iter())
                    .filter_map(|o| Ladder::from_record(&o.record))
                    .collect();
                earlier(self.collision.as_mut(), "level.cform")?.set_ladders(ladders)?;
                self.spawned = Some(objects::spawn(tree, data, &dir, &level_objects)?);
                // The lamps' colour animations (docs/formats/lanims-xr.md),
                // read here with the lamps (our choice: when the game reads
                // the file is unestablished); without the file, none.
                let path = data.aliases.path("$game_data$", "lanims.xr")?;
                self.lights.animations = match vfs::read(tree, &path) {
                    Ok(bytes) => ColourAnimations::parse(&bytes)
                        .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => ColourAnimations::default(),
                    Err(e) => return Err(e),
                };
            }
            // Every texture file the level needs is located here. The
            // presenter reads and uploads them with the first frame that
            // draws the world, the first under the loading image: our
            // departure from section 5, which reads them under this caption.
            Stage::LoadingTextures => {
                let level = earlier(self.level.as_ref(), "level")?;
                let spawned = earlier(self.spawned.as_ref(), "objects")?;
                let entries: Vec<String> = spawned
                    .models
                    .iter()
                    .flat_map(|m| &m.meshes)
                    .map(|m| format!("{}/{}", m.shader, m.texture))
                    .collect();
                let resolved =
                    resolve_surfaces(tree, data, &dir, &level.shaders, &self.draw_items, &entries)?;
                let mut object_surfaces = resolved.object_surfaces.into_iter();
                self.mesh_surfaces = spawned
                    .models
                    .iter()
                    .map(|m| {
                        m.meshes
                            .iter()
                            .map(|_| object_surfaces.next().flatten())
                            .collect()
                    })
                    .collect();
                self.surfaces = resolved.surfaces;
                self.draw_surfaces = resolved.draw_surfaces;
                self.not_drawn = resolved.not_drawn;
                self.objects_not_drawn = resolved.objects_not_drawn;
                let definitions = resolved.definitions;
                // The sky's textures for every hourly section of the cycle
                // (sky.md 3.2; the weather effects' are not, our choice):
                // cube maps, cloud textures and the sun sprites of their
                // lens-flare sections, as the clock moves through the day.
                // A texture of any section that resolves nowhere fails the
                // load, as any texture of the level does (static-shading.md
                // 4.2; for the sections not yet in use, our choice).
                let weather = earlier(self.weather.as_ref(), "weather")?;
                let mut located: HashMap<String, String> = HashMap::new();
                let mut locate = |name: &str| -> io::Result<String> {
                    if let Some(p) = located.get(name) {
                        return Ok(p.clone());
                    }
                    let path = locate_texture(tree, data, &dir, name)?;
                    located.insert(name.to_owned(), path.clone());
                    Ok(path)
                };
                let mut sections = Vec::with_capacity(weather.sections.len());
                let mut flares = Vec::with_capacity(weather.sections.len());
                for (_, s) in &weather.sections {
                    let mut f = Flares::read(&data.system, &s.flares)?;
                    if let Some(sun) = &mut f.sun {
                        sun.texture = locate(&sun.texture)?;
                    }
                    sections.push(SkySection {
                        cube: locate(&s.sky_texture)?,
                        clouds: locate(&s.clouds_texture)?,
                        sun: f.sun.as_ref().map(|s| s.texture.clone()),
                    });
                    flares.push(f);
                }
                self.sky = Some(SkyTextures { sections });
                // The glows' shaders (glows.md 2): chunk 7's entries, and
                // `effects\glow` with each lamp's glow texture; a texture
                // found nowhere fails the load, as the level's do.
                let mut shader_of = |texture: &str, flare: bool| -> io::Result<u32> {
                    let texture = locate(texture)?;
                    let shader = GlowShader { texture, flare };
                    let shaders = &mut self.lights.glow_shaders;
                    Ok(match shaders.iter().position(|s| *s == shader) {
                        Some(i) => i as u32,
                        None => {
                            shaders.push(shader);
                            shaders.len() as u32 - 1
                        }
                    })
                };
                for g in std::mem::take(&mut self.glow_records) {
                    let entry = &level.shaders[usize::from(g.shader)];
                    let (shader, list) = split_shader_entry(entry).unwrap_or(("", ""));
                    let flare = match shader {
                        "effects\\glow" => false,
                        "effects\\flare" => true,
                        _ => {
                            // TODO(#347): no shipped glow has another shader.
                            self.not_drawn
                                .push((entry.clone(), "glow shader not drawn".into()));
                            continue;
                        }
                    };
                    let texture = shading::texture_list(list)
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    let i = shader_of(&texture, flare)?;
                    self.lights.level_glows.push((g.position, g.radius, i));
                }
                self.lights.lamp_glows = spawned
                    .lamps
                    .iter()
                    .map(|l| {
                        l.glow
                            .as_ref()
                            .map(|g| shader_of(&weather_texture_name(&g.texture), false))
                            .transpose()
                    })
                    .collect::<io::Result<_>>()?;
                // The torches' glows: `effects\glow` with `glow_texture`
                // (torch.md 3 step 7, glows.md 2.2).
                if let Some(torches) = &mut self.torches {
                    for t in &mut torches.torches {
                        let texture = weather_texture_name(&t.definition.glow_texture);
                        t.glow_shader = shader_of(&texture, false)?;
                    }
                }
                // dynamic-lights.md 6.4.
                self.light_textures = Some([
                    locate("internal\\internal_light_att")?,
                    locate("internal\\internal_light_attclip")?,
                    locate("internal\\internal_light_attpoint")?,
                ]);
                self.flares = flares;
                // The shaders of the effects the level can show
                // (particle-drawing.md 5.1), their textures located like
                // the level's.
                // (particle-drawing.md 5.1): the placed records', every
                // zone section's of the level's objects and every ambient
                // effect of the weather's sets.
                if let Some((effects, _)) = &self.effects {
                    let sound = earlier(self.sound.as_ref(), "sound data")?;
                    let zones =
                        (self.zone_sections.values()).filter_map(|s| s.idle_particles.as_deref());
                    let weather: Vec<&str> = (sound.sets.values())
                        .flat_map(|s| &s.effects)
                        .map(|e| e.particles.as_str())
                        .collect();
                    let names: Vec<&str> = zones.chain(weather.iter().copied()).collect();
                    let (shaders, not_drawn) =
                        particle_shaders(tree, data, &definitions, effects, &names, &mut locate)?;
                    self.particle_shaders = shaders;
                    self.not_drawn.extend(not_drawn);
                    // The most particles at once: every placed record's and
                    // zone's (each may come online) and the largest ambient
                    // effect (one at a time).
                    let library = effects.library();
                    let capacity = |n: &str| particle_capacity(library, n).unwrap_or(0);
                    let zone_sections = &self.zone_sections;
                    let zones: usize = (self.zone_record_sections.iter())
                        .filter_map(|s| zone_sections.get(s))
                        .filter_map(|s| s.idle_particles.as_deref())
                        .map(capacity)
                        .sum();
                    let placed: usize = (effects.effects())
                        .filter(|p| p.origin == Origin::Placed)
                        .map(|p| capacity(&p.name))
                        .sum();
                    let ambient = weather.iter().map(|n| capacity(n)).max().unwrap_or(0);
                    self.particle_capacity = placed + zones + ambient;
                }
            }
            // The objects' light inputs against the static collision under
            // the start's sun (level-objects.md 5.4), placed under this
            // caption by us. They are kept while the level runs (our
            // choice: the game retests them, the sun's ray as the sun moves).
            Stage::ClientSynchronising => {
                let collision = earlier(self.collision.as_deref(), "level.cform")?;
                let weather = earlier(self.weather.as_ref(), "weather")?;
                let clock = earlier(self.clock.as_ref(), "game clock")?;
                let actor = earlier(self.actor.as_ref(), "actor")?;
                let spawned = earlier(self.spawned.as_ref(), "objects")?;
                let (a, b, t) = weather.bracket(clock.time_of_day());
                let sun = Environment::blend(a, b, t, &[], actor.position, VIS_DISTANCE).sun_dir;
                // The sun-visibility ray goes along −d_B, the direction of
                // the sun's record B (dynamic-lights.md 3.1, 11).
                let d_b = crate::core::lights::sun_b_direction(sun);
                self.dynamic_props = objects::light_inputs(collision, &spawned.instances, d_b);
            }
            // `level.ps_static`'s effects, made, placed and started
            // (level-placed-effects.md sections 1 – 4): read after the
            // level's objects in the game, under this caption here (our
            // placement), with `particles.xr`, read here rather than at
            // game start (our choice; only this needs it).
            Stage::LoadingAiObjects => {
                let records = match vfs::read(tree, &format!("{dir}\\level.ps_static")) {
                    Ok(bytes) => particle_records::parse_placed(&bytes)
                        .map_err(|e| file("level.ps_static", e))?,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
                    Err(e) => return Err(e),
                };
                // The library serves the zones' and the weather's effects too.
                let path = data.aliases.path("$game_data$", "particles.xr")?;
                let library = match vfs::read(tree, &path) {
                    Ok(bytes) => Library::parse(&bytes)
                        .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Library::default(),
                    Err(e) => return Err(e),
                };
                // The effects collide with the level (particle-collision.md).
                let world = self.collision.clone();
                let effects =
                    PlacedEffects::new_in(Arc::new(library), &records, self.now as u32, world)
                        .map_err(|e| invalid(format!("{dir}\\level.ps_static: {e}")))?;
                self.effects = Some((effects, self.now));
            }
            Stage::PrefetchingObjects
            | Stage::ServerStarting
            | Stage::LoadingAlifeSimulator
            | Stage::LoadingShaders
            | Stage::LoadingDetails => {}
        }
        Ok(())
    }

    /// The level after every stage ran, shown from engine clock `now`.
    pub fn finish(mut self, now: u64) -> io::Result<LevelScreen> {
        let name = earlier(self.name, "level")?;
        let actor = earlier(self.actor, "actor")?;
        let level = earlier(self.level, "level")?;
        let geom = earlier(self.geom, "level.geom")?;
        let collision = earlier(self.collision, "level.cform")?;
        let movement = earlier(self.movement, "actor")?;
        let controls = earlier(self.controls, "controls")?;
        let weather = earlier(self.weather, "weather")?;
        let sky = earlier(self.sky, "sky")?;
        let clock = earlier(self.clock, "game clock")?;
        let character = earlier(self.character, "player character")?;
        let spawned = earlier(self.spawned, "objects")?;
        let simulation = earlier(self.simulation, "simulation")?;
        let footsteps = earlier(self.footsteps, "footsteps")?;
        let player_sphere = earlier(self.player_sphere, "player's model")?;
        let torches = earlier(self.torches, "torches")?;
        let detectors = earlier(self.detectors, "detectors")?;
        if self.dynamic_props.len() != spawned.instances.len() {
            return Err(invalid("loading stages out of order: no light inputs"));
        }
        let objects = LevelObjects::new(
            spawned,
            &self.mesh_surfaces,
            &self.dynamic_props,
            self.objects_not_drawn,
        );
        let glow_shaders = self.lights.glow_shaders.clone();
        let lights = LevelLights::new(std::mem::take(&mut self.lights), objects.lamps.clone())?;
        let placements = objects
            .instances
            .iter()
            .map(|i| i.online.then_some(i.transform))
            .collect();
        let instances = objects
            .instances
            .iter()
            .enumerate()
            .map(|(k, i)| (i.id, k))
            .collect();
        // The first update after loading receives the rest of the load's
        // time (level-placed-effects.md section 5.3).
        let effects = self.effects.map(|(mut e, at)| {
            e.advance(now.saturating_sub(at));
            e
        });
        // start-position.md sections 4 and 5: standing at the stored
        // position, the camera at the spawn's angles.
        let player = Player::new(
            movement,
            controls.settings,
            actor.position,
            Angles::from_spawn(actor.angles),
            now,
        );
        let eye = player.view().eye;
        let (a, b, t) = weather.bracket(clock.time_of_day());
        let environment = Environment::blend(a, b, t, &self.modifiers, eye, VIS_DISTANCE);
        let name_for_torches = name.clone();
        let mut screen = LevelScreen {
            world: Arc::new(LevelWorld {
                name,
                dir: self.dir,
                shaders: level.shaders,
                draw_items: self.draw_items,
                spheres: self.spheres,
                surfaces: self.surfaces,
                draw_surfaces: self.draw_surfaces,
                not_drawn: self.not_drawn,
                sky,
                flares: self.flares,
                geometry: geom,
                objects,
                particle_shaders: self.particle_shaders,
                particle_capacity: self.particle_capacity,
                glow_shaders,
                light_textures: self.light_textures,
            }),
            collision,
            sound_data: self.sound,
            fatal: None,
            footsteps: Some(footsteps),
            sky_visibility: SkyVisibility::default(),
            player_sphere,
            frames: 0,
            frame_s: 0.0,
            player,
            character: Some(character),
            controls,
            near: camera::NEAR,
            environment,
            weather,
            clock,
            modifiers: self.modifiers,
            shown: now,
            last: now,
            back: false,
            sun_fade: SunFade::default(),
            simulation: Some(simulation),
            placements: Arc::new(placements),
            instances,
            effects,
            view_size: std::cell::Cell::new((1024, 768)),
            zones: self.zones,
            zone_sections: self.zone_sections,
            weather_effect: None,
            effects_drawn: [true; 3],
            lights,
            global_ms: 0,
            torches: PlayerTorches::new(torches, &name_for_torches),
            detectors: PlayerDetectors::new(detectors),
        };
        screen.torches_frame(0.0);
        screen.update_view();
        screen.sync_zone_lights();
        screen.lights_frame(0.0);
        // TODO(#365, sound-weather-ambient.md 4.3, disputed): whether a frame
        // draws the level, updating the player's sky visibility, before the
        // rule's first read is unestablished; we take that it does (the
        // level drawn under the loading image), so the first read sees the
        // first update's target rather than the initial 0.5.
        screen.objects_gathered();
        Ok(screen)
    }
}

impl LevelScreen {
    /// Loads the new game's level at engine clock `now`: every stage of
    /// [`NewGameLoad`] at once.
    pub fn load(tree: &dyn Vfs, data: &GameData, now: u64) -> io::Result<Self> {
        let mut load = NewGameLoad::default();
        load.set_clock(now);
        for stage in Stage::ALL {
            load.run(stage, tree, data)?;
        }
        load.finish(now)
    }

    /// Places the player with its eye at `eye` (tests and debugging).
    pub fn set_camera(&mut self, eye: Vec3, angles: Angles) {
        let h = self
            .player
            .config
            .eye_height(self.player.body().box_index());
        self.player.place([eye[0], eye[1] - h, eye[2]], angles);
        self.update_view();
    }

    /// Leaves the objects out of the frames drawn (tests and comparisons);
    /// only before the world is shared with a presenter. Returns whether
    /// it could.
    pub fn hide_objects(&mut self) -> bool {
        Arc::get_mut(&mut self.world).is_some_and(|w| {
            w.objects.draws.clear();
            true
        })
    }

    pub fn world(&self) -> &Arc<LevelWorld> {
        &self.world
    }

    /// The level's collision world, for the player's body.
    pub fn collision(&self) -> &CollisionWorld {
        &self.collision
    }

    /// The environment at the camera at the clock's time of day
    /// (static-shading.md 9.2) and the near
    /// plane: 0.01 m while the cube of half-size 0.2 m around the eye meets
    /// a static collision triangle (start-position.md section 7; movable
    /// objects do not exist here).
    fn update_view(&mut self) {
        let eye = self.player.view().eye;
        let (a, b, t) = self.weather.bracket(self.clock.time_of_day());
        self.environment = Environment::blend(a, b, t, &self.modifiers, eye, VIS_DISTANCE);
        let c = camera::NEAR_CLEARANCE;
        let mut found = Vec::new();
        self.collision.query(
            [eye[0] - c, eye[1] - c, eye[2] - c],
            [eye[0] + c, eye[1] + c, eye[2] + c],
            &mut found,
        );
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let close = found
            .iter()
            .any(|&t| geometry::box_intersects(eye, axes, [c; 3], &self.collision.corners(t)));
        self.near = if close {
            camera::NEAR_CLOSE
        } else {
            camera::NEAR
        };
    }

    /// A frame at `now` with game time paused (a movie item's
    /// `pause_state`, level-loading.md section 2.3): the game clock stands
    /// still, nothing moves and the shaders' clock stands still (our
    /// reading: which clock the shaders use is not established, ogf.md
    /// section 10); input is not read. The sun's fade (sky.md 7.3) does
    /// not change either (our choice: the specs do not say what k_t is
    /// while game time is paused).
    pub fn paused(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.last);
        self.shown += elapsed;
        self.last = now;
        self.clock.set_paused(true);
        self.clock.advance(elapsed);
        self.player.paused(now);
        // The frame still draws its lights and glows; no time passes.
        self.lights_frame(0.0);
    }

    /// The level waits until `now` before it first shows (the platform
    /// preparing its world for the GPU, #184): no clock moves, as if it
    /// were made at `now`, but the placed effects' world clock: the wait is
    /// the rest of the load, which their first update receives
    /// (level-placed-effects.md section 5.3).
    pub fn wait(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.last);
        self.shown += elapsed;
        self.last = now;
        self.player.paused(now);
        if let Some(effects) = &mut self.effects {
            effects.advance(elapsed);
        }
    }

    /// One frame at `now` with this frame's `input`. The game clock
    /// advances by the real time since the previous frame, uncapped (our
    /// choice: the specs cap only the movement's and the display's frame
    /// times, player-movement.md 3.2).
    pub fn update(&mut self, now: u64, input: &Input) {
        let elapsed = now.saturating_sub(self.last);
        let dt = (elapsed as f32 / 1000.0).min(MAX_STEP);
        self.last = now;
        // The previous frame's drawing gathers its objects after that
        // frame's sound rules (sound-weather-ambient.md 4.3): the player's
        // sky rays from where it ended, before this frame moves anything.
        self.objects_gathered();
        self.frames += 1;
        self.frame_s = elapsed as f32 / 1000.0;
        // The previous frame's idle sound starts and stops were the sound
        // side's to play.
        self.zones.clear_events();
        self.clock.set_paused(false);
        self.clock.advance(elapsed);
        let player = self.player.state();
        let (request, back) = self.controls.request(input, &player);
        // player-state.md 4.3: the condition's limits for this frame's
        // movement; 4.4: a jump's cost; 4.1, 4.2: the conditions over the
        // game time this frame advanced, with the state the movement left.
        if let Some(c) = &mut self.character {
            self.player.limits = c.movement_limits();
        }
        // The climbing states' clock is the footsteps' step clock
        // (climbing.md, conventions): the global clock.
        self.player.set_step_clock(self.global_ms as u32);
        self.player.frame(&self.collision, now, &request);
        if let Some(c) = &mut self.character {
            if self.player.jump_started() {
                c.jump();
            }
            let f = self.player.state().flags;
            c.update(
                self.clock.time(),
                Activity {
                    moving: f.forward || f.back || f.left || f.right,
                    runs: self.player.state_runs(),
                    sprints: f.sprint,
                },
            );
        }
        self.back |= back;
        self.simulate();
        // The player's torches (torch.md 4, 6, 8, 15.3) after the
        // movement, aimed by this frame's camera.
        self.torches.take_sounds();
        // The previous frame's detector sounds were the sound side's.
        self.detectors.take_sounds();
        let alive = self
            .character
            .as_ref()
            .is_some_and(|c| c.conditions.alive());
        if let Some(c) = &mut self.character {
            let (lights, glows) = (&mut self.lights.lights, &mut self.lights.glows);
            self.torches.sync(c, lights, glows);
            if self.controls.torch_pressed(input) {
                self.torches.torch_command(alive, lights, glows);
            }
            if self.controls.night_vision_pressed(input) {
                self.torches.night_vision_command(alive, c);
            }
        }
        self.torches_frame(elapsed as f32 / 1000.0);
        // The lamps' frame updates (hanging-lamps.md 4) on the global clock.
        self.lights
            .lamps_frame(self.global_ms as f32 / 1000.0, elapsed as f32 / 1000.0);
        // The placed effects on the world clock, which is the global clock
        // (autosave-timing.md 1, sound-weather-ambient.md 3.1): it stands
        // still while the pause is on (the menu, a `pause_state on` intro
        // item, the hidden page), when the app runs no update but
        // [`LevelScreen::paused`] or none at all, so the elapsed time here is
        // the global clock's.
        // The zones' updates after the scheduler's pass over the effect
        // objects, on the same clock (zone-idle-effects.md 6).
        if let Some(effects) = &mut self.effects {
            let eye = self.player.view().eye;
            effects.frame(elapsed, eye);
            let frame = crate::core::zones::Frame {
                clock: effects.clock(),
                elapsed: elapsed as u32,
                camera: eye,
                colours: self.lights.animations(),
            };
            if let Err(e) = self.zones.frame(&frame, effects) {
                self.fatal.get_or_insert(e);
            }
            // The player's detectors (sound-detector.md 3 – 5) in the same
            // pass over the objects and the scheduler's after it, on the
            // same clock, following the inventory as it stands.
            let feet = self.player.state().position;
            if let Some(c) = &self.character {
                self.detectors.sync(c, feet);
            }
            self.detectors
                .frame(effects.clock(), elapsed as u32, feet, eye, &self.zones);
        }
        // The idle lights of zones updated or created this frame.
        self.sync_zone_lights();
        self.draw_updates();
        // sky.md 7.3, k_t from the game clock.
        let (a, b, t) = self.weather.bracket_indices(self.clock.time_of_day());
        self.sun_fade.frame(
            sky::flares_in_effect(a, b, t),
            &self.world.flares,
            dt,
            self.clock.time_factor(),
        );
        self.update_view();
        self.lights_frame(elapsed as f32 / 1000.0);
    }

    /// The player's transform (sound-weather-ambient.md 4.3): at its feet,
    /// turned about y by the body's heading so that the model's +z points
    /// along it (start-position.md 5: the heading turns about y only).
    /// TODO(#365): the specs do not give the actor's transform in
    /// first-person view; this is our reading.
    fn player_transform(&self) -> volya_engine_formats::model::Mat4 {
        let feet = self.player.state().position;
        volya_engine_formats::model::transform(feet, [0.0, -self.player.heading(), 0.0])
    }

    /// The origin of the player's sky rays (sound-weather-ambient.md 4.3):
    /// its transform applied to its model's sphere centre *C*, raised by
    /// 0.3 *R*.
    pub fn sky_ray_origin(&self) -> Vec3 {
        let (c, r) = self.player_sphere;
        let c = volya_engine_formats::model::transform_point(&self.player_transform(), c);
        object_light::ray_origin(c, r)
    }

    /// The drawing of the current frame gathers the level's objects
    /// (sound-weather-ambient.md 4.3): the player's sky visibility is
    /// updated, at most once per frame, from [`Self::sky_ray_origin`]
    /// against the static collision.
    fn objects_gathered(&mut self) {
        let origin = self.sky_ray_origin();
        self.sky_visibility
            .update(self.frames, &self.collision, origin, self.frame_s);
    }

    /// The player's sky visibility as the current frame's rules read it
    /// (sound-weather-ambient.md 4.3): the frame's smoothing step applied
    /// if it has not run, no rays cast.
    pub fn read_sky_visibility(&mut self) -> f32 {
        self.sky_visibility.read(self.frames, self.frame_s)
    }

    /// The player's sky visibility: its latest value and target.
    pub fn sky_visibility(&self) -> (f32, f32) {
        (self.sky_visibility.value(), self.sky_visibility.target())
    }

    /// The player's torches placed and coloured for a frame of `dt`
    /// seconds (torch.md 7), the torch bytes kept (9).
    fn torches_frame(&mut self, dt: f32) {
        if let Some(c) = &mut self.character {
            self.torches
                .sync(c, &mut self.lights.lights, &mut self.lights.glows);
        }
        let v = self.player.view();
        let a = self.player.angles();
        let aim = crate::core::torch::Aim {
            yaw: a.yaw,
            pitch: a.pitch,
        };
        let (animations, lights, glows) = self.lights.parts_mut();
        self.torches.frame(
            self.player.state().position,
            aim,
            v.eye,
            dt,
            self.global_ms as f32 / 1000.0,
            animations,
            lights,
            glows,
        );
    }

    /// The player's torches.
    pub fn torches(&self) -> &PlayerTorches {
        &self.torches
    }

    /// What the touch Torch and Night vision buttons show
    /// ([`PlayerTorches::buttons`]): both hidden while the player can't use
    /// the commands (health 0, torch.md 8).
    pub fn torch_buttons(&self) -> (Option<bool>, Option<bool>) {
        match &self.character {
            Some(c) if c.conditions.alive() => self.torches.buttons(c),
            _ => (None, None),
        }
    }

    /// This frame's night-vision sound commands, taken (the app plays them
    /// on the level's sounds, torch.md 14).
    pub fn take_torch_sounds(&mut self) -> Vec<TorchSound> {
        self.torches.take_sounds()
    }

    /// The player's detectors.
    pub fn detectors(&self) -> &PlayerDetectors {
        &self.detectors
    }

    /// Adds the section of a detector item made after the level loaded
    /// (sound-detector.md 2.1).
    pub fn add_detector_section(&mut self, section: DetectorSection) {
        self.detectors.add_section(section);
    }

    /// The detectors' sound commands of this frame, taken (played by the app
    /// on the level's sounds, sound-detector.md 6).
    pub fn take_detector_sounds(&mut self) -> Vec<DetectorSound> {
        self.detectors.take_sounds()
    }

    /// The player's footsteps.
    pub fn footsteps(&self) -> Option<&PlayerFootsteps> {
        self.footsteps.as_ref()
    }

    /// The player's footsteps on a frame of the level while the pause is
    /// off, after its update, at step clock `now` (S11 3.1; crate::footsteps): the
    /// leg cycle and the steps (S11 2, 3). Fatal as the pair lookup is.
    pub fn footsteps_frame(&mut self, now: u32) -> Result<(), Fatal> {
        let Some(f) = &mut self.footsteps else {
            return Ok(());
        };
        let alive = self.character.as_ref().is_none_or(|c| c.conditions.alive());
        f.frame(now, &self.player, alive)
    }

    /// The level's sound data, once (the app creates its sounds).
    pub fn take_sound_data(&mut self) -> Option<LevelSoundData> {
        self.sound_data.take()
    }

    /// The collision world, shared.
    pub fn shared_collision(&self) -> Arc<CollisionWorld> {
        self.collision.clone()
    }

    /// What this frame's sound rules read.
    pub fn sound_inputs(&mut self) -> SoundInputs<'_> {
        let sky_visibility = self.read_sky_visibility();
        let (a, b, t) = self.weather.bracket(self.clock.time_of_day());
        SoundInputs {
            tau: (self.clock.time() % crate::core::clock::DAY_MS) as u32,
            sections: [a, b],
            t,
            far_plane: self.environment.far_plane,
            sun_dir: self.environment.sun_dir,
            eye: self.player.view().eye,
            sky_visibility,
        }
    }

    /// What this frame's sound rules read, with the level's side of the
    /// weather's ambient effect objects (weather-ambient-effects.md 3).
    pub fn sound_frame_parts(&mut self) -> (SoundInputs<'_>, WeatherParticles<'_>) {
        let sky_visibility = self.read_sky_visibility();
        let (a, b, t) = self.weather.bracket(self.clock.time_of_day());
        let inputs = SoundInputs {
            tau: (self.clock.time() % crate::core::clock::DAY_MS) as u32,
            sections: [a, b],
            t,
            far_plane: self.environment.far_plane,
            sun_dir: self.environment.sun_dir,
            eye: self.player.view().eye,
            sky_visibility,
        };
        let particles = WeatherParticles {
            effects: &mut self.effects,
            object: &mut self.weather_effect,
        };
        (inputs, particles)
    }

    /// The weather's ambient effect object, while it exists.
    pub fn weather_effect(&self) -> Option<&PlacedEffect> {
        let effects = self.effects.as_ref()?;
        effects.get(self.weather_effect?)
    }

    /// The zone objects (zone-idle-effects.md).
    pub fn zones(&self) -> &Zones {
        &self.zones
    }

    /// The zones' idle sound starts and stops of this frame (S15 3).
    pub fn zone_sound_events(&self) -> &[SoundEvent] {
        self.zones.sound_events()
    }

    /// The zones' idle sounds playing now, as starts at the positions of
    /// their latest starts: for sounds created after the zones started.
    pub fn zone_sounds_playing(&self) -> Vec<SoundEvent> {
        (self.zones.zones().iter().enumerate())
            .filter(|(_, z)| z.sound_on)
            .map(|(zone, z)| SoundEvent::Play {
                zone,
                position: z.sound_position,
            })
            .collect()
    }

    /// Each zone's sound names (S15 2.1), by zone.
    pub fn zone_sound_names(&self) -> Vec<ZoneSoundNames> {
        (self.zones.zones().iter())
            .map(|z| z.section.sounds.clone())
            .collect()
    }

    /// The zones as the engine interface reports them.
    pub fn zones_report(&self) -> ZonesReport {
        let effects = self.effects.as_ref();
        let object = |o: Option<usize>| o.and_then(|o| effects?.get(o));
        let zones = (self.zones.zones().iter())
            .map(|z| {
                let effect = object(z.effect);
                ZoneReport {
                    id: z.id,
                    name: z.name.clone(),
                    section: z.section.name.clone(),
                    state: format!("{:?}", z.state).to_lowercase(),
                    fast: z.fast,
                    position: z.position(),
                    effect: z.section.idle_particles.clone(),
                    playing: effect.is_some_and(|p| p.instance.is_playing()),
                    particles: effect.map_or(0, |p| {
                        p.instance.effects().map(|e| e.particles().len()).sum()
                    }),
                    sound: z.section.sounds[crate::core::zones::IDLE].clone(),
                    sound_on: z.sound_on,
                    light: z.light.as_ref().is_some_and(|l| l.active),
                }
            })
            .collect();
        let weather = self.weather_effect().map(|p| WeatherEffectReport {
            name: p.name.clone(),
            playing: p.instance.is_playing(),
            stopping: p.instance.effects().any(|e| e.is_stopping()),
            sphere: {
                let s = p.instance.sphere();
                [s.centre[0], s.centre[1], s.centre[2], s.radius]
            },
            particles: p.instance.effects().map(|e| e.particles().len()).sum(),
        });
        ZonesReport { zones, weather }
    }

    /// One update of the offline simulation (offline-simulation.md 6.2)
    /// with the actor where the player stands, at the clock's game time,
    /// and the objects' visuals following what switched. Runs only in
    /// [`LevelScreen::update`], so it stands still while the game is paused
    /// or the page hidden. A simulation error is fatal (offline-simulation.md
    /// 6.2, level-objects.md 2.3): kept for [`LevelScreen::take_fatal`].
    fn simulate(&mut self) {
        let Some(sim) = &mut self.simulation else {
            return;
        };
        sim.set_actor_position(self.player.state().position);
        let changes = sim.update(
            self.clock.time(),
            self.clock.game_time().normal_time_factor,
            hour(&self.clock),
        );
        match changes {
            Ok(changes) => self.place_objects(&changes),
            Err(e) => self.record_fatal(e.to_string()),
        }
    }

    /// Keeps the first fatal error met while the level runs.
    pub(crate) fn record_fatal(&mut self, error: String) {
        self.fatal.get_or_insert(error);
    }

    /// A fatal error met while the level ran, once.
    pub fn take_fatal(&mut self) -> Option<String> {
        self.fatal.take()
    }

    /// Shows the instances of objects that came online, at their records'
    /// placements (level-objects.md 3.4), and hides those that went
    /// offline or were removed. An object online without a loaded visual
    /// (one that came onto the level by travel) stays undrawn (our
    /// limitation: visuals load with the level).
    fn place_objects(&mut self, changes: &Changes) {
        if changes.is_empty() {
            return;
        }
        for &id in &changes.online {
            self.lights.object_online(id);
        }
        for &id in changes.offline.iter().chain(&changes.removed) {
            self.lights.object_offline(id);
        }
        let Some(sim) = &self.simulation else { return };
        // A zone record coming online for the first time creates its zone
        // object (zone-idle-effects.md 3.1); zones never go offline.
        if let Some(effects) = &mut self.effects {
            for &id in &changes.online {
                let Some(o) = sim.object(id).filter(|_| !self.zones.contains(id)) else {
                    continue;
                };
                let (Some(section), Some(record)) = (
                    self.zone_sections.get(&o.record.header.section),
                    ZoneRecord::of(id, &o.record),
                ) else {
                    continue;
                };
                let section = section.clone();
                if let Err(e) = self
                    .zones
                    .create(&record, section, effects.clock(), effects)
                {
                    self.fatal.get_or_insert(e);
                }
            }
        }
        let mut placements = (*self.placements).clone();
        let mut changed = false;
        for &id in &changes.online {
            if let (Some(&k), Some(o)) = (self.instances.get(&id), sim.object(id)) {
                let h = &o.record.header;
                placements[k] = Some(model_math::transform(h.position, h.angles));
                changed = true;
            }
        }
        for &id in changes.offline.iter().chain(&changes.removed) {
            if let Some(&k) = self.instances.get(&id) {
                placements[k] = None;
                changed = true;
            }
        }
        if changed {
            self.placements = Arc::new(placements);
        }
    }

    /// The level's dynamic light records, for the sources that create and
    /// update their lights each frame (docs/architecture.md, Lights).
    /// The zones' idle lights given to their records among the level's
    /// dynamic lights (zone-idle-effects.md 4.4), each made the first time.
    fn sync_zone_lights(&mut self) {
        let mut zones = std::mem::take(&mut self.zones);
        zones.sync_lights(self.lights_mut());
        self.zones = zones;
    }

    pub fn lights_mut(&mut self) -> &mut Lights {
        &mut self.lights.lights
    }

    /// The level's dynamic light records.
    pub fn lights(&self) -> &Lights {
        &self.lights.lights
    }

    /// The level's lights, glows and lamps.
    pub fn level_lights(&self) -> &LevelLights {
        &self.lights
    }

    /// The level's lights, glows and lamps (tests: settings, hits).
    pub fn level_lights_mut(&mut self) -> &mut LevelLights {
        &mut self.lights
    }

    /// This frame's global clock (sound-weather-ambient.md 3.1), ms: the
    /// lamps' colour animations' time (hanging-lamps.md 4.3).
    pub fn set_global_clock(&mut self, ms: u64) {
        self.global_ms = ms;
    }

    /// The settings of dynamic-lights.md 8 and `r1_glows_per_frame`.
    pub fn set_light_settings(&mut self, settings: LightSettings, glows_per_frame: u32) {
        self.lights.settings = settings;
        self.lights.glows_per_frame = glows_per_frame;
    }

    /// The frame's lights and glows from the latest frame's view, a frame
    /// of `dt` seconds (0 while paused).
    fn lights_frame(&mut self, dt: f32) {
        let (w, h) = self.view_size.get();
        let (view, projection) = self.view_projection(w, h);
        let e = &self.environment;
        let camera = LightCamera {
            eye: self.player.view().eye,
            view,
            projection,
            far: e.far_plane,
        };
        let sun = (e.sun_dir, e.sun_color);
        self.lights.frame(&self.collision, &camera, sun, dt);
    }

    /// The level's placed effects (level-placed-effects.md).
    pub fn effects(&self) -> Option<&PlacedEffects> {
        self.effects.as_ref()
    }

    /// The drawing update (level-placed-effects.md 5.3,
    /// particle-drawing.md 7.1 step 1): every effect whose culling sphere
    /// meets the view is advanced to the clock before it is drawn, after
    /// the scheduler's pass of the frame. The view is that of the latest
    /// frame's size.
    fn draw_updates(&mut self) {
        let (w, h) = self.view_size.get();
        let (view, projection) = self.view_projection(w, h);
        let Some(effects) = &self.effects else {
            return;
        };
        let drawn = self.drawn(effects, &view, &projection);
        if let Some(effects) = &mut self.effects {
            for i in drawn {
                effects.draw_update(i);
            }
        }
    }

    /// The effect objects drawn in a view: those visible (7.1) of the
    /// origins drawn.
    fn drawn(&self, effects: &PlacedEffects, view: &Mat4, projection: &Mat4) -> Vec<usize> {
        let origin = |o: Origin| match o {
            Origin::Placed => 0,
            Origin::Zone => 1,
            Origin::Weather => 2,
        };
        particle_drawing::visible(effects, view, projection)
            .into_iter()
            .filter(|&i| {
                effects
                    .get(i)
                    .is_some_and(|p| self.effects_drawn[origin(p.origin)])
            })
            .collect()
    }

    /// Leaves the effects out of the frames drawn, or draws them again
    /// (tests compare frames with and without them).
    pub fn set_effects_drawn(&mut self, drawn: bool) {
        self.effects_drawn = [drawn; 3];
    }

    /// Draws only the effect objects of `origins` (tests).
    pub fn set_effect_origins_drawn(&mut self, origins: &[Origin]) {
        self.effects_drawn =
            [Origin::Placed, Origin::Zone, Origin::Weather].map(|o| origins.contains(&o));
    }

    /// The effects queued for a frame of `view` and `projection` at
    /// `resolution` (particle-drawing.md 7.1, 7.2).
    fn particle_draws(
        &self,
        resolution: Resolution,
        view: &Mat4,
        projection: &Mat4,
    ) -> Vec<particle_drawing::ParticleDraw> {
        self.view_size.set((resolution.width, resolution.height));
        let Some(effects) = self.effects.as_ref() else {
            return Vec::new();
        };
        let v = self.player.view();
        let camera = crate::core::particles::sprite::Camera {
            eye: v.eye,
            forward: v.direction,
            up: v.up,
        };
        let (w, h) = crate::render::world::supersampled(resolution.width, resolution.height);
        let ssa_min = crate::render::world::ssa_min(w, h, camera::FOV_Y_DEGREES);
        let records = self.drawn(effects, view, projection);
        particle_drawing::queue(
            effects,
            &records,
            &self.world.particle_shaders,
            &camera,
            ssa_min,
        )
    }

    /// What the engine interface reports of the placed effects.
    pub fn effects_report(&self) -> EffectsReport {
        effects_report(self.effects.as_ref())
    }

    /// The offline simulation.
    pub fn simulation(&self) -> Option<&Simulation> {
        self.simulation.as_ref()
    }

    /// An id for an item created in the level: from the simulation's pool;
    /// without a simulation (unit tests), one above every id the player
    /// character holds.
    pub fn take_item_id(&mut self) -> io::Result<u16> {
        if let Some(s) = &mut self.simulation {
            return s.take_id();
        }
        let held = self
            .character
            .iter()
            .flat_map(|c| c.inventory.items.iter().map(|i| i.id))
            .max()
            .unwrap_or(0);
        held.checked_add(1)
            .filter(|&id| id != character::inventory::NO_ID)
            .ok_or_else(|| invalid("no id left"))
    }

    /// The game clock.
    pub fn clock(&self) -> &GameClock {
        &self.clock
    }

    /// Whether the player asked for the menu since the last call
    /// ([`Controls::request`]).
    pub fn take_back_requested(&mut self) -> bool {
        std::mem::take(&mut self.back)
    }

    /// View and projection for a `width` × `height` target
    /// (start-position.md sections 5–7, the smoothing of 7.2 replaced by its
    /// targets, which it allows), from the player's eye.
    pub fn view_projection(&self, width: u32, height: u32) -> (Mat4, Mat4) {
        let v = self.player.view();
        let view = camera::view_matrix(v.eye, v.direction, v.up);
        let projection = camera::projection(
            camera::FOV_Y_DEGREES.to_radians(),
            camera::aspect_factor(width, height),
            self.near,
            self.environment.far_plane,
        );
        (view, projection)
    }

    /// The frame for `resolution`.
    pub fn scene(&self, resolution: Resolution, clock_ms: u64) -> Scene {
        let (view, projection) = self.view_projection(resolution.width, resolution.height);
        let v = self.player.view();
        let d = v.direction;
        let (a, _, _) = self.weather.bracket_indices(self.clock.time_of_day());
        let sun = self.sun_fade.taken.and_then(|s| {
            let sun = self.world.flares.get(s)?.sun.as_ref()?;
            Some((s, sun))
        });
        let particles = self.particle_draws(resolution, &view, &projection);
        let mut scene = Scene::new(resolution);
        scene.world = Some(WorldScene {
            particles,
            lights: self.lights.passes().to_vec(),
            glows: self.lights.sprites().to_vec(),
            covered: false,
            world: self.world.clone(),
            eye: v.eye,
            forward: d,
            fov_y_degrees: camera::FOV_Y_DEGREES,
            view,
            projection,
            environment: self.environment.clone(),
            // The shaders' clock starts when the level is shown (our choice;
            // which clock is unestablished, ogf.md section 10).
            time: clock_ms.saturating_sub(self.shown) as f32 / 1000.0,
            objects: self.placements.clone(),
            sky: Some(sky::sky_frame(
                &self.environment,
                a,
                &SkyCamera {
                    eye: v.eye,
                    forward: d,
                    right: [view[0][0], view[1][0], view[2][0]],
                },
                sun,
                self.sun_fade.fade,
            )),
        });
        scene
    }

    pub fn state(&self) -> LevelState {
        let p = self.player.state();
        let Flags {
            crouch,
            sprint,
            jump,
            falling,
            landing,
            climbing,
            ..
        } = p.flags;
        let angles = self.player.angles();
        let tau = self.clock.time_of_day();
        let (a, b, t) = self.weather.bracket_indices(tau);
        let section = |i: usize| self.weather.sections[i].1.name.clone();
        LevelState {
            name: self.world.name.clone(),
            draws: self.world.draw_items.len(),
            eye: self.player.view().eye,
            yaw: angles.yaw,
            pitch: angles.pitch,
            far_plane: self.environment.far_plane,
            near_plane: self.near,
            game_time: self.clock.time(),
            time_of_day: tau,
            weather_t: t,
            weather_sections: [section(a), section(b)],
            player: PlayerReport {
                position: p.position,
                velocity: p.velocity,
                airborne: p.airborne,
                posture: p.posture,
                moving: p.flags.forward || p.flags.back || p.flags.left || p.flags.right,
                running: self.player.settings.always_run != p.flags.accel,
                crouch,
                sprint,
                jump,
                falling,
                landing,
                climbing,
                crouch_toggled: p.crouch_toggled,
                sprint_requested: p.sprint_requested,
                sprint_locked: self.controls.sprint_lock.locked(),
            },
            character: self.character.as_ref().map(CharacterReport::of),
            objects: self.objects_report(),
            lights: self.lights.report(),
            torch: self.torches.report(&self.lights.lights, &self.lights.glows),
            detectors: self.detectors.report(&self.zones),
        }
    }

    /// The objects' report with the current online objects and drawn
    /// instances.
    fn objects_report(&self) -> ObjectsReport {
        let mut report = self.world.objects.report();
        if let Some(sim) = &self.simulation {
            report.online = sim
                .level_list()
                .filter(|&id| sim.object(id).is_some_and(|o| o.online))
                .count();
        }
        report.with_model = self.placements.iter().filter(|p| p.is_some()).count();
        report
    }

    /// The player in the level.
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// The player character's state (player-state.md).
    pub fn character(&self) -> Option<&CharacterState> {
        self.character.as_ref()
    }

    /// The player character's state, to change (the inventory screen).
    pub fn character_mut(&mut self) -> Option<&mut CharacterState> {
        self.character.as_mut()
    }

    /// The player's controls.
    pub fn controls(&self) -> &Controls {
        &self.controls
    }

    /// The player's controls, to change (a screen over the level keeping
    /// the mouse, [`Controls::keys_only`]).
    pub fn controls_mut(&mut self) -> &mut Controls {
        &mut self.controls
    }

    /// Takes the bindings and movement settings of `controls` (the options
    /// screen changed them during the game).
    pub fn set_controls(&mut self, controls: &Controls) {
        self.controls.bindings = controls.bindings.clone();
        self.controls.settings = controls.settings;
        self.player.settings = controls.settings;
    }
}

#[cfg(test)]
impl LevelScreen {
    /// The level with `character` as its player character.
    pub(crate) fn with_character(mut self, character: CharacterState) -> Self {
        self.character = Some(character);
        self
    }

    /// A level with nothing in it, its player standing at the origin with
    /// `controls` (unit tests of what runs a level; not drawn).
    pub(crate) fn empty(controls: Controls, now: u64) -> Self {
        use crate::core::collision::body::BodyConfig;
        use crate::core::movement::Bobbing;
        let weather = Weather::load(
            &volya_engine_formats::ltx::parse(
                b"[weathers]\ndefault = cycle\n[cycle]\n00:00:00 = w\n[w]\nfar_plane = 100\n\
                  ambient = 0\nhemi_color = 0\nsun_color = 0\nsun_dir = 0\nfog_color = 0\n\
                  fog_distance = 0\nfog_density = 0\nsky_texture = s\nsky_color = 0\n\
                  clouds_texture = c\nclouds_color = 0\nflares =\nrain_density = 0\nthunderbolt =\n",
            )
            .expect("weather"),
            "default",
        )
        .expect("weather");
        let alife = volya_engine_formats::ltx::parse(
            b"[alife]\ntime_factor = 10\nnormal_time_factor = 10\nstart_date = 01.05.2012\n\
              start_time = 05:30:00\n",
        )
        .expect("alife");
        let clock = GameClock::new_game(&alife).expect("clock");
        let movement = MovementConfig {
            walk_accel: 17.0,
            run_coef: 2.1,
            run_back_coef: 1.8,
            walk_back_coef: 0.8,
            crouch_coef: 0.45,
            climb_coef: 0.7,
            sprint_koef: 2.6,
            walk_strafe_coef: 1.0,
            run_strafe_coef: 1.0,
            jump_speed: 6.0,
            air_control_param: 0.1,
            camera_height_factor: 0.85,
            crash_speed_min: 14.0,
            crash_speed_max: 30.0,
            body: BodyConfig {
                boxes: [[0.35, 0.9, 0.35], [0.35, 0.75, 0.35], [0.35, 0.6, 0.35]],
                mass: 80.0,
            },
            lim_pitch: [-1.5, 1.5],
            rot_speed: [3.0, 3.0, 0.0],
            bobbing: Bobbing {
                run_amplitude: 0.0075,
                run_speed: 10.0,
                walk_amplitude: 0.005,
                walk_speed: 7.0,
                limp_amplitude: 0.011,
                limp_speed: 6.0,
            },
        };
        let player = Player::new(
            movement,
            controls.settings,
            [0.0; 3],
            Angles::default(),
            now,
        );
        let (a, b, t) = weather.bracket(clock.time_of_day());
        let environment = Environment::blend(a, b, t, &[], player.view().eye, VIS_DISTANCE);
        LevelScreen {
            world: Arc::new(LevelWorld {
                name: "empty".into(),
                dir: String::new(),
                shaders: Vec::new(),
                draw_items: Vec::new(),
                spheres: Vec::new(),
                surfaces: Vec::new(),
                draw_surfaces: Vec::new(),
                not_drawn: Vec::new(),
                sky: SkyTextures {
                    sections: Vec::new(),
                },
                flares: vec![Flares {
                    name: String::new(),
                    rise: 0.0,
                    down: 0.0,
                    sun: None,
                }],
                geometry: GeomFile::default(),
                objects: LevelObjects::default(),
                particle_shaders: Vec::new(),
                particle_capacity: 0,
                glow_shaders: Vec::new(),
                light_textures: None,
            }),
            collision: Arc::new(
                CollisionWorld::new(Default::default(), Vec::new()).expect("collision"),
            ),
            sound_data: None,
            fatal: None,
            footsteps: None,
            sky_visibility: SkyVisibility::default(),
            player_sphere: ([0.0; 3], 0.0),
            frames: 0,
            frame_s: 0.0,
            player,
            character: None,
            controls,
            near: camera::NEAR,
            environment,
            weather,
            clock,
            modifiers: Vec::new(),
            shown: now,
            last: now,
            back: false,
            sun_fade: SunFade::default(),
            simulation: None,
            placements: Arc::new(Vec::new()),
            instances: HashMap::new(),
            effects: None,
            view_size: std::cell::Cell::new((1024, 768)),
            zones: Zones::default(),
            zone_sections: HashMap::new(),
            weather_effect: None,
            effects_drawn: [true; 3],
            lights: LevelLights::default(),
            torches: PlayerTorches::default(),
            detectors: PlayerDetectors::default(),
            global_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::shaders_xr::Definition;

    /// level.md 3: an entry no draw uses is still created, so one the
    /// library lacks (static-shading.md 3.4) or with too few textures (7.1)
    /// fails the load; a good one has its textures located.
    #[test]
    fn an_unused_shader_table_entry_fails_the_load() {
        let definitions: HashMap<String, Definition> = [(
            "default".to_owned(),
            Definition {
                tag: "LM      ".into(),
                name: "default".into(),
                version: 1,
                properties: Vec::new(),
            },
        )]
        .into();
        let associations = HashMap::new();
        let library = ShaderLibrary {
            definitions: &definitions,
            scripted: &|_| false,
            associations: &associations,
        };
        let create = |entries: &[&str]| {
            let shaders: Vec<String> = entries.iter().map(|&e| e.to_owned()).collect();
            let mut located = Vec::new();
            create_table(&library, &shaders, &mut |t| {
                located.push(t.clone());
                Ok(t)
            })
            .map(|()| located)
        };
        assert_eq!(
            create(&["", "default/a,lmap#1_1,lmap#1_2"]).unwrap(),
            ["a", "lmap#1_1", "lmap#1_2"]
        );
        let err = create(&["default/a,lmap#1_1,lmap#1_2", "nothing/x"]).unwrap_err();
        assert!(
            err.to_string().contains("nothing not found in library"),
            "{err}"
        );
        let err = create(&["default/a,lmap#1_1"]).unwrap_err();
        assert!(err.to_string().contains("Not enought textures"), "{err}");
    }
}
