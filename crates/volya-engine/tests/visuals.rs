// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The `level` file and its visuals from the Russian edition (BYOA): the
//! verification facts of docs/formats/level.md section 10 and
//! docs/formats/ogf.md section 9, the visual-reference properties of
//! docs/formats/level-geom.md sections 6, 7 and 9, and the static draw list.
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::time::Instant;

use sha2::{Digest, Sha256};
use volya_engine::core::level::StaticWorld;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::level::{
    LIGHT_DIRECTIONAL, LIGHT_POINT, Level, split_shader_entry, textures,
};
use volya_engine_formats::level_geom::{Decoded, GeometryRef, LevelGeom, ProgressiveMesh};
use volya_engine_formats::ogf::{Visual, VisualType, coverage_factor};

/// level.md section 10: level, header second u16, shader entries, visuals,
/// portals, sectors, lights, glows, full-detail triangles, distinct
/// geometry references.
type Row = (
    &'static str,
    u16,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    u64,
    usize,
);
const LEVELS: &[Row] = &[
    (
        "l01_escape",
        1,
        396,
        11_538,
        69,
        24,
        41,
        22,
        3_047_891,
        3_448,
    ),
    ("l02_garbage", 1, 290, 7_663, 0, 1, 19, 2, 1_955_697, 2_071),
    (
        "l03_agroprom",
        1,
        469,
        7_148,
        130,
        33,
        43,
        41,
        1_952_857,
        2_481,
    ),
    (
        "l03u_agr_underground",
        1,
        168,
        1_085,
        26,
        18,
        99,
        92,
        603_148,
        899,
    ),
    (
        "l04_darkvalley",
        1,
        459,
        8_471,
        242,
        55,
        75,
        41,
        1_906_791,
        3_041,
    ),
    (
        "l04u_labx18",
        1,
        158,
        1_255,
        32,
        31,
        131,
        130,
        551_673,
        1_076,
    ),
    ("l05_bar", 1, 428, 6_108, 131, 27, 112, 99, 1_473_702, 3_208),
    ("l06_rostok", 1, 430, 5_928, 27, 9, 26, 15, 1_347_420, 3_241),
    (
        "l07_military",
        1,
        522,
        6_638,
        159,
        50,
        71,
        40,
        2_530_087,
        3_200,
    ),
    (
        "l08_yantar",
        1,
        394,
        5_914,
        69,
        26,
        73,
        10,
        1_509_536,
        2_297,
    ),
    (
        "l08u_brainlab",
        2,
        274,
        1_352,
        29,
        21,
        112,
        101,
        497_277,
        1_095,
    ),
    ("l10_radar", 1, 324, 9_885, 80, 23, 53, 24, 3_224_640, 2_804),
    (
        "l10u_bunker",
        2,
        276,
        1_889,
        32,
        27,
        123,
        112,
        839_686,
        1_644,
    ),
    (
        "l11_pripyat",
        1,
        425,
        14_446,
        419,
        159,
        129,
        7,
        3_242_330,
        6_470,
    ),
    ("l12_stancia", 1, 245, 7_275, 1, 2, 13, 11, 1_970_434, 4_367),
    (
        "l12_stancia_2",
        2,
        374,
        9_696,
        0,
        1,
        39,
        9,
        2_186_584,
        7_519,
    ),
    (
        "l12u_control_monolith",
        2,
        144,
        948,
        45,
        27,
        79,
        77,
        402_434,
        829,
    ),
    (
        "l12u_sarcofag",
        2,
        210,
        1_820,
        29,
        25,
        112,
        98,
        788_351,
        1_552,
    ),
    (
        "testers_mp_agroprom",
        1,
        172,
        2_607,
        19,
        4,
        15,
        14,
        640_689,
        1_025,
    ),
    ("testers_mp_atp", 1, 160, 1_697, 32, 7, 13, 1, 601_319, 595),
    (
        "testers_mp_bath",
        2,
        206,
        1_583,
        43,
        11,
        27,
        26,
        392_116,
        1_013,
    ),
    (
        "testers_mp_darkvalley",
        1,
        174,
        1_813,
        72,
        13,
        37,
        16,
        401_624,
        928,
    ),
    (
        "testers_mp_factory",
        1,
        229,
        1_748,
        0,
        1,
        13,
        1,
        391_109,
        1_175,
    ),
    (
        "testers_mp_lost_village",
        2,
        207,
        1_949,
        93,
        30,
        17,
        12,
        532_325,
        1_216,
    ),
    (
        "testers_mp_military_1",
        1,
        225,
        1_699,
        58,
        23,
        37,
        28,
        531_828,
        1_137,
    ),
    (
        "testers_mp_military_2",
        1,
        175,
        1_327,
        32,
        4,
        6,
        6,
        519_073,
        624,
    ),
    ("testers_mp_pool", 1, 75, 276, 34, 22, 53, 48, 63_865, 226),
    (
        "testers_mp_railroad",
        1,
        262,
        3_248,
        53,
        13,
        10,
        1,
        909_140,
        2_143,
    ),
    (
        "testers_mp_rostok",
        1,
        240,
        4_568,
        63,
        8,
        61,
        40,
        1_244_000,
        2_069,
    ),
    (
        "testers_mp_workshop",
        1,
        234,
        2_092,
        81,
        14,
        43,
        42,
        498_977,
        1_328,
    ),
];

