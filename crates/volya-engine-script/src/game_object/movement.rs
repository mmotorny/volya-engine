// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script API X, movement, sight and memory
//! (docs/behavior/script-api-movement-sight-and-memory.md; section numbers
//! are its): the `game_object` members of stalker movement (3), paths and
//! patrol paths (4), space restrictions (5) and items, weapons and
//! animations (10) here; sight, vision, memory, enemies, dangers and covers
//! in [`super::sight`] (6 – 9, 12); the monsters' in [`super::monsters`]
//! (11). Each object's [`Parts`] hold what the members set and read.
//!
//! The spec owns what each call reads or sets at once; what the engine does
//! behind them over time is the characters' (CH-9 … CH-24), which Volya
//! Engine doesn't run yet: the requests are kept and nothing follows them
//! (TODO(#1129)), the level graph and restrictions are outside the scripts
//! (TODO(#1130)), no one perceives (TODO(#1131)). A member whose answer
//! needs such a part raises a script error naming its issue; one whose
//! state the engine keeps answers from it. Until stalker movement (CH-10,
//! #1233) lands, a lightweight fake places a stalker at its destination
//! vertex at once ([`Scripts::fake_stalker_movement`]; every spot is
//! marked FAKE(#1234)).

use volya_engine_formats::cp1251;
use volya_engine_formats::level_ai::LevelAi;
use volya_engine_formats::model;

use super::members::{Call, boolean, member, number};
use super::{Context, Event, GameObject, Kinds, ObjectClass};
use crate::Value;
use crate::classes::EngineClass;
use crate::loader::Scripts;
use crate::values::{Vector, to_bits32, to_f32};
use volya_engine_core::config::{Config, value};
use volya_engine_core::motion_playback::{AnimatedModel, Clock};
use volya_engine_core::stalker_motions::{Outcome, StalkerMotions, StalkerState};

/// The largest 32-bit float (conventions).
pub const FLT_MAX: f32 = f32::MAX;

/// Section 3's fallbacks: standing, standing still, danger, no path (3 is
/// believed "no path").
pub const STANDING: i32 = 1;
pub const STAND: i32 = 2;
pub const DANGER: i32 = 0;
pub const NO_PATH: i32 = 3;

/// The level path's path type (`game_object.level_path`, CH-10 3.1).
pub const LEVEL_PATH: i32 = 1;

/// How far above its level vertex's plane an object is placed
/// (object-visual-and-transform.md 4.1 step 5).
pub const ABOVE_PLANE: f32 = 0.001;

/// `set_item`'s queue size and interval when not given (10.1).
pub const DEFAULT_QUEUE_SIZE: i32 = -1;
pub const DEFAULT_INTERVAL: u32 = 300;

/// The radius of the accessibility test (4.3, 5).
pub const ACCESSIBLE_RADIUS: f32 = 0.001;

/// A body state, movement type, mental state and path type (3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovementStates {
    pub body_state: i32,
    pub movement_type: i32,
    pub mental_state: i32,
    pub path_type: i32,
}

impl Default for MovementStates {
    /// The fallbacks (our choice where a stalker's first states are
    /// unestablished: standing, standing still, danger, no path).
    fn default() -> Self {
        MovementStates {
            body_state: STANDING,
            movement_type: STAND,
            mental_state: DANGER,
            path_type: NO_PATH,
        }
    }
}

/// A stalker's movement request (3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MovementRequest {
    /// The target body state, movement type, mental state, path type.
    pub target: MovementStates,
    pub detail_path_type: i32,
    /// The desired position and direction, kept when marked not set.
    pub desired_position: [f32; 3],
    pub desired_position_set: bool,
    pub desired_direction: [f32; 3],
    pub desired_direction_set: bool,
    pub dest_level_vertex: Option<u32>,
}

/// A stalker's sight request (6.1): `None` for a field not written.
#[derive(Clone, Debug, PartialEq)]
pub struct SightRequest {
    pub sight_type: i32,
    pub vector: Option<[f32; 3]>,
    pub object: Option<u16>,
    pub a: bool,
    pub b: bool,
    pub c: Option<bool>,
}

impl Default for SightRequest {
    /// Before any `set_sight` (unestablished): `eSightTypeDummy`, nothing
    /// set (our choice).
    fn default() -> Self {
        SightRequest {
            sight_type: -1,
            vector: None,
            object: None,
            a: false,
            b: false,
            c: None,
        }
    }
}

