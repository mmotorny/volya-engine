// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The offline simulation on a small synthetic world: two levels, a 7 m
//! grid of level vertices on the first, and a handful of objects.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use volya_engine_formats::cross_table::CrossTable;
use volya_engine_formats::game_graph::{GameGraph, GraphEdge, GraphLevel, GraphVertex};
use volya_engine_formats::level_ai::{LevelAi, LevelAiHeader};
use volya_engine_formats::object::{
    ClassId, Creature, CreatureUpdate, Item, ItemUpdate, Monster, MonsterLike, MonsterLikeUpdate,
    MonsterState, Object, ObjectBlock, PlainItem, PlainVisual, Restrictor, Skeleton, SmartTerrain,
    SmartZone, SmartZoneScript, SpawnHeader, Visual,
};
use volya_engine_formats::patrol_path::{PathPoint, PatrolPath};

const SYSTEM: &str = "\
    [flesh]\nclass = AI_FLESH\ngoing_speed = 2\ncurrent_level_going_speed = 1\n\
    smart_terrain_choose_interval = 00:15:00\nterrain = 255,255,255,255\n\
    [box]\nclass = O_INVBOX\n[smart]\nclass = SMRTTRRN\n[bread]\nclass = II_FOOD\n";

const START: u64 = 63_471_533_400_000;
const SECOND: u64 = 1000;
const LEVEL_ID: [u8; 16] = [1; 16];
const GRAPH_ID: [u8; 16] = [9; 16];

fn system() -> Config {
    ltx::parse(SYSTEM.as_bytes()).unwrap()
}

fn vertex(level_id: u8, at: [f32; 3], level_vertex: u32, types: [u8; 4]) -> GraphVertex {
    let offset = if level_id == 1 { 0.0 } else { 1000.0 };
    GraphVertex {
        level_point: at,
        global_point: [at[0] + offset, at[1], at[2]],
        level_id,
        level_vertex,
        types,
        first_edge: 0,
        edge_count: 0,
        first_point: 0,
        point_count: 0,
    }
}

/// A graph of `vertices` with `edges` as (from, to, distance), each
/// vertex's edges in the order given.
fn graph(mut vertices: Vec<GraphVertex>, edges: &[(u16, u16, f32)]) -> GameGraph {
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
        identifier: if id == 1 { LEVEL_ID } else { [2; 16] },
    };
    GameGraph {
        version: 8,
        identifier: GRAPH_ID,
        levels: vec![level("one", 1), level("two", 2)],
        vertices,
        edges: list,
        point_count: 0,
        points_start: 0,
        trailing: Vec::new(),
    }
}

/// Level one: vertices 0 (0, 0, 0), 1 (7, 0, 0), 2 (0, 0, 7), 3 (7, 0, 7)
/// at the grid's corners; level two: vertices 4 and 5. Edges 0 → 1 → 3
/// (7 m each), 0 → 2 → 3 (8 m each), 3 → 0, 4 → 5 (100 m).
fn world() -> GameGraph {
    graph(
        vec![
            vertex(1, [0.0; 3], 0, [1, 0, 0, 0]),
            vertex(1, [7.0, 0.0, 0.0], 110, [1, 0, 0, 0]),
            vertex(1, [0.0, 0.0, 7.0], 10, [1, 0, 0, 0]),
            vertex(1, [7.0, 0.0, 7.0], 120, [1, 0, 0, 0]),
            vertex(2, [0.0; 3], 0, [1, 0, 0, 0]),
            vertex(2, [100.0, 0.0, 0.0], 1, [1, 0, 0, 0]),
        ],
        &[
            (0, 1, 7.0),
            (0, 2, 8.0),
            (1, 3, 7.0),
            (2, 3, 8.0),
            (3, 0, 9.9),
            (4, 5, 100.0),
        ],
    )
}

