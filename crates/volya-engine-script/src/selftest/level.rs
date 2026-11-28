// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-level.md on every target, one check per
//! section, over the game object basics' test state with a level of our
//! own: a 3 × 3 grid of level vertices and the patrol path of 8.2.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use volya_engine_formats::level_ai::{LevelAi, LevelAiHeader, LevelAiLinks, NO_LINK};
use volya_engine_formats::ltx;
use volya_engine_formats::patrol_path::{PathLink, PathPoint, PatrolPath, PatrolPaths};

use super::ensure;
use super::game_object::{STALKER, State, data, state};
use super::services::run;
use crate::ErrorKind;
use crate::game_object::ObjectClass;
use crate::level::{Level, LevelEvent, LevelState, LevelVertices};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 the level", the_level),
    ("3 level vertices", level_vertices),
    ("4 patrol paths", patrol_paths),
    ("5 cover points and object parameters", cover_points),
    ("6 the client spawn manager", spawn_manager),
    ("6.4 a failing function", spawn_manager_failure),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("level {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The grid's level vertex at column `c`, row `r`.
const fn at(c: u32, r: u32) -> u32 {
    c * 3 + r
}

/// A 3 × 3 grid of cell size 0.7 at height 0, each vertex linked to its
/// neighbours but (1, 1) and (2, 1), with vertex 0's cover values those of
/// 8.1.
pub fn grid() -> LevelVertices {
    let header = LevelAiHeader {
        vertex_count: 9,
        cell_size: 0.7,
        height_range: 10.0,
        box_min: [0.0; 3],
        box_max: [1.4, 10.0, 1.4],
        identifier: [0; 16],
    };
    let mut positions = Vec::new();
    let mut links = Vec::new();
    for c in 0..3 {
        for r in 0..3 {
            positions.push((at(c, r), 0));
            let mut l = [
                if c > 0 { at(c - 1, r) } else { NO_LINK },
                if r < 2 { at(c, r + 1) } else { NO_LINK },
                if c < 2 { at(c + 1, r) } else { NO_LINK },
                if r > 0 { at(c, r - 1) } else { NO_LINK },
            ];
            if (c, r) == (1, 1) {
                l[2] = NO_LINK;
            }
            if (c, r) == (2, 1) {
                l[0] = NO_LINK;
            }
            let cover = if (c, r) == (0, 0) {
                [3, 9, 13, 7]
            } else {
                [0; 4]
            };
            links.push((l, cover));
        }
    }
    LevelVertices {
        ai: Arc::new(LevelAi::new(header, &positions)),
        links: Arc::new(LevelAiLinks::new(&links)),
    }
}

/// The patrol path of 8.2, and a path without points.
fn paths() -> PatrolPaths {
    let point = |name: &str, position, level_vertex| PathPoint {
        name: name.into(),
        position,
        flags: 0,
        level_vertex,
        game_vertex: 1765,
    };
    PatrolPaths::from_paths(vec![
        PatrolPath {
            name: "mil_actor_path".into(),
            points: vec![
                point("wp00", [-229.701, -21.775, 197.692], 101_671),
                point("wp01", [-227.432, -21.775, 197.783], 103_407),
            ],
            links: vec![PathLink {
                from: 0,
                to: 1,
                weight: 1.0,
            }],
        },
        PatrolPath {
            name: "empty".into(),
            points: Vec::new(),
            links: Vec::new(),
        },
    ])
}

/// The level of our own: `l01_escape`'s name and box (2.4), the grid.
fn level() -> Level {
    Level {
        name: Some(b"l01_escape".to_vec()),
        bounding_volume: ([-678.897, -38.667, -633.791], [766.565, 113.877, 811.671]),
        vertices: Some(grid()),
    }
}

/// The game object basics' state with the level registered over `level`;
/// the level started when `started`.
fn level_state(started: bool) -> Result<(State, Rc<RefCell<LevelState>>), String> {
    let mut t = state()?;
    let level_state = Rc::new(RefCell::new(LevelState::new()));
    level_state.borrow_mut().patrol_paths = Some(Rc::new(paths()));
    t.s.register_level(Rc::clone(&level_state))
        .map_err(|e| e.message)?;
    if started {
        t.s.start_level(level()).map_err(|e| e.message)?;
    }
    Ok((t, level_state))
}

/// `code` must stop the state with a fatal error whose report includes
/// `row`.
fn fatal(started: bool, code: &str, row: &str) -> Result<(), String> {
    let (mut t, _) = level_state(started)?;
    let result =
        t.s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains(row), || {
        format!("{code}: no {row:?} in the log:\n{log}")
    })
}