/// The object handler's target `set_item` gives (10.1).
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectTarget {
    pub action: i32,
    pub object: Option<u16>,
    pub queue_size: i32,
    pub interval: u32,
}

/// A stalker's parts (3, 6, 8.1, 10).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StalkerParts {
    pub request: MovementRequest,
    /// The current states, which the movement update moves toward the
    /// request (CH-10; TODO(#1129): nothing moves them).
    pub current: MovementStates,
    /// Whether the path is marked to be rebuilt (3, 4.1).
    pub path_outdated: bool,
    pub sight: SightRequest,
    /// Its motion state and script animation queue (10.3; CH-11).
    pub motions: StalkerMotions,
    pub ignore_monster_threshold: f32,
    pub max_ignore_monster_distance: f32,
    pub object_target: Option<ObjectTarget>,
    /// FAKE(#1234): the fake movement placed it at its destination vertex;
    /// its path is completed until marked to be rebuilt.
    pub arrived: bool,
}

/// A fighter's patrol path (4.1): indices −1 for "none".
#[derive(Clone, Debug, PartialEq)]
pub struct PatrolPath {
    pub name: Option<Vec<u8>>,
    pub start_type: i32,
    pub route_type: i32,
    pub random: bool,
    pub current: i32,
    pub previous: i32,
    pub start: i32,
    /// Marked to be rebuilt.
    pub outdated: bool,
}

impl Default for PatrolPath {
    fn default() -> Self {
        PatrolPath {
            name: None,
            start_type: 0,
            route_type: 0,
            random: false,
            current: -1,
            previous: -1,
            start: -1,
            outdated: false,
        }
    }
}

/// A fighter's parts (4, 5, 7.4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FighterParts {
    pub patrol: PatrolPath,
    /// The detail path's extrapolate length; `None` until set (its first
    /// value is the detail path's, TODO(#1129)).
    pub extrapolate_length: Option<f32>,
    /// The detail path marked to be rebuilt (4.2).
    pub detail_path_outdated: bool,
    /// The movement flag; `None` until set (its first value is
    /// unestablished, TODO(#1129)).
    pub movement_enabled: Option<bool>,
    /// Where it was when movement was disabled (4.2).
    pub disabled_at: [f32; 3],
    /// The path marked to be rebuilt (4.2, 5).
    pub path_outdated: bool,
    /// The restrictions added, by name (5).
    pub out_restrictions: Vec<Vec<u8>>,
    pub in_restrictions: Vec<Vec<u8>>,
    /// Its visual memory's vision flag; `None` until set (CH-13,
    /// TODO(#1131)).
    pub vision_enabled: Option<bool>,
}

/// A living object's eye (7.4): `None` until set (its section's values
/// are CH-13's, TODO(#1131)).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Eye {
    pub fov: Option<f32>,
    pub range: Option<f32>,
}

/// Script API X's parts of an object, by its kinds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    pub eye: Option<Eye>,
    pub fighter: Option<FighterParts>,
    pub stalker: Option<StalkerParts>,
    pub monster: Option<super::monsters::Monster>,
}

impl Parts {
    /// The parts an object of `class` with `kinds` has.
    pub fn of(class: ObjectClass, kinds: Kinds) -> Self {
        Parts {
            eye: kinds.has(Kinds::LIVING).then(Eye::default),
            fighter: kinds.has(Kinds::FIGHTER).then(FighterParts::default),
            stalker: kinds.has(Kinds::STALKER).then(StalkerParts::default),
            monster: kinds
                .has(Kinds::MONSTER)
                .then(|| super::monsters::Monster::of(class)),
        }
    }
}

/// A section key's value read without a presence test (F 2.4): the
/// missing-section or missing-key report's description when absent.
pub(super) fn required_float(system: &Config, section: &str, key: &str) -> Result<f32, String> {
    let Some(found) = system.section(section) else {
        return Err(format!("Can't open section '{section}'"));
    };
    match found.get(key) {
        Some(Some(v)) => Ok(value::float(v)),
        Some(None) => Ok(0.0),
        None => Err(format!("Can't find variable {key} in [{section}]")),
    }
}

/// A section key's value, `default` when the key is absent.
pub(super) fn float_or(system: Option<&Config>, section: &[u8], key: &str, default: f32) -> f32 {
    let section = cp1251::decode(section);
    system
        .and_then(|s| s.section(&section))
        .and_then(|s| s.get(key))
        .flatten()
        .map_or(default, value::float)
}

