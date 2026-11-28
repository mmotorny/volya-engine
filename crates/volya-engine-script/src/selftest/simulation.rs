// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-simulation-registry-and-game-graph.md on every
//! target, one check per section, over a small simulation of our own: two
//! levels of two game vertices each, a patrol path of three points, a
//! flesh carrying bread, a box with a story id and a space restrictor.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::info_portions::{Declarations, InfoPortions};
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::{AlifeConfig, NO_ID, Simulation};
use volya_engine_core::switching::SwitchDistances;
use volya_engine_formats::game_graph::{GameGraph, GraphEdge, GraphLevel, GraphVertex};
use volya_engine_formats::ltx;
use volya_engine_formats::object::{
    ClassId, Creature, CreatureUpdate, Item, ItemUpdate, Monster, MonsterLike, MonsterLikeUpdate,
    MonsterState, Object, ObjectBlock, ObjectRecord, PlainItem, PlainVisual, Restrictor, Skeleton,
    SpawnHeader, Visual,
};
use volya_engine_formats::patrol_path::{PathLink, PathPoint, PatrolPath, PatrolPaths};
use volya_engine_formats::xml;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::files::FileSystem;
use crate::game_object::GameObjects;
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::simulation::story_ids::{CLASSES, constants};
use crate::simulation::{AlifeSimulator, Event, ServerObjects, created_name};
use crate::story::Story;
use crate::{Error, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 alife", alife),
    ("3 finding objects", finding_objects),
    ("4 story ids", story_ids),
    ("5 levels", levels),
    ("6 switching", switching),
    ("7 creating and releasing", creating_and_releasing),
    ("8 restrictions", restrictions),
    ("9 kill_entity", kill_entity),
    ("10 the game graph", game_graph),
    ("11 smart-terrain tasks", tasks),
    ("12 brains and movement", brains),
    ("15 has_info", has_info),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| {
            f().err()
                .map(|e| format!("simulation registry {what}: {e}"))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const FLESH: u16 = 1;
const BREAD: u16 = 2;
const BOX: u16 = 3;
const RESTRICTOR: u16 = 4;

const GAME: &str = "[story_ids]\r\n007 = \"esc_box\"\r\n3 = flesh_story\r\n65535 = \"Invalid\"\r\n\
[spawn_story_ids]\r\n";

const SYSTEM: &str = "[flesh]\r\nclass = AI_FLESH\r\ngoing_speed = 2\r\n\
current_level_going_speed = 1\r\nsmart_terrain_choose_interval = 00:15:00\r\n\
terrain = 255,255,255,255\r\n[bread]\r\nclass = II_FOOD\r\n[box]\r\nclass = O_INVBOX\r\n\
[ammo_9x18_fmj]\r\nclass = AMMO\r\n[restrictor]\r\nclass = SPC_RS_S\r\n";

const PORTIONS: &str =
    r#"<game_information_portions><info_portion id="known"/></game_information_portions>"#;

fn vertex(level_id: u8, at: [f32; 3], level_vertex: u32) -> GraphVertex {
    GraphVertex {
        level_point: at,
        global_point: [at[0] + f32::from(level_id) * 1000.0, at[1], at[2]],
        level_id,
        level_vertex,
        types: [1, 0, 0, 0],
        first_edge: 0,
        edge_count: 0,
        first_point: 0,
        point_count: 0,
    }
}

/// Level `l01` (id 1): vertices 0 (0, 0, 0) and 1 (10, 0, 0); level `l02`
/// (id 2): vertices 2 and 3. Edges 0 ↔ 1 (10 m), 2 → 3.
fn graph() -> GameGraph {
    let mut vertices = vec![
        vertex(1, [0.0; 3], 5),
        vertex(1, [10.0, 0.0, 0.0], 6),
        vertex(2, [0.0; 3], 7),
        vertex(2, [5.0, 0.0, 0.0], 8),
    ];
    let edges = [(0u16, 1u16, 10.0f32), (1, 0, 10.0), (2, 3, 5.0)];
    let mut list = Vec::new();
    for (i, v) in vertices.iter_mut().enumerate() {
        v.first_edge = list.len() as u32;
        for &(_, to, d) in edges.iter().filter(|e| usize::from(e.0) == i) {
            list.push(GraphEdge {
                target: to,
                distance: d,
            });
        }
        v.edge_count = (list.len() as u32 - v.first_edge) as u8;
    }
    let level = |name: &str, id| GraphLevel {
        name: name.into(),
        offset: [0.0; 3],
        id,
        section: format!("level{id:02}"),
        identifier: [id; 16],
    };
    GameGraph {
        version: 8,
        identifier: [9; 16],
        levels: vec![level("l01", 1), level("l02", 2)],
        vertices,
        edges: list,
        point_count: 0,
        points_start: 0,
        trailing: Vec::new(),
    }
}

/// `walk`: points p0 (vertex 0), p1 (vertex 1), p2 (vertex 3), linked
/// 0 → 1 → 2 → 1.
fn paths() -> PatrolPaths {
    let point = |name: &str, gv: u16, lv: u32, x: f32| PathPoint {
        name: name.into(),
        position: [x, 0.0, 0.0],
        flags: 0,
        level_vertex: lv,
        game_vertex: gv,
    };
    let link = |from, to| PathLink {
        from,
        to,
        weight: 1.0,
    };
    PatrolPaths::from_paths(vec![PatrolPath {
        name: "walk".into(),
        points: vec![
            point("p0", 0, 5, 0.0),
            point("p1", 1, 6, 10.0),
            point("p2", 3, 8, 5.0),
        ],
        links: vec![link(0, 1), link(1, 2), link(2, 1)],
    }])
}

fn block(gv: u16, lv: u32, story_id: i32) -> ObjectBlock {
    ObjectBlock {
        game_graph_vertex: gv,
        distance: 0.0,
        direct_control: 1,
        level_graph_node: lv,
        state_flags: 0x07,
        custom_data: String::new().into(),
        story_id,
        spawn_story_id: -1,
    }
}

fn header(id: u16, section: &str, parent: u16) -> SpawnHeader {
    SpawnHeader {
        section: section.into(),
        name: format!("{section}_{id}").into(),
        game_type: 0,
        after_game_type: 0,
        position: [0.0; 3],
        angles: [0.0; 3],
        respawn_time: 0,
        id,
        parent_id: parent,
        phantom_id: NO_ID,
        flags: 0,
        version: 118,
        script_version: 7,
        client_data: Vec::new(),
        spawn_id: id,
    }
}

fn visual() -> Visual {
    Visual {
        model: String::new().into(),
        flags: 0,
    }
}

fn flesh() -> ObjectRecord {
    let update = CreatureUpdate {
        health: 1.0,
        timestamp: 0,
        flags: 0,
        position: [0.0; 3],
        model_yaw: 0.0,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        team: 0,
        squad: 0,
        group: 0,
    };
    let creature = Creature {
        object: block(0, 5, -1),
        visual: visual(),
        team: 0,
        squad: 0,
        group: 0,
        health: 1.0,
        dynamic_out_restrictions: Vec::new(),
        dynamic_in_restrictions: Vec::new(),
        killer_id: NO_ID,
        death_time: 0,
    };
    ObjectRecord {
        header: header(FLESH, "flesh", NO_ID),
        class_id: ClassId::new("AI_FLESH").unwrap(),
        single_player: true,
        object: Object::Monster(Box::new(Monster {
            state: MonsterState {
                monster: MonsterLike {
                    creature,
                    out_restrictions: String::new().into(),
                    in_restrictions: String::new().into(),
                    smart_terrain_id: NO_ID,
                    smart_terrain_task_reached: 0,
                },
                skeleton: Skeleton {
                    startup_animation: String::new().into(),
                    flags: 0,
                    source_id: NO_ID,
                    bone_state: None,
                },
                special_object_id: NO_ID,
            },
            script: None,
            update: MonsterLikeUpdate {
                creature: update,
                next_game_vertex: NO_ID,
                previous_game_vertex: NO_ID,
                distance_from_previous: 0.0,
                distance_to_next: 0.0,
            },
        })),
    }
}

fn bread(id: u16, parent: u16) -> ObjectRecord {
    ObjectRecord {
        header: header(id, "bread", parent),
        class_id: ClassId::new("II_FOOD").unwrap(),
        single_player: true,
        object: Object::Item(PlainItem {
            item: Item {
                object: block(0, 5, -1),
                visual: visual(),
                condition: 1.0,
            },
            update: ItemUpdate {
                flags: 0,
                physics: None,
            },
        }),
    }
}

fn inventory_box() -> ObjectRecord {
    ObjectRecord {
        header: header(BOX, "box", NO_ID),
        class_id: ClassId::new("O_INVBOX").unwrap(),
        single_player: true,
        object: Object::PlainVisual(PlainVisual {
            object: block(1, 6, 7),
            visual: visual(),
        }),
    }
}

fn restrictor() -> ObjectRecord {
    ObjectRecord {
        header: header(RESTRICTOR, "restrictor", NO_ID),
        class_id: ClassId::new("SPC_RS_S").unwrap(),
        single_player: true,
        object: Object::Restrictor(Restrictor {
            object: block(0, 5, -1),
            shapes: Vec::new(),
            restrictor_type: 0,
        }),
    }
}

/// The engine's server objects of the test: bread of section `bread`, the
/// box of spawn record 3.
struct Objects;

impl ServerObjects for Objects {
    fn of_section(&mut self, section: &str) -> io::Result<Option<ObjectRecord>> {
        Ok((section == "bread").then(|| bread(NO_ID, NO_ID)))
    }

    fn of_spawn_record(&mut self, spawn_id: u16) -> io::Result<Option<ObjectRecord>> {
        Ok((spawn_id == 3).then(|| {
            let mut r = inventory_box();
            r.object.object_block_mut().story_id = 8;
            r
        }))
    }
}

/// A test state.
pub(super) struct State {
    pub(super) s: Scripts,
    pub(super) alife: Rc<RefCell<AlifeSimulator>>,
    services: Rc<RefCell<EngineServices>>,
    story: Rc<RefCell<Story>>,
}

fn message(e: Error) -> String {
    e.message
}

/// A state with the simulation registry registered and `_G` loaded; the
/// simulation runs when `running`.
pub(super) fn state(running: bool) -> Result<State, String> {
    state_with(GAME, running)
}

fn state_with(game: &str, running: bool) -> Result<State, String> {
    let tree = Rc::new(Tree(
        [("gamedata/scripts/_g.script".to_owned(), Vec::new())]
            .into_iter()
            .collect(),
    ));
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let game = ltx::parse(game.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(game),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(message)?;
    let story = Rc::new(RefCell::new(Story::new()));
    s.register_story(Rc::clone(&story)).map_err(message)?;
    let alife = Rc::new(RefCell::new(AlifeSimulator::new()));
    s.register_simulation(Rc::clone(&alife)).map_err(message)?;
    s.register_simulation_objects().map_err(message)?;
    s.load_globals().map_err(message)?;
    let parse = |t: &str| xml::parse(t.as_bytes()).map_err(|e| e.to_string());
    let declarations = Declarations::from_documents(
        &[("portions".into(), parse(PORTIONS)?)],
        &[("articles".into(), parse("<encyclopedia/>")?)],
        &[("dialogs".into(), parse("<game_dialogs/>")?)],
        &parse("<game_tasks/>")?,
    )
    .map_err(|e| e.to_string())?;
    story.borrow_mut().info_portions = Some(Rc::new(RefCell::new(InfoPortions::new(
        Arc::new(declarations),
        0,
    ))));
    if running {
        let mut sim = Simulation::new(
            Graph::new(graph()),
            paths(),
            AlifeConfig {
                distances: SwitchDistances::of(150.0, 0.1),
                objects_per_update: 10,
                stay_after_death: 0,
            },
        );
        sim.load(
            vec![flesh(), bread(BREAD, FLESH), inventory_box(), restrictor()],
            &system,
        )
        .map_err(|e| e.to_string())?;
        let mut a = alife.borrow_mut();
        a.simulation = Some(sim);
        a.server_objects = Some(Box::new(Objects));
    }
    Ok(State {
        s,
        alife,
        services,
        story,
    })
}

fn lua(t: &mut State, name: &str, body: &str) -> Result<(), String> {
    run(&mut t.s, name, body)
}

/// Runs `code`, which must stop the state with a fatal error containing
/// `text`.
fn fatal(t: &mut State, code: &str, text: &str) -> Result<(), String> {
    match t.s.exec("=fatal", code.as_bytes()) {
        Err(e) if e.message.contains(text) => Ok(()),
        other => Err(format!("{code}: {other:?}, want a fatal error with {text}")),
    }
}

/// The log's lines that contain `text`.
fn logged(t: &State, text: &str) -> usize {
    let services = t.services.borrow();
    let log = services.console.log();
    log.iter().filter(|l| l.contains(text)).count()
}

/// 1.2: the classes, the globals and the casts that are not registered.
pub fn registered() -> Result<(), String> {
    let mut t = state(false)?;
    lua(
        &mut t,
        "registered",
        r#"
for _, c in ipairs({"alife_simulator", "story_ids", "spawn_story_ids", "CGameGraph",
    "GameGraph__CVertex", "CALifeSmartTerrainTask", "CALifeMonsterBrain", "CALifeHumanBrain",
    "CALifeMonsterMovementManager", "CALifeMonsterDetailPathManager",
    "CALifeMonsterPatrolPathManager", "ipure_alife_load_object", "ipure_alife_save_object",
    "ipure_alife_load_save_object", "ipure_server_object", "cpure_server_object",
    "ipure_schedulable_object"}) do
  eq(type(_G[c]) ~= "nil", true, c)
end
eq(type(alife), "function", "alife")
eq(type(game_graph), "function", "game_graph")
eq(cast_alife_object_to_creature, nil, "cast_alife_object_to_creature")
eq(cast_alife_object_to_trader_abstract, nil, "cast_alife_object_to_trader_abstract")
"#,
    )
}

/// 2: nil without a simulation, the one object with one.
pub fn alife() -> Result<(), String> {
    let mut t = state(false)?;
    lua(&mut t, "no simulation", r#"eq(alife(), nil, "alife()")"#)?;
    let mut t = state(true)?;
    lua(
        &mut t,
        "simulation",
        r#"eq(type(alife()), "userdata", "alife()") eq(rawequal(alife(), alife()), true, "one object")"#,
    )?;
    let sim = t.alife.borrow_mut().end();
    ensure(sim.is_some(), || "end".into())?;
    lua(&mut t, "ended", r#"eq(alife(), nil, "after the end")"#)
}

/// 3: `object`, `valid_object_id`, `actor`, `story_object`, `spawn_id`.
pub fn finding_objects() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "finding",
        r#"
local a = alife()
eq(type(a:object(1)), "userdata", "object(1)")
eq(type(a:object(1, true)), "userdata", "object(1, flag)")
eq(a:object(99), nil, "object(99)")
eq(type(a:object("box_3")), "userdata", "object(name)")
-- Spec J's object of its kind, over the simulation's state.
local b = a:object("box_3")
eq(b:name(), "box_3", "J's name()")
eq(b.id, a:object("box_3").id, "J's id")
eq(type(b.can_switch_online), "function", "J's cse_alife_object members")
eq(a:object("box"), nil, "object(other name)")
eq(a:valid_object_id(65535), false, "valid 65535")
eq(a:valid_object_id(12345), true, "valid 12345")
eq(a:actor(), nil, "actor before its registration")
eq(type(a:story_object(7)), "userdata", "story_object(7)")
eq(a:story_object(-1), nil, "story_object(-1)")
eq(a:story_object(3), nil, "story_object(3)")
eq(a:spawn_id(5), 65535, "spawn_id without the key")
"#,
    )?;
    t.alife.borrow_mut().spawn_story_ids.insert(5, 40);
    lua(
        &mut t,
        "spawn_id",
        r#"eq(alife():spawn_id(5), 40, "spawn_id(5)")"#,
    )
}

/// 4.1, 4.2: the constants and their checks; 4.3: the registry.
pub fn story_ids() -> Result<(), String> {
    let mut t = state(false)?;
    lua(
        &mut t,
        "constants",
        r#"
eq(story_ids.esc_box, 7, "quoted name, decimal key")
eq(story_ids.flesh_story, 3, "unquoted name")
eq(story_ids.Invalid, 65535, "Invalid")
eq(story_ids.INVALID_STORY_ID, -1, "INVALID_STORY_ID")
eq(spawn_story_ids.INVALID_SPAWN_STORY_ID, -1, "INVALID_SPAWN_STORY_ID")
eq(story_ids.nothing, nil, "a name the class lacks")
"#,
    )?;
    let check = |game: &str| {
        let game = ltx::parse(game.as_bytes()).map_err(|e| e.to_string())?;
        Ok::<_, String>(constants(&CLASSES[0], &game))
    };
    ensure(
        check("[other]\r\n")? == Err("cannot find section story_ids".into()),
        || "no section".into(),
    )?;
    ensure(
        check("[story_ids]\r\n1 = \"a b\"\r\n")?
            == Err("Invalid story id description (contains spaces)!".into()),
        || "spaces".into(),
    )?;
    ensure(
        check("[story_ids]\r\n1 = INVALID_STORY_ID\r\n")?
            == Err("INVALID_STORY_ID redifinition!".into()),
        || "reserved".into(),
    )?;
    ensure(
        check("[story_ids]\r\n1 = a\r\n2 = a\r\n")?
            == Err("Duplicated story id description!".into()),
        || "duplicate".into(),
    )?;
    ensure(
        check("[story_ids]\r\n1 = a\r\nx = b\r\n")?
            == Ok(vec![
                ("a".into(), 1),
                ("b".into(), 0),
                ("INVALID_STORY_ID".into(), -1),
            ]),
        || "a key without digits".into(),
    )?;
    ensure(state_with("[x]\r\n", false).is_err(), || {
        "a missing section fails the registration".into()
    })?;
    // 4.3: the story registry refuses a second object with a story id.
    let mut sim = Simulation::new(
        Graph::new(graph()),
        paths(),
        AlifeConfig {
            distances: SwitchDistances::of(150.0, 0.1),
            objects_per_update: 10,
            stay_after_death: 0,
        },
    );
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let mut second = inventory_box();
    second.header.id = 9;
    let e = sim.load(vec![inventory_box(), second], &system).err();
    ensure(
        e.is_some_and(|e| {
            e.to_string() == "Specified story object is already in the Story registry!"
        }),
        || "a story id registered twice".into(),
    )
}

/// 5: `level_id` needs the current level; `level_name` reads the graph.
pub fn levels() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "level_name",
        r#"eq(alife():level_name(2), "l02", "level_name(2)")"#,
    )?;
    fatal(
        &mut t,
        "alife():level_name(5)",
        "there is no specified level in the game graph : 5",
    )?;
    let mut t = state(true)?;
    fatal(&mut t, "alife():level_id()", "there is no current level")
}

/// 6: the switch distance and the state flags.
pub fn switching() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "switching",
        r#"
local a = alife()
eq(a:switch_distance(), 150, "switch_distance()")
a:switch_distance(200)
eq(a:switch_distance(), 200, "set")
a:set_switch_online(3, false)
a:set_switch_offline(3, false)
a:set_interactive(3, true)
"#,
    )?;
    let a = t.alife.borrow();
    let sim = a.simulation.as_ref().expect("running");
    let d = sim.config.distances;
    ensure(d == SwitchDistances::of(200.0, 0.1), || {
        format!("distances {d:?}")
    })?;
    let flags = sim
        .object(BOX)
        .unwrap()
        .record
        .object
        .object_block()
        .state_flags;
    ensure(flags == 0x09, || format!("state flags {flags:#x}"))
}

