// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `level.cform`, `gamemtl.xr` and the player's collision body on the
//! Russian edition (BYOA), checked against docs/formats/level-cform.md
//! section 7. Skips when `VOLYA_GOG_INSTALLER` is unset.

// Numbers are written as the spec writes them.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use volya_engine_core::collision::body::{
    BodyConfig, PlayerBody, StepInput, WALKABLE_NORMAL_Y, drive_direction,
};
use volya_engine_core::collision::geometry::{cross, dot, front_normal, length, sub};
use volya_engine_core::collision::{Cform, CollisionWorld, map_materials};
use volya_engine_core::config;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::gamemtl::{self, ACTOR_OBSTACLE, DYNAMIC, PASS_THROUGH};
use volya_engine_formats::level_cform::{BIT_14, BIT_15};
use volya_engine_formats::spawn;

fn with_tree(test: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    test(&tree);
}

fn materials(vfs: &dyn Vfs) -> Vec<gamemtl::GameMaterial> {
    gamemtl::parse(&read(vfs, "gamedata/gamemtl.xr").unwrap()).unwrap()
}

#[test]
fn game_materials() {
    with_tree(|vfs| {
        let file = read(vfs, "gamedata/gamemtl.xr").unwrap();
        assert_eq!(file.len(), 310_494);
        let top: Vec<(u32, usize)> = Chunks::parse(&file)
            .unwrap()
            .iter()
            .map(|c| (c.id(), c.payload.len()))
            .collect();
        assert_eq!(
            top,
            [(0x1000, 2), (0x1001, 8), (0x1002, 8_990), (0x1003, 301_462)]
        );
        let pairs = Chunks::parse(Chunks::parse(&file).unwrap().find(0x1003).unwrap().payload)
            .unwrap()
            .len();
        assert_eq!(pairs, 1_247);
        let m = gamemtl::parse(&file).unwrap();
        assert_eq!(m.len(), 57);
        let ids: BTreeSet<u32> = m.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), 57);
        assert_eq!((ids.first(), ids.last()), (Some(&0), Some(&93)));
        let count = |flag| m.iter().filter(|m| m.has(flag)).count();
        assert_eq!(count(PASS_THROUGH), 4);
        assert_eq!(count(DYNAMIC), 29);
        assert_eq!(count(ACTOR_OBSTACLE), 1);
        assert_eq!(count(0x4), 20);
        assert_eq!(
            m.iter().filter(|m| m.has(0x4) && !m.has(DYNAMIC)).count(),
            12
        );
        assert_eq!(count(0x1000_0000), 2);
        assert_eq!(count(0x400), 4);
        assert_eq!(count(0x800), 5);
        assert_eq!(
            (m[0].id, m[0].name.as_str(), m[0].flags),
            (0, "default", 0x6000_0010)
        );
        assert_eq!(
            (
                m[0].friction,
                m[0].damping,
                m[0].spring,
                m[0].bounce_start_speed,
                m[0].bounce
            ),
            (1.0, 1.0, 1.0, 1.5, 1.0)
        );
        let fake = &m[45];
        assert_eq!(
            (fake.id, fake.name.as_str(), fake.flags),
            (80, "materials\\fake", 0x1C80)
        );
        assert_eq!(
            [
                fake.friction,
                fake.damping,
                fake.spring,
                fake.bounce_start_speed,
                fake.bounce
            ],
            [1.0, 1.0, 0.7, 0.0, 0.01]
        );
        let death = &m[56];
        assert_eq!(
            (death.id, death.name.as_str(), death.flags),
            (93, "materials\\earth_death", 0x7000_0018)
        );
        assert_eq!((death.impact_factor(), death.damage_rate), (100.0, 150.0));
        let by_name = |n: &str| m.iter().find(|m| m.name == n).unwrap();
        let water = by_name("materials\\water");
        assert_eq!(
            (water.id, water.flags, water.liquid_factor),
            (11, 0x8000_0082, 0.8)
        );
        assert_eq!(by_name("materials\\death").damage_rate, 30.0);
        assert_eq!(by_name("creatures\\actor").friction, 0.8);
        assert_eq!(
            m.iter().position(|m| m.name == "materials\\cloth"),
            Some(46)
        );
        assert_eq!(m.iter().position(|m| m.id == 39), Some(22));
    });
}

/// Area and front normal y of a triangle, in double precision.
fn area(c: &[[f32; 3]; 3]) -> (f64, f64) {
    let d = |v: [f32; 3]| v.map(f64::from);
    let (a, b, c) = (d(c[0]), d(c[1]), d(c[2]));
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
    let x = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    (0.5 * l, if l > 0.0 { x[1] / l } else { 0.0 })
}