/// Level one's 11 × 11 level vertices (k = column · 11 + row) and their
/// cells: the nearest corner vertex, the distance in whole cells.
fn navigation(g: &GameGraph) -> Navigation {
    let header = LevelAiHeader {
        vertex_count: 121,
        cell_size: 0.7,
        height_range: 10.0,
        box_min: [0.0; 3],
        box_max: [7.0, 10.0, 7.0],
        identifier: LEVEL_ID,
    };
    let vertices: Vec<(u32, u16)> = (0..121).map(|p| (p, 0)).collect();
    let ai = LevelAi::new(header, &vertices);
    let cells: Vec<(u16, f32)> = (0..121u32)
        .map(|k| {
            let (c, r) = (k / 11, k % 11);
            let (vx, dx) = if c <= 5 { (0, c) } else { (1, 10 - c) };
            let (vz, dz) = if r <= 5 { (0, r) } else { (1, 10 - r) };
            ((vx + 2 * vz) as u16, dx.max(dz) as f32 * 0.7)
        })
        .collect();
    let cross = CrossTable::new(4, LEVEL_ID, GRAPH_ID, &cells);
    Navigation::new(g, 1, ai, cross).unwrap()
}

fn block(gv: u16, lv: u32, distance: f32, flags: u32) -> ObjectBlock {
    ObjectBlock {
        game_graph_vertex: gv,
        distance,
        direct_control: 1,
        level_graph_node: lv,
        state_flags: flags,
        custom_data: String::new().into(),
        story_id: -1,
        spawn_story_id: -1,
    }
}

fn header(id: u16, section: &str, position: [f32; 3]) -> SpawnHeader {
    SpawnHeader {
        section: section.into(),
        name: format!("{section}{id}").into(),
        game_type: 0,
        after_game_type: 0,
        position,
        angles: [0.0; 3],
        respawn_time: 0,
        id,
        parent_id: NO_ID,
        phantom_id: NO_ID,
        flags: 0x21,
        version: 118,
        script_version: 7,
        client_data: vec![1, 2, 3],
        spawn_id: id,
    }
}

fn creature(object: ObjectBlock, health: f32) -> Creature {
    Creature {
        object,
        visual: Visual {
            model: String::new().into(),
            flags: 0,
        },
        team: 0,
        squad: 0,
        group: 0,
        health,
        dynamic_out_restrictions: Vec::new(),
        dynamic_in_restrictions: Vec::new(),
        killer_id: NO_ID,
        death_time: 0,
    }
}

fn skeleton() -> Skeleton {
    Skeleton {
        startup_animation: String::new().into(),
        flags: 0,
        source_id: NO_ID,
        bone_state: None,
    }
}

/// The actor (from the player-state tests' shape) at `position`.
fn actor(position: [f32; 3], gv: u16, lv: u32) -> ObjectRecord {
    let mut r = crate::character::tests::record();
    r.header.id = ACTOR_ID;
    r.header.position = position;
    let b = r.object.object_block_mut();
    *b = block(gv, lv, 0.0, 0xFFFF_FFFF);
    r
}