fn with_tree(test: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    test(&tree);
}

fn dir(level: &str) -> String {
    format!("gamedata/levels/{level}")
}

fn load_level(vfs: &dyn Vfs, level: &str) -> Level {
    Level::parse(&read(vfs, &format!("{}/level", dir(level))).unwrap()).unwrap()
}

fn load_geom(vfs: &dyn Vfs, level: &str, file: &str) -> LevelGeom {
    let mut f = vfs.open(&format!("{}/{file}", dir(level))).unwrap();
    LevelGeom::read(&mut f).unwrap()
}

#[track_caller]
fn near(a: f32, b: f32, eps: f32) {
    assert!((a - b).abs() <= eps, "{a} is not within {eps} of {b}");
}

#[track_caller]
fn near3(a: [f32; 3], b: [f32; 3], eps: f32) {
    for i in 0..3 {
        near(a[i], b[i], eps);
    }
}

fn engine_shader<'a>(level: &'a Level, v: &Visual) -> &'a str {
    split_shader_entry(&level.shaders[usize::from(v.header.shader)]).map_or("", |s| s.0)
}

/// Full-detail triangles of a visual: whole range, or level 0 of a
/// progressive one (ogf.md section 9).
fn full_triangles(v: &Visual, geom: &LevelGeom) -> u64 {
    let g = v.geometry.unwrap();
    if v.header.kind.is_progressive() {
        u64::from(v.detail_levels(&geom.progressive_meshes).unwrap().steps[0].triangle_count)
    } else {
        u64::from(g.index_count / 3)
    }
}