#[track_caller]
fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} is not within {tolerance} of {b}"
    );
}

#[test]
fn escape_collision_mesh() {
    with_tree(|vfs| {
        let file = read(vfs, "gamedata/levels/l01_escape/level.cform").unwrap();
        assert_eq!(file.len(), 30_383_712);
        let start = Instant::now();
        let cform = Cform::parse(&file).unwrap();
        let parsed = start.elapsed();
        assert_eq!(
            (cform.vertices.len(), cform.triangles.len()),
            (698_013, 1_375_470)
        );
        assert_eq!(cform.bbox_min, [-678.897_03, -38.667_225, -633.791_08]);
        assert_eq!(cform.bbox_max, [766.565, 113.876_54, 811.671_08]);
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut sum = [0f64; 3];
        for v in &cform.vertices {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
                sum[k] += f64::from(v[k]);
            }
        }
        assert_eq!((min, max), (cform.bbox_min, cform.bbox_max));
        near(sum[0], -27_658_748.514_199, 1e-3);
        near(sum[1], -464_806.416_943, 1e-3);
        near(sum[2], 14_863_741.775_122, 1e-3);
        let indices: u64 = cform
            .triangles
            .iter()
            .flat_map(|t| t.vertices)
            .map(u64::from)
            .sum();
        assert_eq!(indices, 1_505_709_882_650);
        let mut used = vec![false; cform.vertices.len()];
        for t in &cform.triangles {
            for i in t.vertices {
                used[i as usize] = true;
            }
        }
        assert!(used.iter().all(|&u| u));
        assert!(
            cform
                .triangles
                .iter()
                .all(|t| t.attributes & 0xC000 == 0x8000)
        );
        let mut per_id = BTreeMap::new();
        let mut per_sector = BTreeMap::new();
        for t in &cform.triangles {
            *per_id.entry(t.material()).or_insert(0) += 1;
            *per_sector.entry(t.sector()).or_insert(0) += 1;
        }
        assert_eq!(
            per_id.into_iter().collect::<Vec<_>>(),
            [
                (0, 1_290),
                (2, 10_514),
                (3, 62_749),
                (4, 1_963),
                (5, 85_042),
                (6, 206),
                (7, 50_962),
                (8, 11_958),
                (10, 210),
                (12, 172),
                (13, 152_733),
                (18, 42_800),
                (19, 114_589),
                (34, 4_483),
                (37, 114_080),
                (39, 579_315),
                (48, 22_544),
                (49, 6_765),
                (53, 10_667),
                (60, 31_359),
                (80, 1_516),
                (81, 69_553)
            ]
        );
        assert_eq!(per_sector.len(), 24);
        assert_eq!(per_sector.last_key_value(), Some((&23, &1_302_201)));
        assert_eq!(per_sector[&0], 2_704);
        assert_eq!(
            cform.triangles.iter().filter(|t| t.is_degenerate()).count(),
            4
        );
        let mut total = 0.0;
        let (mut up, mut up_area) = (0, 0.0);
        for t in &cform.triangles {
            let c = t.vertices.map(|i| cform.vertices[i as usize]);
            let (a, ny) = area(&c);
            total += a;
            if ny > 0.707_106_78 {
                up += 1;
                up_area += a;
            }
        }
        near(total, 2_349_844.74, 0.05);
        assert_eq!(up, 227_721);
        near(up_area, 1_150_394.56, 0.05);

        let lib = materials(vfs);
        // Section 3.3's loaded attribute words, from the parse (the world
        // keeps only the material index).
        let mut mapped = cform.triangles.clone();
        map_materials(&mut mapped, &lib).unwrap();
        assert_eq!(
            mapped.iter().filter(|t| t.attributes & BIT_14 != 0).count(),
            115_596
        );
        assert_eq!(
            mapped.iter().filter(|t| t.attributes & BIT_15 != 0).count(),
            138_140
        );
        assert!(
            mapped
                .iter()
                .zip(&cform.triangles)
                .all(|(m, t)| m.sector() == t.sector())
        );
        drop((mapped, cform));
        // The engine's path: streamed from the file, mapped and indexed.
        let start = Instant::now();
        let f = vfs.open("gamedata/levels/l01_escape/level.cform").unwrap();
        let world = CollisionWorld::read(f, lib).unwrap();
        let loaded = start.elapsed();
        eprintln!(
            "l01_escape: parsed from memory in {parsed:.1?}; read, mapped and indexed in \
             {loaded:.1?}, {:.1} MB",
            world.memory_bytes() as f64 / 1e6
        );
        let pass = (0..world.triangle_count() as u32).filter(|&t| !world.is_solid(t));
        assert_eq!(pass.clone().count(), 115_596);
        assert_eq!(
            pass.filter(|&t| world.material(t).has(ACTOR_OBSTACLE))
                .count(),
            1_516
        );
        // Section 3.3's examples: ids and indices differ.
        let index_of = |id| world.materials().iter().position(|m| m.id == id);
        assert_eq!(
            (index_of(81), index_of(80), index_of(39)),
            (Some(46), Some(45), Some(22))
        );
        start_position(vfs, &world);
    });
}

