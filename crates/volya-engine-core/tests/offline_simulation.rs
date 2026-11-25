// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The verification facts of docs/behavior/offline-simulation.md section 9
//! on the Russian edition (BYOA): `game.graph`, the cross tables and
//! `level.ai` of every level, the patrol paths, the objects' locations,
//! their travel configuration, the path search; then a New Game's registry
//! and the player walking an object online and offline. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![allow(clippy::excessive_precision, clippy::type_complexity)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::io::BufReader;

use sha2::{Digest, Sha256};
use volya_engine_core::config::{self, Config};
use volya_engine_core::simulation::graph::{ANY_TERRAIN, Graph};
use volya_engine_core::simulation::navigation::Navigation;
use volya_engine_core::simulation::{
    ACTOR_ID, AlifeConfig, Changes, NO_ID, Simulation, class_of, uses_level_graph,
};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, read};
use volya_engine_formats::cross_table::CrossTable;
use volya_engine_formats::game_graph::{GameGraph, matches_mask};
use volya_engine_formats::level_ai::LevelAi;
use volya_engine_formats::object::{Kind, ObjectRecord, section};
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_formats::spawn;

fn with_tree(test: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    test(&tree);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// Every record of the spawn file with its index.
fn records(spawn_file: &[u8], system: &Config) -> Vec<(u32, ObjectRecord)> {
    let mut out = Vec::new();
    spawn::for_each_record(spawn_file, |r| {
        let class = class_of(system, &section(r.spawn)?)?.to_owned();
        out.push((
            r.index,
            ObjectRecord::parse(r.spawn, r.update, &class, true)?,
        ));
        Ok(std::ops::ControlFlow::Continue(()))
    })
    .unwrap();
    out
}

/// The 18 levels with data: name, id, offset, first and last vertex,
/// edges, level points (section 9).
const LEVELS: &[(&str, u8, [f32; 3], u16, u16, usize, usize)] = &[
    ("l01_escape", 1, [3000.0, 1000.0, 0.0], 0, 251, 652, 40_733),
    (
        "l02_garbage",
        2,
        [3000.0, 1000.0, 1000.0],
        252,
        415,
        428,
        33_149,
    ),
    (
        "l03_agroprom",
        3,
        [2000.0, 1000.0, 1000.0],
        416,
        702,
        778,
        42_019,
    ),
    (
        "l03u_agr_underground",
        4,
        [2000.0, 800.0, 1000.0],
        703,
        810,
        225,
        585,
    ),
    (
        "l04_darkvalley",
        5,
        [4000.0, 1000.0, 1000.0],
        811,
        1108,
        728,
        33_645,
    ),
    (
        "l04u_labx18",
        6,
        [4000.0, 800.0, 1000.0],
        1109,
        1167,
        123,
        745,
    ),
    (
        "l05_bar",
        7,
        [3000.0, 1000.0, 2000.0],
        1168,
        1307,
        320,
        9_131,
    ),
    (
        "l06_rostok",
        8,
        [2000.0, 1000.0, 2000.0],
        1308,
        1437,
        301,
        6_711,
    ),
    (
        "l08_yantar",
        9,
        [1000.0, 1000.0, 2000.0],
        1438,
        1528,
        220,
        12_306,
    ),
    (
        "l08u_brainlab",
        10,
        [1000.0, 800.0, 2000.0],
        1529,
        1544,
        31,
        795,
    ),
    (
        "l07_military",
        11,
        [3000.0, 1000.0, 3000.0],
        1545,
        1861,
        896,
        41_473,
    ),
    (
        "l10_radar",
        13,
        [3000.0, 1000.0, 4000.0],
        1862,
        2116,
        609,
        22_459,
    ),
    (
        "l11_pripyat",
        14,
        [3000.0, 1000.0, 5000.0],
        2117,
        2272,
        398,
        18_779,
    ),
    (
        "l12_stancia",
        15,
        [2000.0, 1000.0, 8000.0],
        2273,
        2401,
        319,
        30_664,
    ),
    (
        "l12u_sarcofag",
        16,
        [2000.0, 1200.0, 8000.0],
        2402,
        2466,
        136,
        785,
    ),
    (
        "l12u_control_monolith",
        17,
        [2000.0, 1400.0, 8000.0],
        2467,
        2516,
        104,
        382,
    ),
    (
        "l12_stancia_2",
        19,
        [2000.0, 1000.0, 9000.0],
        2517,
        2660,
        321,
        22_989,
    ),
    (
        "l10u_bunker",
        20,
        [3000.0, 800.0, 4000.0],
        2661,
        2791,
        273,
        878,
    ),
];

/// Level, N, `level.gct` bytes, `level.ai` bytes, row length (section 9).
const NAVIGATION: &[(&str, usize, usize, usize, u32)] = &[
    ("l01_escape", 595_499, 3_573_054, 12_505_535, 1_697),
    ("l02_garbage", 382_663, 2_296_038, 8_035_979, 903),
    ("l03_agroprom", 437_421, 2_624_586, 9_185_897, 790),
    ("l03u_agr_underground", 4_932, 29_652, 103_628, 272),
    ("l04_darkvalley", 390_125, 2_340_810, 8_192_681, 1_095),
    ("l04u_labx18", 7_581, 45_546, 159_257, 166),
    ("l05_bar", 99_539, 597_294, 2_090_375, 1_285),
    ("l06_rostok", 67_713, 406_338, 1_422_029, 823),
    ("l07_military", 418_268, 2_509_668, 8_783_684, 770),
    ("l08_yantar", 141_471, 848_886, 2_970_947, 643),
    ("l08u_brainlab", 8_008, 48_108, 168_224, 81),
    ("l10_radar", 227_189, 1_363_194, 4_771_025, 890),
    ("l10u_bunker", 8_821, 52_986, 185_297, 213),
    ("l11_pripyat", 261_219, 1_567_374, 5_485_655, 1_141),
    ("l12_stancia", 477_923, 2_867_598, 10_036_439, 630),
    ("l12_stancia_2", 264_577, 1_587_522, 5_556_173, 551),
    ("l12u_control_monolith", 3_982, 23_952, 83_678, 112),
    ("l12u_sarcofag", 10_672, 64_092, 224_168, 114),
];

fn graph_file(vfs: &dyn Vfs) -> Vec<u8> {
    read(vfs, "gamedata/game.graph").unwrap()
}

#[test]
fn game_graph_facts() {
    with_tree(|vfs| {
        let file = graph_file(vfs);
        assert_eq!(file.len(), 8_128_486);
        assert_eq!(crc32fast::hash(&file), 0xf5e1_987b);
        assert_eq!(
            hex(&Sha256::digest(&file)),
            "a1ddb5f7b25f3f0f859883b0693ecdcc56bb269853c4181e91141eb8260fd76d"
        );
        let g = GameGraph::parse(&file).unwrap();
        assert_eq!(
            (g.version, g.vertices.len(), g.edges.len(), g.point_count),
            (8, 3_512, 8_630, 396_380)
        );
        assert_eq!(hex(&g.identifier), "138f121ee1b47f4f88fcd4dc9e7ca70c");
        assert_eq!(g.levels.len(), 29);
        // 1.3 – 1.5, section 9: the tables' offsets.
        let table = 1_486;
        assert_eq!(g.points_start, table + 42 * 3_512 + 6 * 8_630);
        assert_eq!((table + 42 * 3_512, g.points_start), (148_990, 200_770));
        assert_eq!(g.points_start + 20 * 396_380, 8_128_370);
        assert_eq!(g.trailing.len(), 116);
        assert!(g.trailing.chunks(4).all(|c| c == [4, 0, 0, 0]));
        // 1.2: increasing level id; section names.
        assert!(g.levels.windows(2).all(|w| w[0].id < w[1].id));
        for l in &g.levels {
            assert_eq!(l.section, format!("level{:02}", l.id), "{}", l.name);
        }
        let edge_sum: f64 = g.edges.iter().map(|e| f64::from(e.distance)).sum();
        assert!(near(edge_sum, 275_640.907, 0.05), "{edge_sum}");
        let lv_sum: u64 = g.vertices.iter().map(|v| u64::from(v.level_vertex)).sum();
        assert_eq!(lv_sum, 474_202_540);
        let mut point_sum = 0.0f64;
        for i in 0..g.vertices.len() {
            for p in g.level_points(&file, i as u16).unwrap() {
                point_sum += f64::from(p.distance);
            }
        }
        assert!(near(point_sum, 7_731_072.870, 0.5), "{point_sum}");
        // Runs consecutive in vertex order.
        for w in g.vertices.windows(2) {
            assert_eq!(
                w[1].first_edge,
                w[0].first_edge + u32::from(w[0].edge_count)
            );
            assert_eq!(
                w[1].first_point,
                w[0].first_point + u32::from(w[0].point_count)
            );
        }

        // Section 9's level table.
        for &(name, id, offset, first, last, edges, points) in LEVELS {
            let l = g.level(id).unwrap();
            assert_eq!((l.name.as_str(), l.offset), (name, offset));
            let vs: Vec<usize> = (0..g.vertices.len())
                .filter(|&i| g.vertices[i].level_id == id)
                .collect();
            assert_eq!(
                (vs[0], *vs.last().unwrap(), vs.len()),
                (
                    usize::from(first),
                    usize::from(last),
                    usize::from(last - first) + 1
                ),
                "{name}"
            );
            let e: usize = vs
                .iter()
                .map(|&i| usize::from(g.vertices[i].edge_count))
                .sum();
            let p: usize = vs
                .iter()
                .map(|&i| usize::from(g.vertices[i].point_count))
                .sum();
            assert_eq!((e, p), (edges, points), "{name}");
        }
        let without: Vec<&str> = g
            .levels
            .iter()
            .filter(|l| !LEVELS.iter().any(|x| x.1 == l.id))
            .map(|l| l.name.as_str())
            .collect();
        assert_eq!(without.len(), 11);
        assert!(without.contains(&"l03u_agr_underground_hw"));
        let (e, p) = (2_792..3_512)
            .map(|i| &g.vertices[i])
            .fold((0, 0), |(e, p), v| {
                (
                    e + usize::from(v.edge_count),
                    p + usize::from(v.point_count),
                )
            });
        assert_eq!((e, p), (1_768, 78_152));
        assert_eq!(
            hex(&g.level(1).unwrap().identifier),
            "23ce433a31b3134ca06e2caf5b668452"
        );
        // Global points (1.3), edges between levels (1.4, section 9).
        for v in &g.vertices {
            let o = g.level(v.level_id).unwrap().offset;
            for i in 0..3 {
                assert!((v.level_point[i] + o[i] - v.global_point[i]).abs() <= 0.0005);
            }
        }
        let mut between = 0;
        let mut reverse_missing = 0;
        for (i, v) in g.vertices.iter().enumerate() {
            for e in g.edges_of(i as u16) {
                assert_ne!(usize::from(e.target), i, "an edge to itself");
                let t = &g.vertices[usize::from(e.target)];
                if !g
                    .edges_of(e.target)
                    .iter()
                    .any(|r| usize::from(r.target) == i)
                {
                    reverse_missing += 1;
                }
                if t.level_id != v.level_id {
                    between += 1;
                    let d: f32 = (0..3)
                        .map(|k| (v.global_point[k] - t.global_point[k]).powi(2))
                        .sum::<f32>()
                        .sqrt();
                    assert!((d - e.distance).abs() <= 0.0001 * d.max(1.0), "{i}");
                } else {
                    let h = ((v.level_point[0] - t.level_point[0]).powi(2)
                        + (v.level_point[2] - t.level_point[2]).powi(2))
                    .sqrt();
                    assert!(e.distance >= h - 0.001, "{i} → {}", e.target);
                }
            }
        }
        assert_eq!((between, reverse_missing), (43, 6));
        let edge = |a: u16, b: u16| {
            g.edges_of(a)
                .iter()
                .find(|e| e.target == b)
                .map(|e| e.distance)
                .unwrap()
        };
        assert!((edge(0, 415) - 1_206.288).abs() < 0.001);
        assert!((edge(1, 252) - 21.196).abs() < 0.001);
        assert!((edge(252, 1) - 21.196).abs() < 0.001);
        assert!((edge(645, 646) - 0.0907).abs() < 0.0001);

        // Vertices 0, 4, 2 (section 9).
        let v0 = g.vertex(0).unwrap();
        let close3 = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 0.001);
        assert!(close3(v0.level_point, [368.648, 14.986, -39.828]));
        assert!(close3(v0.global_point, [3368.648, 1014.986, -39.828]));
        assert_eq!(
            (v0.level_vertex, v0.types, v0.point_count),
            (594_270, [1, 0, 0, 0], 25)
        );
        assert_eq!(
            g.edges_of(0).iter().map(|e| e.target).collect::<Vec<_>>(),
            [140, 415]
        );
        assert!((g.edges_of(0)[0].distance - 28.516).abs() < 0.001);
        let v4 = g.vertex(4).unwrap();
        assert!(close3(v4.level_point, [-247.103, -24.788, -134.425]));
        assert_eq!(
            (v4.level_vertex, v4.types, v4.point_count),
            (11_366, [1, 0, 0, 0], 1)
        );
        assert_eq!(g.edges_of(4).len(), 1);
        assert_eq!(g.edges_of(4)[0].target, 7);
        assert!((g.edges_of(4)[0].distance - 3.405).abs() < 0.001);
        assert_eq!(g.vertex(2).unwrap().point_count, 156);
        // 1.6: the type bytes' ranges.
        assert!(
            g.vertices
                .iter()
                .all(|v| v.types[3] == 0 && v.types[2] <= 1 && v.types[1] <= 2 && v.types[0] <= 17)
        );
        assert!((2_661..=2_791).all(|i| g.vertices[i].types[0] == 0));

        // game_levels.ltx agrees for the 18 levels (1.2).
        let levels = config::load(vfs, "gamedata/config/game_levels.ltx").unwrap();
        for &(name, id, offset, ..) in LEVELS {
            let s = levels
                .sections()
                .find(|s| {
                    s.get("id").flatten().map(|v| v.trim().parse::<u8>().ok()) == Some(Some(id))
                })
                .unwrap_or_else(|| panic!("no section with id {id}"));
            let n = s.get("name").flatten().unwrap().trim();
            assert!(n.eq_ignore_ascii_case(name), "{n} {name}");
            let o = config::value::floats::<3>(s.get("offset").flatten().unwrap());
            assert_eq!(o, offset, "{name}");
        }
    });
}

