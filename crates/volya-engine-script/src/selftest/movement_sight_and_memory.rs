// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-movement-sight-and-memory.md on every target,
//! one check per section, over [`GameObjects`] holding objects of our own
//! and a level with one patrol path.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::game_object::data;
use super::services::run;
use crate::game_object::physics::{LevelQueries, Vec3};
use crate::game_object::{
    CREATURE_CLASSES, Event, GameObjects, ITEM_CLASSES, Kinds, ObjectClass, SpawnData,
};
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 what is registered", registered),
    ("2 kinds and fallbacks", kinds),
    ("3 stalker movement", stalker_movement),
    ("4 paths and patrol paths", paths),
    ("5 space restrictions", restrictions),
    ("6 sight", sight),
    ("7 vision and memory", vision_and_memory),
    ("8 enemies and dangers", enemies),
    ("9 covers", covers),
    ("10 items, weapons and animations", items),
    ("11 monsters", monsters),
    ("12 cef_storage", cef_storage),
    ("13 absent names", absent),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("movement {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const PLAYER: u16 = 0;
const STALKER: u16 = 10;
const RIFLE: u16 = 20;
const TRADER: u16 = 30;
const DOG: u16 = 40;
const BLOODSUCKER: u16 = 50;
const ZOMBIE: u16 = 60;

const SYSTEM: &str = "[stalker]\r\nignore_monster_threshold = 0.25\r\nmax_ignore_distance = 7\r\n\
[dog]\r\npanic_threshold = 0.1\r\n[bloodsucker]\r\npanic_threshold = 0.2\r\n\
[zombie]\r\npanic_threshold = 0.5\r\n[no_threshold]\r\n";

/// A level with the patrol path `walk` of three points.
struct Level;

impl LevelQueries for Level {
    fn explosion_normal(&self, _: Vec3) -> Option<Vec3> {
        None
    }
    fn ray_blocked(&self, _: Vec3, _: Vec3) -> bool {
        false
    }
    fn distance_below(&self, _: Vec3, _: f32) -> Option<f32> {
        None
    }
    fn level_box_top(&self) -> f32 {
        0.0
    }
    fn patrol_point(&self, path: &[u8], point: u32) -> Option<Vec3> {
        (path == b"walk" && point < 3).then_some([point as f32, 0.0, 0.0])
    }
}

struct State {
    s: Scripts,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
}

fn spawned() -> Vec<(ObjectClass, SpawnData)> {
    let mut rifle = data(RIFLE, "", "wpn_ak74", "WP_AK74");
    rifle.parent = Some(STALKER);
    vec![
        (
            ObjectClass::Player,
            data(PLAYER, "actor", "actor", "O_ACTOR"),
        ),
        (
            ObjectClass::Named("CAI_Stalker"),
            data(STALKER, "wolf", "stalker", "AI_STL_S"),
        ),
        (ObjectClass::Named("CWeaponAK74"), rifle),
        (
            ObjectClass::Named("CAI_Trader"),
            data(TRADER, "trader", "trader", "AI_TRADE"),
        ),
        (
            ObjectClass::Named("CAI_Dog"),
            data(DOG, "dog", "dog", "AI_DOG_R"),
        ),
        (
            ObjectClass::Named("CAI_Bloodsucker"),
            data(BLOODSUCKER, "sucker", "bloodsucker", "SM_BLOOD"),
        ),
        (
            ObjectClass::Named("CZombie"),
            data(ZOMBIE, "zombie", "zombie", "SM_ZOMBI"),
        ),
    ]
}

/// A state with the game object basics registered, `_G` loaded, a global
/// `obj(id)` giving handles, the level and the objects of [`spawned`].
fn state_with(objects: Vec<(ObjectClass, SpawnData)>) -> Result<State, String> {
    let tree = Rc::new(Tree(
        [("gamedata/scripts/_g.script", "")]
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect(),
    ));
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(|e| e.message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    let files = Rc::new(RefCell::new(crate::files::FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(|e| e.message)?;
    let shared = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&shared), Rc::clone(&services), files)
        .map_err(|e| e.message)?;
    s.physics_state().map_err(|e| e.message)?.borrow_mut().level = Some(Box::new(Level));
    shared.borrow_mut().level_vertex_lights = vec![0; 10];
    let handles = Rc::clone(&shared);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(handles.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    for (class, data) in objects {
        s.spawn_engine_object(class, data)
            .map_err(|e| e.to_string())?;
    }
    Ok(State {
        s,
        objects: shared,
        services,
    })
}

fn state() -> Result<State, String> {
    state_with(spawned())
}

/// `code` must stop the state with a fatal error whose report includes
/// `row`.
fn fatal(code: &str, row: &str) -> Result<(), String> {
    let mut t = state()?;
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

/// The 88 functions of sections 3 – 11.
pub const FUNCTIONS: [&str; 88] = [
    "set_body_state",
    "set_movement_type",
    "set_mental_state",
    "set_path_type",
    "set_detail_path_type",
    "body_state",
    "target_body_state",
    "movement_type",
    "target_movement_type",
    "mental_state",
    "target_mental_state",
    "path_type",
    "detail_path_type",
    "set_desired_position",
    "set_desired_direction",
    "set_dest_level_vertex_id",
    "head_orientation",
    "set_patrol_path",
    "patrol",
    "get_current_point_index",
    "set_start_point",
    "set_previous_point",
    "patrol_path_make_inactual",
    "path_completed",
    "location_on_path",
    "extrapolate_length",
    "movement_enabled",
    "vertex_in_direction",
    "add_restrictions",
    "remove_restrictions",
    "remove_all_restrictions",
    "in_restrictions",
    "out_restrictions",
    "base_in_restrictions",
    "base_out_restrictions",
    "accessible",
    "accessible_nearest",
    "set_sight",
    "sight_params",
    "get_current_direction",
    "memory_visible_objects",
    "memory_hit_objects",
    "not_yet_visible_objects",
    "memory_time",
    "memory_position",
    "enable_memory_object",
    "make_object_visible_somewhen",
    "fov",
    "range",
    "set_fov",
    "set_range",
    "enable_vision",
    "vision_enabled",
    "visibility_threshold",
    "see",
    "best_enemy",
    "get_enemy",
    "get_enemy_strength",
    "ignore_monster_threshold",
    "restore_ignore_monster_threshold",
    "max_ignore_monster_distance",
    "restore_max_ignore_monster_distance",
    "skip_transfer_enemy",
    "best_danger",
    "best_cover",
    "safe_cover",
    "set_item",
    "set_queue_size",
    "weapon_strapped",
    "weapon_unstrapped",
    "accuracy",
    "add_animation",
    "clear_animations",
    "animation_count",
    "animation_slot",
    "play_cycle",
    "hide_weapon",
    "restore_weapon",
    "set_home",
    "remove_home",
    "jump",
    "berserk",
    "set_alien_control",
    "fake_death_fall_down",
    "fake_death_stand_up",
    "set_custom_panic_threshold",
    "set_default_panic_threshold",
    "get_monster_hit_info",
];

fn lua_list(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("{n:?}")).collect();
    format!("{{{}}}", quoted.join(", "))
}

/// Section 1: the functions on every handle, the classes and constants.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "1",
        &format!(
            r#"
local o = obj({RIFLE})
for _, m in ipairs({functions}) do eq(type(o[m]), "function", m) end
local class_mt = getmetatable(game_object)
for _, c in ipairs({{"CSightParams", "danger_object", "MonsterHitInfo", "MonsterSpace",
    "cef_storage", "memory_object", "game_memory_object", "entity_memory_object",
    "visible_memory_object", "sound_memory_object", "hit_memory_object", "memory_info",
    "not_yet_visible_object"}}) do
  eq(getmetatable(_G[c]) == class_mt, true, c .. " is a class")
end
for _, c in ipairs({{"danger_object", "MonsterHitInfo", "cef_storage", "memory_object",
    "memory_info", "hit_memory_object", "not_yet_visible_object"}}) do
  raises(function() return _G[c]() end, "", c .. " has no constructor")
end
local p = CSightParams()
eq(p.m_sight_type, -1, "a new CSightParams' type (our choice)")
eq(p.m_object, nil, "its object")
eq(CSightParams.eSightTypeDummy, -1, "eSightTypeDummy")
eq(CSightParams.eSightTypeFirePosition, 10, "eSightTypeFirePosition")
eq(CSightParams.eSightTypeCoverLookOver, 8, "eSightTypeCoverLookOver")
eq(danger_object.enemy_sound, 7, "enemy_sound")
eq(danger_object.hit, 2, "the perceive type hit")
eq(MonsterSpace.sound_script, 128, "sound_script")
eq(MonsterSpace.head_anim_kind, 3, "head_anim_kind")
"#,
            functions = lua_list(&FUNCTIONS)
        ),
    )?;
    ensure(FUNCTIONS.len() == 88, || "88 functions".into())
}

/// Section 2: the kinds' classes and the fallbacks on the wrong kind.
pub fn kinds() -> Result<(), String> {
    let count = |k: Kinds| {
        CREATURE_CLASSES
            .iter()
            .chain(ITEM_CLASSES.iter())
            .chain(["CCar", "CHelicopter"].iter())
            .filter(|c| ObjectClass::Named(c).kinds().has(k))
            .count()
    };
    let counts = [
        count(Kinds::FIGHTER),
        count(Kinds::STALKER),
        count(Kinds::MONSTER),
        count(Kinds::SCRIPTED),
        count(Kinds::WEAPON),
    ];
    ensure(counts == [18, 1, 17, 20, 19], || {
        format!("fighter, stalker, monster, scripted, weapon classes: {counts:?}")
    })?;
    ensure(!ObjectClass::Player.kinds().has(Kinds::SCRIPTED), || {
        "the player is not scripted".into()
    })?;
    let mut t = state()?;
    run(
        &mut t.s,
        "2",
        &format!(
            r#"
local r = obj({RIFLE})
eq(r:body_state(), 1, "body_state")
eq(r:movement_type(), 2, "movement_type")
eq(r:mental_state(), 0, "mental_state")
eq(r:path_type(), 3, "path_type")
eq(r:patrol(), "", "patrol")
eq(r:get_current_point_index(), -1, "get_current_point_index")
eq(r:fov(), -1, "fov")
eq(r:range(), -1, "range")
eq(r:animation_count(), -1, "animation_count")
eq(r:in_restrictions(), "", "in_restrictions")
eq(r:accessible(1), false, "accessible")
eq(r:accessible_nearest(vector(), vector()), -1, "accessible_nearest")
eq(r:best_enemy(), nil, "best_enemy")
eq(r:get_enemy_strength(), 0, "get_enemy_strength")
eq(r:ignore_monster_threshold(), 0, "ignore_monster_threshold")
eq(r:fake_death_fall_down(), false, "fake_death_fall_down")
eq(r:sight_params().m_sight_type, -1, "sight_params")
eq(r:head_orientation().x > 1e38, true, "head_orientation")
eq(r:weapon_strapped(), false, "weapon_strapped")
eq(r:weapon_unstrapped(), false, "weapon_unstrapped")
eq(r:location_on_path(1, vector()), -1, "location_on_path")
eq(r:accuracy(), -1, "accuracy")
raises(function() return obj({TRADER}):accuracy() end, "TODO(#1132)", "the trader's accuracy")
"#
        ),
    )?;
    fatal(
        &format!("obj({RIFLE}):memory_visible_objects()"),
        "the object is not a fighter",
    )?;
    fatal(
        &format!("obj({TRADER}):visibility_threshold()"),
        "the object is not a fighter",
    )
}

/// Section 3: the movement request and its readers.
pub fn stalker_movement() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "3",
        &format!(
            r#"
local s = obj({STALKER})
eq(s:target_body_state(), 1, "the first target body state (our choice)")
s:set_body_state(0)
s:set_movement_type(1)
s:set_mental_state(1)
s:set_path_type(2)
s:set_detail_path_type(2)
eq(s:target_body_state(), 0, "target_body_state")
eq(s:target_movement_type(), 1, "target_movement_type")
eq(s:target_mental_state(), 1, "target_mental_state")
eq(s:body_state(), 1, "the current body state is the movement update's")
eq(s:detail_path_type(), 0, "detail_path_type is always 0")
s:set_desired_position(vector():set(1, 2, 3))
s:set_desired_position()
s:set_desired_direction(vector():set(0, 0, 1))
s:set_dest_level_vertex_id(5)
s:set_dest_level_vertex_id(10)
local h = s:head_orientation()
eq(h.z, 1, "the head looks along z at heading 0")
"#
        ),
    )?;
    let o = t.objects.borrow();
    let s = o.get(STALKER).and_then(|o| o.movement.stalker.as_ref());
    let Some(s) = s else {
        return Err("no stalker parts".into());
    };
    let r = &s.request;
    ensure(
        r.desired_position == [1.0, 2.0, 3.0] && !r.desired_position_set,
        || format!("the desired position kept, not set: {r:?}"),
    )?;
    ensure(
        r.desired_direction_set && r.desired_direction == [0.0, 0.0, 1.0],
        || format!("the desired direction: {r:?}"),
    )?;
    ensure(r.dest_level_vertex == Some(5) && s.path_outdated, || {
        format!("vertex 5 taken, 10 (the level has 10) not: {r:?}")
    })?;
    ensure(r.target.path_type == 2 && r.detail_path_type == 2, || {
        format!("path types: {r:?}")
    })
}

