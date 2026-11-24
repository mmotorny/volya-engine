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

use crate::core::camera::{self, ActorBody, Angles, Mat4, Vec3};
use crate::core::config;
use crate::core::input::{Action, Control, Input, Key};
use crate::core::level::{DrawItem, StaticWorld};
use crate::core::vfs::{self, Vfs};
use crate::core::weather::{self, Environment, Weather};
use crate::data::GameData;
use crate::scene::{Scene, WorldScene};
use volya_engine_formats::env_mod::{self, EnvModifier};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::level_geom::LevelGeom;
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
    eye: Vec3,
    angles: Angles,
    weather: Weather,
    /// Time of day, seconds.
    time_of_day: f32,
    modifiers: Vec<EnvModifier>,
    environment: Environment,
    /// When the level was shown first (engine clock, ms).
    shown: u64,
    back: bool,
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

impl LevelScreen {
    /// Loads the new game's level at engine clock `now`.
    pub fn load(tree: &dyn Vfs, data: &GameData, now: u64) -> io::Result<Self> {
        let (name, actor) = new_game_level(tree, data)?;
        let dir = data.aliases.path("$game_levels$", &name)?;
        for file in LEVEL_FILES {
            tree.open(&format!("{dir}\\{file}"))
                .map_err(|e| io::Error::new(e.kind(), format!("{dir}\\{file}: {e}")))?;
        }
        let StaticWorld {
            level,
            geom,
            draw_items,
            ..
        } = StaticWorld::load(tree, &dir)?;
        // Optional (sky.md 9.1).
        let modifiers = match vfs::read(tree, &format!("{dir}\\level.env_mod")) {
            Ok(file) => env_mod::parse(&file)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        let body = ActorBody::from_section(
            data.system
                .section(spawn::ACTOR_SECTION)
                .ok_or_else(|| invalid(format!("no section [{}]", spawn::ACTOR_SECTION)))?,
        )?;
        let (weather_name, time_of_day) = start_weather(tree, data, &name)?;
        let weather = Weather::load(&data.system, &weather_name)?;
        let eye = body.eye(actor.position);
        let mut screen = Self {
            world: Arc::new(LevelWorld {
                name,
                dir,
                shaders: level.shaders,
                draw_items,
                geometry: Mutex::new(Some(geom)),
            }),
            eye,
            angles: Angles::from_spawn(actor.angles),
            environment: {
                let (a, b, t) = weather.bracket(time_of_day);
                Environment::blend(a, b, t, &modifiers, eye, VIS_DISTANCE)
            },
            weather,
            time_of_day,
            modifiers,
            shown: now,
            back: false,
        };
        screen.update_environment();
        Ok(screen)
    }

    pub fn world(&self) -> &Arc<LevelWorld> {
        &self.world
    }

    /// The environment at the camera (static-shading.md 9.2). Game time
    /// stays at the start time (our choice for a static world).
    fn update_environment(&mut self) {
        let (a, b, t) = self.weather.bracket(self.time_of_day);
        self.environment = Environment::blend(a, b, t, &self.modifiers, self.eye, VIS_DISTANCE);
    }

    /// One frame at `now` with this frame's `input`.
    pub fn update(&mut self, _now: u64, input: &Input) {
        for action in input.actions() {
            if let Action::Press {
                control: Control::Key(Key::Escape),
                ..
            } = action
            {
                self.back = true;
            }
        }
        self.update_environment();
    }

    /// Whether the player asked to go back to the title menu (our design:
    /// Escape, or the gamepad's east button, which acts as Escape).
    pub fn back_requested(&self) -> bool {
        self.back
    }

    /// View and projection for a `width` × `height` target
    /// (start-position.md sections 5–7, the smoothing of 7.2 replaced by its
    /// targets, which it allows; near plane 0.2 m: collision is not loaded).
    pub fn view_projection(&self, width: u32, height: u32) -> (Mat4, Mat4) {
        let (d, u) = self.angles.direction_up();
        let view = camera::view_matrix(self.eye, d, u);
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
        let mut scene = Scene::new(resolution);
        scene.world = Some(WorldScene {
            world: self.world.clone(),
            eye: self.eye,
            view,
            projection,
            environment: self.environment.clone(),
            time: clock_ms.saturating_sub(self.shown) as f32 / 1000.0,
        });
        scene
    }

    pub fn state(&self) -> LevelState {
        LevelState {
            name: self.world.name.clone(),
            draws: self.world.draw_items.len(),
            eye: self.eye,
            yaw: self.angles.yaw,
            pitch: self.angles.pitch,
            far_plane: self.environment.far_plane,
            time_of_day: self.time_of_day,
        }
    }
}