/// What an object reads from its section when it loads: a monster its
/// panic threshold (11.3, a missing key fatal: the report's description is
/// the error); a stalker its ignore thresholds as their `restore_` members
/// set them (our choice: their first values are unestablished).
pub(crate) fn load(o: &mut GameObject, system: Option<&Config>) -> Result<(), String> {
    let section = o.section.clone().unwrap_or_default();
    if let Some(monster) = &mut o.movement.monster {
        let Some(system) = system else {
            return Err("no system configuration".into());
        };
        let threshold = required_float(system, &cp1251::decode(&section), "panic_threshold")?;
        monster.panic_threshold = threshold;
        monster.default_panic_threshold = threshold;
    }
    if let Some(s) = &mut o.movement.stalker {
        s.ignore_monster_threshold = float_or(system, &section, "ignore_monster_threshold", 1.0);
        s.max_ignore_monster_distance = float_or(system, &section, "max_ignore_distance", 0.0);
    }
    Ok(())
}

/// A script error for an answer the engine can't give yet (our wording).
pub(super) fn unavailable(c: &Call, issue: u32) -> String {
    format!("game_object:{}: not available yet (TODO(#{issue}))", c.name)
}

/// The restrictor names of a restriction text (5): separated by commas,
/// each trimmed, empty ones skipped (our reading of CH-9's lists).
pub fn restrictor_names(text: &[u8]) -> Vec<Vec<u8>> {
    text.split(|&b| b == b',')
        .map(|n| n.trim_ascii().to_vec())
        .filter(|n| !n.is_empty())
        .collect()
}

fn joined(names: &[Vec<u8>]) -> Value {
    Value::String(names.join(&b","[..]))
}

fn none() -> Result<Vec<Value>, String> {
    Ok(Vec::new())
}

fn position(o: &GameObject) -> [f32; 3] {
    let p = o.placement[3];
    [p[0], p[1], p[2]]
}

/// The members of 3, 4, 5 and 10, then those of [`super::sight`] and
/// [`super::monsters`].
pub(super) fn game_object_members(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = stalker_movement(class, ctx);
    let class = paths(class, ctx);
    let class = restrictions(class, ctx);
    let class = items(class, ctx);
    let class = super::sight::game_object_members(class, ctx);
    super::monsters::game_object_members(class, ctx)
}

/// Every class of the spec but `game_object`.
pub(super) fn classes(ctx: &Context) -> Vec<EngineClass> {
    let mut classes = super::sight::classes(ctx);
    classes.extend(super::monsters::classes(ctx));
    classes
}

fn stalker<'a>(c: &'a mut Call) -> Result<Option<&'a mut StalkerParts>, String> {
    Ok(c.object()?.movement.stalker.as_mut())
}

fn fighter<'a>(c: &'a mut Call) -> Result<Option<&'a mut FighterParts>, String> {
    Ok(c.object()?.movement.fighter.as_mut())
}