/// Section 4: patrol paths, the detail path's members.
pub fn paths() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "4",
        &format!(
            r#"
local s = obj({STALKER})
eq(s:patrol(), "", "no patrol path")
s:set_patrol_path("walk", 0, 1, true)
eq(s:patrol(), "walk", "patrol")
eq(s:get_current_point_index(), -1, "the current point")
s:set_start_point(2)
s:set_previous_point(5)
s:set_patrol_path("nowhere", 0, 1, false)
eq(s:patrol(), "", "a name without a path clears it (believed)")
s:set_patrol_path("walk", 2, 1, false)
eq(obj({TRADER}):patrol(), "", "the trader's scripted part's name (TODO)")
eq(s:path_completed(), false, "path_completed")
local out = vector():set(9, 9, 9)
raises(function() return s:location_on_path(15, out) end, "TODO(#1129)",
  "location_on_path without a detail path")
eq(out.x, 0, "out is the position first")
eq(s:location_on_path(15, nil), -1, "location_on_path without out")
raises(function() return s:extrapolate_length() end, "TODO(#1129)", "its first value")
s:extrapolate_length(2.5)
eq(s:extrapolate_length(), 2.5, "extrapolate_length")
raises(function() return s:movement_enabled() end, "TODO(#1129)", "its first value")
s:movement_enabled(false)
eq(s:movement_enabled(), false, "disabled")
s:movement_enabled(true)
eq(s:movement_enabled(), true, "enabled")
s:patrol_path_make_inactual()
raises(function() return s:vertex_in_direction(1, vector(), 2) end, "TODO(#1130)",
  "the level graph's walk")
"#
        ),
    )?;
    let o = t.objects.borrow();
    let f = o.get(STALKER).and_then(|o| o.movement.fighter.as_ref());
    let Some(f) = f else {
        return Err("no fighter parts".into());
    };
    let p = &f.patrol;
    ensure(
        p.name.as_deref() == Some(b"walk".as_slice())
            && p.start == -1
            && p.start_type == 2
            && p.outdated,
        || format!("the path set again resets its indices: {p:?}"),
    )?;
    ensure(f.detail_path_outdated && !f.path_outdated, || {
        format!("not moved since disabled: {f:?}")
    })
}