/// Section 7.1's start position: the triangles on the vertical line, and the
/// body standing on walkable ground there.
fn start_position(vfs: &dyn Vfs, world: &CollisionWorld) {
    let actor = spawn::actor_start(&read(vfs, "gamedata/spawns/all.spawn").unwrap()).unwrap();
    let p = actor.position;
    assert_eq!(p, [-246.726_3, -24.793_261, -134.433_87]);
    let mut found = Vec::new();
    world.query([p[0], -1e4, p[2]], [p[0], 1e4, p[2]], &mut found);
    let mut crossings = Vec::new();
    for t in found {
        let c = world.corners(t);
        let Some(n) = front_normal(&c) else { continue };
        // The line meets the triangle where its xz projection holds p.
        let side = |a: [f32; 3], b: [f32; 3]| {
            (b[0] - a[0]) * (p[2] - a[2]) - (b[2] - a[2]) * (p[0] - a[0])
        };
        let s = [side(c[0], c[1]), side(c[1], c[2]), side(c[2], c[0])];
        if !(s.iter().all(|&x| x >= 0.0) || s.iter().all(|&x| x <= 0.0)) || n[1] == 0.0 {
            continue;
        }
        let y = c[0][1] - (n[0] * (p[0] - c[0][0]) + n[2] * (p[2] - c[0][2])) / n[1];
        crossings.push((y - p[1], t, n));
    }
    crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(crossings.len(), 7, "{crossings:?}");
    assert!(crossings.iter().all(|c| c.0 > 0.0));
    let (dy, t, n) = crossings[0];
    assert_eq!(t, 424_895);
    assert_eq!(world.triangle(t), [250_888, 247_104, 247_153]);
    assert!((dy - 0.000_13).abs() < 0.000_01, "{dy}");
    assert!(
        n[1] > WALKABLE_NORMAL_Y && (n[0] + 0.004_37).abs() < 1e-4,
        "{n:?}"
    );
    let floor = world.material(t);
    assert_eq!(
        (floor.id, floor.name.as_str()),
        (12, "materials\\flooring_tile")
    );
    assert_eq!(world.material_index(t), 11);
    assert!(world.is_solid(t));
    let (dy, t, n) = crossings[1];
    assert!((dy - 3.177_11).abs() < 1e-4 && (n[1] + 0.993_41).abs() < 1e-4);
    assert_eq!(world.material(t).name, "materials\\concrete");
    let rest: Vec<(f32, &str)> = crossings[2..]
        .iter()
        .map(|c| (c.0, world.material(c.1).name.as_str()))
        .collect();
    assert!((rest[0].0 - 5.2036).abs() < 1e-3 && rest[0].1 == "materials\\earth");
    assert!((rest[1].0 - 10.5715).abs() < 1e-3 && (rest[3].0 - 17.1344).abs() < 1e-3);
    assert!(rest[1..].iter().all(|r| r.1 == "materials\\bush"));
    // The body there: standing on the floor tile, walkable, overlapping
    // it by about 0.00013 m, the ceiling clear of the head.
    let system = config::load(vfs, "gamedata/config/system.ltx").unwrap();
    let body_config =
        BodyConfig::from_section(system.section(spawn::ACTOR_SECTION).unwrap()).unwrap();
    assert_eq!(body_config.mass, 80.0);
    let mut body = PlayerBody::new(body_config, p);
    let contacts = body.begin_step(world, &StepInput::default());
    assert!(contacts.walkable, "{contacts:?}");
    let ground = body
        .contacts()
        .iter()
        .max_by(|a, b| a.normal[1].total_cmp(&b.normal[1]))
        .unwrap();
    assert_eq!(ground.triangle, 424_895);
    assert!(
        (ground.depth - 0.000_13).abs() < 0.000_02,
        "{}",
        ground.depth
    );
    assert!(
        body.contacts().iter().all(|c| c.normal[1] > 0.0),
        "{:?}",
        body.contacts()
    );
    let tile = world.corners(424_895);
    let n = cross(sub(tile[1], tile[0]), sub(tile[2], tile[1]));
    assert!(dot(n, ground.normal) / length(n) > 0.9999);
    assert_eq!(body.ground_material(), Some(11));
    body.finish_step([0.0; 3], false);

    // Steps at rest and walking, timed. Gravity and the drive are
    // stand-ins for player-movement's values, for this test only.
    let (gravity, drive) = (9.8, 20.0);
    let mass = body.config.mass;
    let start = Instant::now();
    let steps = 300;
    for i in 0..steps {
        let a = if i < 100 { [0.0; 3] } else { [0.0, 0.0, 1.0] };
        let input = StepInput {
            acceleration: a,
            ..StepInput::default()
        };
        body.step(world, &input, |c| {
            let d = drive_direction(c);
            let f = if c.moving {
                drive * mass * c.traction
            } else {
                0.0
            };
            ([d[0] * f, d[1] * f - gravity * mass, d[2] * f], false)
        });
        if i == 99 {
            let feet = body.position();
            assert!((feet[1] - p[1]).abs() < 0.01, "{feet:?} after resting");
            assert!(!body.airborne());
        }
    }
    eprintln!(
        "l01_escape: {:.1?} per body step, {} contacts at the end",
        start.elapsed() / steps,
        body.contacts().len()
    );
}