/// Section 1 and 7: the names registered, and those that are not.
pub fn registered() -> Result<(), String> {
    let (mut t, _) = level_state(false)?;
    run(
        &mut t.s,
        "1",
        r#"
for _, f in ipairs({"name", "present", "object_by_id", "vertex_position",
    "vertex_in_direction", "cover_in_direction", "patrol_path_exists",
    "get_bounding_volume", "physics_world", "spawn_phantom", "client_spawn_manager",
    "add_call", "remove_call", "remove_calls_for_object"}) do
  eq(type(level[f]), "function", "level." .. f)
end
for _, f in ipairs({"check_object", "debug_actor", "debug_object", "actor", "object"}) do
  eq(level[f], nil, "level." .. f .. " is not registered")
end
for _, c in ipairs({"patrol", "cover_point", "object_params", "client_spawn_manager"}) do
  eq(type(_G[c]), "userdata", c .. " is a class")
end
raises(function() return client_spawn_manager() end, "", "no constructor")
eq(__volya_level, nil, "the Rust side is hidden")
"#,
    )
}

/// Section 2: the level's functions, with and without a level.
pub fn the_level() -> Result<(), String> {
    let (mut t, state) = level_state(false)?;
    run(
        &mut t.s,
        "2.1 none",
        r#"eq(level.present(), false, "no level")"#,
    )?;
    t.s.start_level(level()).map_err(|e| e.message)?;
    run(
        &mut t.s,
        "2",
        &format!(
            r#"
eq(level.present(), true, "present")
eq(level.name(), "l01_escape", "name")
eq(level.object_by_id({STALKER}):id(), {STALKER}, "object_by_id")
eq(level.object_by_id({STALKER}.9):id(), {STALKER}, "truncated")
eq(level.object_by_id(65536 + {STALKER}), nil, "65,536 or more")
eq(level.object_by_id(4321), nil, "no such object")
local b = level.get_bounding_volume()
eq(string.format("%.3f %.3f %.3f", b.min.x, b.min.y, b.min.z), "-678.897 -38.667 -633.791", "min")
eq(string.format("%.3f %.3f %.3f", b.max.x, b.max.y, b.max.z), "766.565 113.877 811.671", "max")
b.min.x = 0
eq(string.format("%.3f", level.get_bounding_volume().min.x), "-678.897", "a new box")
eq(level.client_spawn_manager() ~= nil, true, "the spawn manager")
"#
        ),
    )?;
    ensure(state.borrow_mut().take_events().is_empty(), || {
        "no events yet".into()
    })?;
    // 2.6 needs the section.
    let system = ltx::parse(b"[m_phantom]\r\nclass = AI_PHANT\r\n").map_err(|e| e.to_string())?;
    let files = Rc::clone(&t.s.game_object_state.as_ref().unwrap().files);
    files.borrow_mut().system = Rc::new(system);
    run(
        &mut t.s,
        "2.6",
        "eq(level.spawn_phantom(vector():set(1, 2, 3)), nil, 'nothing')",
    )?;
    let events = state.borrow_mut().take_events();
    ensure(
        events
            == [LevelEvent::SpawnPhantom {
                position: [1.0, 2.0, 3.0],
            }],
        || format!("spawn_phantom: {events:?}"),
    )?;
    t.s.end_level().map_err(|e| e.message)?;
    run(
        &mut t.s,
        "2.1 gone",
        r#"eq(level.present(), false, "ended")"#,
    )?;
    fatal(false, "level.name()", "there is no level")?;
    fatal(false, "level.object_by_id(1)", "there is no level")?;
    fatal(true, "level.physics_world()", "isn't registered")?;
    fatal(
        true,
        "level.spawn_phantom(vector())",
        "Cannot find item with section",
    )
}

