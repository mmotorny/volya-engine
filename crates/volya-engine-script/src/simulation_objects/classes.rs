// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The 60 classes (2.1) with their constructors (2.2), properties and
//! methods (section 3), the engine's own implementations of the
//! dispatched ones (4.2), and which the engine dispatches (4.1).
//!
//! Every class registers its whole set of members, its own and those of
//! all its bases (a class's own first, then each base's in the listed
//! order: a name found first wins), with the first listed base as its
//! engine base; so a second base's members reach it as they do through
//! the first's. Each member's implementation is that of the class that
//! registers it, which `Base.m(self, …)` runs (spec A 6.1).

use std::rc::Rc;

use volya_engine_core::simulation::uses_level_graph;
use volya_engine_core::switching::Candidate;
use volya_engine_formats::object::{Kind, Object, ObjectRecord, TraderLike};

use super::{Context, SimulationObject, truth};
use crate::Value;
use crate::classes::{EngineClass, Objects, Operand};
use crate::data_streams::NetPacket;
use crate::files::{IniFile, config_from_text};
use crate::game_object::game_bytes;
use crate::object_factory::ObjectFactory;
use crate::services::{Fatal, report_fatal};
use crate::simulation::movement::Brain;
use crate::values::{Vector, lua_number};

/// A class of section 2.1: its listed bases, in order, and whether it has
/// the constructor taking one string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Class {
    pub name: &'static str,
    pub bases: &'static [&'static str],
    pub constructible: bool,
}

const fn listed(name: &'static str, bases: &'static [&'static str], constructible: bool) -> Class {
    Class {
        name,
        bases,
        constructible,
    }
}

const DOV: &str = "cse_alife_dynamic_object_visual";

/// The 60 classes (2.1), bases before the classes deriving from them.
pub const CLASSES: [Class; 60] = [
    listed("cse_abstract", &["cpure_server_object"], false),
    listed("cse_shape", &[], false),
    listed("cse_visual", &[], false),
    listed("cse_motion", &[], false),
    listed("cse_ph_skeleton", &[], false),
    listed(
        "cse_alife_schedulable",
        &["ipure_schedulable_object"],
        false,
    ),
    listed("cse_alife_group_abstract", &[], false),
    listed("cse_alife_inventory_item", &[], false),
    listed("cse_alife_trader_abstract", &[], false),
    listed("cse_spectator", &["cse_abstract"], true),
    listed("cse_temporary", &["cse_abstract"], true),
    listed("cse_alife_graph_point", &["cse_abstract"], true),
    listed("cse_alife_object", &["cse_abstract"], true),
    listed("CSE_AbstractVisual", &["cse_visual", "cse_abstract"], true),
    listed(
        "cse_alife_object_climable",
        &["cse_shape", "cse_abstract"],
        true,
    ),
    listed("cse_alife_dynamic_object", &["cse_alife_object"], true),
    listed(DOV, &["cse_alife_dynamic_object", "cse_visual"], true),
    listed(
        "cse_alife_space_restrictor",
        &["cse_alife_dynamic_object", "cse_shape"],
        true,
    ),
    listed(
        "cse_custom_zone",
        &["cse_alife_dynamic_object", "cse_shape"],
        true,
    ),
    listed(
        "cse_alife_online_offline_group",
        &["cse_alife_dynamic_object", "cse_alife_schedulable"],
        true,
    ),
    listed(
        "cse_alife_level_changer",
        &["cse_alife_space_restrictor"],
        true,
    ),
    listed(
        "cse_alife_team_base_zone",
        &["cse_alife_space_restrictor"],
        true,
    ),
    listed(
        "cse_alife_smart_zone",
        &["cse_alife_space_restrictor", "cse_alife_schedulable"],
        true,
    ),
    listed("cse_anomalous_zone", &["cse_custom_zone"], true),
    listed("cse_torrid_zone", &["cse_custom_zone", "cse_motion"], true),
    listed(
        "cse_zone_visual",
        &["cse_anomalous_zone", "cse_visual"],
        true,
    ),
    listed("cse_alife_object_projector", &[DOV], true),
    listed("cse_alife_object_breakable", &[DOV], true),
    listed("cse_alife_mounted_weapon", &[DOV], true),
    listed("cse_alife_creature_abstract", &[DOV], true),
    listed(
        "cse_alife_ph_skeleton_object",
        &[DOV, "cse_ph_skeleton"],
        true,
    ),
    listed("cse_alife_car", &[DOV, "cse_ph_skeleton"], true),
    listed(
        "cse_alife_object_hanging_lamp",
        &[DOV, "cse_ph_skeleton"],
        true,
    ),
    listed("cse_alife_object_physic", &[DOV, "cse_ph_skeleton"], true),
    listed(
        "cse_alife_helicopter",
        &[DOV, "cse_motion", "cse_ph_skeleton"],
        true,
    ),
    listed("cse_alife_item", &[DOV, "cse_alife_inventory_item"], true),
    listed("cse_alife_item_torch", &["cse_alife_item"], true),
    listed("cse_alife_item_ammo", &["cse_alife_item"], true),
    listed("cse_alife_item_weapon", &["cse_alife_item"], true),
    listed("cse_alife_item_detector", &["cse_alife_item"], true),
    listed("cse_alife_item_artefact", &["cse_alife_item"], true),
    listed("cse_alife_item_pda", &["cse_alife_item"], true),
    listed("cse_alife_item_document", &["cse_alife_item"], true),
    listed("cse_alife_item_grenade", &["cse_alife_item"], true),
    listed("cse_alife_item_explosive", &["cse_alife_item"], true),
    listed("cse_alife_item_bolt", &["cse_alife_item"], true),
    listed("cse_alife_item_custom_outfit", &["cse_alife_item"], true),
    listed(
        "cse_alife_item_weapon_shotgun",
        &["cse_alife_item_weapon"],
        true,
    ),
    listed(
        "cse_alife_item_weapon_magazined",
        &["cse_alife_item_weapon"],
        true,
    ),
    listed(
        "cse_alife_item_weapon_magazined_w_gl",
        &["cse_alife_item_weapon_magazined"],
        true,
    ),
    listed(
        "cse_alife_trader",
        &[DOV, "cse_alife_trader_abstract"],
        true,
    ),
    listed(
        "cse_alife_creature_crow",
        &["cse_alife_creature_abstract"],
        true,
    ),
    listed(
        "cse_alife_creature_phantom",
        &["cse_alife_creature_abstract"],
        true,
    ),
    listed(
        "cse_alife_creature_actor",
        &[
            "cse_alife_creature_abstract",
            "cse_alife_trader_abstract",
            "cse_ph_skeleton",
        ],
        true,
    ),
    listed(
        "cse_alife_monster_abstract",
        &["cse_alife_creature_abstract", "cse_alife_schedulable"],
        true,
    ),
    listed(
        "cse_alife_monster_zombie",
        &["cse_alife_monster_abstract"],
        true,
    ),
    listed(
        "cse_alife_monster_base",
        &["cse_alife_monster_abstract", "cse_ph_skeleton"],
        true,
    ),
    listed(
        "cse_alife_psydog_phantom",
        &["cse_alife_monster_base"],
        true,
    ),
    listed(
        "cse_alife_human_abstract",
        &["cse_alife_trader_abstract", "cse_alife_monster_abstract"],
        true,
    ),
    listed(
        "cse_alife_human_stalker",
        &["cse_alife_human_abstract", "cse_ph_skeleton"],
        true,
    ),
];

/// Spec K's classes this spec names (its introduction): bases of two of
/// the 60, and the classes of `brain()`'s and `task(obj)`'s results.
/// Registered without members when no other registration made them.
pub const SPEC_K_CLASSES: [&str; 5] = [
    "cpure_server_object",
    "ipure_schedulable_object",
    "CALifeMonsterBrain",
    "CALifeHumanBrain",
    "CALifeSmartTerrainTask",
];

/// The class of the 60 named `name`.
pub fn class(name: &str) -> Option<&'static Class> {
    CLASSES.iter().find(|c| c.name == name)
}

