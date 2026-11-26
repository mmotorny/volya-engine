// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `level.details` of the Russian edition (BYOA) against the verification
//! facts of docs/formats/level-details.md (sections 1 – 5), and the
//! placement of docs/behavior/detail-objects.md section 4 against its
//! vectors (8.2), with the cache's build and selection times. Run it with
//! `--release` for meaningful times. Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;
use std::time::Instant;

use md5::{Digest, Md5};
use volya_engine_core::collision::CollisionWorld;
use volya_engine_core::details::{Details, Pass};
use volya_engine_core::particles::random::Generator;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::gamemtl;
use volya_engine_formats::level_details::{LevelDetails, NO_MODEL};

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

#[test]
fn every_level_details_file() {
    with_tree(|vfs| {
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
        // Section 1: 31 level directories, 7 without the file.
        assert_eq!(levels.len(), 31, "{levels:?}");
        let mut without = Vec::new();
        let mut counts = BTreeMap::new();
        for name in &levels {
            let dir = format!("gamedata/levels/{name}");
            let Ok(bytes) = read(vfs, &format!("{dir}/level.details")) else {
                without.push(name.clone());
                assert!(vfs.open(&format!("{dir}/build_details.dds")).is_err());
                continue;
            };
            let texture = read(vfs, &format!("{dir}/build_details.dds")).unwrap();
            assert_eq!(texture.len(), 349_672, "{name}");
            // Section 2: chunks 1, 2, 0, uncompressed, filling the file.
            let chunks = Chunks::parse(&bytes).unwrap();
            let ids: Vec<u32> = chunks.iter().map(|c| c.id_word).collect();
            assert_eq!(ids, [1, 2, 0], "{name}");
            let d = LevelDetails::parse(&bytes).unwrap();
            let h = d.header;
            assert_eq!(
                chunks.find(2).unwrap().payload.len(),
                16 * (h.size_x * h.size_z) as usize
            );
            *counts.entry(h.model_count).or_insert(0) += 1;
            // Section 3.1's shipped values.
            for m in &d.models {
                assert_eq!(
                    (m.shader.as_str(), m.texture.as_str()),
                    ("details\\blend", "build_details")
                );
                assert!(m.flags <= 1, "{name}");
                let (lo, hi) = m.bounds();
                if m.still() {
                    assert!(lo[1] > -0.03 && hi[1] < 0.03, "{name}: flat");
                } else {
                    assert!(hi[1] > 0.3, "{name}: rises");
                }
                assert!(0.0 < m.min_scale && m.min_scale <= m.max_scale);
                let mut used = vec![false; m.vertices.len()];
                for &i in &m.indices {
                    used[usize::from(i)] = true;
                }
                assert!(used.iter().all(|&u| u), "{name}: every vertex used");
                assert!(
                    m.vertices
                        .iter()
                        .flat_map(|v| v.uv)
                        .all(|t| (-0.01..=1.01).contains(&t))
                );
            }
            // Section 4.5.
            for s in d.records() {
                assert!(!s.is_empty());
                assert!(s.base() != 0 || s.height() == 0);
                assert!(s.sun() <= 8 && s.hemi() <= 13);
                assert!(
                    s.ids()
                        .iter()
                        .all(|&id| id == NO_MODEL || u32::from(id) < h.model_count)
                );
            }
        }
        without.sort();
        assert_eq!(
            without,
            [
                "l03u_agr_underground",
                "l04u_labx18",
                "l08u_brainlab",
                "l10u_bunker",
                "l12u_control_monolith",
                "l12u_sarcofag",
                "testers_mp_pool"
            ]
        );
        assert_eq!(counts, BTreeMap::from([(20, 3), (21, 21)]));
    });
}

