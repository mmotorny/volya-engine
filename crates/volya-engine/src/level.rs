// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game's level (docs/behavior/level-loading.md): which level, its
//! static world, and the view from the player's start
//! (docs/behavior/start-position.md), under the weather of the game clock's
//! time of day (docs/formats/sav.md sections 5 and 12,
//! docs/behavior/static-shading.md section 9), with its collision world
//! (docs/formats/level-cform.md), walked by the player in first person
//! (docs/behavior/player-movement.md).
//!
//! [`LevelScreen::load`] reads everything synchronously through the VFS;
//! the GPU copy of the geometry is made by the presenter on the first frame
//! drawn ([`LevelWorld::take_geometry`]), after which the CPU copy is gone.

use std::io;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::controls::Controls;
use crate::core::camera::{self, Angles, Mat4, Vec3};
use crate::core::clock::GameClock;
use crate::core::collision::{CollisionWorld, geometry};
use crate::core::config;
use crate::core::config::Config;
use crate::core::input::Input;
use crate::core::level::{DrawItem, Level, LevelGeom};
use crate::core::movement::{Flags, MovementConfig, Player};
use crate::core::vfs::{self, Vfs};
use crate::core::weather::{Environment, Weather};
use crate::data::GameData;
use crate::loading_screen::Stage;
use crate::render::sky::{SkySection, SkyTextures};
use crate::render::world::Surface;
use crate::scene::{Scene, WorldScene};
use crate::shading::{self, ShaderLibrary};
use crate::sky::{self, Flares, SkyCamera, SunFade};
use std::collections::HashMap;
use volya_engine_formats::env_mod::{self, EnvModifier};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::gamemtl;
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
    /// The vertex and index buffers until the presenter uploads them.
    geometry: Mutex<Option<LevelGeom>>,
}

