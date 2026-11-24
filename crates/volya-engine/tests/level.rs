// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Level loading from the Russian edition (BYOA): the chunk container
//! (docs/formats/chunks.md section 9), `level.geom` / `level.geomx`
//! (docs/formats/level-geom.md section 9) and the start position and camera
//! (docs/behavior/start-position.md section 8). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::time::Instant;

use md5::{Digest, Md5};
use volya_engine::core::camera::{self, ActorBody, Angles};
use volya_engine::core::config;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::level_geom::{Decoded, Layout, LevelGeom};
use volya_engine_formats::{env_mod, spawn};

/// Asserts `a` is within `rel` (relative) of `b`.
#[track_caller]
fn near(a: f64, b: f64, rel: f64) {
    assert!(
        (a - b).abs() <= rel * b.abs().max(1.0),
        "{a} is not within {rel} of {b}"
    );
}

/// `(id, payload offset, size)` of every top-level chunk.
fn layout(file: &[u8]) -> Vec<(u32, usize, usize)> {
    Chunks::parse(file)
        .unwrap()
        .iter()
        .map(|c| (c.id(), c.offset, c.payload.len()))
        .collect()
}

fn ids_and_sizes(file: &[u8]) -> Vec<(u32, usize)> {
    layout(file).into_iter().map(|(i, _, s)| (i, s)).collect()
}

fn with_tree(test: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    test(&tree);
}

#[test]
fn chunk_container_from_game_data() {
    with_tree(|vfs| {
        let geom = read(vfs, "gamedata/levels/l01_escape/level.geom").unwrap();
        assert_eq!(geom.len(), 56_995_500);
        assert_eq!(
            layout(&geom),
            [
                (1, 8, 4),
                (9, 20, 37_295_312),
                (10, 37_295_340, 19_330_928),
                (11, 56_626_276, 369_224)
            ]
        );
        let geomx = read(vfs, "gamedata/levels/l01_escape/level.geomx").unwrap();
        assert_eq!(geomx.len(), 13_401_518);
        assert_eq!(
            ids_and_sizes(&geomx),
            [(1, 4), (9, 5_673_404), (10, 7_728_074), (11, 4)]
        );
        let level = read(vfs, "gamedata/levels/l01_escape/level").unwrap();
        assert_eq!(level.len(), 4_094_604);
        assert_eq!(
            ids_and_sizes(&level),
            [
                (1, 4),
                (4, 5_520),
                (6, 4_428),
                (7, 396),
                (3, 4_065_388),
                (2, 17_864),
                (8, 948)
            ]
        );
        assert_eq!(layout(&level)[5].1 - 8, 4_075_776, "chunk 2 header offset");
        // Section 2: the static visuals are the list 0 … 11,537.
        let visuals = Chunks::parse(&level).unwrap();
        let visuals = Chunks::parse(visuals.find(3).unwrap().payload).unwrap();
        assert_eq!(visuals.list().len(), 11_538);

        let shaders = read(vfs, "gamedata/shaders.xr").unwrap();
        assert_eq!(
            ids_and_sizes(&shaders),
            [(0, 0), (1, 3_277), (2, 155_525), (3, 3_531)]
        );
        let garbage = read(vfs, "gamedata/levels/l02_garbage/level").unwrap();
        let ids: Vec<(u32, usize)> = ids_and_sizes(&garbage);
        assert_eq!(
            ids.iter().map(|c| c.0).collect::<Vec<_>>(),
            [1, 4, 6, 7, 3, 2, 8]
        );
        assert_eq!(ids[1], (4, 0));
        let game = read(vfs, "gamedata/levels/testers_mp_agroprom/level.game").unwrap();
        assert_eq!(ids_and_sizes(&game), [(0x1000, 0), (0x2000, 3_456)]);

        // Section 5: the one compressed chunk.
        let thm = read(vfs, "gamedata/textures/map/map_mp_military_2.thm").unwrap();
        assert_eq!(thm.len(), 46_111);
        let chunks = Chunks::parse(&thm).unwrap();
        assert_eq!(chunks.len(), 10);
        let compressed: Vec<_> = chunks.iter().filter(|c| c.is_compressed()).collect();
        assert_eq!(compressed.len(), 1);
        let c = compressed[0];
        assert_eq!((c.id_word, c.payload.len()), (0x8000_0811, 45_965));
        assert_eq!(chunks.iter().nth(1).unwrap().id_word, 0x8000_0811);
        let content = c.content().unwrap();
        assert_eq!(content.len(), 65_536);
        let md5: String = Md5::digest(&content)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(md5, "ecb320c95e2e6845d0069d670567584e");
    });
}