/// 3.
fn stalker_movement(class: EngineClass, ctx: &Context) -> EngineClass {
    type Set = fn(&mut MovementRequest, i32);
    let setters: [(&'static str, Set); 5] = [
        ("set_body_state", |r, v| r.target.body_state = v),
        ("set_movement_type", |r, v| r.target.movement_type = v),
        ("set_mental_state", |r, v| r.target.mental_state = v),
        ("set_path_type", |r, v| r.target.path_type = v),
        ("set_detail_path_type", |r, v| r.detail_path_type = v),
    ];
    let mut class = class;
    for (name, set) in setters {
        class = member(class, ctx, name, move |c| {
            let v = to_bits32(c.number(0)?);
            if let Some(s) = stalker(c)? {
                set(&mut s.request, v);
            }
            none()
        });
    }
    type Get = fn(&StalkerParts) -> i32;
    let getters: [(&'static str, Get, i32); 7] = [
        ("body_state", |s| s.current.body_state, STANDING),
        (
            "target_body_state",
            |s| s.request.target.body_state,
            STANDING,
        ),
        ("movement_type", |s| s.current.movement_type, STAND),
        (
            "target_movement_type",
            |s| s.request.target.movement_type,
            STAND,
        ),
        ("mental_state", |s| s.current.mental_state, DANGER),
        (
            "target_mental_state",
            |s| s.request.target.mental_state,
            DANGER,
        ),
        ("path_type", |s| s.current.path_type, NO_PATH),
    ];
    for (name, get, fallback) in getters {
        class = member(class, ctx, name, move |c| {
            Ok(number(stalker(c)?.map_or(fallback, |s| get(s))))
        });
    }
    // Always 0, whatever the target.
    let class = member(class, ctx, "detail_path_type", |_| Ok(number(0)));
    let class = member(class, ctx, "set_desired_position", |c| {
        let p = match c.arg(0) {
            Value::Nil => None,
            _ => Some(c.vector(0)?),
        };
        if let Some(s) = stalker(c)? {
            match p {
                Some(p) => {
                    s.request.desired_position = [p.x, p.y, p.z];
                    s.request.desired_position_set = true;
                }
                None => s.request.desired_position_set = false,
            }
        }
        none()
    });
    let class = member(class, ctx, "set_desired_direction", |c| {
        let d = match c.arg(0) {
            Value::Nil => None,
            _ => Some(c.vector(0)?),
        };
        if let Some(s) = stalker(c)? {
            match d {
                Some(d) => {
                    s.request.desired_direction = [d.x, d.y, d.z];
                    s.request.desired_direction_set = true;
                }
                None => s.request.desired_direction_set = false,
            }
        }
        none()
    });
    let class = member(class, ctx, "set_dest_level_vertex_id", |c| {
        let v = to_bits32(c.number(0)?) as u32;
        let vertices = c.objects.level_vertex_lights.len();
        if let Some(s) = stalker(c)?
            && (v as usize) < vertices
        {
            if s.request.dest_level_vertex != Some(v) {
                s.path_outdated = true;
            }
            s.request.dest_level_vertex = Some(v);
        }
        none()
    });
    member(class, ctx, "head_orientation", |c| {
        let o = c.object()?;
        let Some(head) = o.stalker.as_ref().map(|s| s.head_heading.0) else {
            return Ok(vec![c.new_vector([FLT_MAX; 3])]);
        };
        // TODO(#1129): the head's pitch isn't kept: 0.
        let d = crate::values::direction_of(-head, -0.0);
        Ok(vec![c.new_vector([d.x, d.y, d.z])])
    })
}

/// Whether the level has patrol path `name` with point `point`.
fn patrol_point(c: &Call, name: &[u8], point: i32) -> bool {
    let physics = c.ctx.physics.borrow();
    point >= 0
        && (physics.level.as_ref()).is_some_and(|l| l.patrol_point(name, point as u32).is_some())
}

/// 4.
fn paths(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "set_patrol_path", |c| {
        let name = c.text(0)?;
        let start = to_bits32(c.number(1)?);
        let route = to_bits32(c.number(2)?);
        let random = c.boolean(3)?;
        // A name without a patrol path is unestablished: the path is
        // cleared (believed).
        let name = patrol_point(c, &name, 0).then_some(name);
        if c.object()?.movement.stalker.is_none() {
            return none();
        }
        let Some(f) = fighter(c)? else {
            return none();
        };
        let p = &mut f.patrol;
        if p.name != name {
            p.name = name;
            p.current = -1;
            p.previous = -1;
            p.start = -1;
        }
        if p.start_type != start || p.route_type != route {
            p.outdated = true;
        }
        p.start_type = start;
        p.route_type = route;
        p.random = random;
        none()
    });
    let class = member(class, ctx, "patrol", |c| {
        let o = c.object()?;
        let stalker = o.movement.stalker.is_some();
        let name = o
            .movement
            .fighter
            .as_ref()
            .and_then(|f| f.patrol.name.clone());
        // TODO(#1130): another scripted object's is its scripted part's
        // (CH-24): "".
        Ok(vec![Value::String(match name {
            Some(name) if stalker => name,
            _ => Vec::new(),
        })])
    });
    let class = member(class, ctx, "get_current_point_index", |c| {
        Ok(number(fighter(c)?.map_or(-1, |f| f.patrol.current)))
    });
    let class = member(class, ctx, "set_start_point", |c| {
        let i = to_bits32(c.number(0)?);
        set_point(c, i, |p, i| p.start = i)
    });
    let class = member(class, ctx, "set_previous_point", |c| {
        let i = to_bits32(c.number(0)?);
        set_point(c, i, |p, i| p.previous = i)
    });
    let class = member(class, ctx, "patrol_path_make_inactual", |c| {
        if let Some(f) = fighter(c)? {
            f.patrol.outdated = true;
        }
        none()
    });
    // FAKE(#1234): a stalker's path is completed once the fake placed it at
    // its destination, until the path is marked to be rebuilt; no other
    // fighter's path is built (TODO(#1129)).
    let class = member(class, ctx, "path_completed", |c| {
        let o = c.object()?;
        let outdated = (o.movement.fighter.as_ref()).is_some_and(|f| f.path_outdated);
        let arrived = (o.movement.stalker.as_ref()).is_some_and(|s| s.arrived && !s.path_outdated);
        Ok(boolean(arrived && !outdated))
    });
    let class = member(class, ctx, "location_on_path", |c| {
        if matches!(c.arg(1), Value::Nil) {
            return Ok(number(-1));
        }
        let o = c.object()?;
        if o.movement.fighter.is_none() {
            return Ok(number(-1));
        }
        let p = position(o);
        let (stalker, vertex) = (o.movement.stalker.is_some(), o.level_vertex);
        let out = c.arg(1).clone();
        c.o.set(&out, Vector::new(p[0], p[1], p[2]));
        // FAKE(#1234): a stalker never has a detail path, and without one
        // the result is its level vertex (CH-10 5.4). Another fighter's
        // without one is a number of its movement state, unestablished
        // which (TODO(#1129)).
        if stalker {
            return Ok(number(vertex));
        }
        Err(unavailable(c, 1129))
    });
    let class = member(class, ctx, "extrapolate_length", |c| {
        if c.args.is_empty() {
            return match fighter(c)? {
                None => Ok(number(0)),
                Some(f) => match f.extrapolate_length {
                    Some(v) => Ok(number(v)),
                    None => Err(unavailable(c, 1129)),
                },
            };
        }
        let v = to_f32(c.number(0)?);
        if let Some(f) = fighter(c)? {
            if f.extrapolate_length
                .is_none_or(|old| (old - v).abs() >= 0.00001)
            {
                f.detail_path_outdated = true;
            }
            f.extrapolate_length = Some(v);
        }
        none()
    });
    let class = member(class, ctx, "movement_enabled", |c| {
        if c.args.is_empty() {
            return match fighter(c)? {
                None => Ok(boolean(false)),
                Some(f) => match f.movement_enabled {
                    Some(v) => Ok(boolean(v)),
                    None => Err(unavailable(c, 1129)),
                },
            };
        }
        let on = c.boolean(0)?;
        let o = c.object()?;
        let p = position(o);
        if let Some(f) = &mut o.movement.fighter {
            if on {
                let moved = (0..3).any(|i| (p[i] - f.disabled_at[i]).abs() >= 0.001);
                if f.movement_enabled == Some(false) && moved {
                    f.path_outdated = true;
                }
            } else {
                f.disabled_at = p;
            }
            f.movement_enabled = Some(on);
        }
        none()
    });
    member(class, ctx, "vertex_in_direction", |c| {
        let _vertex = to_bits32(c.number(0)?);
        c.vector(1)?;
        c.number(2)?;
        if fighter(c)?.is_none() {
            return Ok(number(-1));
        }
        // The vertex's accessibility under restrictions and the level
        // graph's straight walk (spec N) are outside the scripts.
        Err(unavailable(c, 1130))
    })
}

fn set_point(c: &mut Call, i: i32, set: fn(&mut PatrolPath, i32)) -> Result<Vec<Value>, String> {
    let name = fighter(c)?.and_then(|f| f.patrol.name.clone());
    let Some(name) = name else {
        return none();
    };
    if patrol_point(c, &name, i)
        && let Some(f) = fighter(c)?
    {
        set(&mut f.patrol, i);
    }
    none()
}

/// Whether no restriction is in effect (TODO(#1130): the base
/// restrictions from the spawn data aren't known: none).
fn no_restrictions(f: &FighterParts) -> bool {
    f.out_restrictions.is_empty() && f.in_restrictions.is_empty()
}

/// 5.
fn restrictions(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "add_restrictions", |c| {
        let (out, inside) = (c.text(0)?, c.text(1)?);
        if let Some(f) = fighter(c)? {
            if !out.is_empty() || !inside.is_empty() {
                for (list, text) in [
                    (&mut f.out_restrictions, &out),
                    (&mut f.in_restrictions, &inside),
                ] {
                    for name in restrictor_names(text) {
                        if !list.contains(&name) {
                            list.push(name);
                        }
                    }
                }
            }
            f.path_outdated = true;
        }
        none()
    });
    let class = member(class, ctx, "remove_restrictions", |c| {
        let (out, inside) = (c.text(0)?, c.text(1)?);
        if let Some(f) = fighter(c)? {
            if !out.is_empty() || !inside.is_empty() {
                for (list, text) in [
                    (&mut f.out_restrictions, &out),
                    (&mut f.in_restrictions, &inside),
                ] {
                    let names = restrictor_names(text);
                    list.retain(|n| !names.contains(n));
                }
            }
            f.path_outdated = true;
        }
        none()
    });
    let class = member(class, ctx, "remove_all_restrictions", |c| {
        if let Some(f) = fighter(c)?
            && !no_restrictions(f)
        {
            f.out_restrictions.clear();
            f.in_restrictions.clear();
            f.path_outdated = true;
        }
        none()
    });
    let class = member(class, ctx, "in_restrictions", |c| {
        Ok(vec![
            fighter(c)?.map_or(Value::string(""), |f| joined(&f.in_restrictions)),
        ])
    });
    let class = member(class, ctx, "out_restrictions", |c| {
        Ok(vec![fighter(c)?.map_or(Value::string(""), |f| {
            joined(&f.out_restrictions)
        })])
    });
    // TODO(#1130): the spawn data's base restrictions aren't known: none.
    let class = member(class, ctx, "base_in_restrictions", |_| {
        Ok(vec![Value::string("")])
    });
    let class = member(class, ctx, "base_out_restrictions", |_| {
        Ok(vec![Value::string("")])
    });
    let class = member(class, ctx, "accessible", |c| {
        match c.arg(0) {
            Value::Number(_) => drop(to_bits32(c.number(0)?)),
            _ => drop(c.vector(0)?),
        }
        match fighter(c)? {
            None => Ok(boolean(false)),
            Some(f) if no_restrictions(f) => Ok(boolean(true)),
            Some(_) => Err(unavailable(c, 1130)),
        }
    });
    member(class, ctx, "accessible_nearest", |c| {
        c.vector(0)?;
        c.vector(1)?;
        if fighter(c)?.is_none() {
            return Ok(number(-1));
        }
        Err(unavailable(c, 1130))
    })
}