#[test]
fn cross_tables_and_level_ai_of_every_level() {
    with_tree(|vfs| {
        let file = graph_file(vfs);
        let g = GameGraph::parse(&file).unwrap();
        for &(name, n, gct_bytes, ai_bytes, row) in NAVIGATION {
            let dir = format!("gamedata/levels/{name}");
            let gct = read(vfs, &format!("{dir}/level.gct")).unwrap();
            let ai_file = read(vfs, &format!("{dir}/level.ai")).unwrap();
            assert_eq!((gct.len(), ai_file.len()), (gct_bytes, ai_bytes), "{name}");
            let cross = CrossTable::parse(&gct).unwrap();
            let ai = LevelAi::read(&ai_file[..]).unwrap();
            assert_eq!((cross.len(), ai.len()), (n, n), "{name}");
            assert_eq!(ai.header.row_length(), row, "{name}");
            assert_eq!(ai.header.cell_size, 0.7, "{name}");
            let id = LEVELS.iter().find(|l| l.0 == name).unwrap().1;
            let level = g.level(id).unwrap();
            // The three identifiers agree (1.8).
            assert_eq!(cross.game_identifier, g.identifier, "{name}");
            assert_eq!(cross.level_identifier, ai.header.identifier, "{name}");
            assert_eq!(level.identifier, ai.header.identifier, "{name}");
            // Section 2's invariants.
            let vertices: Vec<usize> = (0..g.vertices.len())
                .filter(|&i| g.vertices[i].level_id == id)
                .collect();
            let (lo, hi) = (vertices[0] as u16, *vertices.last().unwrap() as u16);
            assert_eq!(cross.game_vertex_count as usize, vertices.len(), "{name}");
            let mut named = vec![false; vertices.len()];
            for (k, &gv) in cross.game_vertices.iter().enumerate() {
                assert!((lo..=hi).contains(&gv), "{name}: cell {k}");
                named[usize::from(gv - lo)] = true;
                let d = cross.distances[k];
                assert!(
                    near(f64::from(d / 0.7), f64::from((d / 0.7).round()), 1e-3),
                    "{name}: {d}"
                );
            }
            assert!(named.iter().all(|&n| n), "{name}");
            for &i in &vertices {
                let v = &g.vertices[i];
                assert_eq!(
                    cross.cell(v.level_vertex),
                    Some((i as u16, 0.0)),
                    "{name}: {i}"
                );
                // The level point within half a cell of its own level vertex.
                let p = ai.position(v.level_vertex).unwrap();
                assert!(
                    (v.level_point[0] - p[0]).abs() <= 0.35 + 1e-4,
                    "{name}: {i}"
                );
                assert!(
                    (v.level_point[2] - p[2]).abs() <= 0.35 + 1e-4,
                    "{name}: {i}"
                );
                // Level points match their cells and decoded positions.
                for lp in g.level_points(&file, i as u16).unwrap() {
                    let (gv, d) = cross.cell(lp.level_vertex).unwrap();
                    assert_eq!((gv, d), (i as u16, lp.distance), "{name}: {i}");
                    let q = ai.position(lp.level_vertex).unwrap();
                    assert!((lp.position[0] - q[0]).abs() <= 1e-4);
                    assert!((lp.position[2] - q[2]).abs() <= 1e-4);
                    assert!((lp.position[1] - q[1]).abs() <= 1e-5);
                }
            }
            // Section 3: columns and rows within the grid, both maxima reached.
            let s = ai.header.column_count();
            let (mut max_col, mut max_row) = (0, 0);
            for &p in &ai.packed {
                assert!(p / row < s && p % row < row, "{name}");
                max_col = max_col.max(p / row);
                max_row = max_row.max(p % row);
            }
            assert_eq!((max_col, max_row), (s - 1, row - 1), "{name}");
            if name == "l01_escape" {
                assert_eq!(crc32fast::hash(&gct), 0x9c99_40ab);
                assert_eq!(crc32fast::hash(&ai_file), 0xdb52_f6d7);
                let sum: f64 = cross.distances.iter().map(|&d| f64::from(d)).sum();
                assert!(near(sum, 14_024_781.2, 1.0), "{sum}");
                let h = &ai.header;
                assert_eq!(h.box_min, [-283.5, -37.482147, -480.9]);
                assert!(
                    (0..3).all(|i| (h.box_max[i] - [411.59998, 29.764366, 706.3][i]).abs() < 1e-4)
                );
                assert_eq!((h.height_range, s), (67.247513, 994));
                assert_eq!(cross.game_vertices.iter().filter(|&&v| v == 0).count(), 256);
                assert_eq!(
                    cross.game_vertices.iter().filter(|&&v| v == 2).count(),
                    1_564
                );
                // The navigation data loads with its checks.
                let nav = Navigation::read(vfs, "gamedata\\levels\\l01_escape", &g, 1).unwrap();
                assert_eq!(nav.ai.len(), n);
            }
        }
    });
}