/// Directories under `gamedata/levels` holding a `level.geom`.
fn levels(vfs: &dyn Vfs) -> Vec<String> {
    vfs.read_dir("gamedata/levels")
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EntryKind::Dir)
        .map(|e| e.name)
        .filter(|l| vfs.open(&format!("gamedata/levels/{l}/level.geom")).is_ok())
        .collect()
}

#[test]
fn level_geometry_from_game_data() {
    with_tree(|vfs| {
        let start = Instant::now();
        let mut file = vfs.open("gamedata/levels/l01_escape/level.geom").unwrap();
        let geom = LevelGeom::read(&mut file).unwrap();
        eprintln!("read l01_escape level.geom in {:.1?}", start.elapsed());
        assert_eq!(geom.header, Some((14, 18)));

        // Chunk 9.
        use Layout::{LightMapped as C, Tree as A, VertexLit as B};
        let expected = [
            (A, 65_532),
            (A, 65_525),
            (A, 65_535),
            (A, 65_466),
            (A, 17_208),
            (B, 65_533),
            (C, 65_534),
            (B, 65_534),
            (B, 65_531),
            (C, 65_535),
            (C, 65_535),
            (B, 46_595),
            (C, 65_534),
            (C, 65_533),
            (C, 65_535),
            (C, 65_532),
            (C, 65_533),
            (C, 65_533),
            (C, 53_181),
        ];
        let got: Vec<(Layout, u32)> = geom
            .vertex_buffers
            .iter()
            .map(|b| (b.layout.unwrap(), b.vertex_count))
            .collect();
        assert_eq!(got, expected);

        // Chunk 10.
        let counts: Vec<usize> = geom.index_buffers.iter().map(Vec::len).collect();
        assert_eq!(
            counts,
            [
                1_048_575, 1_048_572, 1_048_572, 1_048_572, 1_048_572, 1_048_572, 1_048_575,
                1_048_572, 1_048_575, 228_285
            ]
        );
        let index_sum: u64 = geom
            .index_buffers
            .iter()
            .flatten()
            .map(|&i| u64::from(i))
            .sum();
        assert_eq!(index_sum, 20_158_230_477);

        // Chunk 11.
        let w = &geom.progressive_meshes;
        assert_eq!(w.len(), 97);
        assert_eq!(w.iter().map(|r| r.steps.len()).sum::<usize>(), 45_910);
        assert!(w.iter().all(|r| r.reserved == [0; 4]));
        let triangles: u64 = w
            .iter()
            .flat_map(|r| &r.steps)
            .map(|s| u64::from(s.triangle_count))
            .sum();
        assert_eq!(triangles, 206_758_914);
        let r0 = &w[0].steps;
        assert_eq!(r0.len(), 1_999);
        let s = |i: usize| (r0[i].index_offset, r0[i].triangle_count, r0[i].vertex_count);
        assert_eq!(
            (s(0), s(1_998)),
            ((138_876, 7_441, 6_232), (0, 3_445, 4_234))
        );

        // Decoded sums by layout (double precision, about 6 digits).
        #[derive(Default)]
        struct Sums {
            v: [f64; 8],
            lightmap: [f64; 2],
            colour: [f64; 4],
            hemi: f64,
            wind: f64,
        }
        let mut sums: BTreeMap<&str, Sums> = BTreeMap::new();
        let mut bbox = ([f64::MAX; 3], [f64::MIN; 3]);
        let mut add = |s: &mut Sums, p: [f32; 3], n: [f32; 3], uv: [f32; 2], hemi: f32| {
            for (acc, v) in s.v.iter_mut().zip(p.iter().chain(&n).chain(&uv)) {
                *acc += f64::from(*v);
            }
            s.hemi += f64::from(hemi);
            for i in 0..3 {
                bbox.0[i] = bbox.0[i].min(f64::from(p[i]));
                bbox.1[i] = bbox.1[i].max(f64::from(p[i]));
            }
        };
        for buffer in &geom.vertex_buffers {
            match buffer.decode().unwrap() {
                Decoded::Tree(vs) => {
                    let s = sums.entry("A").or_default();
                    for v in vs {
                        add(s, v.position, v.normal, v.uv, v.light);
                        s.wind += f64::from(v.wind);
                    }
                }
                Decoded::VertexLit(vs) => {
                    let s = sums.entry("B").or_default();
                    for v in vs {
                        add(s, v.position, v.normal, v.uv, v.hemi);
                        for (acc, c) in s.colour.iter_mut().zip(v.light.iter().chain([&v.sun])) {
                            *acc += f64::from(*c);
                        }
                    }
                }
                Decoded::LightMapped(vs) => {
                    let s = sums.entry("C").or_default();
                    for v in vs {
                        add(s, v.position, v.normal, v.uv, v.hemi);
                        s.lightmap[0] += f64::from(v.lightmap_uv[0]);
                        s.lightmap[1] += f64::from(v.lightmap_uv[1]);
                    }
                }
                Decoded::Position(_) => panic!("layout P in level.geom"),
            }
        }
        let (min, max) = bbox;
        let bbox_min = [-706.2949, -96.8817, -1132.7742];
        let bbox_max = [791.9193, 115.4424, 1284.6455];
        for i in 0..3 {
            assert!((min[i] - bbox_min[i]).abs() < 6e-5, "{min:?}");
            assert!((max[i] - bbox_max[i]).abs() < 6e-5, "{max:?}");
        }
        let table = [
            (
                "A",
                [
                    1_543.857,
                    858_158.181,
                    21_295.825,
                    -1_306.682,
                    4_057.522,
                    1_909.765,
                    137_907.235,
                    151_770.555,
                ],
            ),
            (
                "B",
                [
                    -15_952_682.922,
                    -1_189_764.774,
                    -15_682_451.543,
                    -288.678,
                    57_189.588,
                    -3_061.337,
                    123_441.981,
                    128_703.135,
                ],
            ),
            (
                "C",
                [
                    -18_071_883.845,
                    -2_306_859.350,
                    7_820_393.116,
                    198.145,
                    77_973.369,
                    -2_080.129,
                    315_949.164,
                    321_140.995,
                ],
            ),
        ];
        for (name, expected) in table {
            for (got, want) in sums[name].v.iter().zip(expected) {
                near(*got, want, 1e-5);
            }
        }
        let c = &sums["C"];
        near(c.lightmap[0], 322_198.036, 1e-5);
        near(c.lightmap[1], 318_530.206, 1e-5);
        for (got, want) in sums["B"]
            .colour
            .iter()
            .zip([9_400.282, 7_924.125, 5_268.024, 31_010.314])
        {
            near(*got, want, 1e-5);
        }
        near(sums["A"].hemi, 84_581.314, 1e-5);
        near(sums["B"].hemi, 56_213.275, 1e-5);
        near(sums["C"].hemi, 90_685.224, 1e-5);
        near(sums["A"].wind, 170_410.061, 1e-5);
        let vertices: u32 = geom.vertex_buffers.iter().map(|b| b.vertex_count).sum();
        assert_eq!(vertices, 1_165_444);

        // level.geomx.
        let mut file = vfs.open("gamedata/levels/l01_escape/level.geomx").unwrap();
        let geomx = LevelGeom::read(&mut file).unwrap();
        assert_eq!(geomx.header, Some((14, 267)));
        assert!(geomx.progressive_meshes.is_empty());
        let counts: Vec<u32> = geomx
            .vertex_buffers
            .iter()
            .map(|b| b.vertex_count)
            .collect();
        assert_eq!(
            counts,
            [
                65_533, 65_534, 65_534, 65_535, 65_535, 65_534, 65_535, 14_030
            ]
        );
        let mut sum = [0f64; 3];
        let (mut min, mut max) = ([f64::MAX; 3], [f64::MIN; 3]);
        for b in &geomx.vertex_buffers {
            assert_eq!(b.layout, Some(Layout::Position));
            let Some(Decoded::Position(ps)) = b.decode() else {
                panic!()
            };
            for p in ps {
                for i in 0..3 {
                    let v = f64::from(p[i]);
                    sum[i] += v;
                    min[i] = min[i].min(v);
                    max[i] = max[i].max(v);
                }
            }
        }
        for (got, want) in sum
            .iter()
            .zip([-11_284_761.953, -499_529.358, 5_738_854.187])
        {
            near(*got, want, 1e-5);
        }
        for i in 0..3 {
            assert!((min[i] - bbox_min[i]).abs() < 6e-5, "{min:?}");
            assert!((max[i] - bbox_max[i]).abs() < 6e-5, "{max:?}");
        }
        let counts: Vec<usize> = geomx.index_buffers.iter().map(Vec::len).collect();
        assert_eq!(counts, [1_048_575, 1_048_575, 1_048_575, 718_302]);
        let index_sum: u64 = geomx
            .index_buffers
            .iter()
            .flatten()
            .map(|&i| u64::from(i))
            .sum();
        assert_eq!(index_sum, 892_427_944);

        // Invariants over all shipped levels.
        let start = Instant::now();
        let names = levels(vfs);
        assert_eq!(names.len(), 31, "{names:?}");
        let mut no_windows = Vec::new();
        for level in &names {
            for (file, stride, layouts) in [
                ("level.geom", 32, &[A, B, C][..]),
                ("level.geomx", 12, &[Layout::Position][..]),
            ] {
                let path = format!("gamedata/levels/{level}/{file}");
                let data = read(vfs, &path).unwrap();
                let ids: Vec<u32> = ids_and_sizes(&data).iter().map(|c| c.0).collect();
                assert_eq!(ids, [1, 9, 10, 11], "{path}");
                let g = LevelGeom::parse(&data).unwrap();
                assert_eq!(g.header.unwrap().0, 14, "{path}");
                for b in &g.vertex_buffers {
                    assert!(b.vertex_count <= 65_535, "{path}");
                    assert_eq!(b.stride, stride, "{path}");
                    assert!(layouts.contains(&b.layout.unwrap()), "{path}");
                }
                assert!(g.index_buffers.iter().all(|b| b.len() % 3 == 0), "{path}");
                if file == "level.geomx" {
                    assert!(g.progressive_meshes.is_empty(), "{path}");
                } else if g.progressive_meshes.is_empty() {
                    no_windows.push(level.as_str());
                }
            }
        }
        assert_eq!(
            no_windows,
            [
                "l04u_labx18",
                "l10u_bunker",
                "l12u_control_monolith",
                "l12u_sarcofag",
                "testers_mp_pool"
            ]
        );
        eprintln!("checked {} levels in {:.1?}", names.len(), start.elapsed());
    });
}