/// Section 3: positions, the walk and cover, with 8.1's worked values.
pub fn level_vertices() -> Result<(), String> {
    let (mut t, _) = level_state(true)?;
    run(
        &mut t.s,
        "3",
        &format!(
            r#"
local p = level.vertex_position({v7})
eq(string.format("%.2f %.2f %.2f", p.x, p.y, p.z), "1.40 0.00 0.70", "vertex_position")
local x = vector():set(1, 0, 0)
eq(level.vertex_in_direction(0, x, 1.4), {v6}, "reaches the target's vertex")
eq(level.vertex_in_direction(0, vector():set(10, 0, 0), 1.4), {v6}, "normalised")
eq(level.vertex_in_direction(0, x, 50), {v6}, "leaves the level")
eq(level.vertex_in_direction(1, x, 1.4), {v4}, "a missing link stops it")
eq(level.vertex_in_direction(0, vector():set(0, 0, 1), 0.7), 1, "+z")
eq(level.vertex_in_direction(4, vector():set(-1, 0, 0), 0.2), 4, "within its cell")
eq(level.vertex_in_direction(3, vector(), 5), 3, "no direction")
local function cover(x, z) return string.format("%.4f", level.cover_in_direction(0, vector():set(x, 7, z))) end
eq(cover(0, 1), "0.6000", "+z")
eq(cover(-1, 0), "0.2000", "-x")
eq(cover(0, -1), "0.4667", "-z")
eq(cover(1, 0), "1.0000", "+x extrapolates")
eq(cover(1, 1), "0.8000", "(1, 1)")
eq(cover(1, -1), "0.6667", "(1, -1)")
eq(cover(0, 0), "0.6000", "no heading")
eq(level.cover_in_direction(5, x), 0, "no cover")
"#,
            v4 = at(1, 1),
            v6 = at(2, 0),
            v7 = at(2, 1),
        ),
    )?;
    fatal(true, "level.vertex_position(9)", "beyond the level")?;
    fatal(
        true,
        "level.vertex_in_direction(-1, vector(), 1)",
        "beyond the level",
    )?;
    fatal(
        true,
        "level.cover_in_direction(100, vector())",
        "beyond the level",
    )
}