#[test]
fn the_first_level_s_details() {
    with_tree(|vfs| {
        // Section 5.
        let bytes = read(vfs, "gamedata/levels/l01_escape/level.details").unwrap();
        assert_eq!(bytes.len(), 8_392_388);
        let chunks = Chunks::parse(&bytes).unwrap();
        let sizes: Vec<(u32, usize)> = chunks.iter().map(|c| (c.id(), c.payload.len())).collect();
        assert_eq!(sizes, [(1, 17_108), (2, 8_375_232), (0, 24)]);
        let md5 = |id| hex(&Md5::digest(chunks.find(id).unwrap().payload));
        assert_eq!(md5(0), "d42cb184083b56d557086985c2d7053c");
        assert_eq!(md5(1), "48046c9c7488fc33157a94d9bad14584");
        assert_eq!(md5(2), "3cee4ba6cda61198aac6b8d4bf598c43");
        let d = LevelDetails::read(
            &mut vfs
                .open("gamedata/levels/l01_escape/level.details")
                .unwrap(),
        )
        .unwrap();
        let h = d.header;
        assert_eq!(
            (
                h.version,
                h.model_count,
                h.offset_x,
                h.offset_z,
                h.size_x,
                h.size_z
            ),
            (3, 21, 340, 317, 724, 723)
        );
        let table: Vec<(u32, usize, usize, f32, f32)> = d
            .models
            .iter()
            .map(|m| {
                (
                    m.flags,
                    m.vertices.len(),
                    m.indices.len(),
                    m.min_scale,
                    m.max_scale,
                )
            })
            .collect();
        let want = [
            (0, 52, 102, 1.0, 1.5),
            (0, 36, 66, 1.5, 2.0),
            (0, 36, 66, 0.75, 1.1),
            (0, 52, 102, 0.75, 2.0),
            (0, 52, 102, 1.5, 2.0),
            (0, 52, 102, 1.0, 1.4),
            (0, 52, 102, 1.3, 2.0),
            (0, 52, 102, 1.0, 2.0),
            (0, 36, 66, 1.0, 1.3),
            (0, 52, 102, 1.25, 2.0),
            (0, 52, 102, 1.1, 1.5),
            (0, 52, 102, 1.5, 2.0),
            (0, 52, 102, 1.5, 2.0),
            (1, 4, 6, 0.8, 1.3),
            (1, 4, 6, 0.8, 1.3),
            (1, 4, 6, 0.8, 1.3),
            (1, 4, 6, 0.8, 1.3),
            (1, 8, 18, 0.8, 1.3),
            (1, 5, 12, 0.8, 1.3),
            (1, 5, 12, 0.8, 1.3),
            (1, 5, 12, 0.8, 1.3),
        ];
        for (i, (got, want)) in table.iter().zip(want).enumerate() {
            assert_eq!((got.0, got.1, got.2), (want.0, want.1, want.2), "model {i}");
            assert!(
                (got.3 - want.3).abs() < 1e-4 && (got.4 - want.4).abs() < 1e-4,
                "model {i}"
            );
        }
        assert_eq!(table.iter().map(|t| t.1).sum::<usize>(), 667);
        assert_eq!(table.iter().map(|t| t.2).sum::<usize>(), 1_296);
        let mut by_parts = [0u32; 5];
        let (mut dense, mut base0) = (0, 0);
        let (mut base, mut height, mut sun, mut hemi, mut density) = (0u64, 0u64, 0u64, 0u64, 0u64);
        let (mut low, mut top) = (f32::INFINITY, f32::NEG_INFINITY);
        for s in d.records() {
            let ids = s.ids();
            let parts = ids.iter().filter(|&&i| i != NO_MODEL).count();
            by_parts[parts] += 1;
            if (0..4).any(|p| ids[p] != NO_MODEL && s.densities(p) != [0; 4]) {
                dense += 1;
            }
            if s.base() == 0 {
                base0 += 1;
            } else {
                low = low.min(s.y_low());
                top = top.max(s.y_top());
            }
            base += u64::from(s.base());
            height += u64::from(s.height());
            sun += u64::from(s.sun());
            hemi += u64::from(s.hemi());
            density += (0..4)
                .flat_map(|p| s.densities(p))
                .map(u64::from)
                .sum::<u64>();
        }
        assert_eq!(by_parts, [0, 30, 365_446, 5_099, 152_877]);
        assert_eq!((dense, base0), (110_479, 262_894));
        assert_eq!(
            (base, height, sun, hemi),
            (281_470_427, 1_890_395, 1_355_876, 2_341_486)
        );
        assert_eq!(density, 1_591_645);
        assert!(
            (low - -37.6).abs() < 0.05 && (top - 83.3).abs() < 0.05,
            "{low} {top}"
        );
        // The grid's extent (4.1).
        let x0 = -2.0 * h.offset_x as f32;
        let z0 = -2.0 * h.offset_z as f32;
        assert_eq!((x0, x0 + 2.0 * h.size_x as f32), (-680.0, 768.0));
        assert_eq!((z0, z0 + 2.0 * h.size_z as f32), (-634.0, 812.0));
    });
}

fn first_level(vfs: &dyn Vfs) -> (Details, CollisionWorld) {
    let lib = gamemtl::parse(&read(vfs, "gamedata/gamemtl.xr").unwrap()).unwrap();
    let collision = CollisionWorld::read(
        vfs.open("gamedata/levels/l01_escape/level.cform").unwrap(),
        lib,
    )
    .unwrap();
    let grid = LevelDetails::read(
        &mut vfs
            .open("gamedata/levels/l01_escape/level.details")
            .unwrap(),
    )
    .unwrap();
    (Details::new(grid), collision)
}

