// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use volya_engine_formats::skeletal_motions::{MotionDefinition, Partition, Track};

/// A track whose rotation turns about y by `angle` × key and whose
/// translation is (key, 0, 0) · `step`.
fn track(keys: u32, angle: f32, step: f32) -> Track {
    let rotations = (0..keys)
        .map(|k| {
            let h = angle * k as f32 / 2.0;
            [0.0, h.sin(), 0.0, h.cos()].map(|c| (c * 32767.0).round() as i16)
        })
        .collect();
    let translations = (0..keys).map(|k| [k as i8, 0, 0]).collect();
    Track {
        rotations,
        translations,
        size: [step, 0.0, 0.0],
        base: [0.0, 1.0, 0.0],
    }
}

fn definition(name: &str, flags: u16, bone_or_partition: u16, power: f32) -> MotionDefinition {
    MotionDefinition {
        name: name.into(),
        flags,
        bone_or_partition,
        motion_index: 0,
        speed: 1.0,
        power,
        accrue: 2.0,
        falloff: 2.0,
    }
}

/// Bones `root` → `spine` → `head`, `arm` under `root`; partition 0 holds
/// root and spine, partition 1 head; `arm` is in none.
fn motions() -> Arc<Motions> {
    let defs = vec![
        definition("loop", 0, 0xFFFF, 1.0),
        definition("stop", STOP_AT_END, 0, 1.0),
        definition("hit", 1, 0xFFFF, 2.0),
        definition("other", STOP_AT_END, 1, 1.0),
    ];
    let keys = [10, 10, 10, 31];
    let motions = keys
        .iter()
        .enumerate()
        .map(|(m, &n)| Motion {
            keys: n,
            tracks: (0..4)
                .map(|t| track(n, 0.1 * (m + 1) as f32, 0.01 * (t + 1) as f32))
                .collect(),
        })
        .collect();
    let params = MotionParams {
        version: 3,
        partitions: vec![
            Partition {
                name: "torso".into(),
                bones: vec![("root".into(), 0), ("spine".into(), 1)],
            },
            Partition {
                name: "head".into(),
                bones: vec![("head".into(), 2), ("arm".into(), 3)],
            },
        ],
        definitions: defs,
    };
    let names = ["root", "spine", "head", "arm"];
    let parents = [None, Some(0), Some(1), Some(0)];
    let mut binds = vec![IDENTITY; 4];
    binds[1][3] = [0.0, 1.0, 0.0, 1.0];
    Arc::new(Motions::new(
        &names,
        &parents,
        &binds,
        vec![(params, motions)],
    ))
}

const LOOP: MotionId = MotionId {
    set: 0,
    definition: 0,
};
const STOP: MotionId = MotionId {
    set: 0,
    definition: 1,
};

fn clock(ms: u32) -> Clock {
    Clock {
        time_global: ms,
        frame: ms,
    }
}

#[track_caller]
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-5, "{a} != {b}");
}

#[test]
fn values_from_the_definition() {
    // 2.1: stored 1.0 and 2.0; a cycle's falloff lowered below accrue.
    near(speed_value(quantize(1.0)), 0.999_465_94);
    near(speed_value(quantize(2.0)), 1.998_931_9);
    near(rate_value(quantize(1.0)), 1.499_198_9);
    near(rate_value(quantize(2.0)), 2.998_397_8);
    let mut m = AnimatedModel::new(motions());
    let id = m
        .play_cycle(STOP, Start::mixing(None), clock(0))
        .unwrap()
        .unwrap();
    let p = m.playback(id).unwrap();
    near(p.cycle_falloff, 2.996_109);
    assert_eq!((p.falloff, p.target, p.weight), (0.0, 1.0, START_WEIGHT));
    assert!(p.stop_at_end && !p.fade_at_end && p.advancing);
    near(p.length, 10.0 / 30.0);
}

#[test]
fn a_cycle_on_all_partitions_starts_one_per_partition() {
    let mut m = AnimatedModel::new(motions());
    // 3.2: none handed back; each partition its own playback.
    assert_eq!(
        m.play_cycle(LOOP, Start::mixing(Some(7)), clock(0)),
        Ok(None)
    );
    assert_eq!(m.partition_playbacks(0).count(), 1);
    assert_eq!(m.partition_playbacks(1).count(), 1);
    // 3.1 step 4: applied to the partition's bones in entry order.
    assert_eq!(m.bone_playbacks(0).count(), 1);
    assert_eq!(m.bone_playbacks(3).count(), 1);
    // 3.1 step 1: a partition the model lacks starts nothing.
    let start = Start {
        partition: Some(2),
        ..Start::mixing(None)
    };
    assert_eq!(m.play_cycle(STOP, start, clock(0)), Ok(None));
    let start = Start {
        partition: Some(4),
        ..Start::mixing(None)
    };
    assert_eq!(m.play_cycle(STOP, start, clock(0)), Ok(None));
    assert_eq!(m.playbacks().count(), 2);
}