#[test]
fn patrol_paths() {
    with_tree(|vfs| {
        let spawn_file = read(vfs, "gamedata/spawns/all.spawn").unwrap();
        let g = GameGraph::parse(&graph_file(vfs)).unwrap();
        let top = volya_engine_formats::chunks::Chunks::parse(&spawn_file).unwrap();
        assert_eq!(top.find(3).unwrap().payload.len(), 1_070_283);
        let paths = PatrolPaths::parse(&spawn_file).unwrap();
        assert_eq!(paths.paths.len(), 4_294);
        let points: usize = paths.paths.iter().map(|p| p.points.len()).sum();
        let links: Vec<_> = paths.paths.iter().flat_map(|p| &p.links).collect();
        assert_eq!((points, links.len()), (10_367, 7_267));
        assert_eq!(links.iter().filter(|l| l.weight == 1.0).count(), 7_257);
        let unusable = paths
            .paths
            .iter()
            .filter(|p| p.points[0].level_vertex == u32::MAX && p.points[0].game_vertex == 51_048)
            .count();
        assert_eq!(unusable, 242);
        // Section 4: points whose cell names their game vertex.
        let mut crosses: BTreeMap<u8, CrossTable> = BTreeMap::new();
        let (mut valid, mut no_lv_valid, mut invalid_gv) = (0, 0, 0);
        for p in paths.paths.iter().flat_map(|p| &p.points) {
            match g.vertex(p.game_vertex) {
                None => {
                    assert_eq!((p.game_vertex, p.level_vertex), (51_048, u32::MAX));
                    invalid_gv += 1;
                }
                Some(_) if p.level_vertex == u32::MAX => no_lv_valid += 1,
                Some(v) => {
                    let name = &g.level(v.level_id).unwrap().name;
                    let cross = crosses.entry(v.level_id).or_insert_with(|| {
                        CrossTable::parse(
                            &read(vfs, &format!("gamedata/levels/{name}/level.gct")).unwrap(),
                        )
                        .unwrap()
                    });
                    assert_eq!(cross.cell(p.level_vertex).unwrap().0, p.game_vertex);
                    valid += 1;
                }
            }
        }
        assert_eq!((valid, no_lv_valid, invalid_gv), (9_778, 229, 360));
        let zabor = paths.get("mil_graph_zabor_changer_1").unwrap();
        assert_eq!(zabor.points[0].game_vertex, 1_625);
    });
}