/// 7: `create` by section and by spawn record, `create_ammo`, `release`.
pub fn creating_and_releasing() -> Result<(), String> {
    ensure(created_name("wpn_ak74", 57) == "wpn_ak740057", || {
        "name".into()
    })?;
    ensure(created_name("x", 12345) == "x12345", || "a long id".into())?;
    let mut t = state(true)?;
    lua(
        &mut t,
        "create",
        r#"
local a = alife()
local b = a:create("bread", vector():set(1, 2, 3), 6, 1)
eq(type(b), "userdata", "created")
eq(type(a:object("bread0005")), "userdata", "named by section and id")
eq(type(a:create("bread", vector(), 5, 0, 1)), "userdata", "in a parent")
eq(a:create("bread", vector(), 5, 0, 77), nil, "invalid parent")
local box = a:create(3)
eq(type(a:story_object(8)), "userdata", "create(spawn_id) registers its story id")
a:release(a:object(1), true)
eq(a:object(1), nil, "released")
eq(a:object(2), nil, "its bread released first")
eq(a:object(6), nil, "the bread created in it")
"#,
    )?;
    ensure(
        logged(&t, "! invalid parent id [77] specified") == 1,
        || "invalid parent's log".into(),
    )?;
    {
        let a = t.alife.borrow();
        let sim = a.simulation.as_ref().expect("running");
        let o = sim.object(5).ok_or("object 5")?;
        ensure(
            o.record.header.position == [1.0, 2.0, 3.0]
                && o.record.object.object_block().game_graph_vertex == 1
                && o.record.object.object_block().level_graph_node == 6
                && o.record.header.parent_id == NO_ID,
            || "created object's place".into(),
        )?;
    }
    fatal(
        &mut t,
        r#"alife():create("box", vector(), 0, 0)"#,
        "Cannot find item with section box",
    )?;
    // An online object: a destroy event only.
    let mut t = state(true)?;
    t.alife
        .borrow_mut()
        .simulation
        .as_mut()
        .unwrap()
        .object_mut(BOX)
        .unwrap()
        .online = true;
    lua(
        &mut t,
        "release online",
        r#"
alife():release(alife():object(3))
eq(type(alife():object(3)), "userdata", "still registered")
local c = alife():create("bread", vector(), 6, 1, 3)
"#,
    )?;
    let events = t.alife.borrow_mut().take_events();
    ensure(
        events
            == [
                Event::Destroy {
                    id: BOX,
                    server_time: 0,
                },
                Event::SpawnedOnline { id: 5, parent: BOX },
            ],
        || format!("events {events:?}"),
    )
}