/// Whether class `name` is `base` or derives from it.
pub fn derives(name: &str, base: &str) -> bool {
    name == base || class(name).is_some_and(|c| c.bases.iter().any(|b| derives(b, base)))
}

/// The 14 item classes other than `cse_alife_item` (3.10, 4.1).
const ITEMS: [&str; 14] = [
    "cse_alife_item_torch",
    "cse_alife_item_ammo",
    "cse_alife_item_weapon",
    "cse_alife_item_detector",
    "cse_alife_item_artefact",
    "cse_alife_item_pda",
    "cse_alife_item_document",
    "cse_alife_item_grenade",
    "cse_alife_item_explosive",
    "cse_alife_item_bolt",
    "cse_alife_item_custom_outfit",
    "cse_alife_item_weapon_shotgun",
    "cse_alife_item_weapon_magazined",
    "cse_alife_item_weapon_magazined_w_gl",
];

/// 4.1's first row.
const FIRST: [&str; 6] = [
    "CSE_AbstractVisual",
    "cse_alife_graph_point",
    "cse_alife_item",
    "cse_alife_object_climable",
    "cse_spectator",
    "cse_temporary",
];

/// 4.1's 19 classes.
const NINETEEN: [&str; 19] = [
    "cse_alife_dynamic_object",
    DOV,
    "cse_alife_ph_skeleton_object",
    "cse_alife_space_restrictor",
    "cse_alife_level_changer",
    "cse_alife_team_base_zone",
    "cse_alife_online_offline_group",
    "cse_alife_object_projector",
    "cse_alife_object_breakable",
    "cse_alife_mounted_weapon",
    "cse_alife_object_hanging_lamp",
    "cse_alife_object_physic",
    "cse_alife_car",
    "cse_alife_helicopter",
    "cse_alife_trader",
    "cse_custom_zone",
    "cse_anomalous_zone",
    "cse_torrid_zone",
    "cse_zone_visual",
];