/// The objects' locations (5.1, section 9) and the travel configuration
/// (7.2, 7.4, 7.5).
#[test]
fn object_locations_and_travel_configuration() {
    with_tree(|vfs| {
        let spawn_file = read(vfs, "gamedata/spawns/all.spawn").unwrap();
        let g = GameGraph::parse(&graph_file(vfs)).unwrap();
        let system = config::load(vfs, "gamedata/config/system.ltx").unwrap();
        let all = records(&spawn_file, &system);
        assert_eq!(all.len(), 8_641);
        let mut crosses: BTreeMap<u8, CrossTable> = BTreeMap::new();
        let mut escape = 0;
        for (index, r) in &all {
            let b = r.object.object_block();
            let v = g.vertex(b.game_graph_vertex).unwrap();
            let level = g.level(v.level_id).unwrap();
            assert!(
                LEVELS.iter().any(|l| l.1 == v.level_id),
                "{index}: {}",
                level.name
            );
            escape += usize::from(v.level_id == 1);
            let cross = crosses.entry(v.level_id).or_insert_with(|| {
                CrossTable::parse(
                    &read(vfs, &format!("gamedata/levels/{}/level.gct", level.name)).unwrap(),
                )
                .unwrap()
            });
            assert_eq!(
                cross.cell(b.level_graph_node),
                Some((b.game_graph_vertex, b.distance)),
                "{index}"
            );
            // 5.4: the Monster-like update's location fields.
            let update = match &r.object {
                volya_engine_formats::object::Object::Stalker(s) => Some(&s.update),
                volya_engine_formats::object::Object::Monster(m) => Some(&m.update),
                _ => None,
            };
            if let Some(u) = update {
                assert_eq!(
                    (
                        u.next_game_vertex,
                        u.previous_game_vertex,
                        u.distance_from_previous,
                        u.distance_to_next
                    ),
                    (0xFFFF, 0xFFFF, 0.0, 0.0)
                );
            }
        }
        assert_eq!(escape, 870);

        // Section 9's named records of l01_escape.
        let ai = LevelAi::read(BufReader::new(
            vfs.open("gamedata/levels/l01_escape/level.ai").unwrap(),
        ))
        .unwrap();
        let named: &[(u32, &str, &str, u16, u32, f32, [f32; 3])] = &[
            (
                63,
                "level_prefix_actor_0001",
                "O_ACTOR",
                4,
                11_713,
                0.7,
                [-246.4, -24.790, -134.4],
            ),
            (
                84,
                "escape_trader",
                "AI_TRD_S",
                0,
                594_270,
                0.0,
                [368.9, 15.022, -39.9],
            ),
            (
                100,
                "esc_wolf",
                "AI_STL_S",
                61,
                42_599,
                3.5,
                [-210.7, -20.059, -142.8],
            ),
            (
                107,
                "esc_heli",
                "C_HLCP_S",
                0,
                594_270,
                0.0,
                [368.9, 15.022, -39.9],
            ),
            (
                137,
                "esc_fox",
                "AI_STL_S",
                236,
                438_370,
                8.4,
                [133.7, -0.413, 315.0],
            ),
            (
                164,
                "esc_bridge_soldier1",
                "AI_STL_S",
                98,
                339_597,
                16.1,
                [53.9, 1.644, 176.4],
            ),
            (
                285,
                "exit_to_garbage_01",
                "LVL_CHNG",
                1,
                309_788,
                7.0,
                [29.4, 16.571, 686.0],
            ),
            (
                445,
                "esc_shustryi",
                "AI_STL_S",
                118,
                422_012,
                8.4,
                [119.0, -7.488, 7.7],
            ),
        ];
        for &(index, name, class, gv, lv, d, p) in named {
            let r = &all.iter().find(|(i, _)| *i == index).unwrap().1;
            let b = r.object.object_block();
            assert_eq!(
                (
                    r.header.name.as_str(),
                    r.class_id.as_str(),
                    b.game_graph_vertex,
                    b.level_graph_node
                ),
                (name, class, gv, lv)
            );
            assert!((b.distance - d).abs() < 1e-4, "{name}");
            let q = ai.position(lv).unwrap();
            assert!((0..3).all(|i| (q[i] - p[i]).abs() < 0.001), "{name}: {q:?}");
        }
        assert_eq!(all[3449].1.header.name, "exit_to_garbage_01");
        // 5.1: the trader and the helicopter keep a nominal location.
        for (_, r) in all.iter().filter(|(i, _)| [84, 107].contains(i)) {
            assert!(!uses_level_graph(r) || r.kind() == Kind::Trader);
        }

        // Stalkers and monsters (section 9).
        let creatures: Vec<&(u32, ObjectRecord)> = all
            .iter()
            .filter(|(_, r)| matches!(r.kind(), Kind::Stalker | Kind::Monster))
            .collect();
        let stalkers = creatures
            .iter()
            .filter(|(_, r)| r.kind() == Kind::Stalker)
            .count();
        assert_eq!((creatures.len(), stalkers), (1_349, 1_073));
        let mut stalker_section = 0;
        let mut dogs = 0;
        let mut snorks = 0;
        let mut unmatched = Vec::new();
        for (index, r) in &creatures {
            let s = &r.header.section;
            let get = |k: &str| system.get(s, k).flatten().map(str::trim);
            assert_eq!(
                get("smart_terrain_choose_interval"),
                Some("00:15:00"),
                "{s}"
            );
            assert_eq!(
                get("current_level_going_speed").map(config::value::float),
                Some(1.0)
            );
            let terrain = get("terrain").unwrap_or("");
            let masks = volya_engine_core::simulation::graph::terrain_masks(terrain, &system);
            if s == "stalker" {
                stalker_section += 1;
                assert_eq!(get("going_speed").map(config::value::float), Some(2.0));
                assert_eq!(terrain, "255,255,255,255,0,1");
            }
            if r.kind() == Kind::Stalker {
                assert_eq!(masks, [ANY_TERRAIN], "{s}");
            }
            dogs += usize::from(terrain == "dogs_terrain");
            if terrain == "snork_terrain" {
                snorks += 1;
                let mut m = masks.clone();
                m.sort();
                assert_eq!(m, [[10, 1, 255, 255], [255, 0, 255, 255]]);
            }
            let types = g
                .vertex(r.object.object_block().game_graph_vertex)
                .unwrap()
                .types;
            if !masks.iter().any(|&m| matches_mask(types, m)) {
                unmatched.push((*index, r.header.name.clone(), types, masks));
            }
        }
        assert_eq!((stalker_section, dogs, snorks), (717, 90, 68));
        assert_eq!(unmatched.len(), 11);
        assert!(unmatched.iter().any(|(i, n, t, m)| *i == 3362
            && n == "bar_m_dog_e_0009"
            && *t == [8, 1, 0, 0]
            && m == &[[255, 0, 255, 255]]));
        assert!(unmatched.iter().all(|(_, _, _, m)| m != &[ANY_TERRAIN]));
        // The only record with `graph_point_marker`.
        let markers: Vec<&(u32, ObjectRecord)> = all
            .iter()
            .filter(|(_, r)| {
                r.object
                    .object_block()
                    .custom_data
                    .contains("graph_point_marker")
            })
            .collect();
        assert_eq!(markers.len(), 1);
        let (index, r) = markers[0];
        assert_eq!(
            (*index, r.header.name.as_str(), r.header.section.as_str()),
            (5197, "mil_zabor", "physic_destroyable_object")
        );
        assert!(
            r.object
                .object_block()
                .custom_data
                .contains("mil_graph_zabor_changer_1")
        );

        // [alife] and [monsters_common] (section 9).
        let alife = config::load(vfs, "gamedata/config/alife.ltx").unwrap();
        let a = |k: &str| {
            alife
                .get("alife", k)
                .flatten()
                .map(str::trim)
                .unwrap()
                .to_owned()
        };
        assert_eq!(
            [
                a("switch_distance"),
                a("switch_factor"),
                a("process_time"),
                a("update_monster_factor"),
                a("objects_per_update"),
                a("schedule_min"),
                a("schedule_max"),
                a("normal_time_factor"),
            ],
            ["150", "0.1", "900", "0.1", "10", "1", "1", "10"]
        );
        let c = AlifeConfig::read(&alife, &system).unwrap();
        assert_eq!(
            (c.objects_per_update, c.stay_after_death),
            (10, 36 * 3_600_000)
        );
    });
}

