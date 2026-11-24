// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game's level (docs/behavior/level-loading.md): which level, its
//! static world, and the view from the player's start
//! (docs/behavior/start-position.md), under the weather of the start time
//! (docs/behavior/static-shading.md section 9).
//!
//! [`LevelScreen::load`] reads everything synchronously through the VFS;
//! the GPU copy of the geometry is made by the presenter on the first frame
//! drawn ([`LevelWorld::take_geometry`]), after which the CPU copy is gone.

use std::io;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::core::camera::{self, ActorBody, Angles, FreeCamera, Mat4, Vec3};
use crate::core::config;
use crate::core::input::Input;
use crate::core::level::{DrawItem, Level, LevelGeom};
use crate::core::vfs::{self, Vfs};
use crate::core::weather::{self, Environment, Weather};
use crate::data::GameData;
use crate::free_controls::{self, TouchLayout};
use crate::loading_screen::Stage;
use crate::render::sky::SkyTextures;
use crate::render::world::Surface;
use crate::scene::{Scene, Sprite, TextureId, WorldScene};
use crate::shading::{self, ShaderLibrary};
use crate::sky::{self, SkyCamera, SunSource};
use std::collections::HashMap;
use volya_engine_formats::env_mod::{self, EnvModifier};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::shaders_xr;
use volya_engine_formats::spawn;
use volya_engine_formats::ui::Color;
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::ui::picture::Quad;