const CREATURES: [&str; 4] = [
    "cse_alife_creature_abstract",
    "cse_alife_creature_actor",
    "cse_alife_creature_crow",
    "cse_alife_creature_phantom",
];

const MONSTERS: [&str; 6] = [
    "cse_alife_monster_abstract",
    "cse_alife_monster_zombie",
    "cse_alife_monster_base",
    "cse_alife_psydog_phantom",
    "cse_alife_human_abstract",
    "cse_alife_human_stalker",
];

const SET_FIRST: [&str; 4] = ["STATE_Read", "STATE_Write", "init", "FillProps"];
const SET_OBJECT: [&str; 5] = [
    "can_switch_online",
    "can_switch_offline",
    "used_ai_locations",
    "can_save",
    "interactive",
];
const SET_DYNAMIC: [&str; 5] = [
    "keep_saved_data_anyway",
    "on_spawn",
    "on_before_register",
    "on_register",
    "on_unregister",
];
const SET_CREATURE: [&str; 4] = ["g_team", "g_squad", "g_group", "on_death"];
const SET_SMART_ZONE: [&str; 8] = [
    "detect_probability",
    "smart_touch",
    "enabled",
    "suitable",
    "register_npc",
    "unregister_npc",
    "task",
    "update",
];

/// The methods the engine dispatches for objects whose engine base is
/// `class` (4.1); none for the classes without a constructor.
pub fn dispatched_methods(class: &str) -> Vec<&'static str> {
    let mut set = Vec::new();
    let has = |list: &[&str]| list.contains(&class);
    let object = class == "cse_alife_object";
    let dynamic = has(&NINETEEN)
        || has(&ITEMS)
        || has(&CREATURES)
        || has(&MONSTERS)
        || class == "cse_alife_smart_zone";
    if has(&FIRST) || object || dynamic {
        set.extend(SET_FIRST);
    }
    if object || dynamic {
        set.extend(SET_OBJECT);
    }
    if dynamic {
        set.extend(SET_DYNAMIC);
    }
    if has(&ITEMS) {
        set.push("bfUseful");
    }
    if has(&CREATURES) || has(&MONSTERS) {
        set.extend(SET_CREATURE);
    }
    if has(&MONSTERS) {
        set.push("update");
    }
    if class == "cse_alife_smart_zone" {
        set.extend(SET_SMART_ZONE);
    }
    set
}

/// Whether the engine's call of `method` on an object whose engine base is
/// `class` reaches a script override (4.1).
pub fn dispatched(class: &str, method: &str) -> bool {
    dispatched_methods(class).contains(&method)
}

/// The members a class registers itself (section 3; and on each class with
/// a constructor, its dispatched methods, 4.1, with `init` and `FillProps`,
/// 3.12).
fn own_members(class: &Class) -> Vec<&'static str> {
    let mut own: Vec<&'static str> = match class.name {
        "cse_abstract" => vec![
            "id",
            "parent_id",
            "script_version",
            "position",
            "name",
            "section_name",
            "clsid",
            "spawn_ini",
            "STATE_Read",
            "STATE_Write",
            "UPDATE_Read",
            "UPDATE_Write",
        ],
        "cse_alife_object" => vec![
            "m_game_vertex_id",
            "m_level_vertex_id",
            "m_story_id",
            "online",
            "can_switch_online",
            "can_switch_offline",
            "used_ai_locations",
            "can_save",
            "interactive",
            "visible_for_map",
            "move_offline",
        ],
        "cse_alife_dynamic_object" => SET_DYNAMIC.to_vec(),
        "cse_alife_creature_abstract" => vec![
            "team", "squad", "group", "alive", "health", "g_team", "g_squad", "g_group", "on_death",
        ],
        "cse_alife_monster_abstract" => vec![
            "m_smart_terrain_id",
            "smart_terrain_id",
            "clear_smart_terrain",
            "smart_terrain_task_activate",
            "smart_terrain_task_deactivate",
            "rank",
            "brain",
            "update",
        ],
        "cse_alife_trader_abstract" => {
            vec!["rank", "reputation", "community", "profile_name"]
        }
        "cse_alife_human_abstract" => vec!["brain", "set_rank"],
        "cse_alife_smart_zone" => SET_SMART_ZONE.to_vec(),
        "cse_anomalous_zone" => vec!["spawn_artefacts"],
        "CSE_AbstractVisual" => vec!["getStartupAnimation"],
        name if ITEMS.contains(&name) => vec!["bfUseful"],
        _ => Vec::new(),
    };
    for name in dispatched_methods(class.name) {
        if !own.contains(&name) {
            own.push(name);
        }
    }
    own
}