/// Dijkstra's shortest distances from `start` over every edge.
fn shortest(g: &GameGraph, start: u16) -> Vec<f64> {
    let mut d = vec![f64::INFINITY; g.vertices.len()];
    let mut heap = BinaryHeap::new();
    d[usize::from(start)] = 0.0;
    heap.push(Reverse((0u64, start)));
    while let Some(Reverse((bits, v))) = heap.pop() {
        let dv = f64::from_bits(bits);
        if dv > d[usize::from(v)] {
            continue;
        }
        for e in g.edges_of(v) {
            let n = dv + f64::from(e.distance);
            if n < d[usize::from(e.target)] {
                d[usize::from(e.target)] = n;
                heap.push(Reverse((n.to_bits(), e.target)));
            }
        }
    }
    d
}

#[test]
fn path_search_facts() {
    with_tree(|vfs| {
        let graph = Graph::new(GameGraph::parse(&graph_file(vfs)).unwrap());
        let path = graph.path(225, 36, &[ANY_TERRAIN]).unwrap();
        assert_eq!(
            path,
            [
                225, 227, 195, 192, 190, 191, 185, 181, 182, 179, 177, 171, 164, 163, 84, 81, 79,
                73, 75, 72, 44, 107, 106, 104, 102, 100, 34, 36
            ]
        );
        assert!((graph.path_length(&path).unwrap() - 1_108.307).abs() < 0.002);
        assert!((shortest(&graph.graph, 225)[36] - 1_108.176).abs() < 0.002);
        let mut longer = 0;
        let mut worst = (0.0, 0, 0, 0.0, 0.0);
        for a in 0..252u16 {
            let d = shortest(&graph.graph, a);
            for b in 0..252u16 {
                let p = graph.path(a, b, &[ANY_TERRAIN]).expect("connected");
                let found = f64::from(graph.path_length(&p).unwrap());
                let best = d[usize::from(b)];
                if found > best + 1e-3 {
                    longer += 1;
                    if found - best > worst.0 {
                        worst = (found - best, a, b, found, best);
                    }
                }
            }
        }
        assert_eq!(longer, 192);
        assert_eq!((worst.1, worst.2), (232, 189));
        assert!((worst.3 - 163.872).abs() < 0.002 && (worst.4 - 163.130).abs() < 0.002);
    });
}