/// 8: the dynamic restrictions.
pub fn restrictions() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "restrictions",
        r#"
local a = alife()
local f = a:object(1)
a:add_in_restriction(f, 4)
a:add_in_restriction(f, 4)
a:add_out_restriction(f, 4)
a:remove_in_restriction(f, 4)
a:add_in_restriction(f, 99)
a:add_in_restriction(a:object(3), 4)
a:add_in_restriction(f, 3)
a:remove_out_restriction(f, 4)
a:remove_out_restriction(f, 4)
"#,
    )?;
    {
        let a = t.alife.borrow();
        let c = (a
            .simulation
            .as_ref()
            .unwrap()
            .object(FLESH)
            .unwrap()
            .record
            .object
            .creature())
        .unwrap()
        .clone();
        ensure(
            c.dynamic_in_restrictions == [4] && c.dynamic_out_restrictions.is_empty(),
            || {
                format!(
                    "{:?} {:?}",
                    c.dynamic_in_restrictions, c.dynamic_out_restrictions
                )
            },
        )?;
    }
    for line in [
        "! cannot add restriction with id 99 to the entity with id 1, because there is no space restrictor with the specified id",
        "! cannot add restriction with id 4 to the entity with id 3, because there is an object with the specified id, but it is not a creature",
        "! cannot add restriction with id 3 to the entity with id 1, because there is an object with the specified id, but it is not a space restrictor",
        "~ cannot remove restriction with id [4][restrictor_4] to the entity with id [1][flesh_1], because it is not added",
    ] {
        ensure(logged(&t, line) == 1, || format!("log: {line}"))?;
    }
    lua(
        &mut t,
        "remove all",
        r#"
alife():remove_all_restrictions(1, 5)
alife():remove_all_restrictions(1, 4)
alife():remove_all_restrictions(3, 4)
alife():remove_all_restrictions(50, 4)
"#,
    )?;
    let a = t.alife.borrow();
    let c = a
        .simulation
        .as_ref()
        .unwrap()
        .object(FLESH)
        .unwrap()
        .record
        .object
        .creature()
        .unwrap()
        .clone();
    drop(a);
    ensure(c.dynamic_in_restrictions.is_empty(), || "emptied".into())?;
    ensure(
        logged(
            &t,
            "! cannot remove restrictions to the entity with id 50, because there is no creature with the specified id",
        ) == 1
            && logged(
                &t,
                "! cannot remove restriction to the entity with id 3, because there is an object with the specified id, but it is not a creature",
            ) == 1,
        || "remove_all's lines".into(),
    )
}

