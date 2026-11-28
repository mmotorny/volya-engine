// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's scripts from program start to a New Game's first frame
//! (docs/behavior/script-api-engine-entry-points.md section 3; section
//! numbers are its): startup at program start, then the engine's calls
//! into scripts placed in the New Game's loading captions
//! (docs/behavior/loading-screen.md section 5) whose work they belong to.
//! See docs/architecture/scripts.md, Entry points.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use crate::core::character::hits::{Hit, HitType};
use crate::core::config::{self, Config};
use crate::core::simulation::ACTOR_ID;
use crate::core::zones;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::loading_screen::{GAME_TYPE, Loading, Stage};
use crate::objects::{LoadedModel, Names};
use crate::script::classes::OverrideFailure;
use crate::script::entry_points::{GameScripts, Startup};
use crate::script::files::FileSystem;
use crate::script::game_object::{
    EngineState, Event, GameObjects, ObjectClass, Physics, Restrictor, SpawnData, Visual,
};
use crate::script::loader::{Prefix, Trees};
use crate::script::object_factory::{CreatedObject, GameTypeKind, Sides};
use crate::script::services::EngineServices;
use crate::script::story::Story;
use crate::script::{Error, ErrorKind};
use volya_engine_formats::cp1251;
use volya_engine_formats::model;
use volya_engine_formats::object::{Object, ObjectRecord, Shape};

/// The server options of a New Game from the menu, `start
/// server(all/single/alife/new)` (level-loading.md section 2.1).
pub const SERVER_OPTIONS: &str = "all/single/alife/new";

/// The precache phase the client's synchronising asks for (3.3 step
/// 11.6; loading-screen.md's 30 frames).
pub const PRECACHE_FRAMES: u32 = 30;

/// Program start (3.1 step 2): the engine state the script interface
/// reads, and startup on the first game state, over the game's trees.
pub fn start(data: &GameData, trees: Rc<dyn Trees>) -> io::Result<GameScripts> {
    let mut game = Err(io::Error::other("the game's files are unavailable"));
    trees.with_trees(&mut |tree, _| {
        game = config::load_aliased(tree, &data.aliases, "$game_config$", "game.ltx");
    })?;
    let system = Rc::new(data.system.clone());
    // TODO(#941): the services' console is the app's.
    let services = Rc::new(RefCell::new(EngineServices::new(
        data.console(),
        data.system.clone(),
    )));
    // TODO(#1003): the loose folders, the local time zone and the disk
    // changes from the platform.
    let files = FileSystem::new(
        Rc::clone(&trees),
        data.aliases.clone(),
        system,
        Rc::new(game?),
    );
    // Volya Engine has no command line: the default prefix, and the
    // rebuild is never kept (`-keep_lua`).
    let prefix = Prefix::from_command_line(&files.command_line);
    GameScripts::start(Startup {
        trees,
        aliases: data.aliases.clone(),
        prefix,
        services,
        files: Rc::new(RefCell::new(files)),
        // The level's objects as scripts see them, fed by the level
        // ([`client_spawning`], [`level_frame`]).
        objects: Rc::new(RefCell::new(GameObjects::new())),
        // TODO(#1063): the game's story state.
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
    })
    .map_err(fatal)
}

/// A script error's report: the original's fatal error, the game's end.
pub fn fatal(e: Error) -> io::Error {
    io::Error::other(e.message)
}

