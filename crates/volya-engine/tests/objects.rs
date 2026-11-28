// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's spawned objects on New Game from the Russian edition (BYOA):
//! the verification facts of docs/behavior/offline-simulation.md 9.1 (the
//! online objects per class id), docs/behavior/object-visual-and-transform.md
//! 5 (named objects' visuals and transforms) and docs/formats/skeleton-ogf.md
//! 7 (their meshes, the invariants over every model file the records
//! reference).
//! Their drawing (the start view with and without them, an offline stalker
//! drawn as the player walks) is the render-level suite's
//! (`selftest::objects`). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::time::Instant;

use volya_engine::characters::Characters;
use volya_engine::core::character_identity::{model_name, trader_like};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, read};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::objects::{Instance, LightKind, locate_model};
use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::model::{Links, MeshType, Model, ModelType, transform_point};
use volya_engine_formats::object::{self, ObjectRecord};
use volya_engine_formats::spawn;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn with_game(test: impl FnOnce(&dyn Vfs, &GameData)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    test(&tree, &data);
}

#[track_caller]
fn near3(got: [f32; 3], want: [f32; 3], tolerance: f32, what: &str) {
    assert!(
        (0..3).all(|i| (got[i] - want[i]).abs() <= tolerance),
        "{what}: {got:?} != {want:?}"
    );
}

/// offline-simulation.md 9.1: online objects per class id (the classes with any).
const ONLINE: &[(&str, usize)] = &[
    ("AI_CROW", 55),
    ("AI_STL_S", 32),
    ("AI_TRD_S", 1),
    ("AMMO", 1),
    ("ARTEFACT", 1),
    ("C_HLCP_S", 1),
    ("II_BOTTL", 3),
    ("II_FOOD", 2),
    ("LVL_CHNG", 1),
    ("O_ACTOR", 1),
    ("O_BRKBL", 49),
    ("O_CLMBL", 14),
    ("O_HLAMP", 5),
    ("O_INVBOX", 5),
    ("O_PHYS_S", 69),
    ("P_DSTRBL", 127),
    ("RE_SPAWN", 9),
    ("SMRTTRRN", 41),
    ("SM_FLESH", 1),
    ("SPC_RS_S", 75),
    ("ZS_MBALD", 96),
    ("ZS_MINCE", 4),
    ("Z_MBALD", 60),
    ("Z_RADIO", 18),
];

/// The world box of `instance`'s model vertices, unquantized (object-visual-and-transform.md 5.2).
fn world_box(model: &Model, instance: &Instance) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in model.meshes.iter().flat_map(|m| &m.vertices) {
        let p = transform_point(&instance.transform, v.position);
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    (min, max)
}