#[test]
fn start_position_from_game_data() {
    with_tree(|vfs| {
        let file = read(vfs, "gamedata/spawns/all.spawn").unwrap();
        assert_eq!(file.len(), 4_474_073);
        assert_eq!(
            ids_and_sizes(&file),
            [(0, 44), (1, 2_552_770), (2, 850_944), (3, 1_070_283)]
        );
        let top = Chunks::parse(&file).unwrap();
        let objects = Chunks::parse(top.find(1).unwrap().payload).unwrap();
        assert_eq!(
            ids_and_sizes(top.find(1).unwrap().payload),
            [(0, 4), (1, 2_552_742), (2, 0)]
        );
        assert_eq!(objects.find(0).unwrap().payload, 8_641u32.to_le_bytes());

        // Every record (section 8).
        let mut count = 0;
        let mut flags = BTreeMap::new();
        let mut positions = [0f64; 3];
        let mut angles = [0f64; 3];
        let mut actors = Vec::new();
        let mut trader = None;
        spawn::for_each_object(&file, |o| {
            assert_eq!(o.index, count);
            count += 1;
            let p = o.packet;
            let names = o.section.chars().count() + o.name.chars().count() + 2;
            let u16_at = |at: usize| u16::from_le_bytes([p[at], p[at + 1]]);
            let at = 2 + names;
            assert_eq!(&p[at..at + 2], [0x00, 0xFE]);
            let at = at + 2 + 24;
            let four: Vec<u16> = (0..4).map(|i| u16_at(at + 2 * i)).collect();
            assert_eq!(four, [0, 0xFFFF, 0xFFFF, 0xFFFF]);
            *flags.entry(o.flags).or_insert(0) += 1;
            assert_eq!(o.version, 118);
            let at = at + 8 + 4;
            assert_eq!((u16_at(at), u16_at(at + 2)), (6, 0));
            assert_eq!(u32::from(u16_at(at + 4)), o.index);
            assert_eq!(usize::from(u16_at(at + 6)), o.data.len() + 2);
            assert_eq!(at + 8 + o.data.len(), p.len(), "data ends the packet");
            for i in 0..3 {
                positions[i] += f64::from(o.position[i]);
                angles[i] += f64::from(o.angles[i]);
            }
            if o.section == spawn::ACTOR_SECTION {
                actors.push((o.index, o.flags, o.data.len(), p.len()));
            }
            if o.index == 84 {
                trader = Some((o.name.clone(), o.section.clone(), o.position));
            }
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(count, 8_641);
        assert_eq!(
            flags,
            BTreeMap::from([(0x21, 8_629), (0x23, 11), (0x29, 1)])
        );
        for (got, want) in positions.iter().zip([592_183.146, -29_943.946, 71_704.991]) {
            near(*got, want, 1e-7);
        }
        for (got, want) in angles.iter().zip([537.2033, 259.2581, -902.9700]) {
            near(*got, want, 1e-6);
        }
        assert_eq!(actors, [(63, 0x29, 220, 298)]);

        let actor = spawn::actor_start(&file).unwrap();
        assert_eq!(
            (actor.index, actor.name.as_str()),
            (63, "level_prefix_actor_0001")
        );
        let bits = |v: [f32; 3]| v.map(f32::to_bits);
        assert_eq!(
            bits(actor.position),
            [0xC376_B9EF, 0xC1C6_5899, 0xC306_6F12]
        );
        assert_eq!(bits(actor.angles), [0x3B8D_71BC, 0xBFB2_B877, 0xBA34_045D]);
        assert_eq!(
            (actor.game_graph_vertex, actor.level_graph_node),
            (4, 11_713)
        );

        // Eye height (section 4.2).
        // The actor's section, in config\creatures\actor.ltx, inherits from
        // sections of other files: load it through system.ltx.
        let actor_ltx = config::load(vfs, "gamedata/config/system.ltx").unwrap();
        let body = ActorBody::from_section(actor_ltx.section("actor").unwrap()).unwrap();
        assert_eq!(body.camera_height_factor, 0.85);
        assert_eq!(body.box0_size, [0.35, 0.9, 0.35]);
        near(f64::from(body.eye_height()), 1.53, 1e-6);
        let eye = body.eye(actor.position);
        for (got, want) in eye.iter().zip([-246.72630, -23.263261, -134.43387]) {
            near(f64::from(*got), want, 1e-7);
        }

        // Orientation (section 5).
        let a = Angles::from_spawn(actor.angles);
        near(f64::from(a.yaw), 1.3962544, 1e-7);
        near(f64::from(a.pitch), 0.0043165, 1e-4);
        let (d, u) = a.direction_up();
        for (got, want) in d.iter().zip([0.984797, -0.0043165, 0.173655]) {
            assert!((f64::from(*got) - want).abs() < 2e-6, "direction {d:?}");
        }
        for (got, want) in u.iter().zip([0.0042509, 0.999991, 0.00074959]) {
            assert!((f64::from(*got) - want).abs() < 2e-6, "up {u:?}");
        }
        // The trader stands 2.09 m in front, 3.1° off the view direction.
        let (name, section, tp) = trader.unwrap();
        assert_eq!(
            (name.as_str(), section.as_str()),
            ("escape_trader", "m_trader")
        );
        let (dx, dz) = (tp[0] - eye[0], tp[2] - eye[2]);
        near(f64::from(dx.hypot(dz)), 2.09, 3e-3);
        let off = (dx * d[0] + dz * d[2]) / (dx.hypot(dz) * d[0].hypot(d[2]));
        near(f64::from(off.acos().to_degrees()), 3.1, 0.02);

        // View transform (section 6).
        let v = camera::view_matrix(eye, d, u);
        for (got, want) in [v[0][0], v[1][0], v[2][0]]
            .iter()
            .zip([0.173657, 0.0, -0.984806])
        {
            assert!((f64::from(*got) - want).abs() < 2e-6, "right {v:?}");
        }
        for (got, want) in v[3].iter().zip([-89.5456, 24.4126, 266.2201, 1.0]) {
            assert!((f64::from(*got) - want).abs() < 2e-4, "last row {v:?}");
        }

        // Far plane (section 7.1) from l01_escape's environment modifiers.
        let mods = env_mod::parse(&read(vfs, "gamedata/levels/l01_escape/level.env_mod").unwrap())
            .unwrap();
        let applying: Vec<(u32, f32, f32)> = mods
            .iter()
            .filter_map(|m| {
                camera::modifier_weight(m.position, m.radius, m.power, eye)
                    .map(|w| (m.id, w, m.far_plane))
            })
            .collect();
        assert_eq!(
            applying.iter().map(|m| (m.0, m.2)).collect::<Vec<_>>(),
            [(1, 150.0), (6, 300.0)]
        );
        near(f64::from(applying[0].1), 1.59990, 1e-5);
        near(f64::from(applying[1].1), 1.57357, 1e-5);
        let far = camera::far_plane(350.0, 350.0, 0.5, applying.iter().map(|m| (m.1, m.2)), 1.0);
        near(f64::from(far), 254.478, 1e-5);
    });
}