/// A New Game's simulation on the real data, with `l01_escape`'s navigation.
fn new_game(vfs: &dyn Vfs) -> Simulation {
    let spawn_file = read(vfs, "gamedata/spawns/all.spawn").unwrap();
    let g = GameGraph::parse(&graph_file(vfs)).unwrap();
    let system = config::load(vfs, "gamedata/config/system.ltx").unwrap();
    let alife = config::load(vfs, "gamedata/config/alife.ltx").unwrap();
    let paths = PatrolPaths::parse(&spawn_file).unwrap();
    let mut sim = Simulation::new(
        Graph::new(g.clone()),
        paths,
        AlifeConfig::read(&alife, &system).unwrap(),
    );
    // Five supplies and the bolt (player-state.md 3.1, 3.2).
    sim.new_game(&spawn_file, &system, &mut |_, _| Ok(6))
        .unwrap();
    let level = sim.current_level().unwrap();
    let name = g.level(level).unwrap().name.clone();
    sim.set_navigation(
        Navigation::read(vfs, &format!("gamedata\\levels\\{name}"), &g, level).unwrap(),
    )
    .unwrap();
    sim
}

/// The New Game time (sav.md 15) and factor.
const START: u64 = 63_471_533_400_000;

#[test]
fn new_game_registry_and_walking_across_the_switch_distance() {
    with_tree(|vfs| {
        let mut sim = new_game(vfs);
        // 8.1: ids in creation order, the actor 0, its supplies after it.
        assert_eq!(sim.len(), 8_641);
        assert_eq!(sim.current_level(), Some(1));
        let actor = sim.object(ACTOR_ID).unwrap();
        assert_eq!(actor.record.header.name, "level_prefix_actor_0001");
        assert_eq!(sim.by_name("escape_trader").unwrap().id, 84 + 6);
        assert_eq!(sim.actor_items(), 64..70);
        assert_eq!(sim.by_name("esc_wolf").unwrap().id, 100 + 6);
        assert_eq!(sim.level_list().count(), 870);
        assert!(sim.objects().all(|o| !o.online));
        // Every stalker and monster alive with direct control is scheduled.
        let scheduled = sim.schedule().count();
        assert!(scheduled > 1_000 && scheduled <= 1_349, "{scheduled}");

        // The first pass (level-objects.md 2.4, 2.6): 671 online.
        let changes = sim.update(START, 10.0, 5).unwrap();
        assert_eq!(changes.online.len(), 671);
        assert!(changes.offline.is_empty() && changes.removed.is_empty());
        assert!(sim.object(ACTOR_ID).unwrap().online);
        // The objects of other levels stay offline (6.1).
        assert!(
            sim.objects()
                .filter(|o| o.online)
                .all(|o| sim.level_list().any(|id| id == o.id))
        );
        // Nothing switches again in place.
        assert!(sim.update(START + 100, 10.0, 5).unwrap().is_empty());
        // Without smart terrains nothing travels.
        assert!(
            sim.objects()
                .all(|o| o.travel.as_ref().is_none_or(|t| t.path.is_empty()))
        );

        // Walking: `esc_novice_attacker1` (a stalker online by distance) comes
        // online within 135 m and goes offline beyond 165 m.
        let stalker = sim.by_name("esc_novice_attacker1").unwrap();
        let (stalker_id, stalker_at) = (stalker.id, stalker.record.header.position);
        assert!(!stalker.online);
        let toward = |d: f32| [stalker_at[0] - d, stalker_at[1], stalker_at[2]];
        let step = |sim: &mut Simulation, d: f32, t: u64| -> Changes {
            sim.set_actor_position(toward(d));
            sim.update(START + t, 10.0, 5).unwrap()
        };
        assert!(!step(&mut sim, 140.0, 1_000).online.contains(&stalker_id));
        assert!(step(&mut sim, 134.0, 2_000).online.contains(&stalker_id));
        assert!(sim.object(stalker_id).unwrap().online);
        assert!(!step(&mut sim, 160.0, 3_000).offline.contains(&stalker_id));
        assert!(step(&mut sim, 166.0, 4_000).offline.contains(&stalker_id));
        assert!(!sim.object(stalker_id).unwrap().online);
        // The actor's location follows it (5.2, online): the cell of its
        // level vertex holds its game vertex and distance.
        let a = sim
            .object(ACTOR_ID)
            .unwrap()
            .record
            .object
            .object_block()
            .clone();
        let nav = sim.navigation().unwrap();
        assert!(nav.ai.in_cell(a.level_graph_node, toward(166.0)));
        assert_eq!(
            nav.cross.cell(a.level_graph_node),
            Some((a.game_graph_vertex, a.distance))
        );
        // Every object visited keeps the invariant of 5.1.
        for o in sim.objects().filter(|o| o.record.header.parent_id == NO_ID) {
            let b = o.record.object.object_block();
            if sim.level_list().any(|id| id == o.id) {
                assert_eq!(
                    nav.cross.cell(b.level_graph_node),
                    Some((b.game_graph_vertex, b.distance)),
                    "{}",
                    o.record.header.name
                );
            }
        }
    });
}