/// The player, alive and the level's controlled object (10.4; TODO(#1057):
/// no vehicles, so the player is always the controlled object).
fn controlled_player(c: &Call) -> Option<u16> {
    let player = c.objects.iter().find(|o| o.kinds.has(Kinds::PLAYER))?;
    let alive = player.entity.as_ref().is_some_and(|e| e.health > 0.0);
    alive.then_some(player.id)
}

/// The event of 10.4 to the player: hide (`true`) or restore its weapon,
/// slot −1 (believed: all slots).
pub(super) fn hide_weapon_event(c: &mut Call, hide: bool) {
    if let Some(player) = controlled_player(c) {
        c.objects.push(Event::HideWeapon {
            player,
            hide,
            slot: -1,
        });
    }
}

/// 10.
fn items(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "set_item", |c| {
        let action = to_bits32(c.number(0)?);
        let object = c.handle(1)?;
        let queue_size = match c.args.len() {
            0..=2 => DEFAULT_QUEUE_SIZE,
            _ => to_bits32(c.number(2)?),
        };
        let interval = match c.args.len() {
            0..=3 => DEFAULT_INTERVAL,
            _ => to_bits32(c.number(3)?) as u32,
        };
        // TODO(#1132): the handler's states, the strapping replacement and
        // the weapon's fire queue are CH-20's: the target is kept.
        if let Some(s) = stalker(c)? {
            s.object_target = Some(ObjectTarget {
                action,
                object,
                queue_size,
                interval,
            });
        }
        none()
    });
    // TODO(#1132): which weapons have a magazine is CH-20's: none here.
    let class = member(class, ctx, "set_queue_size", |c| {
        c.number(0)?;
        none()
    });
    // TODO(#1132): the active item is script API S's: none, so neither
    // answers for a strappable weapon.
    let class = member(class, ctx, "weapon_strapped", |_| Ok(boolean(false)));
    let class = member(class, ctx, "weapon_unstrapped", |c| {
        Ok(boolean(c.object()?.movement.stalker.is_some()))
    });
    let class = member(class, ctx, "accuracy", |c| {
        if !c.object()?.kinds.has(Kinds::INVENTORY_OWNER) {
            return Ok(number(-1));
        }
        Err(unavailable(c, 1132))
    });
    let class = member(class, ctx, "add_animation", |c| {
        let name = c.text(0)?;
        let (a, b) = (c.boolean(1)?, c.boolean(2)?);
        // A name the model lacks: the diagnostic goes to the routine that
        // returns at once (M 2.3), nothing is appended (CH-11 8.1).
        if let Some(s) = stalker(c)? {
            s.motions.add_animation(&name, a, b);
        }
        none()
    });
    let class = member(class, ctx, "clear_animations", |c| {
        if let Some(s) = stalker(c)? {
            s.motions.clear_animations();
        }
        none()
    });
    let class = member(class, ctx, "animation_count", |c| {
        Ok(number(
            stalker(c)?.map_or(-1, |s| s.motions.animation_count() as i32),
        ))
    });
    let class = member(class, ctx, "animation_slot", |c| {
        if !c.object()?.kinds.has(Kinds::ITEM) {
            return Ok(number(-1));
        }
        Err(unavailable(c, 1132))
    });
    // TODO(#1129): the objects' models aren't animated for the scripts, so
    // every call gives the diagnostic and does nothing.
    let class = member(class, ctx, "play_cycle", |c| {
        c.text(0)?;
        if c.args.len() > 1 {
            c.boolean(1)?;
        }
        none()
    });
    let class = member(class, ctx, "hide_weapon", |c| {
        hide_weapon_event(c, true);
        none()
    });
    member(class, ctx, "restore_weapon", |c| {
        hide_weapon_event(c, false);
        none()
    })
}