#[test]
fn the_tracks_update_advances_by_at_most_66_ms() {
    let mut m = AnimatedModel::new(motions());
    let id = m
        .play_cycle(STOP, Start::mixing(None), clock(0))
        .unwrap()
        .unwrap();
    m.update_tracks(clock(20));
    near(m.playback(id).unwrap().time, 0.02 * 0.999_465_94);
    // The same time_global: nothing.
    m.update_tracks(Clock {
        time_global: 20,
        frame: 99,
    });
    near(m.playback(id).unwrap().time, 0.02 * 0.999_465_94);
    // A second later: 66 ms.
    m.update_tracks(clock(1020));
    near(m.playback(id).unwrap().time, 0.086 * 0.999_465_94);
    // Once per frame: a new time in the same frame does not advance it.
    m.update_tracks(Clock {
        time_global: 1040,
        frame: 1020,
    });
    near(m.playback(id).unwrap().time, 0.086 * 0.999_465_94);
}

#[test]
fn a_mixing_start_fades_the_old_cycle_out() {
    let mut m = AnimatedModel::new(motions());
    let old = m
        .play_cycle(STOP, Start::mixing(None), clock(0))
        .unwrap()
        .unwrap();
    let mut ms = 0;
    for _ in 0..100 {
        ms += 20;
        m.update_tracks(clock(ms));
    }
    let t = m.playback(old).unwrap().time;
    let new = m
        .play_cycle(STOP, Start::mixing(None), clock(ms))
        .unwrap()
        .unwrap();
    let p = m.playback(old).unwrap();
    // 3.3: falling at the new cycle's falloff; stop at end stops it.
    assert_eq!(p.fade, Fade::Out);
    near(p.falloff, 2.996_109);
    assert!(!p.advancing);
    for k in 1..=17 {
        ms += 20;
        m.update_tracks(clock(ms));
        let weight = m.playback(new).unwrap().weight;
        if k == 5 {
            // 9.3, update 105.
            near(m.playback(old).unwrap().weight, 0.700_389);
            near(m.playback(old).unwrap().time, t);
            near(weight, 0.299_839_9);
        }
        if k < 17 {
            assert!(weight < 1.0 && m.playback(old).is_some(), "update {k}");
        }
    }
    // The 17th update: full weight, and the old one removed.
    assert_eq!(m.playback(new).unwrap().weight, 1.0);
    assert!(m.playback(old).is_none());
    assert_eq!(m.bone_playbacks(0).count(), 1);
}

#[test]
fn a_start_without_mixing_replaces_at_once() {
    let mut m = AnimatedModel::new(motions());
    let old = m
        .play_cycle(STOP, Start::mixing(None), clock(0))
        .unwrap()
        .unwrap();
    let start = Start {
        mixing: false,
        ..Start::mixing(None)
    };
    let new = m.play_cycle(STOP, start, clock(0)).unwrap().unwrap();
    assert!(m.playback(old).is_none() || old == new);
    assert_eq!(m.playback(new).unwrap().weight, 1.0);
    assert_eq!(m.partition_playbacks(0).count(), 1);
    // Channels 1 – 3 end nothing.
    let side = Start {
        channel: 1,
        ..Start::mixing(None)
    };
    m.play_cycle(STOP, side, clock(0)).unwrap();
    assert_eq!(m.partition_playbacks(0).count(), 2);
}