/// The engine's calls into scripts in the work of New Game caption
/// `stage`, made before the caption's own work (3.3):
///
/// - `st_server_starting` (steps 8 and 9.1 to 9.3): the server's game
///   object, the game-type function asked on the server side in the
///   state of the latest startup;
/// - `st_loading_alife_simulator` (9.4): the simulation created, with the
///   rebuild and the start callback;
/// - `st_client_connecting_to` (11.2): the client's game object, the
///   game-type function asked on the client side in the new state;
/// - `st_client_spawning` (11.4): none before the caption's work; the
///   level's client objects come after it ([`client_spawning`]);
/// - `st_client_synchronising` (11.6): the precache phase.
///
/// Which caption each step shows under is our placement: the specs give
/// the stages' order, not the captions' boundaries within them.
pub fn stage(scripts: &mut GameScripts, stage: Stage, data: &GameData) -> Result<(), Error> {
    match stage {
        Stage::ServerStarting => {
            scripts.create_game_object(GAME_TYPE, true)?;
            // TODO(#997): gap 3, where the game-type script list is made:
            // as here (9.3), from `script.ltx [single] script`, empty in
            // this edition, so no coroutine.
        }
        Stage::LoadingAlifeSimulator => scripts.create_simulation(SERVER_OPTIONS, &data.system)?,
        // The New Game's records (offline 8.1) are the simulation's
        // (TODO(#950): their script server classes through the factory).
        Stage::ClientConnectingTo => {
            scripts.create_game_object(GAME_TYPE, false)?;
        }
        Stage::ClientSynchronising => scripts
            .services()
            .borrow_mut()
            .device
            .request_precache(PRECACHE_FRAMES),
        _ => {}
    }
    Ok(())
}

/// A New Game starts (3.2 step 7): the game type the services report, and
/// the level start's loading phase opened (spec B 4.1, 4.5).
pub fn new_game(scripts: &GameScripts) {
    let mut services = scripts.services().borrow_mut();
    services.game_started(SERVER_OPTIONS);
    services.open_loading_phase();
}

/// The level ends (the game is left for the menu, or a New Game starts
/// over it): every object the scripts still see is destroyed, its binder
/// getting `net_destroy()` (spec L 5.6; TODO(#1019): gap 2, the simplest
/// reading). A script error is fatal.
pub fn level_ended(scripts: &mut GameScripts) -> Result<(), Error> {
    let ids: Vec<u16> = scripts.objects().borrow().iter().map(|o| o.id).collect();
    for id in ids {
        let destroyed = scripts.scripts().destroy_object(id);
        destroyed.map_err(|f| override_fatal(scripts, f))?;
    }
    Ok(())
}

/// The New Game's load ended, with its level or not (3.3 step 12): the
/// loading phase closed.
pub fn load_ended(scripts: &GameScripts) {
    scripts.services().borrow_mut().close_loading_phase();
}

/// An object of the level as the scripts' spawning takes it (script API M
/// 3, 4, 5.4, 6.2): its record, its visual loaded with the level (with its
/// owner's names) and its shell's physics part; whether it is a door under
/// the door scheme (trader-door.md 1).
pub struct ClientObject<'a> {
    pub id: u16,
    pub record: &'a ObjectRecord,
    pub model: Option<(&'a LoadedModel, &'a Names)>,
    pub physics: Option<Physics>,
    pub door: bool,
}

/// What the level's spawning gives the scripts (3.3 step 11.4): the
/// objects online at the start, byte 11 of each level vertex (M 4) and the
/// game time.
pub struct ClientSpawning<'a> {
    pub objects: Vec<ClientObject<'a>>,
    pub vertex_lights: Vec<u8>,
    pub game_time: u64,
}

/// `st_client_spawning` (3.3 step 11.4), after the caption's own work
/// (our placement: the objects' visuals and shells are made by it): the
/// level vertices' light bytes and the objects online at the start, each
/// created through the factory and spawned (spec C 6, spec M 3, spec L
/// 5.1); the server time the engine clock `now` (ms).
pub fn client_spawning(
    scripts: &mut GameScripts,
    loading: &mut Loading,
    system: &Config,
    now: u64,
) -> Result<(), Error> {
    let spawning = loading.client_objects().map_err(io_fatal)?;
    {
        let mut o = scripts.objects().borrow_mut();
        o.level_vertex_lights = spawning.vertex_lights;
        o.server_time = now as u32;
        o.game_time = spawning.game_time;
    }
    for object in spawning.objects {
        spawn_client_object(scripts, system, object)?;
    }
    Ok(())
}

