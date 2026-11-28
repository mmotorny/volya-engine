// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-simulation-objects.md on every target, one
//! check per section, over records and script classes of our own.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;
use volya_engine_formats::object::{
    ClassId, Creature, CreatureUpdate, FLAG_VERSION, Item, ItemUpdate, Monster, MonsterLike,
    MonsterLikeUpdate, MonsterState, Object, ObjectBlock, ObjectRecord, PlainItem, Restrictor,
    SCRIPT_VERSION, Skeleton, SmartZone, SmartZoneScript, SpawnHeader, Trader, TraderLike, VERSION,
    Visual,
};

use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::Value;
use crate::classes::EnginePart;
use crate::files::FileSystem;
use crate::game_object::{GameObjects, SpawnData};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::simulation_objects::{CLASSES, dispatched, engine_class_of};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("2.1 classes", class_list),
    ("2.2 constructing and init", constructing),
    ("3.1 properties", properties),
    ("3.2 cse_abstract", abstract_methods),
    ("3.3 state flags", state_flags),
    ("3.5 to 3.11 kinds", kinds),
    ("4.1 dispatch", dispatch),
    ("4.2, 5 engine calls", engine_calls),
    ("spawn data", spawn_data),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("simulation objects {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const SYSTEM: &str = "[flesh]\r\nclass = AI_FLESH\r\nrank = 7\r\n\
    [flesh_unranked]\r\nclass = AI_FLESH\r\n\
    [trader]\r\nclass = AI_TRADE\r\nrank = 9\r\n\
    [smart]\r\nclass = SMRTZONE\r\n\
    [medkit]\r\nclass = II_MEDKI\r\n\
    [switch]\r\nclass = O_SWITCH\r\n\
    [game_relations]\r\ncommunities = actor, 0, stalker, 5, trader, 15\r\n";

/// Script classes of our own over the engine's.
const OBJS: &str = r##"
calls = {}
function note(s) table.insert(calls, s) end
function taken() local c = table.concat(calls, ", ") calls = {} return c end
kept = {}
function make(c, section) local o = objs[c](section) table.insert(kept, o) return o end

class "flesh" (cse_alife_monster_base)
function flesh:__init(section) super(section) self.mine = 1 end
function flesh:can_switch_online() note("can_switch_online") return switch_online end
function flesh:on_register() cse_alife_monster_base.on_register(self) note("on_register") end
function flesh:STATE_Write(packet)
  note("STATE_Write")
  if write_base then cse_alife_monster_base.STATE_Write(self, packet) end
end
function flesh:STATE_Read(packet, size)
  cse_alife_monster_base.STATE_Read(self, packet, size)
  note("STATE_Read " .. packet:r_tell() .. " " .. size .. " " .. self:name())
end
function flesh:on_death(killer)
  cse_alife_monster_base.on_death(self, killer)
  note("on_death " .. tostring(killer and killer.id))
end
function flesh:g_team() return 300 end
function flesh:update() note("update") end
function flesh:UPDATE_Read(packet) note("never") end

class "item" (cse_alife_item)
function item:__init(section) super(section) end
function item:on_register() note("item on_register") end

class "weapon" (cse_alife_item_weapon)
function weapon:__init(section) super(section) end
function weapon:on_register() note("weapon on_register") end
function weapon:bfUseful() return false end

class "smart" (cse_alife_smart_zone)
function smart:__init(section) super(section) end
function smart:enabled(obj) return obj:name() == "flesh" end
function smart:suitable(obj) return 0.1 end
function smart:task(obj) return "task" end
function smart:update() note("smart update") cse_alife_smart_zone.update(self) end

class "other" (cse_alife_object)
function other:__init(section) super(section) end
function other:init() note("init") return made_instead end

class "trader" (cse_alife_trader)
function trader:__init(section) super(section) end
function trader:on_unregister() note("on_unregister") end
"##;

/// A test state.
struct State {
    s: Scripts,
    services: Rc<RefCell<EngineServices>>,
}

fn state() -> Result<State, String> {
    let tree = Tree(
        [
            ("gamedata/scripts/_g.script", ""),
            ("gamedata/scripts/objs.script", OBJS),
        ]
        .iter()
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect(),
    );
    let tree = Rc::new(tree);
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
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(|e| e.message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(objects, Rc::clone(&services), files)
        .map_err(|e| e.message)?;
    s.register_data_streams(Rc::clone(&services))
        .map_err(|e| e.message)?;
    s.register_simulation_objects().map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    s.load_namespace("objs").map_err(|e| e.message)?;
    Ok(State { s, services })
}

/// A new script object of `class` (objs.script) for `section`, kept.
fn make(t: &mut State, class: &str, section: &str) -> Result<EnginePart, String> {
    let results =
        t.s.exec(
            "=make",
            format!("return objs.make(\"{class}\", \"{section}\")").as_bytes(),
        )
        .map_err(|e| e.message)?;
    match results.as_slice() {
        [Value::Object(part)] => Ok(*part),
        other => Err(format!("{class}({section}): {other:?}")),
    }
}

fn taken(t: &mut State) -> Result<String, String> {
    let r =
        t.s.exec("=taken", b"return objs.taken()")
            .map_err(|e| e.message)?;
    Ok(r.first()
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

fn header(id: u16, section: &str, name: &str) -> SpawnHeader {
    SpawnHeader {
        section: section.into(),
        name: name.into(),
        game_type: 0,
        after_game_type: 254,
        position: [1.0, 2.0, 3.0],
        angles: [0.0; 3],
        respawn_time: 0,
        id,
        parent_id: 0xFFFF,
        phantom_id: 0xFFFF,
        flags: FLAG_VERSION,
        version: VERSION,
        script_version: SCRIPT_VERSION,
        client_data: Vec::new(),
        spawn_id: 0,
    }
}

fn object_block(flags: u32) -> ObjectBlock {
    ObjectBlock {
        game_graph_vertex: 12,
        distance: 0.0,
        direct_control: 1,
        level_graph_node: 3_000_000_000,
        state_flags: flags,
        custom_data: "[logic]\r\ncfg = a.ltx\r\n".into(),
        story_id: -1,
        spawn_story_id: -1,
    }
}

fn visual() -> Visual {
    Visual {
        model: "m".into(),
        flags: 0,
    }
}

fn flesh_record(id: u16, health: f32) -> Result<ObjectRecord, String> {
    let creature = Creature {
        object: object_block(0x3E),
        visual: visual(),
        team: 1,
        squad: 2,
        group: 3,
        health,
        dynamic_out_restrictions: Vec::new(),
        dynamic_in_restrictions: Vec::new(),
        killer_id: 0xFFFF,
        death_time: 5,
    };
    Ok(ObjectRecord {
        header: header(id, "flesh", "flesh"),
        class_id: ClassId::new("AI_FLESH").map_err(|e| e.to_string())?,
        single_player: true,
        object: Object::Monster(Box::new(Monster {
            state: MonsterState {
                monster: MonsterLike {
                    creature,
                    out_restrictions: "".into(),
                    in_restrictions: "".into(),
                    smart_terrain_id: 40,
                    smart_terrain_task_reached: 0,
                },
                skeleton: Skeleton {
                    startup_animation: "".into(),
                    flags: 0,
                    source_id: 0xFFFF,
                    bone_state: None,
                },
                special_object_id: 0xFFFF,
            },
            script: None,
            update: MonsterLikeUpdate {
                creature: CreatureUpdate {
                    health,
                    timestamp: 0,
                    flags: 0,
                    position: [1.0, 2.0, 3.0],
                    model_yaw: 0.0,
                    yaw: 0.0,
                    pitch: 0.0,
                    roll: 0.0,
                    team: 1,
                    squad: 2,
                    group: 3,
                },
                next_game_vertex: 0xFFFF,
                previous_game_vertex: 0xFFFF,
                distance_from_previous: 0.0,
                distance_to_next: 0.0,
            },
        })),
    })
}

fn trader_record(id: u16) -> Result<ObjectRecord, String> {
    Ok(ObjectRecord {
        header: header(id, "trader", "trader"),
        class_id: ClassId::new("AI_TRADE").map_err(|e| e.to_string())?,
        single_player: true,
        object: Object::Trader(Box::new(Trader {
            object: object_block(0x206),
            visual: visual(),
            trader: TraderLike {
                money: 5000,
                specific_character: "esc_trader".into(),
                trader_flags: 0,
                character_profile: "sim_trader".into(),
                community: 2,
                rank: 300,
                reputation: -4,
                character_name: "".into(),
            },
        })),
    })
}

fn smart_record(id: u16) -> Result<ObjectRecord, String> {
    Ok(ObjectRecord {
        header: header(id, "smart", "smart"),
        class_id: ClassId::new("SMRTZONE").map_err(|e| e.to_string())?,
        single_player: true,
        object: Object::SmartZone(SmartZone {
            restrictor: Restrictor {
                object: object_block(0),
                shapes: Vec::new(),
                restrictor_type: 0,
            },
            script: SmartZoneScript::None,
        }),
    })
}

fn medkit_record(id: u16) -> Result<ObjectRecord, String> {
    Ok(ObjectRecord {
        header: header(id, "medkit", "medkit"),
        class_id: ClassId::new("II_MEDKI").map_err(|e| e.to_string())?,
        single_player: true,
        object: Object::Item(PlainItem {
            item: Item {
                object: object_block(0x206),
                visual: visual(),
                condition: 1.0,
            },
            update: ItemUpdate {
                flags: 0,
                physics: None,
            },
        }),
    })
}

/// A script object of `class` for `section` with `record` as its state.
fn with_state(
    t: &mut State,
    class: &str,
    section: &str,
    record: ObjectRecord,
) -> Result<EnginePart, String> {
    let part = make(t, class, section)?;
    t.s.set_simulation_state(part, record)
        .map_err(|e| e.message)?;
    Ok(part)
}

/// Makes `name` a global holding the object of `part`.
fn global(t: &mut State, name: &str, part: EnginePart) -> Result<(), String> {
    t.s.lua_mut()
        .register_object_function("__volya_test_object", move |_| Ok(Some(part)))
        .map_err(|e| e.message)?;
    t.s.exec(
        "=global",
        format!("{name} = __volya_test_object()").as_bytes(),
    )
    .map_err(|e| e.message)?;
    Ok(())
}

/// 2.1.
pub fn class_list() -> Result<(), String> {
    let mut t = state()?;
    let mut code = String::new();
    for c in &CLASSES {
        code.push_str(&format!(
            "eq(type({0}), \"userdata\", \"{0} is a class\")\n",
            c.name
        ));
        if c.constructible {
            code.push_str(&format!(
                "eq(type({0}(\"smart\")), \"userdata\", \"{0} constructs\")\n",
                c.name
            ));
        } else {
            code.push_str(&format!(
                "eq(pcall({0}, \"smart\"), false, \"{0} has no constructor\")\n",
                c.name
            ));
        }
    }
    for k in [
        "cpure_server_object",
        "ipure_schedulable_object",
        "CALifeMonsterBrain",
    ] {
        code.push_str(&format!("eq(type({k}), \"userdata\", \"spec K's {k}\")\n"));
    }
    // A second base's members reach a class.
    code.push_str(
        r#"
local s = cse_alife_trader("trader")
eq(type(s.community), "function", "the trader's second base: community")
eq(type(s.on_register), "function", "the trader's first base: on_register")
local z = cse_alife_smart_zone("smart")
eq(type(z.update), "function", "the schedulable smart zone: update")
eq(type(cse_alife_online_offline_group("smart").on_register), "function", "a group")
eq(cse_alife_item("smart").bfUseful, nil, "cse_alife_item has no bfUseful")
eq(type(cse_alife_item_bolt("smart").bfUseful), "function", "the bolt's bfUseful")
eq(cse_anomalous_zone("smart").update, nil, "no update on the anomalous zone")
eq(cse_alife_object("smart").OnEvent, nil, "OnEvent is not registered (3.12)")
"#,
    );
    run(&mut t.s, "2.1", &code)
}

/// 2.2.
pub fn constructing() -> Result<(), String> {
    let mut t = state()?;
    let flesh = make(&mut t, "flesh", "flesh")?;
    let fields =
        t.s.with_simulation_object(flesh, |o| (o.class, o.section.clone(), o.rank, o.class_id));
    ensure(
        fields
            == Some((
                "cse_alife_monster_base",
                b"flesh".to_vec(),
                7,
                class_id(b"AI_FLESH"),
            )),
        || format!("a flesh's engine part: {fields:?}"),
    )?;
    let unranked = make(&mut t, "flesh", "flesh_unranked")?;
    let rank = t.s.with_simulation_object(unranked, |o| o.rank);
    ensure(rank == Some(0), || {
        format!("rank without the key: {rank:?}")
    })?;
    let trader = make(&mut t, "trader", "trader")?;
    let rank = t.s.with_simulation_object(trader, |o| o.rank);
    ensure(rank == Some(0), || {
        format!("a trader's section rank is not read: {rank:?}")
    })?;
    // init: the object itself; an override's result.
    let used =
        t.s.initialize_server_object(flesh)
            .map_err(|e| e.to_string())?;
    ensure(used == flesh, || format!("init: {used:?}"))?;
    let other = make(&mut t, "other", "smart")?;
    let instead = make(&mut t, "trader", "trader")?;
    global(&mut t, "made_instead", instead)?;
    let used =
        t.s.initialize_server_object(other)
            .map_err(|e| e.to_string())?;
    ensure(used == instead, || {
        format!("init's override result: {used:?}")
    })?;
    ensure(taken(&mut t)? == "init", || "the override ran".into())?;
    t.s.exec("=nil", b"made_instead = nil")
        .map_err(|e| e.message)?;
    let result = t.s.initialize_server_object(other);
    ensure(result.is_err(), || format!("a missing result: {result:?}"))
}

/// 3.1.
pub fn properties() -> Result<(), String> {
    let mut t = state()?;
    let flesh = with_state(&mut t, "flesh", "flesh", flesh_record(7, 0.5)?)?;
    global(&mut t, "f", flesh)?;
    run(
        &mut t.s,
        "3.1",
        r#"
f = objs.kept[1]
eq(f.id, 7, "id")
eq(f.parent_id, 65535, "parent_id")
eq(f.script_version, 7, "script_version")
local p = f.position
eq(p.x + p.y + p.z, 6, "position")
p.x = 10
eq(f.position.x, 1, "position is a copy")
f.position = vector():set(4, 5, 6)
eq(f.position.z, 6, "position is written")
eq(f.m_game_vertex_id, 12, "m_game_vertex_id")
eq(f.m_level_vertex_id, 3000000000, "m_level_vertex_id, unsigned")
eq(f.m_story_id, 4294967295, "m_story_id of -1")
eq(f.online, false, "online")
eq(f.team .. f.squad .. f.group, "123", "team, squad, group")
f.team = 260
eq(f.team, 4, "team taken as a byte")
eq(f.m_smart_terrain_id, 40, "m_smart_terrain_id")
f.m_smart_terrain_id = 65535
eq(f:smart_terrain_id(), 65535, "m_smart_terrain_id is written")
f.id = 3
eq(f.id, 3, "a read-only property assigned is a field of the script object")
"#,
    )?;
    let id = t.s.with_simulation_object(flesh, |o| o.id());
    ensure(id == Some(7), || format!("the engine's id stays: {id:?}"))
}

/// 3.2.
pub fn abstract_methods() -> Result<(), String> {
    let mut t = state()?;
    let flesh = make(&mut t, "flesh", "flesh")?;
    run(
        &mut t.s,
        "3.2 before",
        r#"
local f = objs.kept[1]
eq(f:section_name(), "flesh", "section_name")
eq(pcall(f.name, f), false, "the state isn't read yet")
"#,
    )?;
    t.s.read_state(flesh, flesh_record(7, 0.5)?)
        .map_err(|e| e.to_string())?;
    let spawn = flesh_record(7, 0.5)?
        .write_spawn_packet()
        .map_err(|e| e.to_string())?;
    // The engine's own leaves the packet where the scripted additions
    // start (none for this class id: its end); the data size counts itself.
    let size = spawn.bytes.len() - spawn.object_state + 2;
    let notes = taken(&mut t)?;
    let want = format!("STATE_Read {} {size} flesh", spawn.bytes.len());
    ensure(notes == want, || {
        format!("STATE_Read's override and arguments: {notes}, not {want}")
    })?;
    let clsid =
        t.s.object_factory()
            .number(class_id(b"AI_FLESH"))
            .to_string();
    run(
        &mut t.s,
        "3.2",
        r#"
local f = objs.kept[1]
eq(f:name(), "flesh", "name")
eq(f:clsid(), clsid_of_flesh, "clsid")
local ini = f:spawn_ini()
eq(ini:r_string("logic", "cfg"), "a.ltx", "spawn_ini")
eq(f:spawn_ini():section_exist("logic"), true, "spawn_ini again")
cse_abstract.UPDATE_Read(f, nil)
cse_alife_monster_base.UPDATE_Write(f, nil)
cse_abstract.STATE_Read(f, nil, 0)
"#
        .replace("clsid_of_flesh", &clsid)
        .as_str(),
    )?;
    let log = t.services.borrow().console.log().join("\n");
    for m in ["UPDATE_Read", "UPDATE_Write", "STATE_Read"] {
        let line = format!("Attempt to call pure virtual method {m}");
        ensure(log.contains(&line), || format!("the log lacks {line:?}"))?;
    }
    let size_arg = notes.split(' ').nth(2).unwrap_or_default().to_owned();
    ensure(size_arg != "0", || "the data size".into())
}

/// 3.3.
pub fn state_flags() -> Result<(), String> {
    let mut t = state()?;
    with_state(&mut t, "item", "medkit", medkit_record(5)?)?;
    run(
        &mut t.s,
        "3.3",
        r#"
local o = objs.kept[1]
eq(o:can_switch_online(), true, "flag 0x02 set")
eq(o:can_switch_offline(), true, "flag 0x04 set")
o:can_switch_online(false)
eq(o:can_switch_online(), false, "the setter clears 0x02")
o:can_switch_online(true)
eq(o:can_switch_online(), true, "the setter sets 0x02")
eq(o:interactive(), false, "0x08, 0x10, 0x20 not all set")
eq(o:can_save(), true, "0x200")
eq(o:used_ai_locations(), false, "0x80 clear")
eq(o:visible_for_map(), false, "0x400")
o:visible_for_map(true)
eq(o:visible_for_map(), true, "0x400 set")
eq(o:move_offline(), true, "0x40 clear")
o:move_offline(false)
eq(o:move_offline(), false, "false sets 0x40")
"#,
    )
}

/// 3.5 to 3.11.
pub fn kinds() -> Result<(), String> {
    let mut t = state()?;
    with_state(&mut t, "flesh", "flesh", flesh_record(7, 0.5)?)?;
    with_state(&mut t, "trader", "trader", trader_record(8)?)?;
    with_state(&mut t, "smart", "smart", smart_record(9)?)?;
    with_state(&mut t, "weapon", "medkit", medkit_record(10)?)?;
    run(
        &mut t.s,
        "3.5 to 3.11",
        r#"
local f, tr, z, w = objs.kept[1], objs.kept[2], objs.kept[3], objs.kept[4]
eq(f:alive(), true, "alive")
eq(f:health(), 0.5, "health")
eq(cse_alife_monster_base.g_team(f), 1, "g_team's default")
eq(f:g_squad() .. f:g_group(), "23", "g_squad, g_group")
eq(f:smart_terrain_id(), 40, "smart_terrain_id")
f:clear_smart_terrain()
eq(f:smart_terrain_id(), 65535, "clear_smart_terrain")
f:smart_terrain_task_activate()
f:smart_terrain_task_deactivate()
eq(f:rank(), 7, "a monster's rank is its section key")
eq(type(f:brain()), "userdata", "the brain")
eq(tr:rank(), 300, "a trader's rank")
eq(tr:reputation(), -4, "reputation")
eq(tr:community(), "trader", "community by position")
eq(tr:profile_name(), "sim_trader", "profile_name")
eq(cse_alife_smart_zone.detect_probability(z), 0, "detect_probability")
eq(cse_alife_smart_zone.enabled(z, f), false, "enabled")
eq(cse_alife_smart_zone.suitable(z, f), 0, "suitable")
eq(cse_alife_smart_zone.task(z, f), nil, "task")
eq(cse_alife_item_weapon.bfUseful(w), true, "bfUseful")
eq(CSE_AbstractVisual("smart"):getStartupAnimation(), nil, "getStartupAnimation")
"#,
    )?;
    // set_rank, and a community outside the list.
    run(
        &mut t.s,
        "3.7",
        r#"
local tr = objs.kept[2]
eq(tr.set_rank, nil, "set_rank is the characters' (the listing's)")
eq(type(cse_alife_human_stalker("trader").set_rank), "function", "a stalker's set_rank")
"#,
    )?;
    let trader = make(&mut t, "trader", "trader")?;
    let mut record = trader_record(11)?;
    if let Object::Trader(tr) = &mut record.object {
        tr.trader.community = 3;
    }
    t.s.set_simulation_state(trader, record)
        .map_err(|e| e.message)?;
    let r = t.s.exec("=c", b"return objs.kept[5]:community()");
    ensure(
        matches!(&r, Err(e) if e.message.contains("item by index not found")),
        || format!("a community outside the list: {r:?}"),
    )
}

/// 4.1.
pub fn dispatch() -> Result<(), String> {
    // The table.
    let sets = [
        ("cse_alife_item", "on_register", false),
        ("cse_alife_item", "STATE_Read", true),
        ("cse_alife_item_weapon", "on_register", true),
        ("cse_alife_item_weapon", "bfUseful", true),
        ("cse_alife_object", "can_save", true),
        ("cse_alife_object", "on_spawn", false),
        ("cse_alife_trader", "on_death", false),
        ("cse_alife_creature_actor", "on_death", true),
        ("cse_alife_human_stalker", "update", true),
        ("cse_anomalous_zone", "update", false),
        ("cse_alife_smart_zone", "task", true),
        ("cse_alife_smart_zone", "g_team", false),
        ("cse_alife_monster_base", "UPDATE_Read", false),
        ("cse_alife_monster_base", "spawn_ini", false),
        ("cse_abstract", "STATE_Read", false),
    ];
    for (class, method, want) in sets {
        ensure(dispatched(class, method) == want, || {
            format!("{class}.{method} dispatched: want {want}")
        })?;
    }
    let constructible = CLASSES.iter().filter(|c| c.constructible).count();
    ensure(constructible == 51, || {
        format!("{constructible} constructible")
    })?;
    let mut t = state()?;
    let item = with_state(&mut t, "item", "medkit", medkit_record(5)?)?;
    let weapon = with_state(&mut t, "weapon", "medkit", medkit_record(6)?)?;
    let flesh = with_state(&mut t, "flesh", "flesh", flesh_record(7, 0.5)?)?;
    let ran = t.s.on_register(item).map_err(|e| e.to_string())?;
    ensure(ran && taken(&mut t)?.is_empty(), || {
        "cse_alife_item: on_register isn't dispatched".into()
    })?;
    let ran = t.s.on_register(weapon).map_err(|e| e.to_string())?;
    ensure(!ran && taken(&mut t)? == "weapon on_register", || {
        "a weapon's override, without its base call".into()
    })?;
    let useful = t.s.useful(weapon).map_err(|e| e.to_string())?;
    ensure(!useful, || "bfUseful's override".into())?;
    let ran = t.s.on_register(flesh).map_err(|e| e.to_string())?;
    ensure(ran && taken(&mut t)? == "on_register", || {
        "the base call runs the engine's".into()
    })?;
    // The getter is dispatched; the setter and an object field are not.
    t.s.exec("=s", b"switch_online = 'yes'")
        .map_err(|e| e.message)?;
    let on = t.s.can_switch_online(flesh).map_err(|e| e.to_string())?;
    ensure(on && taken(&mut t)? == "can_switch_online", || {
        "can_switch_online's override, its result's truth".into()
    })?;
    t.s.exec("=s", b"switch_online = nil")
        .map_err(|e| e.message)?;
    let on = t.s.can_switch_online(flesh).map_err(|e| e.to_string())?;
    ensure(!on, || "nil is false".into())?;
    taken(&mut t)?;
    let off = t.s.can_switch_offline(flesh).map_err(|e| e.to_string())?;
    ensure(off, || {
        "can_switch_offline's default (0x04 set, alive)".into()
    })?;
    let team =
        t.s.creature_byte(flesh, "g_team")
            .map_err(|e| e.to_string())?;
    ensure(team == 44, || format!("g_team taken as a byte: {team}"))?;
    t.s.exec(
        "=field",
        b"objs.kept[3].can_save = function() objs.note('field') return false end",
    )
    .map_err(|e| e.message)?;
    let saved = t.s.can_save(flesh).map_err(|e| e.to_string())?;
    // The engine's own: flag 0x200 is clear.
    ensure(!saved && taken(&mut t)?.is_empty(), || {
        "an object field is never called".into()
    })
}

/// 4.2 and section 5.
pub fn engine_calls() -> Result<(), String> {
    let mut t = state()?;
    let flesh = make(&mut t, "flesh", "flesh")?;
    t.s.read_state(flesh, flesh_record(7, 0.5)?)
        .map_err(|e| e.to_string())?;
    taken(&mut t)?;
    // 5.1: STATE_Write's base call writes the engine's state.
    let written =
        t.s.write_state(flesh, Value::Nil)
            .map_err(|e| e.to_string())?;
    ensure(written.is_none(), || "no base call, no state".into())?;
    t.s.exec("=w", b"write_base = true")
        .map_err(|e| e.message)?;
    let written =
        t.s.write_state(flesh, Value::Nil)
            .map_err(|e| e.to_string())?;
    ensure(written.is_some_and(|r| r.header.id == 7), || {
        "the base call writes it".into()
    })?;
    taken(&mut t)?;
    // 5.7: the override replaces the engine's update; the smart zone's
    // engine update does nothing.
    let ran =
        t.s.scheduled_object_update(flesh)
            .map_err(|e| e.to_string())?;
    ensure(!ran && taken(&mut t)? == "update", || {
        "update's override".into()
    })?;
    let zone = with_state(&mut t, "smart", "smart", smart_record(9)?)?;
    let ran =
        t.s.scheduled_object_update(zone)
            .map_err(|e| e.to_string())?;
    ensure(!ran && taken(&mut t)? == "smart update", || {
        "the smart zone's update".into()
    })?;
    // 5.9: the smart terrain's questions.
    let enabled =
        t.s.smart_zone_enabled(zone, flesh)
            .map_err(|e| e.to_string())?;
    let suitable =
        t.s.smart_zone_suitable(zone, flesh)
            .map_err(|e| e.to_string())?;
    let task =
        t.s.smart_zone_task(zone, flesh)
            .map_err(|e| e.to_string())?;
    ensure(
        enabled && suitable == 0.1_f32 && task == Value::from("task"),
        || format!("enabled {enabled}, suitable {suitable}, task {task:?}"),
    )?;
    let p = t.s.detect_probability(zone).map_err(|e| e.to_string())?;
    ensure(p == 0.0, || format!("detect_probability {p}"))?;
    // 5.8: on_death.
    let killer = with_state(&mut t, "trader", "trader", trader_record(8)?)?;
    t.s.on_death(flesh, Some(killer), 123_456)
        .map_err(|e| e.to_string())?;
    let creature =
        t.s.with_simulation_object(flesh, |o| {
            o.record
                .as_ref()
                .and_then(|r| r.object.creature())
                .map(|c| (c.killer_id, c.death_time, c.health))
        })
        .flatten();
    ensure(creature == Some((8, 123_456, -1.0)), || {
        format!("the death: {creature:?}")
    })?;
    ensure(taken(&mut t)? == "on_death 8", || {
        "on_death's override".into()
    })?;
    // 5.4: a dead creature's death time.
    let dead = with_state(&mut t, "flesh", "flesh", flesh_record(12, 0.0)?)?;
    t.s.on_spawn(dead).map_err(|e| e.to_string())?;
    let time =
        t.s.with_simulation_object(dead, |o| {
            o.record
                .as_ref()
                .and_then(|r| r.object.creature())
                .map(|c| c.death_time)
        })
        .flatten();
    ensure(time == Some(0), || {
        format!("the death time at spawn: {time:?}")
    })?;
    // 5.5: on_unregister's override.
    t.s.on_unregister(killer).map_err(|e| e.to_string())?;
    ensure(taken(&mut t)? == "on_unregister", || "on_unregister".into())?;
    let keep =
        t.s.keep_saved_data_anyway(flesh)
            .map_err(|e| e.to_string())?;
    let interactive = t.s.interactive(flesh).map_err(|e| e.to_string())?;
    let uses = t.s.used_ai_locations(flesh).map_err(|e| e.to_string())?;
    ensure(!keep && interactive && uses, || {
        format!("keep {keep}, interactive {interactive}, used_ai_locations {uses}")
    })
}

/// M 10.2's and L 5.1's `data` (#1029): the simulation object.
pub fn spawn_data() -> Result<(), String> {
    let mut t = state()?;
    let results =
        t.s.exec(
            "=client",
            br#"
class "switch" (CGameObject)
function switch:__init() super() end
function switch:net_Spawn(data)
  spawned_from = data:name() .. " " .. data.id
  return CGameObject.net_Spawn(self, data)
end
return switch()
"#,
        )
        .map_err(|e| e.message)?;
    let [Value::Object(client)] = results.as_slice() else {
        return Err(format!("the client {results:?}"));
    };
    let server =
        t.s.new_simulation_object(engine_class_of(&medkit_record(20)?), b"medkit")
            .map_err(|e| e.message)?;
    t.s.set_simulation_state(server, medkit_record(20)?)
        .map_err(|e| e.message)?;
    let data = SpawnData {
        id: 20,
        name: b"medkit".to_vec(),
        section: b"switch".to_vec(),
        class_id: class_id(b"O_SWITCH"),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
        server: Some(server),
    };
    let spawned = t.s.spawn(*client, data).map_err(|e| e.to_string())?;
    let from =
        t.s.exec("=from", b"return spawned_from")
            .map_err(|e| e.message)?;
    ensure(
        spawned && from.first() == Some(&Value::from("medkit 20")),
        || format!("net_Spawn's data: {spawned} {from:?}"),
    )?;
    let class = t.s.with_simulation_object(server, |o| o.class);
    ensure(class == Some("cse_alife_item"), || {
        format!("the engine's class of a medkit: {class:?}")
    })
}