#[test]
fn level_file_from_game_data() {
    with_tree(|vfs| {
        // l01_escape (section 10).
        let level = load_level(vfs, "l01_escape");
        assert_eq!(level.header, (14, 1));
        let s = &level.shaders;
        assert_eq!(s.len(), 396);
        assert!(s[0].is_empty() && s[1..].iter().all(|e| !e.is_empty()));
        let engines: BTreeSet<&str> = s
            .iter()
            .filter_map(|e| split_shader_entry(e))
            .map(|e| e.0)
            .collect();
        assert_eq!(engines.len(), 18);
        assert_eq!(s[1], "effects\\glow/glow\\glow_04");
        assert_eq!(s[395], "def_shaders\\def_objects_lod/act\\act_monolit_corp");
        let names: usize = s
            .iter()
            .filter_map(|e| split_shader_entry(e))
            .map(|e| textures(e.1).count())
            .sum();
        assert_eq!(names, 917);
        assert_eq!(level.visuals.len(), 11_538);

        let p = &level.portals;
        let mut by_count = BTreeMap::new();
        for portal in p {
            *by_count.entry(portal.vertices.len()).or_insert(0) += 1;
        }
        assert_eq!(by_count, BTreeMap::from([(4, 63), (5, 1), (6, 5)]));
        assert_eq!((p[0].sector_a, p[0].sector_b), (16, 23));
        let (n, d) = p[0].plane().unwrap();
        near3(n, [-1.0, 0.0, 0.0], 1e-5);
        near(d, -215.882, 1e-3);
        let (c, r) = p[0].sphere();
        near3(c, [-215.882, -18.394, -167.358], 1e-3);
        near(r, 0.881, 1e-3);

        let roots: Vec<u32> = level.sectors.iter().map(|s| s.root).collect();
        let mut expected = vec![9621, 9622, 9626];
        expected.extend(9631..=9643);
        expected.extend([9647, 9648, 9649, 9652, 9656, 9662, 9665, 11537]);
        assert_eq!(roots, expected);
        let entries: usize = level.sectors.iter().map(|s| s.portals.len()).sum();
        assert_eq!(entries, 138);
        assert_eq!(level.sectors[0].portals, [25, 57]);

        let l = &level.lights;
        assert_eq!(l.len(), 41);
        let sun: Vec<_> = l.iter().filter(|l| l.kind == LIGHT_DIRECTIONAL).collect();
        assert_eq!(sun.len(), 1);
        near3(sun[0].direction, [0.840, -0.423, 0.340], 1e-3);
        assert_eq!(l.iter().filter(|l| l.kind == LIGHT_POINT).count(), 40);

        let g = &level.glows;
        assert_eq!(g.len(), 22);
        let mut by_shader = BTreeMap::new();
        for glow in g {
            *by_shader.entry(glow.shader).or_insert(0) += 1;
            assert_eq!(
                split_shader_entry(&s[usize::from(glow.shader)]).unwrap().0,
                "effects\\glow"
            );
        }
        assert_eq!(by_shader, BTreeMap::from([(1, 12), (2, 4), (3, 6)]));
        near(g.iter().map(|g| g.radius).sum(), 17.4, 1e-3);
        near3(g[0].position, [30.484, 19.756, 679.066], 1e-3);
        assert_eq!(g[0].radius, 1.0);

        // All 30 levels: the table, invariants and totals.
        let start = Instant::now();
        let (mut portals, mut sectors, mut lights, mut glows) = (0, 0, 0, 0);
        let mut glow_engines = BTreeMap::new();
        let (mut a_only, mut b_only) = (0, 0);
        for &(name, hdr, shaders, visuals, np, ns, nl, ng, _, _) in LEVELS {
            let file = read(vfs, &format!("{}/level", dir(name))).unwrap();
            let ids: BTreeSet<u32> = Chunks::parse(&file)
                .unwrap()
                .iter()
                .map(|c| c.id())
                .collect();
            assert_eq!(ids, BTreeSet::from([1, 2, 3, 4, 6, 7, 8]), "{name}");
            let level = Level::parse(&file).unwrap();
            let got = (
                level.header,
                level.shaders.len(),
                level.visuals.len(),
                level.portals.len(),
                level.sectors.len(),
                level.lights.len(),
                level.glows.len(),
            );
            assert_eq!(got, ((14, hdr), shaders, visuals, np, ns, nl, ng), "{name}");
            let s = &level.shaders;
            assert!(s[0].is_empty(), "{name}");
            assert!(s[1..].iter().all(|e| e.contains('/')), "{name}");
            assert_eq!(s.iter().collect::<HashSet<_>>().len(), s.len(), "{name}");
            for (i, portal) in level.portals.iter().enumerate() {
                let i = i as u16;
                assert!(
                    level.sectors[usize::from(portal.sector_a)]
                        .portals
                        .contains(&i)
                );
                assert!(
                    level.sectors[usize::from(portal.sector_b)]
                        .portals
                        .contains(&i)
                );
                let (n, d) = portal.plane().unwrap();
                let side = |sector: u16| {
                    let root = &level.visuals[level.sectors[usize::from(sector)].root as usize];
                    let c = root.header.sphere_centre;
                    n[0] * c[0] + n[1] * c[1] + n[2] * c[2] + d > 0.0
                };
                match (side(portal.sector_a), side(portal.sector_b)) {
                    (true, false) => a_only += 1,
                    (false, true) => b_only += 1,
                    _ => {}
                }
            }
            let mut has_parent = vec![false; level.visuals.len()];
            for v in &level.visuals {
                for &c in &v.children {
                    has_parent[c as usize] = true;
                }
            }
            let parentless: BTreeSet<u32> = (0..level.visuals.len() as u32)
                .filter(|&i| !has_parent[i as usize])
                .collect();
            let roots: BTreeSet<u32> = level.sectors.iter().map(|s| s.root).collect();
            assert_eq!(parentless, roots, "{name}");
            let directional = level
                .lights
                .iter()
                .filter(|l| l.kind == LIGHT_DIRECTIONAL)
                .count();
            let point = level
                .lights
                .iter()
                .filter(|l| l.kind == LIGHT_POINT)
                .count();
            assert_eq!((directional, point), (1, level.lights.len() - 1), "{name}");
            for glow in &level.glows {
                let e = split_shader_entry(&s[usize::from(glow.shader)])
                    .unwrap()
                    .0
                    .to_owned();
                *glow_engines.entry(e).or_insert(0) += 1;
            }
            portals += level.portals.len();
            sectors += level.sectors.len();
            lights += level.lights.len();
            glows += level.glows.len();
        }
        assert_eq!(
            (portals, sectors, lights, glows),
            (2_100, 709, 1_682, 1_166)
        );
        assert_eq!(
            glow_engines,
            BTreeMap::from([
                ("effects\\flare".to_owned(), 1),
                ("effects\\glow".to_owned(), 1_165)
            ])
        );
        assert_eq!((a_only, b_only), (1_288, 3));
        eprintln!(
            "checked {} level files in {:.1?}",
            LEVELS.len(),
            start.elapsed()
        );
    });
}