/// Section 5: restrictions.
pub fn restrictions() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "5",
        &format!(
            r#"
local s = obj({STALKER})
eq(s:accessible(vector()), true, "accessible without restrictions")
eq(s:accessible(3), true, "a vertex without restrictions")
s:add_restrictions("a, b", "c")
s:add_restrictions("b", "")
eq(s:out_restrictions(), "a,b", "out_restrictions")
eq(s:in_restrictions(), "c", "in_restrictions")
eq(s:base_out_restrictions(), "", "base_out_restrictions")
raises(function() return s:accessible(vector()) end, "TODO(#1130)", "under restrictions")
s:remove_restrictions("a", "")
eq(s:out_restrictions(), "b", "removed")
s:remove_all_restrictions()
eq(s:out_restrictions() .. s:in_restrictions(), "", "all removed")
raises(function() return s:accessible_nearest(vector(), vector()) end, "TODO(#1130)",
  "accessible_nearest")
"#
        ),
    )
}

/// Section 6: `set_sight`'s forms (15's worked example), `sight_params`.
pub fn sight() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "6",
        &format!(
            r#"
local s = obj({STALKER})
local pos = vector():set(1, 2, 3)
s:set_sight(10, pos, 0)
local p = s:sight_params()
eq(p.m_sight_type, 3, "fire position becomes position")
eq(p.m_vector.y, 2, "its vector")
eq(p.m_object, nil, "no object")
s:set_sight(obj({DOG}), true, true)
p = s:sight_params()
eq(p.m_sight_type, 9, "fire object")
eq(rawequal(p.m_object, obj({DOG})), true, "its object")
s:set_sight(obj({DOG}))
eq(s:sight_params().m_sight_type, 4, "object")
s:set_sight(2, nil)
eq(s:sight_params().m_vector.x > 1e38, true, "a vector not set (our choice)")
s:set_sight(6, true, false)
eq(s:sight_params().m_sight_type, 6, "search")
raises(function() s:set_sight("x") end, "no form", "no form takes a text")
obj({DOG}):set_sight(2, pos)
eq(obj({DOG}):sight_params().m_sight_type, -1, "not a stalker")
eq(obj({DOG}):get_current_direction().x, 0, "not the searchlight")
"#
        ),
    )?;
    let o = t.objects.borrow();
    let sight = o
        .get(STALKER)
        .and_then(|o| o.movement.stalker.as_ref())
        .map(|s| s.sight.clone());
    ensure(
        sight.as_ref().is_some_and(|s| s.a && !s.b && s.c.is_none()),
        || format!("(t, a, b) sets A and B: {sight:?}"),
    )
}

