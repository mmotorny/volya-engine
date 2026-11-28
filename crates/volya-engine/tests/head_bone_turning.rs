// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Head bone turning from the Russian edition's data (BYOA): the worked
//! example of docs/behavior/head-bone-turning.md section 6 (the trader at
//! the start of a New Game turning his head toward the player) and the
//! level's trader drawn with his head turned. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeSet;
use std::sync::Arc;

use volya_engine::core::head_bone_turning::{HEAD_BONE, turn, turning_angle};
use volya_engine::core::motion_playback::{AnimatedModel, Clock, Motions, PlaybackId, Start};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, read};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_motions::read_motion_sets;
use volya_engine::objects::locate_model;
use volya_engine_formats::model::{Mat4, Model};
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

fn row(m: &Mat4, r: usize) -> [f32; 3] {
    [m[r][0], m[r][1], m[r][2]]
}

/// The heading of direction `d` (forward (−sin *h*, 0, cos *h*)).
fn heading_of(d: [f32; 3]) -> f32 {
    (-d[0]).atan2(d[2])
}

#[test]
fn the_worked_example() {
    with_game(|tree, data| {
        let screen = LevelScreen::load(tree, data, 0).unwrap();
        let trader = (screen.world().objects.instances.iter())
            .find(|i| i.name == "escape_trader")
            .unwrap();
        let x = trader.transform;
        // At his online height (object-visual-and-transform.md 4.5).
        near3(
            row(&x, 3),
            [-244.6927, -24.864_836, -133.96],
            1e-3,
            "trader",
        );
        let player = screen.player().state().position;
        near3(
            [player[0], 0.0, player[2]],
            [-246.7263, 0.0, -134.4339],
            1e-3,
            "player",
        );
        let a = turning_angle(player, &x);
        assert!((a - 0.250_533_6).abs() < 1e-5, "A = {a}");

        // The desk motion `idle` (all partitions) at t = 0.
        let path = locate_model(tree, data, "", &trader.visual).unwrap();
        let model = Model::parse(&read(tree, &path).unwrap()).unwrap();
        let names: Vec<String> = model.bones.iter().map(|b| b.name.clone()).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let parents: Vec<Option<usize>> = model.bones.iter().map(|b| b.parent).collect();
        let binds = model.bind_pose().unwrap();
        let sets = read_motion_sets(tree, data, "", &path, names.len()).unwrap();
        let motions = Arc::new(Motions::new(&refs, &parents, &binds, sets));
        let head = names.iter().position(|n| n == HEAD_BONE).unwrap();
        assert_eq!((head, names.len()), (12, 55));
        assert_eq!(names[parents[head].unwrap()], "bip01_neck");

        let mut m = AnimatedModel::new(motions.clone());
        let idle = motions.cycle("idle").unwrap();
        let start = Start {
            mixing: false,
            ..Start::mixing(None)
        };
        let clock = Clock {
            time_global: 0,
            frame: 0,
        };
        m.play_cycle(idle, start, clock).unwrap();
        let ids: Vec<PlaybackId> = m.playbacks().map(|(i, _)| i).collect();
        for i in ids {
            m.playback_mut(i).unwrap().time = 0.0;
        }
        let (mut plain, mut turned) = (Vec::new(), Vec::new());
        m.pose(&mut plain);
        m.pose_following(&mut turned, |b, t| {
            if b == head {
                turn(t, a);
            }
        });

        // The head bone keeps its position and turns by about A.
        near3(
            row(&turned[head], 3),
            [-0.0311, 1.1592, 0.0958],
            1e-4,
            "head",
        );
        let world = std::array::from_fn(|c| {
            (0..3).map(|k| turned[head][3][k] * x[k][c]).sum::<f32>() + x[3][c]
        });
        near3(
            world,
            // Raised with him by his online height (object-visual-and-transform.md 4.5).
            [-244.789, -23.799 + 0.0932, -133.989],
            1e-3,
            "head in the world",
        );
        let z = heading_of(row(&turned[head], 2)) - heading_of(row(&plain[head], 2));
        assert!((z - 0.2515).abs() < 1e-3, "the head's z axis turned {z}");

        // Its descendants are the partition `head`'s 10 bones and turn with
        // it; no other bone moves.
        let descends = |mut b: usize| {
            while let Some(p) = parents[b] {
                if p == head {
                    return true;
                }
                b = p;
            }
            false
        };
        let subtree: BTreeSet<&str> = (0..names.len())
            .filter(|&b| descends(b))
            .map(|b| names[b].as_str())
            .collect();
        let head_partition: BTreeSet<&str> = [
            "bip01_ponytail1",
            "bip01_ponytail2",
            "bone01",
            "bone02",
            "bone04",
            "bone06",
            "bone08",
            "bone10",
            "bone12",
            "bone14",
        ]
        .into();
        assert_eq!(subtree, head_partition);
        let bone14 = names.iter().position(|n| n == "bone14").unwrap();
        near3(
            row(&plain[bone14], 3),
            [-0.0367, 1.2541, 0.1915],
            1e-4,
            "bone14",
        );
        near3(
            row(&turned[bone14], 3),
            [-0.0573, 1.2553, 0.1868],
            1e-4,
            "bone14 turned",
        );
        for b in (0..names.len()).filter(|&b| b != head && !descends(b)) {
            assert_eq!(plain[b], turned[b], "{}", names[b]);
        }
    });
}

#[test]
fn the_level_turns_the_trader_head() {
    with_game(|tree, data| {
        let mut screen = LevelScreen::load(tree, data, 0).unwrap();
        assert!(
            screen
                .play_cycle(tree, data, "escape_trader", "idle")
                .unwrap()
        );
        for k in 1..=5 {
            screen.set_global_clock(20 * k);
            screen.update(20 * k, &Default::default());
        }
        let k = (screen.world().objects.instances.iter())
            .position(|i| i.name == "escape_trader")
            .unwrap();
        let x = screen.world().objects.instances[k].transform;
        let callback = *screen.motions().head_callback(k).unwrap();
        assert_eq!((callback.bone, callback.transform), (12, x));
        let a = turning_angle(screen.player().state().position, &x);
        let m = screen.motions().model(k).unwrap();
        let (mut pose, mut render) = (Vec::new(), Vec::new());
        m.pose_following(&mut pose, |b, t| {
            if b == callback.bone {
                turn(t, a);
            }
        });
        m.render_matrices(&pose, &mut render);
        let poses = screen.motions().poses();
        let first = poses.first[k].unwrap() as usize;
        for (b, kb) in render.iter().enumerate() {
            let drawn = &poses.matrices[first + b];
            for c in 0..3 {
                for r in 0..4 {
                    assert_eq!(drawn[c][r], kb[r][c], "bone {b}");
                }
            }
        }
        // The turn shows: the head's drawn matrix is not the motions' alone.
        let mut plain = Vec::new();
        m.pose(&mut plain);
        assert!((0..3).any(|r| (0..3).any(|c| (plain[12][r][c] - pose[12][r][c]).abs() > 0.1)));
    });
}