#[test]
fn a_stop_at_end_cycle_holds_its_last_key_and_notifies_once() {
    let mut m = AnimatedModel::new(motions());
    let id = m
        .play_cycle(STOP, Start::mixing(Some(42)), clock(0))
        .unwrap()
        .unwrap();
    let mut ms = 0;
    let mut ended = None;
    for k in 1..40 {
        ms += 20;
        m.update_tracks(clock(ms));
        let n = m.take_notifications();
        if !n.is_empty() {
            assert!(ended.is_none(), "notified twice");
            assert_eq!(n[0].value, 42);
            assert_eq!(n[0].playback, id);
            ended = Some(k);
        }
    }
    // 5.1: the first update past (N − 1) / 30 = 0.3 s: 16 · 0.02 · 0.99947.
    assert_eq!(ended, Some(16));
    let p = m.playback(id).unwrap();
    near(p.time, 9.0 / 30.0);
    assert!(!p.advancing);
    assert_eq!((p.weight, p.fade), (1.0, Fade::In));
}

#[test]
fn a_cycle_on_another_channel_fades_at_its_end() {
    let mut m = AnimatedModel::new(motions());
    let start = Start {
        channel: 2,
        ..Start::mixing(None)
    };
    let id = m.play_cycle(STOP, start, clock(0)).unwrap().unwrap();
    assert!(m.playback(id).unwrap().fade_at_end);
    let mut ms = 0;
    for _ in 0..16 {
        ms += 20;
        m.update_tracks(clock(ms));
    }
    let p = m.playback(id).unwrap();
    assert_eq!((p.fade, p.falloff), (Fade::Out, END_FALLOFF));
    // From the weight it had (0.96 − 0.04 in that update) at falloff 2:
    // removed 23 updates later.
    near(p.weight, 1e-7 + 16.0 * 0.02 * 2.998_397_8 - 0.04);
    for _ in 0..22 {
        ms += 20;
        m.update_tracks(clock(ms));
        assert!(m.playback(id).is_some());
    }
    for _ in 0..1 {
        ms += 20;
        m.update_tracks(clock(ms));
    }
    assert!(m.playback(id).is_none());
}

#[test]
fn an_fx_motion_rises_and_falls() {
    let mut m = AnimatedModel::new(motions());
    assert_eq!(
        m.play_fx("nothing", 1.0),
        Err(Fatal::NoFx("nothing".into()))
    );
    let id = m.play_fx("hit", 1.0).unwrap().unwrap();
    // 6: P = power · k; applied to the root and every bone below it.
    near(m.playback(id).unwrap().target, 1.998_931_9);
    for b in 0..4 {
        assert_eq!(m.bone_playbacks(b).count(), 1, "bone {b}");
    }
    let mut ms = 0;
    let mut turned = None;
    let mut ended = None;
    for k in 1..60 {
        ms += 20;
        m.update_tracks(clock(ms));
        match m.playback(id) {
            Some(p) if p.fade == Fade::Out && turned.is_none() => {
                near(p.weight, p.target);
                turned = Some(k);
            }
            None if ended.is_none() => ended = Some(k),
            _ => {}
        }
    }
    // 1 / (accrue · speed) = 0.33369 s rising and as long falling.
    assert_eq!(turned, Some(17));
    assert_eq!(ended, Some(34));
    assert_eq!(m.bone_playbacks(2).count(), 0);
}

#[test]
fn limits() {
    let mut m = AnimatedModel::new(motions());
    for _ in 0..16 {
        assert!(m.play_fx("hit", 1.0).unwrap().is_some());
    }
    // 6: a 17th fx starts nothing.
    assert_eq!(m.play_fx("hit", 1.0), Ok(None));
    // 8: a bone holds 16; a 17th replaces the smallest weight there only.
    let side = Start {
        channel: 1,
        ..Start::mixing(None)
    };
    let id = m.play_cycle(STOP, side, clock(0)).unwrap().unwrap();
    assert_eq!(m.bone_playbacks(0).count(), 16);
    // Fade at end: not applied to a full bone.
    assert!(!m.bone_playbacks(0).any(|p| p == id));
    let start = Start {
        channel: 0,
        ..Start::mixing(None)
    };
    let id = m.play_cycle(STOP, start, clock(0)).unwrap().unwrap();
    let bone0: Vec<_> = m.bone_playbacks(0).collect();
    assert_eq!(bone0.len(), 16);
    assert_eq!(bone0[0], id, "in the first fx's place");
    // The replaced fx keeps its other bones.
    assert_eq!(m.bone_playbacks(2).count(), 16);
    // 8: 256 playbacks in all, then fatal.
    let mut m = AnimatedModel::new(motions());
    for _ in 0..256 {
        m.play_cycle(STOP, side, clock(0)).unwrap();
    }
    assert_eq!(
        m.play_cycle(STOP, side, clock(0)),
        Err(Fatal::TooManyPlaybacks)
    );
}