/// 9: the health 0 and the death place.
pub fn kill_entity() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "kill",
        r#"alife():kill_entity(alife():object(1), 2)"#,
    )?;
    let a = t.alife.borrow();
    let o = a.simulation.as_ref().unwrap().object(FLESH).unwrap();
    let health = o.record.object.creature().unwrap().health;
    let gv = o.record.object.object_block().game_graph_vertex;
    ensure(health == 0.0 && gv == 2, || {
        format!("health {health}, vertex {gv}")
    })?;
    let Object::Monster(m) = &o.record.object else {
        return Err("a monster".into());
    };
    ensure(
        m.update.next_game_vertex == 2 && m.update.previous_game_vertex == 2,
        || "the Monster-like update's vertices".into(),
    )
}

/// 10: `game_graph()` and its vertices.
pub fn game_graph() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "game graph",
        r#"
local g = game_graph()
eq(rawequal(g, game_graph()), true, "one object")
eq(g:valid_vertex_id(3), true, "valid 3")
eq(g:valid_vertex_id(4), false, "valid 4")
local v = g:vertex(1)
eq(g:vertex_id(v), 1, "vertex_id")
eq(v:level_id(), 1, "level_id")
eq(v:level_vertex_id(), 6, "level_vertex_id")
eq(v:level_point().x, 10, "level_point")
eq(g:vertex(3):game_point().x, 2005, "game_point")
eq(g:accessible(1), true, "accessible")
g:accessible(1, false)
eq(g:accessible(1), false, "cleared")
eq(alife():level_name(game_graph():vertex(2):level_id()), "l02", "the smart terrain's level")
"#,
    )
}