/// Box of points, as (min, max).
fn bounds(points: impl Iterator<Item = [f32; 3]>) -> ([f32; 3], [f32; 3]) {
    let mut b = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in points {
        for i in 0..3 {
            b.0[i] = b.0[i].min(p[i]);
            b.1[i] = b.1[i].max(p[i]);
        }
    }
    b
}

#[test]
fn visuals_from_game_data() {
    with_tree(|vfs| {
        // l01_escape (ogf.md section 9).
        let level = load_level(vfs, "l01_escape");
        let geom = load_geom(vfs, "l01_escape", "level.geom");
        let file = read(vfs, &format!("{}/level", dir("l01_escape"))).unwrap();
        let top = Chunks::parse(&file).unwrap();
        let mut sha = Sha256::new();
        for c in Chunks::parse(top.find(3).unwrap().payload).unwrap().list() {
            sha.update(Chunks::parse(c.payload).unwrap().find(1).unwrap().payload);
        }
        let hex: String = sha.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "a9c3107c8cf69a74a10781f79e503a3076d69b6ec76b1ce00b44266dd8026045"
        );
        let v = &level.visuals;
        let mut kinds = BTreeMap::new();
        for x in v {
            *kinds.entry(x.header.kind).or_insert(0) += 1;
        }
        use VisualType::*;
        assert_eq!(
            kinds,
            BTreeMap::from([
                (Mesh, 2_861),
                (Group, 1_917),
                (Progressive, 411),
                (BillboardGroup, 1_839),
                (Tree, 2_822),
                (ProgressiveTree, 1_688)
            ])
        );
        let refs: Vec<GeometryRef> = v.iter().filter_map(|x| x.geometry).collect();
        assert_eq!(refs.len(), 7_782);
        let distinct: HashSet<_> = refs.iter().collect();
        assert_eq!(distinct.len(), 3_448);
        assert_eq!(
            refs.iter().map(|g| u64::from(g.index_count)).sum::<u64>(),
            29_743_374
        );
        let triangles: u64 = v
            .iter()
            .filter(|x| x.geometry.is_some())
            .map(|x| full_triangles(x, &geom))
            .sum();
        assert_eq!(triangles, 3_047_891);
        let own: usize = v
            .iter()
            .filter_map(|x| x.window_list.as_ref())
            .map(|w| w.steps.len())
            .sum();
        assert_eq!(own, 38_599);
        let via: usize = v
            .iter()
            .filter(|x| x.header.kind == ProgressiveTree)
            .map(|x| {
                x.detail_levels(&geom.progressive_meshes)
                    .unwrap()
                    .steps
                    .len()
            })
            .sum();
        assert_eq!(via, 309_021);
        let mut bb = BTreeMap::new();
        for x in v.iter().filter(|x| x.header.kind == BillboardGroup) {
            *bb.entry(x.children.len()).or_insert(0) += 1;
        }
        assert_eq!(
            bb,
            BTreeMap::from([(1, 109), (2, 1_173), (3, 179), (4, 377), (10, 1)])
        );

        let v0 = &v[0];
        assert_eq!((v0.header.kind, v0.header.shader), (Progressive, 4));
        let g = v0.geometry.unwrap();
        assert_eq!(
            [
                g.vertex_buffer,
                g.first_vertex,
                g.vertex_count,
                g.index_buffer,
                g.first_index,
                g.index_count
            ],
            [5, 31_041, 128, 5, 583_479, 1_611]
        );
        let w = &v0.window_list.as_ref().unwrap().steps;
        assert_eq!(w.len(), 49);
        assert_eq!(
            (w[0].index_offset, w[0].triangle_count, w[0].vertex_count),
            (1_131, 160, 128)
        );
        assert_eq!(
            (w[48].index_offset, w[48].triangle_count, w[48].vertex_count),
            (0, 64, 80)
        );

        let t = &v[3272];
        assert_eq!((t.header.kind, t.header.shader), (ProgressiveTree, 344));
        let g = t.geometry.unwrap();
        assert_eq!(
            [
                g.vertex_buffer,
                g.first_vertex,
                g.vertex_count,
                g.index_buffer,
                g.first_index,
                g.index_count
            ],
            [0, 0, 6_232, 0, 0, 161_199]
        );
        assert_eq!(t.window_record, Some(0));
        assert_eq!(
            t.detail_levels(&geom.progressive_meshes)
                .unwrap()
                .steps
                .len(),
            1_999
        );
        let tree = t.tree.unwrap();
        near3(
            [tree.transform[12], tree.transform[13], tree.transform[14]],
            [22.2026, 15.8762, 664.0648],
            1e-4,
        );
        for (a, b) in tree.scale.iter().zip([0.0, 0.0, 0.0, 0.6078, 0.5493]) {
            near(*a, b, 5e-5);
        }
        for (a, b) in tree.bias.iter().zip([0.0, 0.0, 0.0, 0.2493, 0.1848]) {
            near(*a, b, 5e-5);
        }
        // Texture coordinates and sway rigidity of visuals 3272 and 3277.
        for (i, uv, rigidity) in [
            (3272, (-0.0009765625, 1.0), (0.1699, 1.0547)),
            (3277, (0.00049, 0.99609), (0.5815, 0.6919)),
        ] {
            let g = v[i].geometry.unwrap();
            let Some(Decoded::Tree(vs)) = geom.vertex_buffers[g.vertex_buffer as usize].decode()
            else {
                panic!()
            };
            let used = &vs[g.first_vertex as usize..][..g.vertex_count as usize];
            let fold = |f: &dyn Fn(&volya_engine_formats::level_geom::TreeVertex) -> f32| {
                used.iter()
                    .map(f)
                    .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)))
            };
            let u = fold(&|x| x.uv[0].min(x.uv[1]));
            let u_hi = fold(&|x| x.uv[0].max(x.uv[1]));
            near(u.0, uv.0, 5e-6);
            near(u_hi.1, uv.1, 5e-6);
            let r = fold(&|x| x.wind);
            near(r.0, rigidity.0, 5e-5);
            near(r.1, rigidity.1, 5e-5);
        }

        let b = &v[3273];
        assert_eq!((b.header.kind, b.header.shader), (BillboardGroup, 345));
        assert_eq!(level.shaders[345], "details\\lod/level_lods");
        assert_eq!(b.children, [3272]);
        near(coverage_factor(&b.header), 0.77375, 5e-6);
        let faces = b.billboards.as_ref().unwrap();
        near3(faces.face_normal(0), [-0.9067, -0.0120, -0.4215], 5e-5);
        let c = b.header.sphere_centre;
        let choice = faces.choose_faces(c, [c[0] + 100.0, c[1], c[2]]);
        assert_eq!((choice.f1, choice.f2, choice.beta), (7, 0, 135));

        // All 30 levels.
        let start = Instant::now();
        let mut kinds = BTreeMap::new();
        let (mut with_geometry, mut distinct, mut triangles, mut own, mut via) = (0, 0, 0, 0, 0);
        let mut group_children = (usize::MAX, 0);
        let mut bb = BTreeMap::new();
        let mut lit = BTreeMap::new();
        for &(name, .., level_triangles, level_distinct) in LEVELS {
            let level = load_level(vfs, name);
            let geom = load_geom(vfs, name, "level.geom");
            let positions: Vec<Vec<[f32; 3]>> = geom
                .vertex_buffers
                .iter()
                .map(|b| b.positions().unwrap())
                .collect();
            let v = &level.visuals;
            let mut parents = vec![0; v.len()];
            let mut parent_kind = vec![None; v.len()];
            let mut lvl_triangles = 0;
            let mut refs = HashSet::new();
            for (i, x) in v.iter().enumerate() {
                let h = &x.header;
                *kinds.entry(h.kind).or_insert(0) += 1;
                assert_eq!(
                    h.kind == Group,
                    h.shader == 0,
                    "{name} visual {i}: shader index 0 iff group"
                );
                for &c in &x.children {
                    assert!((c as usize) < i, "{name} visual {i}: child {c}");
                    parents[c as usize] += 1;
                    parent_kind[c as usize] = Some(h.kind);
                    let ch = &v[c as usize].header;
                    for k in 0..3 {
                        assert!(ch.box_min[k] >= h.box_min[k] - 1e-3, "{name} {i} > {c}");
                        assert!(ch.box_max[k] <= h.box_max[k] + 1e-3, "{name} {i} > {c}");
                    }
                    if h.kind == BillboardGroup {
                        assert!(v[c as usize].header.kind.is_tree(), "{name} {i}");
                    }
                }
                match h.kind {
                    Group => {
                        group_children.0 = group_children.0.min(x.children.len());
                        group_children.1 = group_children.1.max(x.children.len());
                    }
                    BillboardGroup => *bb.entry(x.children.len()).or_insert(0) += 1,
                    _ => {}
                }
                if let Some(t) = x.tree {
                    let m = t.transform;
                    assert_eq!(
                        [m[3], m[7], m[11], m[15]],
                        [0.0, 0.0, 0.0, 1.0],
                        "{name} {i}"
                    );
                    let rgb = |a: [f32; 5]| a[..3].iter().any(|&c| c != 0.0);
                    assert_eq!(rgb(t.scale), rgb(t.bias), "{name} {i}");
                    if rgb(t.scale) {
                        *lit.entry((h.kind, name == "l01_escape")).or_insert(0) += 1;
                    }
                    if name == "l08_yantar" && i == 4785 {
                        for (a, b) in t.scale.iter().zip([0.6025, 0.4636, 0.1926, 0.2670, 0.0]) {
                            near(*a, b, 5e-5);
                        }
                        for (a, b) in t.bias.iter().zip([0.1071, 0.0824, 0.0341, 0.1175, 0.0]) {
                            near(*a, b, 5e-5);
                        }
                    }
                }
                let Some(g) = x.geometry else { continue };
                with_geometry += 1;
                refs.insert(g);
                lvl_triangles += full_triangles(x, &geom);
                let vb = &geom.vertex_buffers[g.vertex_buffer as usize];
                assert_eq!(vb.stride, 32);
                // Ranges, indices below vc and every vertex of the range
                // used (so the minimum index is 0) are checked per distinct
                // reference in `visual_geometry_references_from_game_data`;
                // the used vertices are therefore the whole range.
                let ps = &positions[g.vertex_buffer as usize];
                let used = ps[g.first_vertex as usize..][..g.vertex_count as usize]
                    .iter()
                    .copied();
                let (min, max) = match x.tree {
                    Some(t) => bounds(used.map(|p| t.transform_point(p))),
                    None => bounds(used),
                };
                for k in 0..3 {
                    near(min[k], h.box_min[k], 0.01);
                    near(max[k], h.box_max[k], 0.01);
                }
                if let Some(w) = x.window_list.as_ref() {
                    own += w.steps.len();
                    check_detail_levels(w, &g, &geom, &format!("{name} visual {i}"));
                }
                if h.kind == ProgressiveTree {
                    let w = x.detail_levels(&geom.progressive_meshes).unwrap();
                    via += w.steps.len();
                    check_detail_levels(w, &g, &geom, &format!("{name} visual {i}"));
                }
            }
            for (i, x) in v.iter().enumerate() {
                assert!(parents[i] <= 1, "{name} visual {i}: {} parents", parents[i]);
                if x.header.kind.is_tree() {
                    assert_eq!(parents[i], 1, "{name} tree {i}");
                    assert_eq!(parent_kind[i], Some(BillboardGroup), "{name} tree {i}");
                }
            }
            assert_eq!(lvl_triangles, level_triangles, "{name}");
            assert_eq!(refs.len(), level_distinct, "{name}");
            distinct += refs.len();
            triangles += lvl_triangles;
        }
        assert_eq!(
            kinds,
            BTreeMap::from([
                (Mesh, 55_807),
                (Group, 22_407),
                (Progressive, 6_770),
                (BillboardGroup, 15_236),
                (Tree, 18_644),
                (ProgressiveTree, 14_802)
            ])
        );
        assert_eq!(kinds.values().sum::<usize>(), 133_666);
        assert_eq!(
            (with_geometry, distinct, triangles),
            (96_023, 64_721, 36_756_603)
        );
        assert_eq!((own, via), (1_186_900, 3_287_550));
        assert_eq!(group_children, (2, 124));
        assert_eq!(
            bb,
            BTreeMap::from([
                (1, 2_759),
                (2, 8_870),
                (3, 1_507),
                (4, 2_094),
                (5, 2),
                (10, 4)
            ])
        );
        assert_eq!(
            lit,
            BTreeMap::from([
                ((Tree, false), 728 - 15),
                ((Tree, true), 15),
                ((ProgressiveTree, false), 967 - 15),
                ((ProgressiveTree, true), 15)
            ])
        );
        eprintln!(
            "checked the visuals of {} levels in {:.1?}",
            LEVELS.len(),
            start.elapsed()
        );
    });
}