#[test]
fn bones_blend_by_weight() {
    let v = |x: f32| -> Value { ([0.0, 0.0, 0.0, 1.0], [x, 0.0, 0.0]) };
    // 7.1: one value whatever its weight; two by a₁ / (a₀ + a₁).
    assert_eq!(combine(&mut [(v(3.0), 1e-7)]), v(3.0));
    let two = combine(&mut [(v(0.0), 0.75), (v(4.0), 0.25)]);
    near(two.1[0], 1.0);
    assert_eq!(combine(&mut [(v(0.0), 0.0), (v(4.0), 0.0)]).1[0], 0.0);
    // Three or more: by decreasing weight.
    let three = combine(&mut [(v(0.0), 0.25), (v(4.0), 0.5), (v(8.0), 0.25)]);
    // 4, then 0 at 0.25 / 0.75, then 8 at 0.25: (4 · 2/3) · 0.75 + 2.
    near(three.1[0], 4.0);
    assert_eq!(combine(&mut []), ([0.0; 4], [0.0; 3]));
}

#[test]
fn the_pose_follows_the_hierarchy() {
    let mut m = AnimatedModel::new(motions());
    let mut pose = Vec::new();
    // 7.3: no playback: every L the identity.
    m.pose(&mut pose);
    assert!(pose.iter().all(|t| *t == IDENTITY));
    m.play_cycle(LOOP, Start::mixing(None), clock(0)).unwrap();
    m.pose(&mut pose);
    // At t = 0 every bone is at its key 0: translation (0, 1, 0) under
    // its parent's.
    near(pose[0][3][1], 1.0);
    near(pose[1][3][1], 2.0);
    near(pose[2][3][1], 3.0);
    near(pose[3][3][1], 2.0);
    // K = B⁻¹ · T: bone 1's bind at (0, 1, 0).
    let mut k = Vec::new();
    m.render_matrices(&pose, &mut k);
    near(k[1][3][1], 1.0);
    // Halfway between keys 2 and 3 of the looping cycle (8 / 30 s ahead
    // its translation is (2.5 · 0.01, 1, 0)).
    m.update_tracks(clock(66));
    m.update_tracks(clock(85));
    let t = m.partition_playbacks(0).next().unwrap().time;
    m.pose(&mut pose);
    let f = t * 30.0;
    near(pose[0][3][0], 0.01 * f);
}

#[test]
fn another_channel_adds_to_channel_0() {
    let mut m = AnimatedModel::new(motions());
    m.play_cycle(LOOP, Start::mixing(None), clock(0)).unwrap();
    let side = Start {
        channel: 1,
        partition: Some(0),
        ..Start::mixing(None)
    };
    let stop = m.play_cycle(STOP, side, clock(0)).unwrap().unwrap();
    // At t = 0 a channel's value relative to its key 0 is no change.
    let mut pose = Vec::new();
    m.pose(&mut pose);
    near(pose[0][3][1], 1.0);
    near(pose[0][3][0], 0.0);
    for ms in [66, 132, 198] {
        m.update_tracks(clock(ms));
    }
    let t = m.playback(stop).unwrap().time;
    let loop_t = m.partition_playbacks(0).next().unwrap().time;
    m.pose(&mut pose);
    // Translations add: loop's (0.01 · 30 t) plus stop's relative one.
    near(pose[0][3][0], 0.01 * 30.0 * loop_t + 0.01 * 30.0 * t);
    m.factors[1] = 0.5;
    m.pose(&mut pose);
    near(pose[0][3][0], 0.01 * 30.0 * loop_t + 0.5 * 0.01 * 30.0 * t);
}

#[test]
fn scale_and_interp() {
    // A quarter turn about y, halved: an eighth.
    let h = std::f32::consts::FRAC_PI_4;
    let r = [0.0, h.sin(), 0.0, h.cos()];
    let s = scale(r, 0.5);
    near(s[1], (h / 2.0).sin());
    near(s[3], (h / 2.0).cos());
    assert_eq!(scale([0.0, 0.0, 0.0, 1.0], 0.5), [0.0, 0.0, 0.0, 1.0]);
    // Opposite signs take the shorter way; equal keys stay.
    let q = interp(r, r.map(|c| -c), 0.5);
    near(q[1], r[1]);
    let i = interp([0.0, 0.0, 0.0, 1.0], r, 0.0);
    near(i[3], 1.0);
}