/// Section 7: vision and memory, all empty.
pub fn vision_and_memory() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "7",
        &format!(
            r#"
local s = obj({STALKER})
local n = 0
for r in s:not_yet_visible_objects() do n = n + 1 end
for r in s:memory_visible_objects() do n = n + 1 end
for r in s:memory_hit_objects() do n = n + 1 end
eq(n, 0, "nothing remembered")
eq(s:memory_time(obj({DOG})), 0, "memory_time")
eq(s:memory_position(obj({DOG})).x, 0, "memory_position")
s:enable_memory_object(obj({DOG}), false)
s:make_object_visible_somewhen(obj({DOG}))
raises(function() return s:fov() end, "TODO(#1131)", "the eye's first fov")
s:set_fov(90)
s:set_range(50)
eq(s:fov(), 90, "fov")
eq(s:range(), 50, "range")
obj({TRADER}):set_fov(10)
raises(function() return obj({TRADER}):fov() end, "TODO(#1131)", "set_fov needs a fighter")
s:enable_vision(false)
eq(s:vision_enabled(), false, "vision_enabled")
eq(s:see(obj({PLAYER})), false, "see")
eq(obj({TRADER}):see(obj({PLAYER})), false, "the trader sees nothing yet")
eq(s:see("dog"), false, "see a section")
raises(function() return s:visibility_threshold() end, "TODO(#1131)", "visibility_threshold")
"#
        ),
    )
}