/// FAKE(#1234): stalker movement (CH-10, #1233) until the level graph and
/// paths exist (CH-8, CH-9). At an update of a living stalker whose
/// movement is enabled (CH-10 3.1; not disabled, as its first value is
/// unestablished), its current states are those of a stalker without an
/// unfinished detail path (3.4): standing still, with the target body and
/// mental states (and path type, believed). Told to walk or run a level
/// path without a desired position (3.3: with one the path ends at that
/// position), it is placed at once where that path ends: at its
/// destination vertex's x and z, [`ABOVE_PLANE`] over the vertex's plane
/// (object-visual-and-transform.md 4.1 step 5), its level vertex the
/// destination, body and head facing the desired direction's x–z heading
/// when one is set ((0, 1) when its squared length is below 0.001, 5.1),
/// else as they were. Its path then counts as completed (X 4.1) until
/// marked to be rebuilt. Simplified: no walking or turning, no game vertex
/// change, no restrictions.
pub fn fake_movement_update(o: &mut GameObject, ai: Option<&LevelAi>) {
    let alive = o.entity.as_ref().is_none_or(|e| e.health > 0.0);
    let (Some(s), Some(f)) = (&mut o.movement.stalker, &mut o.movement.fighter) else {
        return;
    };
    if !alive || f.movement_enabled == Some(false) {
        return;
    }
    let r = &s.request;
    s.current = MovementStates {
        body_state: r.target.body_state,
        movement_type: STAND,
        mental_state: r.target.mental_state,
        path_type: r.target.path_type,
    };
    let walks = r.target.movement_type != STAND && r.target.path_type == LEVEL_PATH;
    let Some(vertex) = (r.dest_level_vertex).filter(|_| walks && !r.desired_position_set) else {
        return;
    };
    if s.arrived && !s.path_outdated && !f.path_outdated {
        return;
    }
    let Some([x, _, z]) = ai.and_then(|ai| ai.position(vertex)) else {
        return;
    };
    let Some(height) = ai.and_then(|ai| ai.plane_height(vertex, x, z)) else {
        return;
    };
    let heading = r.desired_direction_set.then(|| {
        let [dx, _, dz] = r.desired_direction;
        if dx * dx + dz * dz < 0.001 {
            0.0
        } else {
            dx.atan2(dz)
        }
    });
    s.arrived = true;
    s.path_outdated = false;
    f.path_outdated = false;
    let heading = heading
        .or_else(|| o.fighter.as_ref().map(|f| f.body_heading.0))
        .unwrap_or(0.0);
    o.placement = model::transform([x, height + ABOVE_PLANE, z], [0.0, -heading, 0.0]);
    o.level_vertex = vertex;
    if let Some(f) = &mut o.fighter {
        f.body_heading = (heading, heading);
    }
    if let Some(s) = &mut o.stalker {
        s.head_heading = (heading, heading);
    }
}