/// The scripts' part of a level frame, after the level's update: the
/// server and game times; the player's placement, its feet and its body's
/// heading (our reading of M 4); the objects that went offline or were
/// removed destroyed, those that came online spawned (M 2.1); the player's
/// use action on the object it targeted (M 6.3); then what the members
/// asked of the engine ([`apply_events`]).
pub fn level_frame(
    scripts: &mut GameScripts,
    level: &mut LevelScreen,
    system: &Config,
    now: u64,
) -> Result<(), Error> {
    {
        let mut o = scripts.objects().borrow_mut();
        o.server_time = now as u32;
        o.game_time = level.clock().time();
        if let Some(player) = o.get_mut(ACTOR_ID) {
            let p = level.player();
            player.placement = model::transform(p.state().position, [0.0, -p.heading(), 0.0]);
        }
    }
    let changes = level.take_script_changes();
    for &id in changes.offline.iter().chain(&changes.removed) {
        let destroyed = scripts.scripts().destroy_object(id);
        destroyed.map_err(|f| override_fatal(scripts, f))?;
    }
    for &id in &changes.online {
        if let Some(object) = level.client_object(id) {
            spawn_client_object(scripts, system, object)?;
        }
    }
    if let Some(target) = level.take_used()
        && scripts.objects().borrow().get(target).is_some()
    {
        // The engine's own handling that follows a usable object's (taking
        // hold of it, 6.3) is not made here; the door's use is the level's.
        let used = scripts.scripts().player_use(target, ACTOR_ID);
        used.map_err(|f| override_fatal(scripts, f))?;
    }
    apply_events(scripts, level);
    Ok(())
}

/// Spawns `object` in the scripts (spec C 6.1, 6.2; spec M 3, 10.2; spec
/// L 5.1): its client object created through the factory for its
/// section's class id; a script entry's object spawns through its
/// `net_Spawn`, an engine entry's as the engine's own kind (the player's
/// for `O_ACTOR` in the single-player game type). Its visual, physics part
/// and restrictor shape are given as it is made. A spawning that fails is
/// the scripts' to report (L 5.1); a script error is fatal.
pub fn spawn_client_object(
    scripts: &mut GameScripts,
    system: &Config,
    object: ClientObject,
) -> Result<(), Error> {
    if !binding_runs(system, &object) {
        return Ok(());
    }
    let data = SpawnData::from_record(object.id, object.record, system).map_err(io_fatal)?;
    let state = EngineState {
        visual: object.model.map(|(m, names)| visual_of(m, names)),
        physics: object.physics,
        restrictor: restrictor_of(object.record),
    };
    scripts.objects().borrow_mut().prepare(object.id, state);
    let created = scripts.scripts().create_client_object(data.class_id)?;
    let spawned = match created.object {
        CreatedObject::Script(client) => scripts.scripts().spawn(client, data).map(|_| ()),
        CreatedObject::Engine { entry_id, sides } => {
            let game_type = scripts.services().borrow().game_type as u32;
            let class = match sides {
                Sides::ByGameType if GameTypeKind::of(game_type) == GameTypeKind::ZeroOrOne => {
                    ObjectClass::Player
                }
                _ => ObjectClass::Unnamed(entry_id),
            };
            let spawned = scripts.scripts().spawn_engine_object(class, data);
            spawned.map(|_| ())
        }
    };
    spawned.map_err(|f| override_fatal(scripts, f))
}

/// Whether `object` spawns in the scripts: one without a binding function
/// (spec L 3.1; an empty value finds none), or a door under the door
/// scheme, whose binder (`bind_physic_object.init`, L 8) runs on the
/// script interface implemented so far. TODO(#1077): every object, once
/// the interface the other binders call is implemented.
fn binding_runs(system: &Config, object: &ClientObject) -> bool {
    let section = object.record.header.section.as_str();
    match system.get(section, "script_binding") {
        None | Some(None) => true,
        Some(Some(name)) => name.trim().is_empty() || object.door,
    }
}