/// Section 4: the store, the class and its members, with 8.2's path.
pub fn patrol_paths() -> Result<(), String> {
    let (mut t, _) = level_state(true)?;
    run(
        &mut t.s,
        "4",
        r#"
eq(level.patrol_path_exists("mil_actor_path"), true, "exists")
eq(level.patrol_path_exists("MIL_ACTOR_PATH"), false, "exact")
eq(level.patrol_path_exists("way1"), false, "missing")
eq(patrol.start, 0, "start") eq(patrol.stop, 0, "stop") eq(patrol.nearest, 2, "nearest")
eq(patrol.custom, 3, "custom") eq(patrol.next, 4, "next") eq(patrol.continue, 1, "continue")
eq(patrol.dummy, -1, "dummy")
local p = patrol("mil_actor_path")
eq(p:count(), 2, "count")
eq(p:index("wp01"), 1, "index")
eq(p:index("wp02"), -1, "no index")
eq(p:name(0), "wp00", "name")
eq(p:terminal(0), false, "terminal 0")
eq(p:terminal(1), true, "terminal 1")
eq(p:flags(0):get(), 0, "flags")
eq(p:flag(0, 3), false, "flag")
eq(p:level_vertex_id(0), 101671, "level vertex")
eq(p:game_vertex_id(1), 1765, "game vertex")
eq(p:get_nearest(p:point(1)), 1, "nearest")
eq(p:get_nearest(vector():set(-300, 0, 0)), 0, "nearest to point 0")
local q = p:point(5)
eq(string.format("%.3f %.3f %.3f", q.x, q.y, q.z), "-229.701 -21.775 197.692", "point 0 for 5")
for _, args in ipairs({{"mil_actor_path", 1}, {"mil_actor_path", 1, 0},
    {"mil_actor_path", 1, 0, nil}, {"mil_actor_path", 1, 0, false, 3}}) do
  eq(patrol(unpack(args)):count(), 2, "constructed with " .. #args)
end
raises(function() return patrol() end, "no constructor", "no name")
raises(function() return patrol(5) end, "no constructor", "a number for a name")
"#,
    )?;
    fatal(true, "patrol('way1'):count()", "the patrol has no path")?;
    fatal(
        true,
        "patrol('mil_actor_path'):name(2)",
        "a point the path lacks",
    )?;
    fatal(true, "patrol('empty'):point(0)", "the path has no points")?;
    fatal(
        true,
        "patrol('empty'):get_nearest(vector())",
        "the path has no points",
    )
}

/// Section 5: the classes scripts receive but never make.
pub fn cover_points() -> Result<(), String> {
    let (mut t, _) = level_state(true)?;
    run(
        &mut t.s,
        "5",
        r#"
raises(function() return cover_point() end, "", "no cover point constructor")
raises(function() return object_params() end, "", "no object parameters constructor")
eq(type(cover_point.position), "function", "position")
eq(type(cover_point.level_vertex_id), "function", "level_vertex_id")
"#,
    )
}

/// Section 6: add, remove, the engine's calls and the drops.
pub fn spawn_manager() -> Result<(), String> {
    let (mut t, _) = level_state(true)?;
    run(
        &mut t.s,
        "6.2",
        &format!(
            r##"
calls = {{}}
local function record(...) calls[#calls + 1] = {{n = select("#", ...), ...}} end
local csm = level.client_spawn_manager()
csm:add({STALKER}, 0, record)
eq(#calls, 1, "called at once")
eq(calls[1].n, 2, "id, handle") eq(calls[1][1], {STALKER}, "the id")
eq(calls[1][2]:id(), {STALKER}, "the handle")
csm:add(65536 + {STALKER}, 0, record, "v")
eq(calls[2][1], "v", "the bound value first") eq(calls[2][2], {STALKER}, "low 16 bits")
csm:add(77, 5, function(id) calls[#calls + 1] = "5" end)
csm:add(77, 2, function(id) calls[#calls + 1] = "2" end)
csm:add(77, 9, record, "nine")
csm:add(77, 9, record, "replaced")
csm:add(78, 3, record)
csm:add(79, 4, record)
csm:add(79, 6, record)
csm:remove(79, 6)
csm:remove(79, 6)
csm:remove(80, 1)
raises(function() csm:add(77, 1) end, "no form", "no function")
calls = {{}}
"##
        ),
    )?;
    let spawn = |t: &mut State, id: u16| -> Result<(), String> {
        let d = data(id, "x", "stalker", "AI_STL_S");
        t.s.spawn_engine_object(ObjectClass::Named("CAI_Stalker"), d)
            .map_err(|e| e.to_string())?;
        t.s.client_spawned(id).map_err(|e| e.message)
    };
    spawn(&mut t, 77)?;
    t.s.spawn_requester_gone(3).map_err(|e| e.message)?;
    spawn(&mut t, 78)?;
    spawn(&mut t, 79)?;
    run(
        &mut t.s,
        "6.4",
        r#"
eq(calls[1], "2", "requester 2 first") eq(calls[2], "5", "then 5")
eq(calls[3][1], "replaced", "then 9, replaced") eq(calls[3][2], 77, "its id")
eq(calls[3][3]:id(), 77, "its handle")
eq(calls[4][1], 79, "78's entry dropped with its requester; 79's of 4 kept")
eq(#calls, 4, "each once")
"#,
    )?;
    t.s.client_spawned(77).map_err(|e| e.message)?;
    run(
        &mut t.s,
        "6.4 once",
        r#"eq(#calls, 4, "dropped once called")"#,
    )?;
    run(
        &mut t.s,
        "6.5",
        "level.client_spawn_manager():add(90, 1, function() calls[#calls + 1] = 'late' end)",
    )?;
    t.s.end_level().map_err(|e| e.message)?;
    t.s.start_level(level()).map_err(|e| e.message)?;
    spawn(&mut t, 90)?;
    run(
        &mut t.s,
        "6.5 cleared",
        r#"eq(#calls, 4, "emptied with the level")"#,
    )
}

/// 6.2 step 1 and 6.4: a failing function is fatal (D 1.2).
pub fn spawn_manager_failure() -> Result<(), String> {
    fatal(
        true,
        &format!(
            "level.client_spawn_manager():add({STALKER}, 0, function() error('at once', 0) end)"
        ),
        "at once",
    )?;
    let (mut t, _) = level_state(true)?;
    run(
        &mut t.s,
        "6.4 failing",
        "level.client_spawn_manager():add(91, 0, function() error('later', 0) end)",
    )?;
    let d = data(91, "x", "stalker", "AI_STL_S");
    t.s.spawn_engine_object(ObjectClass::Named("CAI_Stalker"), d)
        .map_err(|e| e.to_string())?;
    let result = t.s.client_spawned(91);
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal && e.message == "LUA error: later"),
        || format!("a failing call: {result:?}"),
    )?;
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains("later"), || format!("not logged:\n{log}"))
}