/// 11: tasks name points of the patrol paths.
pub fn tasks() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "tasks",
        r#"
local t = CALifeSmartTerrainTask("walk")
eq(t:game_vertex_id(), 0, "point 0's game vertex")
eq(t:level_vertex_id(), 5, "level vertex")
local u = CALifeSmartTerrainTask("walk", 2)
eq(u:game_vertex_id(), 3, "point 2")
eq(u:position().x, 5, "position")
"#,
    )?;
    fatal(
        &mut t,
        r#"CALifeSmartTerrainTask("walk", 7):position()"#,
        "the task has no point",
    )?;
    let mut t = state(true)?;
    fatal(
        &mut t,
        r#"CALifeSmartTerrainTask("nowhere")"#,
        "no patrol path nowhere",
    )
}

/// 12: brains, the movement manager and its path managers.
pub fn brains() -> Result<(), String> {
    let mut t = state(true)?;
    ensure(
        t.s.simulation_brain(BOX).map_err(message)? == Value::Nil,
        || "the box has no brain".into(),
    )?;
    let brain = t.s.simulation_brain(FLESH).map_err(message)?;
    (t.s.lua_mut())
        .register("get_brain", move |_| Ok(vec![brain.clone()]))
        .map_err(message)?;
    lua(
        &mut t,
        "brain",
        r#"
local brain = get_brain()
local m = brain:movement()
brain:update()
eq(m:path_type(), 3, "no smart terrain: path type 3")
eq(m:actual(), true, "actual") eq(m:completed(), true, "completed")
local d = m:detail()
eq(d:completed(), true, "at its own target")
eq(d:failed(), true, "no path")
d:target(1)
eq(d:completed(), false, "a target elsewhere")
d:target(CALifeSmartTerrainTask("walk", 2))
d:target(0, 5, vector())
eq(d:completed(), true, "back at its place")
eq(d.speed, nil, "speed is registered with a tab")
d["speed\t"](d, 7)
eq(d["speed\t"](d), 7, "the stored speed")
brain:can_choose_alife_tasks(false)
local p = m:patrol()
eq(p:actual(), false, "not started")
p:path("walk")
p:route_type(0)
m:path_type(2)
brain:update()
"#,
    )?;
    // The brain's update set path type 3 again: run the patrol path
    // manager directly.
    let mut a = t.alife.borrow_mut();
    let a = &mut *a;
    let sim = a.simulation.as_ref().unwrap();
    let brain = a.brains.get_mut(&FLESH).ok_or("brain state")?;
    let p = &mut brain.patrol;
    ensure(p.path.as_deref() == Some("walk"), || "path".into())?;
    p.update(sim, (0, 5))?;
    ensure(
        p.started && p.current == Some(1) && p.previous == Some(0),
        || format!("first step {p:?}"),
    )?;
    p.update(sim, (1, 6))?;
    ensure(p.current == Some(2), || format!("second step {p:?}"))?;
    p.update(sim, (3, 8))?;
    ensure(p.completed, || "a dead end with route type 0".into())?;
    // Route type 1 turns back at the dead end.
    p.completed = false;
    p.route_type = 1;
    p.current = Some(2);
    p.previous = Some(1);
    p.update(sim, (3, 8))?;
    ensure(
        !p.completed && p.current == Some(1) && p.previous == Some(1),
        || format!("turning back {p:?}"),
    )?;
    // Nearest start: the first point on the object's vertex.
    p.started = false;
    p.start_type = 2;
    p.update(sim, (1, 0))?;
    ensure(p.current == Some(1), || format!("nearest {p:?}"))?;
    p.use_randomness = true;
    (p.current, p.previous) = (Some(2), Some(1));
    ensure(p.update(sim, (3, 8)).is_err(), || "division by zero".into())
}

/// 15: `has_info`, `dont_has_info` over the information portions.
pub fn has_info() -> Result<(), String> {
    let mut t = state(true)?;
    lua(
        &mut t,
        "has_info",
        r#"eq(alife():has_info(0, "known"), false, "not held") eq(alife():dont_has_info(0, "known"), true, "dont")"#,
    )?;
    {
        let story = t.story.borrow();
        let portions = story.info_portions.as_ref().unwrap();
        portions
            .borrow_mut()
            .give(0, "known", 0)
            .map_err(|e| e.to_string())?;
    }
    lua(
        &mut t,
        "held",
        r#"eq(alife():has_info(0, "known"), true, "held") eq(alife():has_info(5, "known"), false, "another object")"#,
    )?;
    Ok(())
}