/// A flesh (`AI_FLESH`, no scripted additions) at vertex `gv`.
fn flesh(id: u16, gv: u16, lv: u32, position: [f32; 3]) -> ObjectRecord {
    let update = CreatureUpdate {
        health: 1.0,
        timestamp: 0,
        flags: 0,
        position,
        model_yaw: 0.0,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        team: 0,
        squad: 0,
        group: 0,
    };
    ObjectRecord {
        header: header(id, "flesh", position),
        class_id: ClassId::new("AI_FLESH").unwrap(),
        single_player: true,
        object: Object::Monster(Box::new(Monster {
            state: MonsterState {
                monster: MonsterLike {
                    creature: creature(block(gv, lv, 0.0, 0xFFFF_FFBF), 1.0),
                    out_restrictions: String::new().into(),
                    in_restrictions: String::new().into(),
                    smart_terrain_id: NO_ID,
                    smart_terrain_task_reached: 0,
                },
                skeleton: skeleton(),
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

/// An inventory box switching by distance (state flags 0x07).
fn inventory_box(id: u16, gv: u16, lv: u32, position: [f32; 3]) -> ObjectRecord {
    ObjectRecord {
        header: header(id, "box", position),
        class_id: ClassId::new("O_INVBOX").unwrap(),
        single_player: true,
        object: Object::PlainVisual(PlainVisual {
            object: block(gv, lv, 0.0, 0x07),
            visual: Visual {
                model: "box".into(),
                flags: 0,
            },
        }),
    }
}

fn bread(id: u16, parent: u16) -> ObjectRecord {
    let mut h = header(id, "bread", [0.0; 3]);
    h.parent_id = parent;
    ObjectRecord {
        header: h,
        class_id: ClassId::new("II_FOOD").unwrap(),
        single_player: true,
        object: Object::Item(PlainItem {
            item: Item {
                object: block(0, 0, 0.0, 0x07),
                visual: Visual {
                    model: String::new().into(),
                    flags: 0,
                },
                condition: 1.0,
            },
            update: ItemUpdate {
                flags: 0,
                physics: None,
            },
        }),
    }
}

fn smart_terrain(id: u16) -> ObjectRecord {
    ObjectRecord {
        header: header(id, "smart", [3.5, 0.0, 3.5]),
        class_id: ClassId::new("SMRTTRRN").unwrap(),
        single_player: true,
        object: Object::SmartZone(SmartZone {
            restrictor: Restrictor {
                object: block(0, 60, 3.5, 0x03),
                shapes: Vec::new(),
                restrictor_type: 0,
            },
            script: SmartZoneScript::SmartTerrain(SmartTerrain {
                duration_end: None,
                idle_end: None,
                working: None,
            }),
        }),
    }
}

fn config(online: f32, offline: f32) -> AlifeConfig {
    AlifeConfig {
        distances: SwitchDistances { online, offline },
        objects_per_update: 10,
        stay_after_death: 36 * HOUR_MS,
    }
}

/// A patrol path `name` whose point 0 is at `gv`, `lv`, `position`.
fn path(name: &str, gv: u16, lv: u32, position: [f32; 3]) -> PatrolPath {
    PatrolPath {
        name: name.into(),
        points: vec![PathPoint {
            name: "p0".into(),
            position,
            flags: 0,
            level_vertex: lv,
            game_vertex: gv,
        }],
        links: Vec::new(),
    }
}

/// A simulation of `records` over [`world`] with level one's navigation.
fn simulation(records: Vec<ObjectRecord>, config: AlifeConfig) -> Simulation {
    let g = world();
    let nav = navigation(&g);
    let paths = PatrolPaths::from_paths(vec![path("to_3", 3, 119, [7.0, 0.0, 6.3])]);
    let mut sim = Simulation::new(Graph::new(g), paths, config);
    sim.load(records, &system()).unwrap();
    sim.set_navigation(nav).unwrap();
    sim
}

#[test]
fn path_search_ties_masks_and_accessibility() {
    // A square: 0 → 1 → 3 and 0 → 2 → 3, equal lengths and estimates.
    let g = graph(
        vec![
            vertex(1, [0.0; 3], 0, [1, 0, 0, 0]),
            vertex(1, [1.0, 0.0, 0.0], 0, [5, 0, 0, 0]),
            vertex(1, [0.0, 0.0, 1.0], 0, [1, 0, 0, 0]),
            vertex(1, [1.0, 0.0, 1.0], 0, [1, 0, 0, 0]),
        ],
        &[(0, 1, 1.0), (0, 2, 1.0), (1, 3, 1.0), (2, 3, 1.0)],
    );
    let mut graph = Graph::new(g);
    // Of equal f, the most recently opened first: through 2.
    assert_eq!(graph.path(0, 3, &[ANY]), Some(vec![0, 2, 3]));
    assert_eq!(graph.path(0, 0, &[ANY]), Some(vec![0]));
    assert_eq!(graph.path(3, 0, &[ANY]), None);
    // Vertex 1 matches only a mask with t0 = 5; 2 is then not admitted.
    assert_eq!(
        graph.path(0, 3, &[[5, 255, 255, 255], [255, 0, 0, 1]]),
        None
    );
    assert_eq!(
        graph.path(0, 1, &[[5, 255, 255, 255]]),
        Some(vec![0, 1]),
        "the start is never tested"
    );
    graph.set_accessible(2, false);
    assert_eq!(graph.path(0, 3, &[ANY]), Some(vec![0, 1, 3]));
    // An inaccessible start: the masks do not apply.
    graph.set_accessible(0, false);
    assert_eq!(graph.path(0, 3, &[[9, 9, 9, 9]]), Some(vec![0, 1, 3]));
    assert_eq!(graph.path_length(&[0, 1, 3]), Some(2.0));
    assert_eq!(graph.path_length(&[0, 3]), None);
}

const ANY: TerrainMask = graph::ANY_TERRAIN;

#[test]
fn the_switch_rule() {
    let d = SwitchDistances {
        online: 135.0,
        offline: 165.0,
    };
    // Offline: on and (not off or within 135 m).
    assert!(switched(false, (true, true), 135.0, &d));
    assert!(!switched(false, (true, true), 136.0, &d));
    assert!(switched(false, (true, false), 1_000.0, &d));
    assert!(!switched(false, (false, false), 0.0, &d));
    // Online: stays unless off and (not on or beyond 165 m).
    assert!(switched(true, (true, true), 165.0, &d));
    assert!(!switched(true, (true, true), 166.0, &d));
    assert!(switched(true, (false, false), 1_000.0, &d));
    assert!(!switched(true, (false, true), 0.0, &d));
}

#[test]
fn level_list_registries_and_client_data() {
    let mut on_two = flesh(3, 4, 0, [0.0; 3]);
    on_two.object.object_block_mut().story_id = 77;
    let records = vec![
        actor([0.35, 0.0, 0.35], 0, 0),
        inventory_box(1, 0, 0, [0.0; 3]),
        bread(2, 1),
        on_two,
        smart_terrain(4),
    ];
    let sim = simulation(records, config(3.0, 5.0));
    assert_eq!(sim.current_level(), Some(1));
    // The held item and the object of level two are not in the list.
    assert_eq!(sim.level_list().collect::<Vec<_>>(), [0, 1, 4]);
    assert_eq!(sim.schedule().collect::<Vec<_>>(), [3]);
    assert_eq!(sim.by_story_id(77), Some(3));
    // Client data kept only on the current level (sav.md 8).
    let data = |id| sim.object(id).unwrap().record.header.client_data.len();
    assert_eq!((data(1), data(2), data(3)), (3, 3, 0));
}

#[test]
fn walking_switches_by_distance_and_carried_items_follow() {
    let records = vec![
        actor([0.0, 0.0, 7.0], 2, 10),
        inventory_box(1, 0, 0, [0.0; 3]),
        flesh(2, 1, 110, [7.0, 0.0, 0.0]),
    ];
    let mut sim = simulation(records, config(3.0, 5.0));
    let c = sim.update(START, 10.0, 5).unwrap();
    // The actor (distance 0) comes online; the box at 7 m does not.
    assert_eq!(c.online, [0]);
    sim.set_actor_position([0.0, 0.0, 3.0]);
    assert_eq!(sim.update(START + SECOND, 10.0, 5).unwrap().online, [1]);
    assert!(sim.object(1).unwrap().online);
    // 4.5 m: between the distances, it stays.
    sim.set_actor_position([0.0, 0.0, 4.5]);
    assert!(sim.update(START + 2 * SECOND, 10.0, 5).unwrap().is_empty());
    sim.set_actor_position([0.0, 0.0, 5.5]);
    let c = sim.update(START + 3 * SECOND, 10.0, 5).unwrap();
    assert_eq!(c.offline, [1]);
    // Client data is discarded when it switches offline.
    assert!(sim.object(1).unwrap().record.header.client_data.is_empty());
    // The monster left the schedule while online, and is back after.
    sim.set_actor_position([6.0, 0.0, 0.0]);
    let c = sim.update(START + 4 * SECOND, 10.0, 5).unwrap();
    assert_eq!(c.online, [2]);
    assert!(!sim.schedule().any(|id| id == 2));
    sim.set_actor_position([0.0, 0.0, 7.0]);
    sim.update(START + 5 * SECOND, 10.0, 5).unwrap();
    assert!(sim.schedule().any(|id| id == 2));
}

#[test]
fn location_updates() {
    let records = vec![
        actor([0.1, 0.0, 0.1], 0, 0),
        // Offline at vertex 1's own level vertex but standing where the
        // cell names vertex 3: it arrives there, its position kept.
        flesh(1, 1, 110, [6.9, 0.0, 6.2]),
        // Offline with an invalid level vertex: nothing changes.
        flesh(2, 1, 5_000, [7.0, 0.0, 3.5]),
    ];
    let mut sim = simulation(records, config(0.5, 1.0));
    sim.set_actor_position([0.0, 0.0, 2.8]);
    sim.update(START, 10.0, 5).unwrap();
    // Online: level vertex, game vertex and distance from the cell (5.2).
    let a = sim
        .object(ACTOR_ID)
        .unwrap()
        .record
        .object
        .object_block()
        .clone();
    assert_eq!((a.level_graph_node, a.game_graph_vertex), (4, 0));
    assert!((a.distance - 2.8).abs() < 1e-5);
    let f = sim.object(1).unwrap();
    let b = f.record.object.object_block();
    assert_eq!(
        (b.game_graph_vertex, b.level_graph_node, b.distance),
        (3, 120, 0.0)
    );
    assert_eq!(f.record.header.position, [6.9, 0.0, 6.2]);
    let b = sim.object(2).unwrap().record.object.object_block().clone();
    assert_eq!((b.game_graph_vertex, b.level_graph_node), (1, 5_000));
}

/// Scripts giving every object smart terrain `terrain` and path `path`,
/// counting looks.
struct Fixed {
    accept: bool,
    path: &'static str,
    looks: Rc<RefCell<u32>>,
}

impl SmartTerrains for Fixed {
    fn suitability(&mut self, _: u16, _: u16) -> Option<f32> {
        *self.looks.borrow_mut() += 1;
        self.accept.then_some(1.0)
    }

    fn join(&mut self, _: u16, _: u16) {}

    fn task(&mut self, _: u16, _: u16) -> Option<String> {
        Some(self.path.into())
    }
}

#[test]
fn smart_terrain_choice_and_its_interval() {
    let records = vec![
        actor([0.0, 0.0, 7.0], 2, 10),
        flesh(1, 1, 110, [7.0, 0.0, 0.0]),
        smart_terrain(2),
    ];
    let mut sim = simulation(records, config(0.5, 1.0));
    let looks = Rc::new(RefCell::new(0));
    sim.set_scripts(Box::new(Fixed {
        accept: false,
        path: "to_3",
        looks: looks.clone(),
    }));
    sim.update(START, 10.0, 5).unwrap();
    assert_eq!(*looks.borrow(), 1);
    // Not before 15 game minutes have passed since the last look.
    sim.update(START + 14 * 60 * SECOND, 10.0, 5).unwrap();
    assert_eq!(*looks.borrow(), 1);
    sim.update(START + 15 * 60 * SECOND, 10.0, 5).unwrap();
    assert_eq!(*looks.borrow(), 2);
    // Accepted: it joins, and its first movement update only starts the
    // clock.
    sim.set_scripts(Box::new(Fixed {
        accept: true,
        path: "to_3",
        looks: looks.clone(),
    }));
    sim.update(START + 30 * 60 * SECOND, 10.0, 5).unwrap();
    let o = sim.object(1).unwrap();
    assert_eq!(o.record.object.monster_like().unwrap().smart_terrain_id, 2);
    let t = o.travel.as_ref().unwrap();
    assert_eq!((t.last_look, t.clock), (0, START + 30 * 60 * SECOND));
    assert_eq!(t.target.game_vertex, 3);
    assert!(t.path.is_empty());
}

#[test]
fn travel_over_game_time() {
    let records = vec![actor([0.0, 0.0, 7.0], 2, 10), flesh(1, 0, 0, [0.0; 3])];
    let mut sim = simulation(records, config(0.5, 1.0));
    sim.object_mut(1)
        .unwrap()
        .record
        .object
        .monster_like_mut()
        .unwrap()
        .smart_terrain_id = 9;
    let looks = Rc::new(RefCell::new(0));
    sim.set_scripts(Box::new(Fixed {
        accept: false,
        path: "to_3",
        looks,
    }));
    let at = |sim: &Simulation| {
        let o = sim.object(1).unwrap();
        let t = o.travel.as_ref().unwrap();
        // Progress to the millimetre.
        let progress = (t.progress * 1000.0).round() / 1000.0;
        (o.record.object.object_block().game_graph_vertex, progress)
    };
    // The clock starts.
    sim.update(START, 10.0, 5).unwrap();
    assert_eq!(at(&sim), (0, 0.0));
    // 50 game seconds at ×10 and 1 m/s on the current level: 5 m of the
    // 7 m edge 0 → 1 (the search takes 0 → 1 → 3, 14 m, over 0 → 2 → 3).
    sim.update(START + 50 * SECOND, 10.0, 5).unwrap();
    assert_eq!(
        sim.object(1).unwrap().travel.as_ref().unwrap().path,
        [0, 1, 3]
    );
    assert_eq!(at(&sim), (0, 5.0));
    // The same time again (paused or hidden): nothing.
    sim.update(START + 50 * SECOND, 10.0, 5).unwrap();
    assert_eq!(at(&sim), (0, 5.0));
    // 30 s more: arrives at 1 after 2 m, the remaining 1 m on 1 → 3.
    sim.update(START + 80 * SECOND, 10.0, 5).unwrap();
    let (v, p) = at(&sim);
    assert_eq!(v, 1);
    assert!((p - 1.0).abs() < 1e-4, "{p}");
    assert_eq!(
        sim.object(1).unwrap().record.header.position,
        [7.0, 0.0, 0.0]
    );
    // 60 s more: at the target's game vertex, at its level point.
    sim.update(START + 140 * SECOND, 10.0, 5).unwrap();
    assert_eq!(at(&sim), (3, 0.0));
    let o = sim.object(1).unwrap();
    assert_eq!(o.record.header.position, [7.0, 0.0, 7.0]);
    // The next movement update takes the target's position (7.5 step 3).
    sim.update(START + 141 * SECOND, 10.0, 5).unwrap();
    let o = sim.object(1).unwrap();
    assert_eq!(o.record.header.position, [7.0, 0.0, 6.3]);
    assert_eq!(o.record.object.object_block().level_graph_node, 119);
}

#[test]
fn travel_off_the_current_level_uses_going_speed() {
    let records = vec![actor([0.0, 0.0, 7.0], 2, 10), flesh(1, 4, 0, [0.0; 3])];
    let mut sim = simulation(records, config(0.5, 1.0));
    sim.paths = PatrolPaths::from_paths(vec![path("far", 5, 1, [100.0, 0.0, 0.0])]);
    sim.object_mut(1)
        .unwrap()
        .record
        .object
        .monster_like_mut()
        .unwrap()
        .smart_terrain_id = 9;
    sim.set_scripts(Box::new(Fixed {
        accept: false,
        path: "far",
        looks: Rc::new(RefCell::new(0)),
    }));
    sim.update(START, 10.0, 5).unwrap();
    sim.update(START + 100 * SECOND, 10.0, 5).unwrap();
    // 100 game s / 10 · 2 m/s.
    let t = sim.object(1).unwrap().travel.as_ref().unwrap().clone();
    assert!((t.progress - 20.0).abs() < 1e-4);
    // Arriving on another level: still not in the current level's list.
    sim.update(START + 600 * SECOND, 10.0, 5).unwrap();
    assert_eq!(
        sim.object(1)
            .unwrap()
            .record
            .object
            .object_block()
            .game_graph_vertex,
        5
    );
    assert!(!sim.level_list().any(|id| id == 1));
}

#[test]
fn dead_bodies_are_removed_with_what_they_carry() {
    let mut body = flesh(1, 3, 120, [7.0, 0.0, 7.0]);
    if let Some(c) = body.object.creature_mut() {
        c.health = 0.0;
        c.death_time = START;
    }
    // Dead creatures never switch offline (level-objects.md 2.3), so it
    // must be offline: its state flags lack 0x02 (cannot switch online).
    body.object.object_block_mut().state_flags = 0x05;
    let records = vec![actor([0.0; 3], 0, 0), body, bread(2, 1)];
    let mut sim = simulation(records, config(0.5, 1.0));
    let c = sim.update(START + 36 * HOUR_MS - 1, 10.0, 5).unwrap();
    assert!(c.removed.is_empty());
    let c = sim.update(START + 36 * HOUR_MS, 10.0, 5).unwrap();
    assert_eq!(c.removed, [2, 1]);
    assert!(sim.object(1).is_none() && sim.object(2).is_none());
}

#[test]
fn removal_ends_on_a_cycle_of_parents() {
    let mut body = flesh(1, 3, 120, [7.0, 0.0, 7.0]);
    if let Some(c) = body.object.creature_mut() {
        c.health = 0.0;
        c.death_time = START;
    }
    body.object.object_block_mut().state_flags = 0x05;
    // Malformed: the body carries 2, 2 carries 3, 3 carries the body.
    body.header.parent_id = NO_ID;
    let mut a = bread(2, 1);
    a.header.parent_id = 1;
    let mut b = bread(3, 2);
    b.header.parent_id = 2;
    let mut sim = simulation(vec![actor([0.0; 3], 0, 0), body, a, b], config(0.5, 1.0));
    sim.object_mut(1).unwrap().record.header.parent_id = 3;
    // In the list despite its parent (it is no item): removed with both.
    sim.level_list.insert(1);
    let mut changes = Changes::default();
    sim.remove(1, &mut changes);
    let mut removed = changes.removed.clone();
    removed.sort();
    assert_eq!(removed, [1, 2, 3]);
}

#[test]
fn client_data_kept_by_the_level_of_the_top_container() {
    // A box on level one with direct control 0: not in the list, but on
    // the current level, so its client data stays (sav.md 8).
    let mut b = inventory_box(1, 0, 0, [0.0; 3]);
    b.object.object_block_mut().direct_control = 0;
    let sim = simulation(vec![actor([0.0; 3], 0, 0), b], config(0.5, 1.0));
    assert!(!sim.level_list().any(|id| id == 1));
    assert_eq!(sim.object(1).unwrap().record.header.client_data.len(), 3);
}

#[test]
fn ids_after_a_load_and_fractional_hours() {
    let mut sim = simulation(
        vec![actor([0.0; 3], 0, 0), inventory_box(40, 0, 0, [0.0; 3])],
        config(0.5, 1.0),
    );
    assert_eq!(sim.take_id().unwrap(), 41);
    assert_eq!(sim.take_id().unwrap(), 42);
    let alife = ltx::parse(b"[alife]\nswitch_distance = 150\nswitch_factor = 0.1\n").unwrap();
    let system = ltx::parse(b"[monsters_common]\nstay_after_death_time_interval = 0.5\n").unwrap();
    let c = AlifeConfig::read(&alife, &system).unwrap();
    assert_eq!(c.stay_after_death, HOUR_MS / 2);
}

#[test]
fn intervals() {
    assert_eq!(interval_ms("00:15:00"), Some(900_000));
    assert_eq!(interval_ms("01:00:30"), Some(3_630_000));
    assert_eq!(interval_ms("15"), None);
}

/// ltx-config.md 11: a stalker's or monster's `going_speed`,
/// `smart_terrain_choose_interval` and `terrain` are read (7.2, 7.4), so a
/// missing one is fatal; `current_level_going_speed` defaults to
/// `going_speed`.
#[test]
fn a_missing_travel_key_is_fatal() {
    let records = || vec![actor([0.0, 0.0, 7.0], 2, 10), flesh(1, 0, 0, [0.0; 3])];
    for key in ["going_speed", "smart_terrain_choose_interval", "terrain"] {
        let without: String = SYSTEM
            .lines()
            .filter(|l| !l.starts_with(&format!("{key} =")))
            .map(|l| format!("{l}\n"))
            .collect();
        let system = ltx::parse(without.as_bytes()).unwrap();
        let mut sim = Simulation::new(
            Graph::new(world()),
            PatrolPaths::default(),
            config(0.5, 1.0),
        );
        let err = sim.load(records(), &system).unwrap_err();
        assert!(err.to_string().contains(key), "{key}: {err}");
    }
    let lenient = SYSTEM.replace("current_level_going_speed = 1\n", "");
    assert_ne!(lenient, SYSTEM);
    let system = ltx::parse(lenient.as_bytes()).unwrap();
    let mut sim = Simulation::new(
        Graph::new(world()),
        PatrolPaths::default(),
        config(0.5, 1.0),
    );
    assert!(sim.load(records(), &system).is_ok());
}