/// Section 8: enemies, the ignore thresholds, dangers.
pub fn enemies() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "8",
        &format!(
            r#"
local s = obj({STALKER})
eq(s:best_enemy(), nil, "best_enemy")
eq(s:get_enemy(), nil, "get_enemy")
eq(s:best_danger(), nil, "best_danger")
eq(s:ignore_monster_threshold(), 0.25, "the section's (our choice)")
s:ignore_monster_threshold(3)
eq(s:ignore_monster_threshold(), 1, "limited to 1")
s:max_ignore_monster_distance(12)
eq(s:max_ignore_monster_distance(), 12, "max_ignore_monster_distance")
s:restore_ignore_monster_threshold()
eq(s:ignore_monster_threshold(), 0.25, "restored")
eq(s:max_ignore_monster_distance(), 12, "the other kept")
s:restore_max_ignore_monster_distance()
eq(s:max_ignore_monster_distance(), 7, "restored")
raises(function() return s:get_enemy_strength() end, "TODO(#1131)", "get_enemy_strength")
obj({DOG}):skip_transfer_enemy(true)
"#
        ),
    )?;
    let o = t.objects.borrow();
    let skip =
        (o.get(DOG).and_then(|o| o.movement.monster.as_ref())).map(|m| m.skip_transfer_enemy);
    ensure(skip == Some(true), || {
        format!("skip_transfer_enemy {skip:?}")
    })
}