impl Scripts {
    /// FAKE(#1234): object `id`'s stalker movement at the end of its think
    /// step, after its decision planner (CH-10 conventions;
    /// [`fake_movement_update`]), over the started level's vertices.
    pub fn fake_stalker_movement(&mut self, id: u16) {
        let vertices = self.level_vertices();
        let Some(basics) = self.game_object_state.as_ref() else {
            return;
        };
        if let Some(o) = basics.objects.borrow_mut().get_mut(id) {
            fake_movement_update(o, vertices.as_ref().map(|v| &*v.ai));
        }
    }
}

/// What the stalker motion update reads of stalker `o` (stalker-motions.md
/// 1): its current states and its body's and head's headings as the
/// engine keeps them.
// TODO(#1129): its speed, path and sight turning are the movement's
// (CH-10), whose fake (#1234) keeps it still: no path, not turning. TODO(#1132):
// the item in hand and the object handler are CH-20's: none. Its critical
// wounds (CH-21) and its sounds for the head (CH-25) aren't followed:
// none, silent.
fn stalker_state(o: &GameObject) -> StalkerState {
    let states = o.movement.stalker.as_ref().map(|s| s.current);
    let body = o.fighter.as_ref().map_or((0.0, 0.0), |f| f.body_heading);
    let head = o.stalker.as_ref().map_or((0.0, 0.0), |s| s.head_heading);
    let state =
        |v: Option<i32>, fallback: u8| v.and_then(|v| u8::try_from(v).ok()).unwrap_or(fallback);
    StalkerState {
        body_state: state(states.map(|s| s.body_state), STANDING as u8),
        movement_type: state(states.map(|s| s.movement_type), STAND as u8),
        mental_state: state(states.map(|s| s.mental_state), DANGER as u8),
        body_yaw: body.0,
        body_target_yaw: body.1,
        head_yaw: head.0,
        head_target_yaw: head.1,
        ..StalkerState::default()
    }
}