#[test]
fn level_objects_from_game_data() {
    with_game(|tree, data| {
        let start = Instant::now();
        let mut screen = LevelScreen::load(tree, data, 0).unwrap();
        eprintln!("level loaded in {:.1?}", start.elapsed());
        let objects = &screen.world().objects;
        let by_class: BTreeMap<&str, usize> = objects
            .online_by_class
            .iter()
            .map(|(c, n)| (c.as_str(), *n))
            .collect();
        assert_eq!(
            by_class,
            ONLINE.iter().copied().collect(),
            "online per class id"
        );
        assert_eq!(objects.online, 671);

        // Online objects with a model, without the actor: 352. The others
        // loaded may come online as the player moves.
        let instances: Vec<&Instance> = objects.instances.iter().filter(|i| i.online).collect();
        assert_eq!(instances.len(), 352);
        assert!(objects.instances.len() > 352);
        let count = |class: &str| instances.iter().filter(|i| i.class_id == class).count();
        assert_eq!((count("AI_STL_S"), count("AI_TRD_S")), (32, 1));
        // The other 319: 101 model files, 163,064 vertices and 1,012,059
        // indices of every detail window counted once per object.
        let others: Vec<&Instance> = instances
            .iter()
            .copied()
            .filter(|i| !matches!(i.class_id.as_str(), "AI_STL_S" | "AI_TRD_S"))
            .collect();
        assert_eq!(others.len(), 319);
        let files: BTreeSet<usize> = others.iter().map(|i| i.model).collect();
        assert_eq!(files.len(), 101);
        let (vertices, indices) = others.iter().fold((0, 0), |(v, i), o| {
            let m = &objects.models[o.model];
            (
                v + m.meshes.iter().map(|m| m.vertex_count).sum::<u32>(),
                i + m.meshes.iter().map(|m| m.index_count).sum::<u32>(),
            )
        });
        assert_eq!((vertices, indices), (163_064, 1_012_059));
        assert_eq!(count("AI_CROW"), 55);
        assert_eq!(count("SM_FLESH"), 1);
        assert_eq!(count("O_HLAMP"), 5);

        let named = |name: &str| {
            instances
                .iter()
                .find(|i| i.name == name)
                .unwrap_or_else(|| panic!("{name} not drawn"))
        };
        let model_of = |i: &Instance| {
            let path = objects.names.get(objects.models[i.model].path);
            (
                path.to_owned(),
                Model::parse(&read(tree, path).unwrap()).unwrap(),
            )
        };

        // A stalker shows its supplies choice's visual, one of those its
        // profile offers (object-visual-and-transform.md 2 item 3, 5.1).
        let sim = screen.simulation().unwrap();
        let mut characters = Characters::new(tree, data);
        let novice: &[&str] = &[
            "actors\\novice\\green_stalker_2",
            "actors\\novice\\green_stalker_3",
            "actors\\novice\\green_stalker_4",
            "actors\\novice\\green_stalker_5",
        ];
        let offered: [(&str, &[&str]); 5] = [
            ("esc_lager1", novice),
            ("esc_lager2", novice),
            ("esc_specnaz1", &["actors\\soldier\\soldier_spetsnaz"]),
            (
                "esc_blokpost_commander",
                &["actors\\soldier\\soldier_beret_1"],
            ),
            (
                "esc_stalker_corps1_0001",
                &[
                    "actors\\neytral\\stalker_neytral_balon_8",
                    "actors\\neytral\\stalker_neytral_hood_2",
                    "actors\\neytral\\stalker_neytral_hood_uzor",
                ],
            ),
        ];
        for (name, offered) in offered {
            let record = &sim.by_name(name).unwrap().record;
            let id = trader_like(record).unwrap().specific_character.clone();
            let visual = model_name(&characters.specific_character(&id).unwrap().visual);
            let drawn = named(name).visual.to_lowercase();
            assert_eq!(drawn, visual, "{name}");
            assert!(offered.contains(&drawn.as_str()), "{name}: {drawn}");
        }
        for name in ["esc_wolf", "esc_trup_scene", "esc_suicidal_flesh"] {
            named(name);
        }
        for name in ["esc_fox", "esc_killer1", "esc_stalker_fanat"] {
            assert!(instances.iter().all(|i| i.name != name), "{name} drawn");
        }

        // Object 79, the trader's door.
        let door = named("trader_door");
        assert_eq!(door.index, 79);
        assert_eq!(door.visual, "physics\\door\\door_trader");
        let (path, m) = model_of(door);
        assert!(
            path.to_lowercase()
                .ends_with("meshes\\physics\\door\\door_trader.ogf"),
            "{path}"
        );
        assert_eq!(read(tree, &path).unwrap().len(), 96_656);
        near3(
            [
                door.transform[0][0],
                door.transform[0][1],
                door.transform[0][2],
            ],
            [-0.000_017, 0.0, -1.0],
            1e-6,
            "R[0]",
        );
        near3(
            [
                door.transform[2][0],
                door.transform[2][1],
                door.transform[2][2],
            ],
            [1.0, 0.0, -0.000_017],
            1e-6,
            "R[2]",
        );
        assert_eq!((m.kind, m.bones.len()), (ModelType::Skeleton, 3));
        let names: Vec<&str> = m.bones.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["link", "door", "lock"]);
        let pose = m.bind_pose().unwrap();
        for (b, want) in pose.iter().zip([
            [0.0, 1.9743, 0.0],
            [0.0, 1.3675, 0.0],
            [1.3061, 0.7045, 0.0],
        ]) {
            near3(
                [b[3][0], b[3][1], b[3][2]],
                want,
                1e-4,
                "door bind position",
            );
        }
        let mesh = &m.meshes[0];
        assert_eq!(m.meshes.len(), 1);
        assert_eq!(
            (mesh.kind, mesh.links),
            (MeshType::SkinnedProgressive, Links::One)
        );
        assert_eq!((mesh.vertices.len(), mesh.indices.len()), (1_177, 12_285));
        assert_eq!(mesh.windows.as_ref().unwrap().steps.len(), 19);
        assert_eq!(
            (mesh.texture.as_str(), mesh.shader.as_str()),
            ("door\\door_trader", "models\\weapons")
        );
        let sum = |f: &dyn Fn(&volya_engine_formats::model::SkinnedVertex) -> [f32; 3]| {
            mesh.vertices.iter().fold([0f64; 3], |s, v| {
                let p = f(v);
                [
                    s[0] + f64::from(p[0]),
                    s[1] + f64::from(p[1]),
                    s[2] + f64::from(p[2]),
                ]
            })
        };
        let sp = sum(&|v| v.position);
        let sn = sum(&|v| v.normal);
        let st = sum(&|v| [v.uv[0], v.uv[1], 0.0]);
        let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-2);
        assert!(close(sp, [1078.4926, 1716.3511, 25.1468]), "{sp:?}");
        assert!(close(sn, [42.6894, -8.6118, 30.0575]), "{sn:?}");
        assert!(close(st, [583.1458, 295.4061, 0.0]), "{st:?}");
        let (min, max) = world_box(&m, door);
        near3(min, [-248.4411, -24.6881, -135.1301], 1e-3, "door box min");
        near3(max, [-248.0823, -21.9532, -133.6777], 1e-3, "door box max");

        // Object 89, a lamp near the start.
        let lamp = named("trader_light_0003");
        assert_eq!(lamp.visual, "physics\\light\\lampa_01_on");
        let (path, m) = model_of(lamp);
        assert_eq!(read(tree, &path).unwrap().len(), 54_863);
        assert_eq!((m.bones.len(), m.meshes.len()), (7, 2));
        let meshes: Vec<_> = m
            .meshes
            .iter()
            .map(|k| {
                (
                    k.kind,
                    k.vertices.len(),
                    k.indices.len(),
                    k.texture.as_str(),
                    k.shader.as_str(),
                )
            })
            .collect();
        assert_eq!(
            meshes,
            [
                (
                    MeshType::Skinned,
                    699,
                    2_268,
                    "prop\\prop_iten4",
                    "models\\model"
                ),
                (
                    MeshType::Skinned,
                    85,
                    288,
                    "prop\\prop_iten4",
                    "models\\selflight"
                ),
            ]
        );
        let (min, max) = world_box(&m, lamp);
        near3(min, [-245.6193, -23.9447, -134.7003], 1e-3, "lamp box min");
        near3(max, [-245.3585, -23.5131, -134.4912], 1e-3, "lamp box max");

        // Object 0, a breakable in the level directory.
        let brkbl = named("meshes\\brkbl#0.ogf");
        assert_eq!(brkbl.index, 0);
        let (path, m) = model_of(brkbl);
        assert!(
            path.to_lowercase()
                .ends_with("levels\\l01_escape\\meshes\\brkbl#0.ogf"),
            "{path}"
        );
        assert_eq!(read(tree, &path).unwrap().len(), 11_622);
        for (r, want) in [
            [-0.033_655, 0.220_967, -0.974_700],
            [-0.556_858, 0.805_701, 0.201_881],
            [0.829_926, 0.549_564, 0.095_932],
        ]
        .iter()
        .enumerate()
        {
            let t = brkbl.transform[r];
            near3([t[0], t[1], t[2]], *want, 2e-6, "brkbl#0 rotation");
        }
        assert_eq!((m.bones.len(), m.meshes.len()), (9, 1));
        let k = &m.meshes[0];
        assert_eq!(
            (
                k.kind,
                k.vertices.len(),
                k.indices.len(),
                k.texture.as_str(),
                k.shader.as_str()
            ),
            (
                MeshType::Skinned,
                134,
                282,
                "glas\\glas_dirt",
                "models\\window"
            )
        );
        let (min, max) = world_box(&m, brkbl);
        near3(min, [-79.5802, -3.2542, -77.1837], 1e-3, "brkbl box min");
        near3(max, [-79.4313, -3.0329, -76.6842], 1e-3, "brkbl box max");

        // Object 658, an inventory box.
        let cache = named("level_prefix_inventory_box_0003");
        assert_eq!(cache.index, 658);
        assert_eq!(
            cache.visual,
            "physics\\equipment_cache\\equipment_box_01_case"
        );
        let (path, m) = model_of(cache);
        assert_eq!(read(tree, &path).unwrap().len(), 2_183);
        let k = &m.meshes[0];
        assert_eq!(
            (
                m.bones.len(),
                k.vertices.len(),
                k.indices.len(),
                k.texture.as_str(),
                k.shader.as_str()
            ),
            (1, 24, 36, "glow\\glow_ghost", "models\\model_aref")
        );
        let (min, max) = world_box(&m, cache);
        near3(min, [-248.0426, -24.7945, -136.1114], 1e-3, "box min");
        near3(max, [-246.9847, -24.3651, -135.2594], 1e-3, "box max");

        // Object 84, the trader: its specific character's visual.
        let trader = named("escape_trader");
        assert_eq!(
            (trader.index, trader.visual.as_str()),
            (84, "actors\\trader\\trader")
        );
        let (path, m) = model_of(trader);
        assert_eq!(read(tree, &path).unwrap().len(), 417_583);
        assert_eq!((m.kind, m.bones.len()), (ModelType::AnimatedSkeleton, 55));
        assert_eq!(
            m.bones[m.bones.iter().position(|b| b.parent.is_none()).unwrap()].name,
            "bip01_pelvis"
        );
        let k = &m.meshes[0];
        assert_eq!(
            (
                k.kind,
                k.links,
                k.vertices.len(),
                k.indices.len(),
                k.windows.as_ref().unwrap().steps.len()
            ),
            (
                MeshType::SkinnedProgressive,
                Links::Two,
                3_338,
                87_048,
                1_362
            )
        );
        assert_eq!(
            (k.texture.as_str(), k.shader.as_str()),
            ("act\\act_barman", "models\\model")
        );
        let weights: f64 = k.vertices.iter().map(|v| f64::from(v.weight)).sum();
        assert!((weights - 670.5939).abs() < 1e-2, "{weights}");
        let pose = m.bind_pose().unwrap();
        for (bone, want) in [
            ("bip01_pelvis", [0.0, 0.8264, -0.1119]),
            ("bip01_head", [-0.0001, 1.5192, -0.0752]),
        ] {
            let b = pose[m.bone(bone).unwrap()];
            near3([b[3][0], b[3][1], b[3][2]], want, 1e-4, bone);
        }
        let (min, max) = world_box(&m, trader);
        near3(min, [-244.821, -24.9555, -134.8202], 1e-3, "trader box min");
        near3(
            max,
            [-244.4099, -23.2295, -133.1037],
            1e-3,
            "trader box max",
        );

        // Object 107, the helicopter.
        let heli = named("esc_heli");
        assert_eq!(
            (heli.index, heli.visual.as_str()),
            (107, "physics\\vehicles\\mi24\\veh_mi24_u_01")
        );
        let (_, m) = model_of(heli);
        assert_eq!((m.kind, m.bones.len()), (ModelType::AnimatedSkeleton, 29));
        let meshes: Vec<_> = m
            .meshes
            .iter()
            .map(|k| {
                (
                    k.kind,
                    k.links,
                    k.vertices.len(),
                    k.indices.len(),
                    k.shader.as_str(),
                )
            })
            .collect();
        let p = (MeshType::SkinnedProgressive, Links::One);
        assert_eq!(
            meshes,
            [
                (p.0, p.1, 9_833, 270_786, "models\\weapons"),
                (p.0, p.1, 92, 192, "models\\transparent"),
                (p.0, p.1, 418, 5_340, "models\\transparent"),
            ]
        );
        let (min, max) = world_box(&m, heli);
        near3(min, [-329.1732, 16.4293, 75.0037], 1e-3, "heli box min");
        near3(max, [-308.898, 22.1571, 91.5756], 1e-3, "heli box max");

        // hanging-lamps.md 9.3: the five lamps online, all spot lights without shadows.
        let lamps: BTreeMap<&str, (LightKind, bool, bool, bool)> = objects
            .lamps
            .iter()
            .map(|l| {
                (
                    l.name.as_str(),
                    (l.main.kind, l.main.shadows, l.ambient.is_some(), l.on),
                )
            })
            .collect();
        let spot = |ambient| (LightKind::Spot, false, ambient, true);
        assert_eq!(
            lamps,
            [
                ("trader_light_0000", spot(false)),
                ("trader_light_0003", spot(false)),
                ("trader_light_0005", spot(false)),
                ("trader_tv_0000", spot(true)),
                ("trader_tv_0002", spot(true)),
            ]
            .into_iter()
            .collect()
        );
        let l = objects
            .lamps
            .iter()
            .find(|l| l.name == "trader_light_0003")
            .unwrap();
        assert_eq!(
            (l.main.range, l.main.colour_animator.as_str()),
            (2.0, "light 40 hz")
        );
        near3(l.main.colour, [0.8; 3], 1e-6, "lamp colour");
        assert_eq!(l.glow.as_ref().unwrap().texture, "glow\\glow_orange");
        assert!(
            objects
                .lamps
                .iter()
                .find(|l| l.name == "trader_tv_0002")
                .unwrap()
                .physics_shell
        );

        let report = objects.report();
        eprintln!(
            "objects: {report:?}; vertex data {:.2} MiB; not drawn: {:?}",
            (report.vertices * 16 + report.indices * 2) as f64 / (1 << 20) as f64,
            objects.not_drawn
        );
        assert!(
            objects
                .not_drawn
                .iter()
                .all(|(e, _)| e.starts_with("models\\x") || e.starts_with("models\\pautina")),
            "{:?}",
            objects.not_drawn
        );

        // Walking (offline-simulation.md 6.2): the stalker
        // `esc_novice_attacker1`, loaded but offline, comes online once
        // the player is within 135 m (its drawing is the render-level
        // suite's).
        drop(sim);
        let sim = screen.simulation().unwrap();
        let o = sim.by_name("esc_novice_attacker1").unwrap();
        let (id, at) = (o.id, o.record.header.position);
        drop(sim);
        let k = objects.instances.iter().position(|i| i.id == id).unwrap();
        assert!(!objects.instances[k].online);
        let report = screen.state().objects;
        assert_eq!((report.online, report.with_model), (671, 352));
        screen.set_camera([at[0] - 130.0, at[1] + 1.6, at[2]], Default::default());
        screen.update(16, &Default::default());
        let report = screen.state().objects;
        assert!(report.with_model > 352, "{report:?}");
    });
}