/// Applies the events the scripts' members raised (spec M): the player's
/// forced placement and camera angles (4), hits on the player (5.6,
/// player-hits.md) and the visibility switch (6.5). A death event's
/// handling (5.2) is the death's (#248); a stalker's wounded flag has no
/// behaviour to update, and an explosion is spec Y's (#1056).
pub fn apply_events(scripts: &mut GameScripts, level: &mut LevelScreen) {
    let events = scripts.objects().borrow_mut().take_events();
    for event in events {
        match event {
            Event::ForcePlacement { object, placement } if object == ACTOR_ID => {
                let [x, y, z, _] = placement[3];
                level.force_player_position([x, y, z]);
            }
            Event::CameraAngles { object, angles } if object == ACTOR_ID => {
                level.set_camera_angles(angles);
            }
            Event::Hit(hit) if hit.target == ACTOR_ID => {
                // An unknown hit type, and a bone the player's model lacks,
                // are the hit handling's fatal errors (player-hits.md 2, 4).
                let kind = u32::try_from(hit.hit_type)
                    .map_err(|_| io::Error::other("hit type"))
                    .and_then(HitType::new);
                match kind {
                    Ok(kind) => {
                        let _ = level.hit(Hit {
                            kind,
                            power: hit.power,
                            bone: Some(hit.bone),
                            armour_piercing: hit.armour_piercing.unwrap_or(0.0),
                        });
                    }
                    Err(_) => level
                        .record_fatal(format!("Unknown/unregistered hit type {}", hit.hit_type)),
                }
            }
            Event::Visibility { object, hidden } => level.set_object_hidden(object, hidden),
            _ => {}
        }
    }
}

/// The visual of `model` as the place members read it (M 4): its header
/// sphere's centre and its bones at their bind matrices, the root first.
/// It has no `IRender_Visual` for `CGameObject:Visual()`: no script class
/// the shipped game instantiates reads it (M 10.2).
fn visual_of(model: &LoadedModel, names: &Names) -> Visual {
    Visual {
        part: None,
        centre: model.header.sphere_centre,
        bones: (0..model.bone_count())
            .map(|i| {
                let name = names.get(model.bone_name(i));
                let bytes = cp1251::encode(name).unwrap_or_else(|| name.as_bytes().to_vec());
                (bytes, model.bind(i))
            })
            .collect(),
        root: 0,
    }
}

/// The shapes of a record that has a Shape block (sav.md 7.4.2).
fn shapes_of(record: &ObjectRecord) -> Option<&[Shape]> {
    Some(match &record.object {
        Object::Zone(o) => &o.shapes,
        Object::Anomaly(o) => &o.anomaly.zone.shapes,
        Object::VisualAnomaly(o) => &o.anomaly.zone.shapes,
        Object::TorridZone(o) => &o.zone.shapes,
        Object::LevelChanger(o) => &o.shapes,
        Object::Restrictor(o) => &o.shapes,
        Object::SmartZone(o) => &o.restrictor.shapes,
        Object::TeamBase(o) => &o.restrictor.shapes,
        Object::Climbable(o) => &o.shapes,
        _ => return None,
    })
}

/// A restrictor's shape in level space (M 6.2, whose derivation is the
/// engine's: our reading): each of the record's shapes placed by its
/// placement, a box as its 6 face planes (its axes span it about its
/// centre, as the zones read it), the bounding sphere the zones' shape
/// sphere placed.
fn restrictor_of(record: &ObjectRecord) -> Option<Restrictor> {
    let h = &record.header;
    Some(placed(
        shapes_of(record)?,
        &model::transform(h.position, h.angles),
    ))
}