/// Every member of `class` with the class whose implementation it is:
/// its own, then each base's in order, a name found first winning.
pub fn members(class: &Class) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = own_members(class)
        .into_iter()
        .map(|m| (m, class.name))
        .collect();
    for base in class.bases {
        let Some(base) = self::class(base) else {
            continue;
        };
        for (m, owner) in members(base) {
            if !out.iter().any(|(n, _)| *n == m) {
                out.push((m, owner));
            }
        }
    }
    out
}

/// The class of the engine's own server object for `record`
/// ([`engine_class`] of its class id and kind).
pub fn engine_class_of(record: &ObjectRecord) -> &'static str {
    engine_class(record.class_id.as_str(), record.kind())
}

/// The class of the engine's own server object of class id `id`, of kind
/// `kind` (our reading where the spec names none: the class whose bases
/// hold its kind's blocks, sav.md 7.4.2; artefacts as the scripted
/// artefact's base, 6).
pub fn engine_class(id: &str, kind: Kind) -> &'static str {
    use Kind::*;
    match kind {
        Player => "cse_alife_creature_actor",
        Stalker => "cse_alife_human_stalker",
        Trader => "cse_alife_trader",
        Monster if id == "AI_ZOM" => "cse_alife_monster_zombie",
        Monster => "cse_alife_monster_base",
        CrowOrPhantom if id == "AI_PHANT" => "cse_alife_creature_phantom",
        CrowOrPhantom => "cse_alife_creature_crow",
        Vehicle => "cse_alife_car",
        Helicopter => "cse_alife_helicopter",
        Item if id.starts_with("AF_") || id == "ARTEFACT" || id == "SCRPTART" => {
            "cse_alife_item_artefact"
        }
        Item => "cse_alife_item",
        Torch => "cse_alife_item_torch",
        Pda => "cse_alife_item_pda",
        Document => "cse_alife_item_document",
        Outfit => "cse_alife_item_custom_outfit",
        Ammunition => "cse_alife_item_ammo",
        Knife => "cse_alife_item_weapon",
        MagazineWeapon => "cse_alife_item_weapon_magazined",
        WeaponWithGrenadeLauncher => "cse_alife_item_weapon_magazined_w_gl",
        Shotgun => "cse_alife_item_weapon_shotgun",
        Zone => "cse_custom_zone",
        Anomaly => "cse_anomalous_zone",
        VisualAnomaly => "cse_zone_visual",
        TorridZone => "cse_torrid_zone",
        LevelChanger => "cse_alife_level_changer",
        Restrictor => "cse_alife_space_restrictor",
        SmartZone => "cse_alife_smart_zone",
        TeamBase => "cse_alife_team_base_zone",
        PlainVisual => DOV,
        StationaryGun => "cse_alife_mounted_weapon",
        HangingLamp => "cse_alife_object_hanging_lamp",
        PhysicsObject => "cse_alife_object_physic",
        Breakable => "cse_alife_object_breakable",
        Climbable => "cse_alife_object_climable",
        SkeletonObject => "cse_alife_ph_skeleton_object",
        OnOffGroup => "cse_alife_online_offline_group",
        MonsterGroup => "cse_alife_monster_base",
    }
}

/// The engine classes of this spec, with spec K's it needs first.
pub(crate) fn classes(ctx: &Context) -> Vec<EngineClass> {
    // Spec K's registration makes them when it is registered.
    let mut out: Vec<EngineClass> = if ctx.alife.is_some() {
        Vec::new()
    } else {
        SPEC_K_CLASSES.iter().map(|n| EngineClass::new(n)).collect()
    };
    for class in &CLASSES {
        let mut c = EngineClass::new(class.name);
        if let Some(base) = class.bases.first() {
            c = c.base(base);
        }
        if class.constructible {
            let made = ctx.clone();
            let name = class.name;
            c = c
                .constructor(
                    1,
                    |args| matches!(args.first(), Some(Value::String(_))).then_some(0),
                    move |part, args| {
                        let Some(Value::String(section)) = args.first() else {
                            // Our wording (2.2 has the one string).
                            return Err(format!("{name}: a section name expected"));
                        };
                        let system = Rc::clone(&made.files.borrow().system);
                        Ok(SimulationObject::new(
                            Some(part),
                            name,
                            section.clone(),
                            &system,
                        ))
                    },
                )
                .constructible();
        }
        for (member, owner) in members(class) {
            c = add_member(c, ctx, owner, member);
        }
        out.push(c);
    }
    out
}

// --- Members ------------------------------------------------------------

type Result<T> = std::result::Result<T, String>;