/// The spawn file's name: the first server parameter `all`
/// (level-loading.md section 2.1).
pub const SPAWN_NAME: &str = "all";
/// Files a level directory must hold to be a level (section 2.2).
pub const LEVEL_FILES: [&str; 4] = ["level", "level.ltx", "level.geom", "level.cform"];
/// `rs_vis_distance` of the shipped `user.ltx` (start-position.md 7.1); the
/// user settings are not read for it here (our choice).
pub const VIS_DISTANCE: f32 = 1.0;
/// Longest frame the free camera moves by, seconds.
const MAX_STEP: f32 = 0.1;
/// Time of day when `config\alife.ltx` gives none (our choice; the shipped
/// file says 05:30:00).
const DEFAULT_START_TIME: f32 = 5.5 * 3600.0;

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
    /// The sky's textures as file paths (sky.md sections 5–7).
    pub sky: SkyTextures<String>,
    /// The sun sprite, its texture as a file path.
    pub sun: Option<SunSource>,
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
    camera: FreeCamera,
    weather: Weather,
    /// Time of day, seconds.
    time_of_day: f32,
    modifiers: Vec<EnvModifier>,
    environment: Environment,
    /// When the level was shown first (engine clock, ms).
    shown: u64,
    /// The previous update (engine clock, ms).
    last: u64,
    back: bool,
    /// Whether touch was used: the touch controls show from then on.
    touch_seen: bool,
    /// The sun sprite's fade (sky.md 7.3) and whether the first frame,
    /// on which it stays 0, has passed.
    sun_fade: f32,
    sun_started: bool,
    /// How many times faster game time runs than real time.
    time_factor: f32,
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
    pub time_of_day: f32,
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
/// includes) and the start time (`[alife] start_time`), level-loading.md
/// section 2.6. The weather script is replaced by its result.
fn start_weather(tree: &dyn Vfs, data: &GameData, level: &str) -> io::Result<(String, f32)> {
    let game = config::load(tree, &data.aliases.path("$game_config$", "game.ltx")?)?;
    let name = game
        .get(level, "weathers")
        .flatten()
        // Our choice when the level has no `weathers` key.
        .unwrap_or("default")
        .to_owned();
    let alife = match data.system.section("alife") {
        Some(_) => data.system.clone(),
        None => config::load(tree, &data.aliases.path("$game_config$", "alife.ltx")?)?,
    };
    let time = alife
        .get("alife", "start_time")
        .flatten()
        .and_then(weather::parse_time)
        .unwrap_or(DEFAULT_START_TIME);
    Ok((name, time))
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

/// `[alife] time_factor`, presumed to be the clock's rate for a New Game
/// (sky.md 7.3); 10 when absent (the shipped value; our choice).
fn time_factor(tree: &dyn Vfs, data: &GameData) -> io::Result<f32> {
    let alife = match data.system.section("alife") {
        Some(_) => data.system.clone(),
        None => config::load(tree, &data.aliases.path("$game_config$", "alife.ltx")?)?,
    };
    Ok(alife
        .get("alife", "time_factor")
        .flatten()
        .map_or(10.0, config::value::float))
}

/// A new game's level being loaded stage by stage
/// (docs/behavior/loading-screen.md section 5): [`NewGameLoad::run`] does
/// the part of each stage's work this engine has, in the stages' order;
/// [`NewGameLoad::finish`] makes the level. Stages whose systems do not
/// exist here (server, life simulation, collision, details, AI, objects)
/// do nothing.
#[derive(Default)]
pub struct NewGameLoad {
    name: Option<String>,
    actor: Option<spawn::ActorStart>,
    dir: String,
    level: Option<Level>,
    geom: Option<LevelGeom>,
    draw_items: Vec<DrawItem>,
    spheres: Vec<[f32; 4]>,
    modifiers: Vec<EnvModifier>,
    body: Option<ActorBody>,
    weather: Option<Weather>,
    time_of_day: f32,
    time_factor: f32,
    surfaces: Vec<Surface<String>>,
    draw_surfaces: Vec<Option<u32>>,
    not_drawn: Vec<(String, String)>,
    sky: Option<SkyTextures<String>>,
    sun: Option<SunSource>,
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
            Stage::LoadingGeometry => {
                let mut f = tree.open(&format!("{dir}\\level.geom"))?;
                self.geom = Some(LevelGeom::read(&mut f).map_err(|e| file("level.geom", e))?);
            }
            // The static visuals' draw list and bounding spheres.
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
            // The actor and the start weather (level-loading.md 2.6).
            Stage::ClientSpawning => {
                let name = earlier(self.name.as_deref(), "level")?;
                self.body = Some(ActorBody::from_section(
                    data.system
                        .section(spawn::ACTOR_SECTION)
                        .ok_or_else(|| invalid(format!("no section [{}]", spawn::ACTOR_SECTION)))?,
                )?);
                let (weather_name, time_of_day) = start_weather(tree, data, name)?;
                self.weather = Some(Weather::load(&data.system, &weather_name)?);
                self.time_of_day = time_of_day;
                self.time_factor = time_factor(tree, data)?;
            }
            // Every texture file the level needs is located here; the
            // presenter reads and uploads them with the first frame that
            // draws the world (the first frame under the loading image).
            Stage::LoadingTextures => {
                let level = earlier(self.level.as_ref(), "level")?;
                (self.surfaces, self.draw_surfaces, self.not_drawn) =
                    resolve_surfaces(tree, data, &dir, &level.shaders, &self.draw_items)?;
                // The sky's textures: the bracketing sections' cube maps and
                // cloud textures; the sun sprite of the lens-flare section in
                // effect. Game time stays at the start, so these stay too.
                let weather = earlier(self.weather.as_ref(), "weather")?;
                let (a, b, t) = weather.bracket(self.time_of_day);
                let locate = |name: &str| locate_texture(tree, data, &dir, name);
                self.sky = Some(SkyTextures {
                    cubes: [locate(&a.sky_texture)?, locate(&b.sky_texture)?],
                    clouds: [locate(&a.clouds_texture)?, locate(&b.clouds_texture)?],
                    sun: None,
                });
                self.sun = SunSource::read(&data.system, sky::flares_section(a, b, t))
                    .map(|s| {
                        Ok::<_, io::Error>(SunSource {
                            texture: locate(&s.texture)?,
                            ..s
                        })
                    })
                    .transpose()?;
            }
            Stage::PrefetchingObjects
            | Stage::ServerStarting
            | Stage::LoadingAlifeSimulator
            | Stage::ServerConnecting
            | Stage::LoadingCform
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
        let body = earlier(self.body, "actor body")?;
        let weather = earlier(self.weather, "weather")?;
        let sky = earlier(self.sky, "sky")?;
        let eye = body.eye(actor.position);
        let (a, b, t) = weather.bracket(self.time_of_day);
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
                sun: self.sun,
                geometry: Mutex::new(Some(geom)),
            }),
            camera: FreeCamera::new(eye, Angles::from_spawn(actor.angles)),
            environment,
            weather,
            time_of_day: self.time_of_day,
            modifiers: self.modifiers,
            shown: now,
            last: now,
            back: false,
            touch_seen: false,
            sun_fade: 0.0,
            sun_started: false,
            time_factor: self.time_factor,
        };
        screen.update_environment();
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

    /// Places the free camera (tests and debugging).
    pub fn set_camera(&mut self, eye: Vec3, angles: Angles) {
        self.camera = FreeCamera::new(eye, angles);
        self.update_environment();
    }

    pub fn world(&self) -> &Arc<LevelWorld> {
        &self.world
    }

    /// The environment at the camera (static-shading.md 9.2). Game time
    /// stays at the start time (our choice for a static world).
    fn update_environment(&mut self) {
        let (a, b, t) = self.weather.bracket(self.time_of_day);
        self.environment =
            Environment::blend(a, b, t, &self.modifiers, self.camera.eye, VIS_DISTANCE);
    }

    /// One frame at `now` with this frame's `input`.
    pub fn update(&mut self, now: u64, input: &Input) {
        // Frames longer than MAX_STEP (the first after loading) move the
        // camera as if MAX_STEP long, so it does not jump (our choice).
        let dt = (now.saturating_sub(self.last) as f32 / 1000.0).min(MAX_STEP);
        self.last = now;
        let (m, back) = free_controls::free_move(input, dt);
        self.camera.apply(&m, dt);
        self.back |= back;
        self.touch_seen |= !input.contacts().is_empty();
        // sky.md 7.3: 0 on the first frame, then rising by λ↑·dt·k_t.
        if let Some(sun) = &self.world.sun {
            if self.sun_started {
                self.sun_fade = (self.sun_fade + sun.rise * dt * self.time_factor).min(1.0);
            }
            self.sun_started = true;
        }
        self.update_environment();
    }

    /// Whether the player asked to go back to the title menu
    /// ([`free_controls`]).
    pub fn back_requested(&self) -> bool {
        self.back
    }

    /// View and projection for a `width` × `height` target
    /// (start-position.md sections 5–7, the smoothing of 7.2 replaced by its
    /// targets, which it allows; near plane 0.2 m: collision is not loaded).
    pub fn view_projection(&self, width: u32, height: u32) -> (Mat4, Mat4) {
        let (d, u) = self.camera.angles.direction_up();
        let view = camera::view_matrix(self.camera.eye, d, u);
        let projection = camera::projection(
            camera::FOV_Y_DEGREES.to_radians(),
            camera::aspect_factor(width, height),
            camera::NEAR,
            self.environment.far_plane,
        );
        (view, projection)
    }

    /// The frame for `resolution`.
    pub fn scene(&self, resolution: Resolution, clock_ms: u64) -> Scene {
        let (view, projection) = self.view_projection(resolution.width, resolution.height);
        let (d, _) = self.camera.angles.direction_up();
        let mut scene = Scene::new(resolution);
        scene.world = Some(WorldScene {
            world: self.world.clone(),
            eye: self.camera.eye,
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
                &SkyCamera {
                    eye: self.camera.eye,
                    forward: d,
                    right: [view[0][0], view[1][0], view[2][0]],
                },
                self.world.sun.as_ref(),
                self.sun_fade,
            )),
        });
        if self.touch_seen {
            touch_overlay(&mut scene);
        }
        scene
    }

    pub fn state(&self) -> LevelState {
        LevelState {
            name: self.world.name.clone(),
            draws: self.world.draw_items.len(),
            eye: self.camera.eye,
            yaw: self.camera.angles.yaw,
            pitch: self.camera.angles.pitch,
            far_plane: self.environment.far_plane,
            time_of_day: self.time_of_day,
        }
    }
}