impl Scripts {
    /// Stalker `id`'s motion update (stalker-motions.md 3.3) on its
    /// `model`, whose health is above 0 (3.2): the ends its model reached
    /// handed to its slots, callback 29 (`script_animation`) called when a
    /// script animation ended, then the script mode or the normal
    /// selection, `draw` the global generator. The body's target heading
    /// follows the legs (7.2, 7.3). Nothing for an object that isn't a
    /// living stalker; a script error, or a fatal error of the playback, is
    /// fatal.
    pub fn update_stalker_motions(
        &mut self,
        id: u16,
        model: &mut AnimatedModel,
        clock: Clock,
        draw: &mut dyn FnMut() -> u32,
    ) -> Result<Option<Outcome>, crate::Error> {
        let objects = self.game_objects()?;
        let alive = |o: &GameObject| o.entity.as_ref().is_some_and(|e| e.health > 0.0);
        let ended = {
            let mut objects = objects.borrow_mut();
            let Some(o) = objects.get_mut(id).filter(|o| alive(o)) else {
                return Ok(None);
            };
            let Some(s) = o.movement.stalker.as_mut() else {
                return Ok(None);
            };
            s.motions
                .take_script_animation_end(model, clock.time_global)
        };
        if ended {
            self.raise(id, &super::callbacks::ObjectEvent::ScriptAnimation)?;
        }
        let callback_set = self.has_callback(id, 29)?;
        let mut objects = objects.borrow_mut();
        let Some(o) = objects.get_mut(id).filter(|o| alive(o)) else {
            return Ok(None);
        };
        let state = stalker_state(o);
        let Some(s) = o.movement.stalker.as_mut() else {
            return Ok(None);
        };
        let out = (s.motions)
            .update(model, &state, callback_set, clock, draw)
            .map_err(|e| crate::Error {
                kind: crate::ErrorKind::Fatal,
                message: e.to_string(),
            })?;
        if let (Some(yaw), Some(f)) = (out.body_target_yaw, o.fighter.as_mut()) {
            f.body_heading.1 = yaw;
        }
        Ok(Some(out))
    }
}