impl LevelWorld {
    /// The geometry, once: whoever takes it owns the only copy.
    pub fn take_geometry(&self) -> Option<LevelGeom> {
        self.geometry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

/// The level shown.
pub struct LevelScreen {
    world: Arc<LevelWorld>,
    collision: CollisionWorld,
    player: Player,
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
    let level = graph.vertex_level(actor.game_graph_vertex)?;
    Ok((level.name.clone(), actor))
}

/// The weather name of `level` (`[<level>] weathers` of `game.ltx`'s
/// includes), level-loading.md section 2.6. The weather script is replaced
/// by its result.
fn start_weather(tree: &dyn Vfs, data: &GameData, level: &str) -> io::Result<String> {
    let game = config::load(tree, &data.aliases.path("$game_config$", "game.ltx")?)?;
    Ok(game
        .get(level, "weathers")
        .flatten()
        // Our choice when the level has no `weathers` key.
        .unwrap_or("default")
        .to_owned())
}

/// `config\alife.ltx`, or the system configuration where it holds
/// `[alife]` already.
fn alife(tree: &dyn Vfs, data: &GameData) -> io::Result<Config> {
    match data.system.section("alife") {
        Some(_) => Ok(data.system.clone()),
        None => config::load(tree, &data.aliases.path("$game_config$", "alife.ltx")?),
    }
}

/// The file of texture `name`: `$level$<name>.dds`, then
/// `$game_textures$<name>.dds` (the saved-games folder has no place on this
/// platform); neither is "Can't find texture" (static-shading.md 4.2).
fn locate_texture(tree: &dyn Vfs, data: &GameData, dir: &str, name: &str) -> io::Result<String> {
    let level = format!("{dir}\\{name}.dds");
    if tree.open(&level).is_ok() {
        return Ok(level);
    }
    let path = data
        .aliases
        .path("$game_textures$", &format!("{name}.dds"))?;
    tree.open(&path)
        .map_err(|_| invalid(format!("Can't find texture '{name}'")))?;
    Ok(path)
}

/// The surfaces of the shader entries `draw_items` use, each draw item's
/// surface, and the entries not drawn with why. A texture that resolves
/// nowhere fails the load ("Can't find texture", static-shading.md 4.2).
#[allow(clippy::type_complexity)]
fn resolve_surfaces(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    shaders: &[String],
    draw_items: &[DrawItem],
) -> io::Result<(
    Vec<Surface<String>>,
    Vec<Option<u32>>,
    Vec<(String, String)>,
)> {
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
    let mut surfaces = Vec::new();
    let mut by_entry: HashMap<u16, Option<u32>> = HashMap::new();
    let mut not_drawn = Vec::new();
    let mut draw_surfaces = Vec::with_capacity(draw_items.len());
    for item in draw_items {
        if let Some(s) = by_entry.get(&item.shader) {
            draw_surfaces.push(*s);
            continue;
        }
        let entry = shaders
            .get(usize::from(item.shader))
            .map_or("", String::as_str);
        let surface = match library.resolve(entry) {
            Ok(surface) => {
                let [a, b, c, d] = surface.textures.clone();
                let mut resolved = surface.map_textures(|_| None::<String>);
                resolved.textures = [
                    a.map(&mut locate).transpose()?,
                    b.map(&mut locate).transpose()?,
                    c.map(&mut locate).transpose()?,
                    d.map(&mut locate).transpose()?,
                ];
                surfaces.push(resolved);
                Some(surfaces.len() as u32 - 1)
            }
            Err(why) => {
                not_drawn.push((entry.to_owned(), why));
                None
            }
        };
        by_entry.insert(item.shader, surface);
        draw_surfaces.push(surface);
    }
    Ok((surfaces, draw_surfaces, not_drawn))
}

/// A new game's level being loaded stage by stage
/// (docs/behavior/loading-screen.md section 5): [`NewGameLoad::run`] does
/// the part of each stage's work this engine has, in the stages' order;
/// [`NewGameLoad::finish`] makes the level. Stages whose systems do not
/// exist here (server, life simulation, details, AI, objects) do nothing.
#[derive(Default)]
pub struct NewGameLoad {
    name: Option<String>,
    actor: Option<spawn::ActorStart>,
    dir: String,
    level: Option<Level>,
    geom: Option<LevelGeom>,
    collision: Option<CollisionWorld>,
    draw_items: Vec<DrawItem>,
    spheres: Vec<[f32; 4]>,
    modifiers: Vec<EnvModifier>,
    movement: Option<MovementConfig>,
    controls: Option<Controls>,
    weather: Option<Weather>,
    clock: Option<GameClock>,
    surfaces: Vec<Surface<String>>,
    draw_surfaces: Vec<Option<u32>>,
    not_drawn: Vec<(String, String)>,
    sky: Option<SkyTextures<String>>,
    flares: Vec<Flares>,
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

    /// Does the work of `stage`.
    pub fn run(&mut self, stage: Stage, tree: &dyn Vfs, data: &GameData) -> io::Result<()> {
        let dir = self.dir.clone();
        let file =
            |name: &str, e: io::Error| io::Error::new(e.kind(), format!("{dir}\\{name}: {e}"));
        match stage {
            // The level and its start (level-loading.md section 2.2).
            Stage::CreatingNewGame => {
                let (name, actor) = new_game_level(tree, data)?;
                self.dir = data.aliases.path("$game_levels$", &name)?;
                self.name = Some(name);
                self.actor = Some(actor);
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
                self.level = Some(Level::parse(&bytes).map_err(|e| file("level", e))?);
            }
            // The collision mesh with its materials mapped and indexed
            // (level-cform.md sections 2, 3.3). `gamemtl.xr` is read here,
            // not at game start as the game does (our choice; only this
            // needs it). Without it there are no materials, so the mapping
            // fails as the game's does (section 1).
            Stage::LoadingCform => {
                let path = data.aliases.path("$game_data$", "gamemtl.xr")?;
                let materials = match vfs::read(tree, &path) {
                    Ok(bytes) => gamemtl::parse(&bytes)
                        .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
                    Err(e) => return Err(e),
                };
                // Read as a stream into the world's own form, so the file
                // is never held whole.
                let f = tree
                    .open(&format!("{dir}\\level.cform"))
                    .map_err(|e| file("level.cform", e))?;
                self.collision =
                    Some(CollisionWorld::read(f, materials).map_err(|e| file("level.cform", e))?);
            }
            Stage::LoadingGeometry => {
                let mut f = tree
                    .open(&format!("{dir}\\level.geom"))
                    .map_err(|e| file("level.geom", e))?;
                self.geom = Some(LevelGeom::read(&mut f).map_err(|e| file("level.geom", e))?);
            }
            // The static visuals' draw list and bounding spheres (placed
            // under this caption by us: the spec names the spatial index).
            Stage::LoadingSpatialDb => {
                let level = earlier(self.level.as_ref(), "level")?;
                let geom = earlier(self.geom.as_ref(), "level.geom")?;
                let draw_items = level
                    .static_draw_items(geom)
                    .map_err(|e| file("level", e))?;
                self.spheres = draw_items
                    .iter()
                    .map(|d| {
                        let h = &level.visuals[d.visual as usize].header;
                        let c = h.sphere_centre;
                        [c[0], c[1], c[2], h.sphere_radius]
                    })
                    .collect();
                self.draw_items = draw_items;
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
            // New Game's clock (sav.md section 5), placed under this caption
            // by us (the client creates the objects here).
            Stage::ClientSpawning => {
                let name = earlier(self.name.as_deref(), "level")?;
                // player-movement.md section 1: the actor's section is the
                // one its spawn record names.
                self.movement = Some(MovementConfig::read(&data.system, spawn::ACTOR_SECTION)?);
                self.controls = Some(data.controls.clone());
                let weather_name = start_weather(tree, data, name)?;
                self.weather = Some(Weather::load(&data.system, &weather_name)?);
                self.clock = Some(GameClock::new_game(&alife(tree, data)?)?);
            }
            // Every texture file the level needs is located here. The
            // presenter reads and uploads them with the first frame that
            // draws the world, the first under the loading image: our
            // departure from section 5, which reads them under this caption.
            Stage::LoadingTextures => {
                let level = earlier(self.level.as_ref(), "level")?;
                (self.surfaces, self.draw_surfaces, self.not_drawn) =
                    resolve_surfaces(tree, data, &dir, &level.shaders, &self.draw_items)?;
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
                    let mut f = Flares::read(&data.system, &s.flares);
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
                self.flares = flares;
            }
            Stage::PrefetchingObjects
            | Stage::ServerStarting
            | Stage::LoadingAlifeSimulator
            | Stage::ServerConnecting
            | Stage::LoadingShaders
            | Stage::LoadingDetails
            | Stage::LoadingAiObjects
            | Stage::ClientSynchronising => {}
        }
        Ok(())
    }

    /// The level after every stage ran, shown from engine clock `now`.
    pub fn finish(self, now: u64) -> io::Result<LevelScreen> {
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
                geometry: Mutex::new(Some(geom)),
            }),
            collision,
            player,
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
        };
        screen.update_view();
        Ok(screen)
    }
}