/// The touch controls ([`free_controls`]) drawn over the world: translucent
/// squares with a cross (back) and arrows (up, down).
fn touch_overlay(scene: &mut Scene) {
    let (w, h) = (
        scene.resolution.width as f32,
        scene.resolution.height as f32,
    );
    let layout = TouchLayout::new(w, h);
    let panel = Color::rgba(0, 0, 0, 96);
    let mark = Color::rgba(255, 255, 255, 192);
    let square = |layer: &str, r: [f32; 4], color| {
        Sprite::picture(
            layer,
            TextureId::White,
            &Quad {
                min: (r[0], r[1]),
                max: (r[2], r[3]),
                uv_min: (0.0, 0.0),
                uv_max: (1.0, 1.0),
                color,
            },
        )
    };
    // A shape from four corners (two equal corners make a triangle).
    let shape = |layer: &str, corners: [[f32; 2]; 4]| Sprite {
        layer: layer.into(),
        texture: TextureId::White,
        corners,
        uv_min: [0.0, 0.0],
        uv_max: [1.0, 1.0],
        color: mark,
        corner_colors: None,
    };
    for (layer, r) in [
        ("touch back", layout.back),
        ("touch up", layout.up),
        ("touch down", layout.down),
    ] {
        scene.sprites.push(square(layer, r, panel));
        let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        let s = (r[2] - r[0]) * 0.3;
        let t = s * 0.15;
        match layer {
            "touch back" => {
                for k in [1.0f32, -1.0] {
                    // A bar from (−s, −k·s) to (s, k·s), t thick.
                    let (nx, ny) = (k * t, -t);
                    scene.sprites.push(shape(
                        layer,
                        [
                            [cx - s - nx, cy - k * s - ny],
                            [cx + s - nx, cy + k * s - ny],
                            [cx + s + nx, cy + k * s + ny],
                            [cx - s + nx, cy - k * s + ny],
                        ],
                    ));
                }
            }
            _ => {
                let k = if layer == "touch up" { -1.0 } else { 1.0 };
                let tip = [cx, cy + k * s];
                scene.sprites.push(shape(
                    layer,
                    [tip, tip, [cx + s, cy - k * s], [cx - s, cy - k * s]],
                ));
            }
        }
    }
}