/// skeleton-ogf.md 7.1: every model file the records of all levels name.
#[test]
fn referenced_model_files_from_game_data() {
    with_game(|tree, data| {
        let spawn_file = read(
            tree,
            &data.aliases.path("$game_spawn$", "all.spawn").unwrap(),
        )
        .unwrap();
        let graph = GameGraph::parse(
            &read(
                tree,
                &data.aliases.path("$game_data$", "game.graph").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let mut files: BTreeMap<String, usize> = BTreeMap::new();
        let (mut records, mut in_level) = (0, 0);
        spawn::for_each_record(&spawn_file, |r| {
            let section = object::section(r.spawn)?;
            let class = data
                .system
                .get(&section, "class")
                .flatten()
                .unwrap()
                .trim()
                .to_owned();
            let record = ObjectRecord::parse(r.spawn, r.update, &class, true)?;
            let Some(visual) = record.object.visual().filter(|v| !v.model.is_empty()) else {
                return Ok(ControlFlow::Continue(()));
            };
            records += 1;
            let level = graph.vertex_level(record.object.object_block().game_graph_vertex)?;
            let dir = data.aliases.path("$game_levels$", &level.name)?;
            let path = locate_model(tree, data, &dir, &visual.model)?;
            if path.to_lowercase().starts_with(&dir.to_lowercase()) {
                in_level += 1;
            }
            *files.entry(path.to_lowercase()).or_default() += 1;
            Ok(ControlFlow::Continue(()))
        })
        .unwrap();
        assert_eq!((records, in_level, records - in_level), (5_373, 406, 4_967));
        assert_eq!(files.len(), 819);

        let start = Instant::now();
        let mut types = BTreeMap::new();
        let mut children = BTreeMap::new();
        let (mut vertices, mut indices, mut unit, mut zero) = (0usize, 0usize, 0usize, 0usize);
        let mut bytes = 0;
        for path in files.keys() {
            let file = read(tree, path).unwrap();
            bytes += file.len();
            let m = Model::parse(&file).unwrap_or_else(|e| panic!("{path}: {e}"));
            let chunks = Chunks::parse(&file).unwrap();
            let motions = if chunks.find(19).is_some() {
                "19"
            } else if chunks.find(14).is_some() && chunks.find(15).is_some() {
                "14+15"
            } else {
                ""
            };
            *types.entry((m.kind as u8, motions)).or_insert(0) += 1;
            assert_eq!(m.header.shader, 0, "{path}");
            for k in &m.meshes {
                *children.entry((k.kind as u8, k.links)).or_insert(0) += 1;
                vertices += k.vertices.len();
                indices += k.indices.len();
                // The header box is the box of the positions; children lie
                // inside the model's box.
                let mut min = [f32::INFINITY; 3];
                let mut max = [f32::NEG_INFINITY; 3];
                for v in &k.vertices {
                    for i in 0..3 {
                        min[i] = min[i].min(v.position[i]);
                        max[i] = max[i].max(v.position[i]);
                    }
                    let n = v.normal;
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if (len - 1.0).abs() <= 0.01 {
                        unit += 1;
                    } else if len == 0.0 {
                        zero += 1;
                    }
                    assert!((0.0..=1.0).contains(&v.weight), "{path}");
                }
                assert_eq!((min, max), (k.header.box_min, k.header.box_max), "{path}");
                for i in 0..3 {
                    assert!(
                        min[i] >= m.header.box_min[i] && max[i] <= m.header.box_max[i],
                        "{path}"
                    );
                }
                assert_eq!(k.indices.iter().min(), Some(&0), "{path}");
                if let Some(w) = &k.windows {
                    let first = w.steps[0];
                    assert_eq!(
                        first.index_offset as usize + 3 * usize::from(first.triangle_count),
                        k.indices.len()
                    );
                    assert_eq!(usize::from(first.vertex_count), k.vertices.len());
                    for s in w.steps.windows(2) {
                        assert!(
                            s[1].triangle_count <= s[0].triangle_count
                                && s[1].vertex_count <= s[0].vertex_count
                        );
                    }
                    for s in &w.steps {
                        let range = s.index_offset as usize
                            ..s.index_offset as usize + 3 * usize::from(s.triangle_count);
                        assert!(
                            k.indices[range].iter().all(|&i| i < s.vertex_count),
                            "{path}"
                        );
                    }
                }
            }
            m.bind_pose().unwrap();
        }
        eprintln!(
            "{} model files, {:.1} MB, parsed in {:.1?}",
            files.len(),
            bytes as f64 / 1e6,
            start.elapsed()
        );
        assert_eq!(
            types,
            [((3, "14+15"), 38), ((3, "19"), 117), ((10, ""), 664)]
                .into_iter()
                .collect()
        );
        assert_eq!(
            children,
            [
                ((4, Links::One), 378),
                ((4, Links::Two), 250),
                ((5, Links::One), 581),
                ((5, Links::Two), 31),
            ]
            .into_iter()
            .collect()
        );
        assert_eq!((vertices, indices), (931_510, 16_298_541));
        assert_eq!((unit, zero), (931_504, 6));
    });
}
