// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Motion playback from the Russian edition's data (BYOA): the
//! verification facts of docs/formats/skeletal-motions.md 6.2 (the
//! trader's `idle` and the crow's `norm_fly_fwd` evaluated) and of
//! docs/behavior/motion-playback.md section 9 (the trader's and the crow's
//! definitions, the ends of the trader's motions, a cross-fade on the
//! trader and a cross-fade and an fx motion on the crow), on a New Game's
//! level: the trader drawn posed. Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::sync::Arc;

use volya_engine::core::motion_playback::{
    AnimatedModel, Clock, Fade, MotionId, Motions, PlaybackId, Start,
};
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

/// The model of visual `visual` with its motions, and its bone names.
fn motions(tree: &dyn Vfs, data: &GameData, visual: &str) -> (Arc<Motions>, Vec<String>) {
    let path = locate_model(tree, data, "", visual).unwrap();
    let model = Model::parse(&read(tree, &path).unwrap()).unwrap();
    let names: Vec<String> = model.bones.iter().map(|b| b.name.clone()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let parents: Vec<Option<usize>> = model.bones.iter().map(|b| b.parent).collect();
    let binds: Vec<Mat4> = model.bind_pose().unwrap();
    let sets = read_motion_sets(tree, data, "", &path, names.len()).unwrap();
    let motions = Motions::new(&refs, &parents, &binds, sets);
    (Arc::new(motions), names)
}

fn clock(ms: u32) -> Clock {
    Clock {
        time_global: ms,
        frame: ms / 20,
    }
}

/// Bone `name`'s position: row 3 of *T*.
fn position(model: &AnimatedModel, names: &[String], name: &str) -> [f32; 3] {
    let mut pose = Vec::new();
    model.pose(&mut pose);
    let b = names.iter().position(|n| n == name).unwrap();
    [pose[b][3][0], pose[b][3][1], pose[b][3][2]]
}

fn sum(model: &AnimatedModel) -> [f32; 3] {
    let mut pose = Vec::new();
    model.pose(&mut pose);
    std::array::from_fn(|c| pose.iter().map(|t| t[3][c]).sum())
}

#[track_caller]
fn near3(got: [f32; 3], want: [f32; 3], tolerance: f32, what: &str) {
    assert!(
        (0..3).all(|i| (got[i] - want[i]).abs() <= tolerance),
        "{what}: {got:?} != {want:?}"
    );
}

#[track_caller]
fn near(got: f32, want: f32, tolerance: f32, what: &str) {
    assert!((got - want).abs() <= tolerance, "{what}: {got} != {want}");
}

/// `name`'s cycle started alone, without mixing, at time `t`.
fn alone(motions: &Arc<Motions>, name: &str, t: f32) -> AnimatedModel {
    let mut m = AnimatedModel::new(motions.clone());
    let id = motions.cycle(name).unwrap();
    let start = Start {
        mixing: false,
        ..Start::mixing(None)
    };
    m.play_cycle(id, start, clock(0)).unwrap();
    let ids: Vec<PlaybackId> = m.playbacks().map(|(i, _)| i).collect();
    for i in ids {
        m.playback_mut(i).unwrap().time = t;
    }
    m
}

#[test]
fn the_pose_at_a_time() {
    with_game(|tree, data| {
        // skeletal-motions 6.2: the trader's `idle`, definition 24.
        let (trader, names) = motions(tree, data, "actors\\trader\\trader");
        assert_eq!(
            trader.cycle("idle"),
            Some(MotionId {
                set: 0,
                definition: 24
            })
        );
        let m = alone(&trader, "idle", 0.0);
        near3(
            position(&m, &names, "bip01_pelvis"),
            [0.0, 0.5446, -0.1096],
            2e-4,
            "pelvis",
        );
        near3(
            position(&m, &names, "bip01_head"),
            [-0.0311, 1.1592, 0.0958],
            2e-4,
            "head",
        );
        near3(sum(&m), [-3.9849, 49.9574, 11.7492], 1e-3, "sum");
        let m = alone(&trader, "idle", 1.25);
        near3(
            position(&m, &names, "bip01_head"),
            [-0.0309, 1.1632, 0.0897],
            2e-4,
            "head 1.25",
        );
        near3(sum(&m), [-3.9936, 50.0255, 11.6602], 1e-3, "sum 1.25");
        // The crow's `norm_fly_fwd`.
        let (crow, names) = motions(tree, data, "monsters\\crow\\crow");
        let m = alone(&crow, "norm_fly_fwd", 0.0);
        near3(
            position(&m, &names, "right_leg2"),
            [0.0380, -0.0227, -0.0537],
            2e-4,
            "leg",
        );
        near3(sum(&m), [-0.0054, 0.7885, -0.0979], 1e-3, "crow sum");
        let m = alone(&crow, "norm_fly_fwd", 0.5);
        near3(
            position(&m, &names, "right_leg2"),
            [0.0380, 0.0047, -0.0537],
            2e-4,
            "leg 0.5",
        );
        near3(
            position(&m, &names, "root"),
            [0.0, 0.0274, 0.0],
            2e-4,
            "root 0.5",
        );
        near3(sum(&m), [0.0020, 0.7121, -0.1300], 1e-3, "crow sum 0.5");
    });
}

#[test]
fn the_definitions() {
    with_game(|tree, data| {
        // motion-playback 9.1.
        let (trader, _) = motions(tree, data, "actors\\trader\\trader");
        assert_eq!(trader.partition_count(), 2);
        let mut on = [0; 3];
        for k in 0..65 {
            let d = trader.definition(MotionId {
                set: 0,
                definition: k,
            });
            assert_eq!(d.flags, 2, "{}", d.name);
            let fast = [
                "talk_kind_0",
                "talk_glad_0",
                "talk_1",
                "talk_0",
                "talk_angry_0",
            ]
            .iter()
            .chain(&["talk_2", "talk_3"])
            .any(|n| *n == d.name);
            assert_eq!(d.speed, if fast { 2.0 } else { 1.0 }, "{}", d.name);
            on[match d.bone_or_partition {
                0xFFFF => 0,
                p => usize::from(p) + 1,
            }] += 1;
        }
        assert_eq!(on, [34, 22, 9]);
        let mut m = AnimatedModel::new(trader.clone());
        let id = trader.cycle("talk_0").unwrap();
        let p = m
            .play_cycle(id, Start::mixing(None), clock(0))
            .unwrap()
            .unwrap();
        let p = m.playback(p).unwrap();
        near(p.speed, 1.998_931_9, 1e-6, "speed");
        near(p.accrue, 2.998_397_8, 1e-6, "accrue");
        near(p.cycle_falloff, 2.996_109, 1e-6, "falloff");
        // The crow's `hit`: effective power and falloff, not lowered.
        let (crow, _) = motions(tree, data, "monsters\\crow\\crow");
        let mut m = AnimatedModel::new(crow.clone());
        let hit = m.play_fx("hit", 1.0).unwrap().unwrap();
        let p = m.playback(hit).unwrap();
        near(p.target, 1.998_931_9, 1e-6, "power");
        near(p.falloff, 2.998_397_8, 1e-6, "fx falloff");
        assert_eq!(m.bone_playbacks(0).count(), 1);
        assert_eq!(
            (0..crow.bone_count())
                .filter(|&b| m.bone_playbacks(b).count() == 1)
                .count(),
            crow.bone_count(),
            "the root and every bone below it"
        );
    });
}

#[test]
fn the_ends_of_the_traders_motions() {
    with_game(|tree, data| {
        // motion-playback 9.2.
        let (trader, _) = motions(tree, data, "actors\\trader\\trader");
        for (name, end, notified) in [
            ("idle_2", 399, [0, 1].as_slice()),
            ("spinka_idle", 82, &[0, 1]),
            ("vstuplenie", 1168, &[0]),
            ("vstuplenie_head", 1168, &[1]),
            ("talk_0", 25, &[1]),
        ] {
            let mut m = AnimatedModel::new(trader.clone());
            let id = trader.cycle(name).unwrap();
            m.play_cycle(id, Start::mixing(Some(7)), clock(0)).unwrap();
            let mut at = None;
            for k in 1..=1300u32 {
                m.update_tracks(clock(20 * k));
                let n = m.take_notifications();
                if n.is_empty() {
                    continue;
                }
                assert!(at.is_none(), "{name}: notified again at {k}");
                at = Some(k);
                let partitions: Vec<u8> = n.iter().map(|n| n.partition.unwrap()).collect();
                assert_eq!(partitions, notified, "{name}");
                assert!(n.iter().all(|n| n.value == 7));
            }
            assert_eq!(at, Some(end), "{name}");
        }
    });
}

/// The playbacks of `m` as (time, weight, fading in) per partition.
fn states(m: &AnimatedModel, partition: usize) -> Vec<(f32, f32, bool)> {
    m.partition_playbacks(partition)
        .map(|p| (p.time, p.weight, p.fade == Fade::In))
        .collect()
}

#[test]
fn a_cross_fade_on_the_trader() {
    with_game(|tree, data| {
        // motion-playback 9.3.
        let (trader, names) = motions(tree, data, "actors\\trader\\trader");
        let mut m = AnimatedModel::new(trader.clone());
        let idle = trader.cycle("idle_2").unwrap();
        let spinka = trader.cycle("spinka_idle").unwrap();
        m.play_cycle(idle, Start::mixing(Some(1)), clock(0))
            .unwrap();
        let head = |m: &AnimatedModel| position(m, &names, "bip01_head");
        let mut notified = Vec::new();
        for k in 1..=182u32 {
            m.update_tracks(clock(20 * k));
            notified.extend(m.take_notifications());
            if k == 100 {
                m.play_cycle(spinka, Start::mixing(Some(2)), clock(20 * k))
                    .unwrap();
            }
            let s = states(&m, 0);
            match k {
                1 => {
                    near(s[0].0, 0.01999, 1e-5, "1: time");
                    near(s[0].1, 0.05997, 1e-5, "1: weight");
                    near3(head(&m), [-0.0293, 1.1716, -0.3809], 2e-4, "1");
                }
                100 => {
                    assert!(!s[0].2 && s[1].2);
                    near(s[0].0, 1.99893, 1e-5, "100: time");
                    assert_eq!((s[0].1, s[1].0, s[1].1), (1.0, 0.0, 1e-7));
                    near3(head(&m), [-0.0025, 1.1712, -0.3770], 2e-4, "100");
                }
                105 => {
                    near(s[0].1, 0.70039, 1e-5, "105: weight out");
                    near(s[1].0, 0.09995, 1e-5, "105: time");
                    near(s[1].1, 0.29984, 1e-5, "105: weight in");
                    near3(head(&m), [-0.0095, 1.1764, -0.3666], 2e-4, "105");
                }
                117 => {
                    assert_eq!(s.len(), 1, "117: idle_2 removed");
                    near(s[0].0, 0.33982, 1e-5, "117: time");
                    assert_eq!(s[0].1, 1.0);
                    near3(head(&m), [-0.0142, 1.2143, -0.2443], 2e-4, "117");
                }
                120 => near3(head(&m), [-0.0117, 1.2179, -0.2191], 2e-4, "120"),
                182 => {
                    near(s[0].0, 1.63333, 1e-5, "182: time");
                    near3(head(&m), [-0.0320, 1.1590, 0.0967], 2e-4, "182");
                }
                _ => {}
            }
        }
        // idle_2 faded out before its end never notifies; spinka_idle for
        // partitions 0 and 1 at update 182.
        let got: Vec<(u32, u8)> = notified
            .iter()
            .map(|n| (n.value, n.partition.unwrap()))
            .collect();
        assert_eq!(got, [(2, 0), (2, 1)]);
        assert!(m.partition_playbacks(0).all(|p| !p.advancing));
    });
}

#[test]
fn a_cross_fade_and_an_fx_motion_on_the_crow() {
    with_game(|tree, data| {
        // motion-playback 9.4.
        let (crow, names) = motions(tree, data, "monsters\\crow\\crow");
        let mut m = AnimatedModel::new(crow.clone());
        let fly = crow.cycle("norm_fly_fwd").unwrap();
        let idle = crow.cycle("norm_idle").unwrap();
        m.play_cycle(fly, Start::mixing(None), clock(0)).unwrap();
        let at = |m: &AnimatedModel, bone| position(m, &names, bone);
        for k in 1..=114u32 {
            m.update_tracks(clock(20 * k));
            if k == 50 {
                m.play_cycle(idle, Start::mixing(None), clock(20 * k))
                    .unwrap();
            }
            if k == 80 {
                m.play_fx("hit", 1.0).unwrap();
            }
            let fx: Vec<(f32, f32, bool)> = m
                .fx_playbacks()
                .map(|p| (p.time, p.weight, p.fade == Fade::In))
                .collect();
            let rows: &[(u32, [f32; 3], [f32; 3])] = &[
                (25, [0.0, 0.0274, 0.0], [0.0380, 0.0047, -0.0537]),
                (55, [0.0, 0.0217, 0.0], [0.0346, -0.0016, -0.0537]),
                (70, [0.0, 0.0, 0.0], [0.0266, -0.0247, -0.0535]),
                (85, [0.0, 0.0810, 0.0], [0.0101, 0.1078, -0.0643]),
                (97, [0.0, 0.1388, 0.0], [-0.0011, 0.2030, -0.0370]),
                (113, [0.0, 0.0068, 0.0], [0.0235, -0.0092, -0.0595]),
                (114, [0.0, 0.0, 0.0], [0.0266, -0.0247, -0.0535]),
            ];
            if let Some((_, root, leg)) = rows.iter().find(|r| r.0 == k) {
                near3(at(&m, "root"), *root, 2e-4, &format!("{k}: root"));
                near3(at(&m, "right_leg2"), *leg, 2e-4, &format!("{k}: leg"));
            }
            let s = states(&m, 0);
            match k {
                55 => {
                    near(s[0].0, 1.09941, 1e-5, "55: fly time");
                    near(s[0].1, 0.70039, 1e-5, "55: fly weight");
                    assert!(!s[0].2);
                    near(s[1].1, 0.29984, 1e-5, "55: idle weight");
                }
                85 => {
                    near(fx[0].0, 0.09995, 1e-5, "85: hit time");
                    near(fx[0].1, 0.59904, 1e-4, "85: hit weight");
                    assert!(fx[0].2);
                }
                97 => {
                    near(fx[0].1, 1.99893, 1e-5, "97: hit weight");
                    assert!(!fx[0].2);
                }
                113 => near(fx[0].1, 0.08201, 1e-4, "113: hit weight"),
                114 => {
                    assert!(fx.is_empty(), "114: hit ended");
                    near(s[0].0, 1.27932, 1e-5, "114: idle time");
                }
                _ => {}
            }
        }
    });
}

#[test]
fn the_trader_plays_on_the_level() {
    with_game(|tree, data| {
        let mut screen = LevelScreen::load(tree, data, 0).unwrap();
        assert!(
            !screen
                .play_cycle(tree, data, "escape_trader", "no such cycle")
                .unwrap()
        );
        assert!(
            screen
                .play_cycle(tree, data, "escape_trader", "idle_2")
                .unwrap()
        );
        let mut now = 0;
        for _ in 0..30 {
            now += 20;
            screen.set_global_clock(now);
            screen.update(now, &Default::default());
        }
        let k = (screen.world().objects.instances.iter())
            .position(|i| i.name == "escape_trader")
            .unwrap();
        let m = screen.motions().model(k).unwrap();
        let t = m.partition_playbacks(0).next().unwrap().time;
        near(t, 0.6 * 0.999_465_94, 1e-4, "idle_2 time");
        let poses = screen.motions().poses();
        let first = poses.first[k].unwrap() as usize;
        assert_eq!(poses.matrices.len(), m.motions().bone_count());
        assert_eq!(first, 0);
        assert!(poses.first.iter().filter(|f| f.is_some()).count() == 1);
        // A bone moved from its bind pose: K is not the identity.
        assert!(poses.matrices.iter().any(|k| (k[0][0] - 1.0).abs() > 1e-3));
    });
}