/// Maximum of `values[a..b]` in constant time after an `n log n` build.
struct RangeMax {
    levels: Vec<Vec<u16>>,
}

impl RangeMax {
    fn new(values: &[u16]) -> Self {
        let mut levels = vec![values.to_vec()];
        let mut width = 1;
        while 2 * width <= values.len() {
            let prev = levels.last().unwrap();
            let next = (0..=values.len() - 2 * width)
                .map(|i| prev[i].max(prev[i + width]))
                .collect();
            levels.push(next);
            width *= 2;
        }
        Self { levels }
    }

    fn max(&self, a: usize, b: usize) -> u16 {
        let k = (b - a).ilog2() as usize;
        self.levels[k][a].max(self.levels[k][b - (1 << k)])
    }
}

/// The statements of level-geom.md section 6 / ogf.md section 6 for the
/// detail levels `w` of a visual with geometry `g`.
fn check_detail_levels(w: &ProgressiveMesh, g: &GeometryRef, geom: &LevelGeom, what: &str) {
    assert!(w.reserved == [0; 4], "{what}");
    let s = &w.steps;
    assert!(!s.is_empty(), "{what}");
    assert_eq!(
        s[0].index_offset + 3 * u32::from(s[0].triangle_count),
        g.index_count,
        "{what}"
    );
    assert_eq!(u32::from(s[0].vertex_count), g.vertex_count, "{what}");
    assert_eq!(s.last().unwrap().index_offset, 0, "{what}");
    for p in s.windows(2) {
        assert!(p[1].triangle_count <= p[0].triangle_count, "{what}");
        assert!(p[1].vertex_count <= p[0].vertex_count, "{what}");
        assert!(p[1].index_offset <= p[0].index_offset, "{what}");
    }
    let range = &geom.index_buffers[g.index_buffer as usize][g.first_index as usize..]
        [..g.index_count as usize];
    let max = RangeMax::new(range);
    for step in s {
        let a = step.index_offset as usize;
        let b = a + 3 * usize::from(step.triangle_count);
        assert!(b <= range.len(), "{what}: window outside the range");
        if b > a {
            assert!(
                max.max(a, b) < step.vertex_count,
                "{what}: index beyond the step's vertices"
            );
        }
    }
}