/// Section 9: no cover is found.
pub fn covers() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "9",
        &format!(
            r#"
local s = obj({STALKER})
eq(s:best_cover(vector(), vector(), 10, 1, 20), nil, "best_cover")
eq(s:safe_cover(vector(), 10, 1), nil, "safe_cover")
"#
        ),
    )
}

/// Section 10: `set_item`, animations, `hide_weapon` and `restore_weapon`.
pub fn items() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "10",
        &format!(
            r#"
local s = obj({STALKER})
s:set_item(6, obj({RIFLE}), 3)
s:add_animation("idle", true, false)
s:add_animation("walk", true, true)
eq(s:animation_count(), 2, "two queued")
s:clear_animations()
eq(s:animation_count(), 0, "cleared")
s:add_animation("idle", true, false)
raises(function() s:add_animation("idle") end, "", "one form, three arguments")
s:play_cycle("idle")
s:play_cycle("idle", false)
s:set_queue_size(3)
eq(s:weapon_unstrapped(), true, "no strappable active item")
raises(function() return obj({RIFLE}):animation_slot() end, "TODO(#1132)", "animation_slot")
obj({DOG}):hide_weapon()
s:restore_weapon()
"#
        ),
    )?;
    let target = (t.objects.borrow().get(STALKER))
        .and_then(|o| o.movement.stalker.as_ref())
        .and_then(|s| s.object_target.clone());
    ensure(
        target.as_ref().is_some_and(|t| {
            t.action == 6 && t.object == Some(RIFLE) && t.queue_size == 3 && t.interval == 300
        }),
        || format!("set_item's target: {target:?}"),
    )?;
    let events = t.objects.borrow_mut().take_events();
    let hides: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::HideWeapon { player, hide, slot } => Some((*player, *hide, *slot)),
            _ => None,
        })
        .collect();
    ensure(hides == [(PLAYER, true, -1), (PLAYER, false, -1)], || {
        format!("the player's weapon events: {hides:?}")
    })?;
    // A dead player gets none.
    if let Some(e) = (t.objects.borrow_mut().get_mut(PLAYER)).and_then(|o| o.entity.as_mut()) {
        e.health = 0.0;
    }
    run(&mut t.s, "10 dead", "obj(0):hide_weapon()")?;
    let events = t.objects.borrow_mut().take_events();
    ensure(events.is_empty(), || format!("a dead player: {events:?}"))
}