/// `shapes` placed by `x` as [`restrictor_of`] places them.
fn placed(shapes: &[Shape], x: &model::Mat4) -> Restrictor {
    let point = |p: [f32; 3]| model::transform_point(x, p);
    let direction = |d: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|c| d[0] * x[0][c] + d[1] * x[1][c] + d[2] * x[2][c])
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (centre, radius) = zones::shape_sphere(shapes);
    let mut r = Restrictor {
        bound: (point(centre), radius),
        ..Default::default()
    };
    for shape in shapes {
        match *shape {
            Shape::Sphere { centre, radius } => r.spheres.push((point(centre), radius)),
            Shape::Box([a, b, c, o]) => {
                let o = point(o);
                let mut planes = [([0.0; 3], 0.0); 6];
                for (k, axis) in [a, b, c].into_iter().enumerate() {
                    let u = direction(axis);
                    let l = dot(u, u).sqrt();
                    let n = if l > 0.0 { u.map(|v| v / l) } else { [0.0; 3] };
                    let m = dot(n, o);
                    planes[2 * k] = (n, -m - l * 0.5);
                    planes[2 * k + 1] = (n.map(|v| -v), m - l * 0.5);
                }
                r.boxes.push(planes);
            }
        }
    }
    r
}

/// A load's error, fatal as the original's.
fn io_fatal(e: io::Error) -> Error {
    Error {
        kind: ErrorKind::Fatal,
        message: e.to_string(),
    }
}

/// An override's failure (spec A 5.3): its log line written, then the
/// game's end.
fn override_fatal(scripts: &GameScripts, failure: OverrideFailure) -> Error {
    if let Some(line) = &failure.log {
        scripts
            .services()
            .borrow_mut()
            .console
            .log_mut()
            .message(line);
    }
    failure.error
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the sphere (`p`, `radius`) is inside `r` (M 6.2).
    fn inside(r: &Restrictor, p: [f32; 3], radius: f32) -> bool {
        let d =
            |a: [f32; 3], b: [f32; 3]| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt();
        if d(p, r.bound.0) >= r.bound.1 + radius {
            return false;
        }
        r.spheres.iter().any(|&(c, s)| d(p, c) < s + radius)
            || (r.boxes.iter()).any(|b| {
                b.iter()
                    .all(|(n, o)| n[0] * p[0] + n[1] * p[1] + n[2] * p[2] + o <= radius)
            })
    }

    #[test]
    fn a_restrictors_shapes_are_placed_in_level_space() {
        // A 2 × 4 × 6 box about (1, 0, 0) and a sphere of radius 1 at
        // (0, 0, 5), turned a quarter about y and moved to (10, 0, 0).
        let shapes = [
            Shape::Box([
                [2.0, 0.0, 0.0],
                [0.0, 4.0, 0.0],
                [0.0, 0.0, 6.0],
                [1.0, 0.0, 0.0],
            ]),
            Shape::Sphere {
                centre: [0.0, 0.0, 5.0],
                radius: 1.0,
            },
        ];
        let x = model::transform([10.0, 0.0, 0.0], [0.0, std::f32::consts::FRAC_PI_2, 0.0]);
        let r = placed(&shapes, &x);
        let box_centre = model::transform_point(&x, [1.0, 0.0, 0.0]);
        let sphere_centre = model::transform_point(&x, [0.0, 0.0, 5.0]);
        assert_eq!(r.boxes.len(), 1);
        assert_eq!(r.spheres, [(sphere_centre, 1.0)]);
        assert!(inside(&r, box_centre, 0.001));
        assert!(inside(&r, sphere_centre, 0.001));
        // The box's x half-extent, 1 m, now lies along the turned axis.
        let along = |t: f32| model::transform_point(&x, [1.0 + t, 0.0, 0.0]);
        assert!(inside(&r, along(0.9), 0.001));
        assert!(!inside(&r, along(1.2), 0.001));
        assert!(inside(&r, along(1.2), 0.3));
        assert!(!inside(&r, [10.0, 50.0, 0.0], 0.001));
    }
}