/// Directories under `gamedata/levels` holding a `level` (with
/// `mp_military_2` of `mods\mp_military_2.xdb0`).
fn all_levels(vfs: &dyn Vfs) -> Vec<String> {
    vfs.read_dir("gamedata/levels")
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EntryKind::Dir)
        .map(|e| e.name)
        .filter(|l| vfs.open(&format!("{}/level", dir(l))).is_ok())
        .collect()
}

#[test]
fn visual_geometry_references_from_game_data() {
    with_tree(|vfs| {
        // level-geom.md section 9 for l01_escape.
        let level = load_level(vfs, "l01_escape");
        let v = &level.visuals;
        assert_eq!(v.iter().filter(|x| x.geometry.is_some()).count(), 7_782);
        assert_eq!(v.iter().filter(|x| x.geometry_x.is_some()).count(), 2_967);
        let records: BTreeSet<u32> = v
            .iter()
            .filter(|x| x.header.kind == VisualType::ProgressiveTree)
            .map(|x| x.window_record.unwrap())
            .collect();
        assert_eq!(records, (0..97).collect());

        // Sections 6 and 7 over every level.
        let start = Instant::now();
        let names = all_levels(vfs);
        assert_eq!(names.len(), 31, "{names:?}");
        let (mut with_x, mut plain_x, mut plain_same) = (0, 0, 0);
        let (mut positions_differ, mut aref_x) = (0, 0);
        for name in &names {
            let level = load_level(vfs, name);
            let geom = load_geom(vfs, name, "level.geom");
            let geomx = load_geom(vfs, name, "level.geomx");
            let pos: Vec<Vec<[f32; 3]>> = geom
                .vertex_buffers
                .iter()
                .map(|b| b.positions().unwrap())
                .collect();
            let pos_x: Vec<Vec<[f32; 3]>> = geomx
                .vertex_buffers
                .iter()
                .map(|b| b.positions().unwrap())
                .collect();
            let mut checked = HashSet::new();
            for (i, x) in level.visuals.iter().enumerate() {
                let Some(g) = x.geometry else { continue };
                let what = format!("{name} visual {i}");
                if checked.insert(g) {
                    // Every vertex of the range used: distinct indices 0 … vc − 1.
                    let tri = g.triangles(&geom).unwrap();
                    let mut used = vec![false; g.vertex_count as usize];
                    for &k in tri.iter().flatten() {
                        used[(k - g.first_vertex) as usize] = true;
                    }
                    assert!(used.iter().all(|&u| u), "{what}: unused vertex");
                }
                if x.header.kind == VisualType::ProgressiveTree {
                    check_detail_levels(
                        x.detail_levels(&geom.progressive_meshes).unwrap(),
                        &g,
                        &geom,
                        &what,
                    );
                }
                let engine = engine_shader(&level, x);
                let Some(gx) = x.geometry_x else { continue };
                with_x += 1;
                assert!(!x.header.kind.is_tree(), "{what}");
                assert!(!engine.starts_with("effects\\"), "{what}: {engine}");
                assert!(
                    !engine.ends_with("\\def_trans") && !engine.ends_with("\\def_trans_v"),
                    "{what}"
                );
                if engine.ends_with("\\def_aref") || engine.ends_with("\\def_aref_v") {
                    aref_x += 1;
                }
                assert!(gx.vertex_count <= g.vertex_count, "{what}");
                let tri = g.triangles(&geom).unwrap();
                let tri_x = gx.triangles(&geomx).unwrap();
                let p = &pos[g.vertex_buffer as usize];
                let px = &pos_x[gx.vertex_buffer as usize];
                let key = |q: [f32; 3]| q.map(f32::to_bits);
                let set = |t: &[[u32; 3]], ps: &[[f32; 3]]| {
                    let mut s: Vec<[u32; 3]> =
                        t.iter().flatten().map(|&k| key(ps[k as usize])).collect();
                    s.sort_unstable();
                    s.dedup();
                    s
                };
                if set(&tri, p) != set(&tri_x, px) {
                    positions_differ += 1;
                }
                if !x.header.kind.is_progressive() {
                    plain_x += 1;
                    let triples = |t: &[[u32; 3]], ps: &[[f32; 3]]| {
                        let mut out: Vec<[[u32; 3]; 3]> =
                            t.iter().map(|c| c.map(|k| key(ps[k as usize]))).collect();
                        out.sort_unstable();
                        out
                    };
                    if triples(&tri, p) == triples(&tri_x, px) {
                        plain_same += 1;
                    }
                }
            }
        }
        assert_eq!(with_x, 59_913);
        assert_eq!(positions_differ, 3);
        assert_eq!((plain_x, plain_same), (53_469, 53_468));
        assert_eq!(aref_x, 8);
        eprintln!(
            "checked the geometry references of {} levels in {:.1?}",
            names.len(),
            start.elapsed()
        );
    });
}

#[test]
fn static_draw_list_from_game_data() {
    with_tree(|vfs| {
        let start = Instant::now();
        for &(name, .., triangles, _) in LEVELS {
            let world = StaticWorld::load(vfs, &dir(name)).unwrap();
            let items = &world.draw_items;
            let drawable = world
                .level
                .visuals
                .iter()
                .filter(|v| v.geometry.is_some())
                .count();
            assert_eq!(items.len(), drawable, "{name}: every drawable visual once");
            let drawn: u64 = items.iter().map(|i| u64::from(i.triangle_count)).sum();
            assert_eq!(drawn, triangles, "{name}");
            let trees = world
                .level
                .visuals
                .iter()
                .filter(|v| v.header.kind.is_tree())
                .count();
            assert_eq!(
                items.iter().filter(|i| i.tree.is_some()).count(),
                trees,
                "{name}"
            );
            assert!(
                items
                    .iter()
                    .all(|i| !world.level.shaders[usize::from(i.shader)].is_empty())
            );
            if name == "l01_escape" {
                assert_eq!(items.len(), 7_782);
            }
        }
        eprintln!(
            "built the static draw lists of {} levels in {:.1?}",
            LEVELS.len(),
            start.elapsed()
        );
    });
}