fn number(n: impl Into<f64>) -> Vec<Value> {
    vec![Value::Number(n.into())]
}

fn boolean(b: bool) -> Vec<Value> {
    vec![Value::Boolean(b)]
}

/// Lua's number conversion to an integer (rounded as `luaconf.h`
/// converts).
fn integer(v: &Value) -> i64 {
    lua_number(v).round_ties_even() as i64
}

/// A getter (no arguments) and setter (one) overload of a state flag
/// (3.3): `get` reads the flags, `set` changes them for the argument's
/// truth.
fn flag_method(
    c: EngineClass,
    name: &str,
    get: fn(&ObjectRecord) -> Result<bool>,
    set: fn(&mut u32, bool),
) -> EngineClass {
    c.overridable(name, move |o: &mut SimulationObject, args| {
        match args.first() {
            None => Ok(boolean(get(&*o.state()?)?)),
            Some(b) => {
                let mut record = o.state_mut()?;
                set(&mut record.object.object_block_mut().state_flags, truth(b));
                Ok(Vec::new())
            }
        }
    })
}

fn flags(r: &ObjectRecord) -> u32 {
    r.object.object_block().state_flags
}

fn set_bit(bit: u32) -> fn(&mut u32, bool) {
    match bit {
        0x02 => |f, b| set_flag(f, 0x02, b),
        0x04 => |f, b| set_flag(f, 0x04, b),
        _ => |f, b| set_flag(f, 0x400, b),
    }
}

fn set_flag(flags: &mut u32, bit: u32, on: bool) {
    if on {
        *flags |= bit;
    } else {
        *flags &= !bit;
    }
}

/// The engine's predicates *on* and *off* (offline 6.5.2) with the first
/// renderer in use.
fn predicate(r: &ObjectRecord, online: bool) -> Result<bool> {
    let candidate = Candidate::of(r);
    let m = candidate.renderer_match().map_err(|e| e.to_string())?;
    Ok(if online {
        candidate.engine_on(m)
    } else {
        candidate.engine_off(m)
    })
}

fn creature(r: &ObjectRecord) -> Result<&volya_engine_formats::object::Creature> {
    r.object
        .creature()
        .ok_or_else(|| "the simulation object has no Creature block".to_owned())
}

fn creature_mut(r: &mut ObjectRecord) -> Result<&mut volya_engine_formats::object::Creature> {
    r.object
        .creature_mut()
        .ok_or_else(|| "the simulation object has no Creature block".to_owned())
}

fn monster_like(r: &ObjectRecord) -> Result<&volya_engine_formats::object::MonsterLike> {
    r.object
        .monster_like()
        .ok_or_else(|| "the simulation object has no Monster-like block".to_owned())
}

fn monster_like_mut(
    r: &mut ObjectRecord,
) -> Result<&mut volya_engine_formats::object::MonsterLike> {
    r.object
        .monster_like_mut()
        .ok_or_else(|| "the simulation object has no Monster-like block".to_owned())
}

fn trader_like(r: &ObjectRecord) -> Result<&TraderLike> {
    match &r.object {
        Object::Player(p) => Ok(&p.trader),
        Object::Stalker(s) => Ok(&s.trader),
        Object::Trader(t) => Ok(&t.trader),
        _ => Err("the simulation object has no Trader-like block".to_owned()),
    }
}

fn trader_like_mut(r: &mut ObjectRecord) -> Result<&mut TraderLike> {
    match &mut r.object {
        Object::Player(p) => Ok(&mut p.trader),
        Object::Stalker(s) => Ok(&mut s.trader),
        Object::Trader(t) => Ok(&mut t.trader),
        _ => Err("the simulation object has no Trader-like block".to_owned()),
    }
}

/// A byte property of the Creature block (3.1): the number taken as an
/// 8-bit unsigned value (believed; the conversion is our choice).
fn creature_byte(
    c: EngineClass,
    name: &str,
    field: fn(&mut volya_engine_formats::object::Creature) -> &mut u8,
) -> EngineClass {
    c.property_with_objects(
        name,
        move |o: &SimulationObject, _| {
            let mut copy = creature(&*o.state()?)?.clone();
            Ok(Value::Number(f64::from(*field(&mut copy))))
        },
        Some(Box::new(move |o: &mut SimulationObject, _, v: &Value| {
            *field(creature_mut(&mut *o.state_mut()?)?) = integer(v) as u8;
            Ok(())
        })),
    )
}

/// A read-only property of the state.
fn state_property(c: EngineClass, name: &str, get: fn(&ObjectRecord) -> f64) -> EngineClass {
    c.property_with_objects(
        name,
        move |o: &SimulationObject, _| Ok(Value::Number(get(&*o.state()?))),
        None,
    )
}