/// Section 11: the monsters, 15's worked example of the panic thresholds.
pub fn monsters() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "11",
        &format!(
            r#"
local d = obj({DOG})
d:set_custom_panic_threshold(0.5)
obj({TRADER}):set_custom_panic_threshold(0.9)
obj({TRADER}):set_default_panic_threshold()
obj({STALKER}):set_custom_panic_threshold(0.9)
d:set_home("walk", 1, 5, true)
d:berserk()
d:jump(vector():set(1, 0, 0), 2)
local z = obj({ZOMBIE})
eq(z:fake_death_fall_down(), true, "fake death starts")
eq(z:fake_death_fall_down(), false, "already")
z:fake_death_stand_up()
eq(z:fake_death_fall_down(), true, "again after standing up")
eq(d:fake_death_fall_down(), false, "not a zombie")
local h = d:get_monster_hit_info()
eq(h.who, nil, "no hitter")
eq(h.direction.z, 1, "direction")
eq(h.time, 0, "time")
h.time = 12.7
eq(h.time, 12, "made an integer")
h.who = obj({PLAYER})
eq(rawequal(h.who, obj({PLAYER})), true, "who assigned")
h.direction = vector():set(1, 0, 0)
eq(h.direction.x, 1, "direction assigned")
eq(d:get_monster_hit_info().time, 0, "the copy only")
obj({BLOODSUCKER}):set_alien_control(true)
"#
        ),
    )?;
    {
        let o = t.objects.borrow();
        let dog = o.get(DOG).and_then(|o| o.movement.monster.clone());
        ensure(
            dog.as_ref().is_some_and(|m| {
                m.panic_threshold == 0.5
                    && m.default_panic_threshold == 0.1
                    && m.home.path.as_deref() == Some(b"walk".as_slice())
                    && m.home.aggressive
                    && m.berserk_start.is_some()
                    && m.jump == Some(([1.0, 0.0, 0.0], 2.0))
            }),
            || format!("the dog: {dog:?}"),
        )?;
        let trader = o.get(TRADER).map(|o| o.movement.monster.is_some());
        ensure(trader == Some(false), || "the trader is no monster".into())?;
    }
    run(
        &mut t.s,
        "11 default",
        &format!("obj({DOG}):set_default_panic_threshold() obj({DOG}):remove_home()"),
    )?;
    {
        let o = t.objects.borrow();
        let dog = o.get(DOG).and_then(|o| o.movement.monster.clone());
        ensure(
            dog.as_ref().is_some_and(|m| {
                m.panic_threshold == 0.1 && m.home.path.is_none() && !m.home.aggressive
            }),
            || format!("the dog's default: {dog:?}"),
        )?;
    }
    let events = t.objects.borrow_mut().take_events();
    ensure(
        events
            == [
                Event::Visibility {
                    object: BLOODSUCKER,
                    hidden: true,
                },
                Event::HideWeapon {
                    player: PLAYER,
                    hide: true,
                    slot: -1,
                },
            ],
        || format!("the control's events: {events:?}"),
    )?;
    // A monster whose section lacks the key: fatal as it loads.
    let mut missing = spawned();
    missing.push((
        ObjectClass::Named("CAI_Flesh"),
        data(70, "flesh", "no_threshold", "AI_FLESH"),
    ));
    match state_with(missing) {
        Err(e) if e.contains("Can't find variable panic_threshold in [no_threshold]") => Ok(()),
        Err(e) => Err(format!("a missing key: {e}")),
        Ok(_) => Err("a missing key loads".into()),
    }
}

/// Section 12: `cef_storage`'s `evaluate`.
pub fn cef_storage() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "12",
        r#"eq(type(cef_storage.evaluate), "function", "evaluate")"#,
    )
}

/// Section 13: the absent names are no members.
pub fn absent() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "13",
        &format!(
            r#"
local s = obj({STALKER})
for _, m in ipairs({{"nearest_accessible_position", "set_node_evaluator",
    "set_path_evaluator"}}) do
  eq(s[m], nil, m)
end
"#
        ),
    )
}