#[test]
fn the_placement_vectors() {
    with_tree(|vfs| {
        let (mut details, collision) = first_level(vfs);
        // detail-objects.md 8.2: cells −8 … 7 on both axes. The vectors
        // come from a double-precision model; positions on a rounding
        // boundary may differ in rare cases, so the sums are held to a
        // small tolerance and the counts to a few instances.
        let cases = [
            (0.2, 3_526, -1_279.7, 3_393.87),
            (0.3, 1_981, -684.6, 1_893.56),
            (0.4, 1_059, -354.4, 993.49),
            (0.6, 633, -232.8, 599.86),
        ];
        let mut random = Generator::new(1);
        for (rho, count, heights, scales) in cases {
            let start = Instant::now();
            let mut n: u32 = 0;
            let (mut y, mut s) = (0f64, 0f64);
            let mut per_model: BTreeMap<u8, u32> = BTreeMap::new();
            for cz in -8..8 {
                for cx in -8..8 {
                    let slot = details.grid().slot(cx, cz);
                    let placed = details.place_cell([cx, cz], rho, &collision, &mut random);
                    for (part, list) in placed.iter().enumerate() {
                        for inst in list {
                            n += 1;
                            y += f64::from(inst.position[1]);
                            s += f64::from(inst.scale);
                            *per_model.entry(slot.ids()[part]).or_default() += 1;
                        }
                    }
                }
            }
            eprintln!(
                "ρ {rho}: {n} instances, heights {y:.1}, scales {s:.2} in {:.1?}",
                start.elapsed()
            );
            assert!(n.abs_diff(count) <= 3, "ρ {rho}: {n} instances");
            assert!((y - heights).abs() < 2.0, "ρ {rho}: heights {y}");
            assert!((s - scales).abs() < 2.0, "ρ {rho}: scales {s}");
            if rho == 0.3 {
                let want = BTreeMap::from([
                    (1, 28),
                    (4, 14),
                    (5, 9),
                    (6, 3),
                    (8, 326),
                    (9, 457),
                    (10, 3),
                    (12, 251),
                    (13, 218),
                    (14, 26),
                    (15, 172),
                    (16, 45),
                    (17, 32),
                    (18, 10),
                    (19, 244),
                    (20, 143),
                ]);
                for (m, w) in &want {
                    let got = per_model.get(m).copied().unwrap_or(0);
                    assert!(got.abs_diff(*w) <= 2, "model {m}: {got}, expected {w}");
                }
                assert_eq!(
                    per_model.keys().collect::<Vec<_>>(),
                    want.keys().collect::<Vec<_>>()
                );
            }
        }
    });
}

#[test]
fn the_cache_at_the_start_position() {
    with_tree(|vfs| {
        let mut random = Generator::new(1);
        let (mut details, collision) = first_level(vfs);
        // The New Game's start eye (start-position.md 8), looking along +x.
        let eye = [-246.7263, -23.26326, -134.43387];
        let mut vp = [[0.0f32; 4]; 4];
        // A 90° view along +x: clip x = −z, y = y, w = x (relative).
        vp[2][0] = -1.0;
        vp[1][1] = 1.0;
        vp[0][2] = 1.0 - 0.0001;
        vp[0][3] = 1.0;
        vp[3] = [eye[2], -eye[1], -eye[0] * (1.0 - 0.0001), -eye[0]];
        let ssa_min = 12.25 / (1448.0 * 1086.0);
        let pass = |frame| Pass {
            frame,
            eye,
            view_projection: vp,
            ssa_min,
            density: 0.2,
        };
        // The first pass after loading draws nothing (TODO(#407)).
        assert!(details.pass(&pass(1), &collision, &mut random).instances.is_empty());
        let start = Instant::now();
        let frame = details.pass(&pass(2), &collision, &mut random);
        let built = start.elapsed();
        assert_eq!(details.centre(), Details::camera_cell(eye));
        assert_eq!(details.pending(), 0);
        let placed = details.instances().count();
        let start = Instant::now();
        const FRAMES: u64 = 100;
        let mut drawn = 0;
        for f in 3..3 + FRAMES {
            drawn = details.pass(&pass(f), &collision, &mut random).instances.len();
        }
        let selection = start.elapsed() / FRAMES as u32;
        eprintln!(
            "the start's cache: {placed} instances placed in {built:.1?} (first update), {} \
             drawn then {drawn}; selection {selection:.1?} a frame; {:.1} MB held",
            frame.instances.len(),
            details.memory_bytes() as f64 / 1e6,
        );
        assert!(placed > 0 && drawn > 0);
        // Walking one cell along x: 49 new entries, at most 7 a frame.
        let mut walked = pass(3 + FRAMES);
        walked.eye[0] += 2.0;
        details.pass(&walked, &collision, &mut random);
        assert_eq!(details.pending(), 42);
        for f in 4 + FRAMES..10 + FRAMES {
            walked.frame = f;
            details.pass(&walked, &collision, &mut random);
        }
        assert_eq!(details.pending(), 0);
    });
}