/// The pure-virtual default of `cse_abstract` (3.2): a log line naming the
/// method (what follows its `in` is not stated: our line ends with the
/// name).
fn pure_virtual(
    ctx: &Context,
    name: &'static str,
) -> impl Fn(&mut SimulationObject, &[Value]) -> Result<Vec<Value>> + use<> {
    let services = Rc::clone(&ctx.services);
    move |_, _| {
        services
            .borrow_mut()
            .console
            .log_mut()
            .message(&format!("Attempt to call pure virtual method {name}"));
        Ok(Vec::new())
    }
}

/// The community list's names in position order (identity 1.3).
fn community_names(system: &volya_engine_core::config::Config) -> Vec<String> {
    system
        .get("game_relations", "communities")
        .flatten()
        .map(|list| {
            list.split(',')
                .map(str::trim)
                .step_by(2)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the engine's `on_spawn` of `owner` is the creatures' step
/// (5.4).
fn creature_spawn_step(owner: &str) -> bool {
    matches!(
        owner,
        "cse_alife_creature_abstract"
            | "cse_alife_creature_crow"
            | "cse_alife_creature_phantom"
            | "cse_alife_creature_actor"
            | "cse_alife_monster_abstract"
            | "cse_alife_monster_zombie"
            | "cse_alife_monster_base"
            | "cse_alife_psydog_phantom"
    )
}

/// Member `name` as class `owner` registers it.
fn add_member(
    c: EngineClass,
    ctx: &Context,
    owner: &'static str,
    name: &'static str,
) -> EngineClass {
    match name {
        // 3.1, cse_abstract.
        "id" => state_property(c, name, |r| f64::from(r.header.id)),
        "parent_id" => state_property(c, name, |r| f64::from(r.header.parent_id)),
        "script_version" => state_property(c, name, |r| f64::from(r.header.script_version)),
        "position" => c.property_with_objects(
            name,
            |o: &SimulationObject, objects: &Objects| {
                let [x, y, z] = o.state()?.header.position;
                Ok(objects.new_object("vector", Vector::new(x, y, z)))
            },
            Some(Box::new(
                |o: &mut SimulationObject, objects: &Objects, v: &Value| {
                    match objects.get::<Vector>(v) {
                        Some(Operand::Other(p)) => {
                            o.state_mut()?.header.position = [p.x, p.y, p.z];
                            Ok(())
                        }
                        // Our wording (depends on A).
                        _ => Err("position: a vector expected".into()),
                    }
                },
            )),
        ),
        // 3.2.
        "name" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(vec![Value::String(game_bytes(&o.state()?.header.name))])
        }),
        "section_name" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(vec![Value::String(o.section.clone())])
        }),
        "clsid" => {
            let factory = Rc::clone(&ctx.factory);
            c.overridable(name, move |o: &mut SimulationObject, _| {
                let mut f = factory.borrow_mut();
                let n = f.get_or_insert_with(ObjectFactory::new).number(o.class_id);
                Ok(number(n as f64))
            })
        }
        "spawn_ini" => {
            let ctx = ctx.clone();
            c.overridable_with_objects(name, move |o: &mut SimulationObject, objects, _| {
                ctx.stopped()?;
                if o.spawn_ini.is_none() {
                    let custom = game_bytes(&o.state()?.object.object_block().custom_data);
                    let loaded = config_from_text(&ctx.files.borrow(), &custom);
                    match loaded {
                        Ok(config) => o.spawn_ini = Some(Rc::new(config)),
                        Err(e) => {
                            return Err(report_fatal(
                                &ctx.services,
                                &ctx.fatal,
                                Fatal {
                                    function: "CInifile::Load",
                                    file: file!(),
                                    line: line!(),
                                    expression: "fatal error",
                                    description: "<no expression>",
                                    arguments: &[&e.to_string()],
                                },
                            ));
                        }
                    }
                }
                let config = o.spawn_ini.as_ref().map(Rc::clone).expect("just made");
                Ok(vec![objects.new_object("ini_file", IniFile(config))])
            })
        }
        "STATE_Read" => {
            // The engine's own reads the engine's state: the packet's read
            // position then where the scripted additions start (sav.md
            // 7.5), for the script part to read on.
            let read = |o: &mut SimulationObject, objects: &Objects, args: &[Value]| {
                if let Some(pending) = o.pending.take() {
                    let packet = args.first().unwrap_or(&Value::Nil);
                    if let Some(Operand::Other(mut p)) = objects.get::<NetPacket>(packet) {
                        p.read_position = pending.scripted_additions;
                        objects.set(packet, p);
                    }
                    if o.simulation.is_none() {
                        o.record = Some(pending.record);
                    }
                }
                Ok(Vec::new())
            };
            if owner == "cse_abstract" {
                c.method_with_objects_and_default(name, read, pure_virtual(ctx, "STATE_Read"))
            } else {
                c.overridable_with_objects(name, read)
            }
        }
        "STATE_Write" => {
            let write = |o: &mut SimulationObject, _: &[Value]| {
                o.ran.state_write = true;
                Ok(Vec::new())
            };
            if owner == "cse_abstract" {
                c.method_with_default(name, write, pure_virtual(ctx, "STATE_Write"))
            } else {
                c.overridable(name, write)
            }
        }
        // The update data is part of the state the engine holds; reading
        // and writing it through spec I's packets is TODO(#1246).
        "UPDATE_Read" | "UPDATE_Write" => c.method_with_default(
            name,
            |_: &mut SimulationObject, _| Ok(Vec::new()),
            pure_virtual(ctx, name),
        ),
        // 3.1, 3.3: cse_alife_object.
        "m_game_vertex_id" => state_property(c, name, |r| {
            f64::from(r.object.object_block().game_graph_vertex)
        }),
        "m_level_vertex_id" => state_property(c, name, |r| {
            f64::from(r.object.object_block().level_graph_node)
        }),
        "m_story_id" => state_property(c, name, |r| {
            f64::from(r.object.object_block().story_id as u32)
        }),
        "online" => c.property_with_objects(
            name,
            |o: &SimulationObject, _| Ok(Value::Boolean(o.online)),
            None,
        ),
        "can_switch_online" => flag_method(c, name, |r| predicate(r, true), set_bit(0x02)),
        "can_switch_offline" => flag_method(c, name, |r| predicate(r, false), set_bit(0x04)),
        "used_ai_locations" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(boolean(uses_level_graph(&*o.state()?)))
        }),
        "can_save" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(boolean(o.state()?.can_be_saved()))
        }),
        "interactive" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(boolean(flags(&*o.state()?) & 0x38 == 0x38))
        }),
        "visible_for_map" => flag_method(c, name, |r| Ok(flags(r) & 0x400 != 0), set_bit(0x400)),
        "move_offline" => flag_method(
            c,
            name,
            |r| Ok(flags(r) & 0x40 == 0),
            |f, b| set_flag(f, 0x40, !b),
        ),
        // 3.4, 4.2, 5.3 to 5.5.
        "keep_saved_data_anyway" => {
            c.overridable(name, |_: &mut SimulationObject, _| Ok(boolean(false)))
        }
        "on_spawn" => c.overridable(name, move |o: &mut SimulationObject, _| {
            if creature_spawn_step(owner) {
                // The death time 0 for a creature with health ≤ 0
                // (believed). TODO(#1115): the team from `species`, a
                // monster's item, the anomalies' artefacts (5.4, 3.9).
                if let Ok(mut r) = o.state_mut()
                    && let Ok(c) = creature_mut(&mut r)
                    && c.health <= 0.0
                {
                    c.death_time = 0;
                }
            }
            Ok(Vec::new())
        }),
        // TODO(#1115): `cse_alife_online_offline_group`'s step.
        "on_before_register" => c.overridable(name, |_: &mut SimulationObject, _| Ok(Vec::new())),
        "on_register" => c.overridable(name, |o: &mut SimulationObject, _| {
            o.ran.on_register = true;
            Ok(Vec::new())
        }),
        "on_unregister" => c.overridable(name, |o: &mut SimulationObject, _| {
            o.ran.on_unregister = true;
            Ok(Vec::new())
        }),
        // 3.1, 3.5: creatures.
        "team" => creature_byte(c, name, |c| &mut c.team),
        "squad" => creature_byte(c, name, |c| &mut c.squad),
        "group" => creature_byte(c, name, |c| &mut c.group),
        "alive" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(boolean(creature(&*o.state()?)?.health > 0.0))
        }),
        "health" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(number(creature(&*o.state()?)?.health))
        }),
        "g_team" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(number(creature(&*o.state()?)?.team))
        }),
        "g_squad" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(number(creature(&*o.state()?)?.squad))
        }),
        "g_group" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(number(creature(&*o.state()?)?.group))
        }),
        "on_death" => {
            let state = Rc::clone(&ctx.state);
            c.overridable(name, move |o: &mut SimulationObject, _| {
                let now = state.borrow().game_time;
                let mut r = o.state_mut()?;
                let c = creature_mut(&mut r)?;
                c.death_time = now;
                c.health = -1.0;
                Ok(Vec::new())
            })
        }
        // 3.1, 3.6: monsters.
        "m_smart_terrain_id" => c.property_with_objects(
            name,
            |o: &SimulationObject, _| {
                Ok(Value::Number(f64::from(
                    monster_like(&*o.state()?)?.smart_terrain_id,
                )))
            },
            Some(Box::new(|o: &mut SimulationObject, _, v: &Value| {
                monster_like_mut(&mut *o.state_mut()?)?.smart_terrain_id = integer(v) as u16;
                Ok(())
            })),
        ),
        "smart_terrain_id" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(number(monster_like(&*o.state()?)?.smart_terrain_id))
        }),
        "clear_smart_terrain" => c.overridable(name, |o: &mut SimulationObject, _| {
            monster_like_mut(&mut *o.state_mut()?)?.smart_terrain_id = 0xFFFF;
            Ok(Vec::new())
        }),
        "smart_terrain_task_activate" => c.overridable(name, |o: &mut SimulationObject, _| {
            monster_like_mut(&mut *o.state_mut()?)?.smart_terrain_task_reached = 1;
            Ok(Vec::new())
        }),
        "smart_terrain_task_deactivate" => c.overridable(name, |o: &mut SimulationObject, _| {
            monster_like_mut(&mut *o.state_mut()?)?.smart_terrain_task_reached = 0;
            Ok(Vec::new())
        }),
        "rank" if owner == "cse_alife_monster_abstract" => {
            c.overridable(name, |o: &mut SimulationObject, _| Ok(number(o.rank)))
        }
        // Spec K's brain of the object (its 12 keeps a brain's state by
        // object id): made at each call, as K's own are; a character's is
        // the character brain (our choice where 3.7 leaves the overload
        // unestablished, believed the more derived class's).
        "brain" => c.overridable_with_objects(name, |o: &mut SimulationObject, objects, _| {
            let class = if derives(o.class, "cse_alife_human_abstract") {
                "CALifeHumanBrain"
            } else {
                "CALifeMonsterBrain"
            };
            Ok(vec![objects.new_object(class, Brain { id: o.id() })])
        }),
        "update" if owner == "cse_alife_smart_zone" => {
            c.overridable(name, |_: &mut SimulationObject, _| Ok(Vec::new()))
        }
        "update" => c.overridable(name, |o: &mut SimulationObject, _| {
            o.ran.update = true;
            Ok(Vec::new())
        }),
        // 3.7: characters. TODO(#1116): the choice of a specific character
        // (identity 2) before `rank`, `reputation` and `set_rank`; the
        // values as the state holds them.
        "rank" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(number(trader_like(&*o.state()?)?.rank))
        }),
        "reputation" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(number(trader_like(&*o.state()?)?.reputation))
        }),
        "set_rank" => c.overridable(name, |o: &mut SimulationObject, args| {
            trader_like_mut(&mut *o.state_mut()?)?.rank =
                integer(args.first().unwrap_or(&Value::Nil)) as i32;
            Ok(Vec::new())
        }),
        "community" => {
            let ctx = ctx.clone();
            c.overridable(name, move |o: &mut SimulationObject, _| {
                let position = trader_like(&*o.state()?)?.community;
                let names = community_names(&ctx.files.borrow().system);
                match usize::try_from(position).ok().and_then(|p| names.get(p)) {
                    Some(n) => Ok(vec![Value::String(game_bytes(n))]),
                    None => Err(report_fatal(
                        &ctx.services,
                        &ctx.fatal,
                        Fatal {
                            function: "CSE_ALifeTraderAbstract::CommunityName",
                            file: file!(),
                            line: line!(),
                            expression: "fatal error",
                            description: "<no expression>",
                            arguments: &[
                                "item by index not found in section game_relations, line communities",
                            ],
                        },
                    )),
                }
            })
        }
        "profile_name" => c.overridable(name, |o: &mut SimulationObject, _| {
            Ok(vec![Value::String(game_bytes(
                &trader_like(&*o.state()?)?.character_profile,
            ))])
        }),
        // 3.8: smart zones.
        "detect_probability" | "suitable" => {
            c.overridable(name, |_: &mut SimulationObject, _| Ok(number(0.0)))
        }
        "smart_touch" | "register_npc" | "unregister_npc" => {
            c.overridable(name, |_: &mut SimulationObject, _| Ok(Vec::new()))
        }
        "enabled" => c.overridable(name, |_: &mut SimulationObject, _| Ok(boolean(false))),
        "task" => c.overridable(name, |_: &mut SimulationObject, _| Ok(vec![Value::Nil])),
        // 3.9: the rule is unestablished (TODO(#1115)): nothing is made.
        "spawn_artefacts" => c.overridable(name, |_: &mut SimulationObject, _| Ok(Vec::new())),
        // 3.10.
        "bfUseful" => c.overridable(name, |_: &mut SimulationObject, _| Ok(boolean(true))),
        // 3.11: no state of these classes holds a startup animation of the
        // visual part (sav.md 7.4.2), so nothing (our reading).
        "getStartupAnimation" => {
            c.overridable(name, |_: &mut SimulationObject, _| Ok(vec![Value::Nil]))
        }
        // 2.2, 5.2.
        "init" => c.object_method(name, |o: &mut SimulationObject, _| Ok(o.part)),
        "FillProps" => c.overridable(name, |_: &mut SimulationObject, _| Ok(Vec::new())),
        other => unreachable!("no member {other}"),
    }
}