#[test]
fn every_level_mesh() {
    with_tree(|vfs| {
        let lib = materials(vfs);
        let levels: Vec<String> = vfs
            .read_dir("gamedata/levels")
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == EntryKind::Dir)
            .map(|e| e.name)
            .filter(|n| {
                vfs.open(&format!("gamedata/levels/{n}/level.cform"))
                    .is_ok()
            })
            .collect();
        // The 30 shipped levels of section 7.3; others (a mod's) are
        // checked for the same invariants.
        let shipped = |n: &str| n.starts_with('l') || n.starts_with("testers_mp_");
        assert_eq!(
            levels.iter().filter(|n| shipped(n)).count(),
            30,
            "{levels:?}"
        );
        let mut ids = BTreeSet::new();
        let mut sizes = Vec::new();
        for name in &levels {
            let dir = format!("gamedata/levels/{name}");
            let cform = Cform::parse(&read(vfs, &format!("{dir}/level.cform")).unwrap()).unwrap();
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for v in &cform.vertices {
                for k in 0..3 {
                    min[k] = min[k].min(v[k]);
                    max[k] = max[k].max(v[k]);
                }
            }
            assert_eq!((min, max), (cform.bbox_min, cform.bbox_max), "{name}");
            let mut used = vec![false; cform.vertices.len()];
            for t in &cform.triangles {
                for i in t.vertices {
                    used[i as usize] = true;
                }
                if shipped(name) {
                    ids.insert(t.material());
                }
            }
            assert!(used.iter().all(|&u| u), "{name}: unused vertices");
            let bits = cform.triangles[0].attributes & 0xC000;
            assert!(
                cform
                    .triangles
                    .iter()
                    .all(|t| t.attributes & 0xC000 == bits),
                "{name}: bits 14, 15 vary"
            );
            let sectors = cform.triangles.iter().map(|t| t.sector()).max().unwrap() + 1;
            let level = read(vfs, &format!("{dir}/level")).unwrap();
            let level = Chunks::parse(&level).unwrap();
            let chunk8 = Chunks::parse(level.find(8).unwrap().payload).unwrap();
            assert_eq!(sectors as usize, chunk8.len(), "{name}: sectors");
            if shipped(name) {
                sizes.push((cform.triangles.len(), cform.vertices.len(), name.clone()));
            }
            // Every id exists and is not dynamic: the mapping succeeds.
            CollisionWorld::new(cform, lib.clone()).unwrap();
        }
        assert_eq!(
            ids.into_iter().collect::<Vec<_>>(),
            [
                0, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 15, 16, 18, 19, 34, 37, 39, 48, 49, 53, 60,
                80, 81, 88, 93
            ]
        );
        sizes.sort();
        assert_eq!(sizes[0], (41_467, 21_564, "testers_mp_pool".into()));
        assert_eq!(sizes[29], (2_043_680, 1_091_297, "l11_pripyat".into()));
    });
}