impl LevelScreen {
    /// Loads the new game's level at engine clock `now`: every stage of
    /// [`NewGameLoad`] at once.
    pub fn load(tree: &dyn Vfs, data: &GameData, now: u64) -> io::Result<Self> {
        let mut load = NewGameLoad::default();
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
    }

    /// One frame at `now` with this frame's `input`. The game clock
    /// advances by the real time since the previous frame, uncapped (our
    /// choice: the specs cap only the movement's and the display's frame
    /// times, player-movement.md 3.2).
    pub fn update(&mut self, now: u64, input: &Input) {
        let elapsed = now.saturating_sub(self.last);
        let dt = (elapsed as f32 / 1000.0).min(MAX_STEP);
        self.last = now;
        self.clock.set_paused(false);
        self.clock.advance(elapsed);
        let player = self.player.state();
        let (request, back) = self.controls.request(input, &player);
        self.player.frame(&self.collision, now, &request);
        self.back |= back;
        // sky.md 7.3, k_t from the game clock.
        let (a, b, t) = self.weather.bracket_indices(self.clock.time_of_day());
        self.sun_fade.frame(
            sky::flares_in_effect(a, b, t),
            &self.world.flares,
            dt,
            self.clock.time_factor(),
        );
        self.update_view();
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
        let mut scene = Scene::new(resolution);
        scene.world = Some(WorldScene {
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
                crouch_toggled: p.crouch_toggled,
                sprint_requested: p.sprint_requested,
                sprint_locked: self.controls.sprint_lock.locked(),
            },
        }
    }

    /// The player in the level.
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// The player's controls.
    pub fn controls(&self) -> &Controls {
        &self.controls
    }
}
