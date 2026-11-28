// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use crate::motion_playback::STOP_AT_END;
use volya_engine_formats::model::Mat4;
use volya_engine_formats::skeletal_motions::{
    Motion, MotionDefinition, MotionParams, Partition, Track,
};

const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn track() -> Track {
    Track {
        rotations: Box::new([[0, 0, 0, 32767]]),
        translations: Box::default(),
        size: [0.0; 3],
        base: [0.0; 3],
    }
}

/// A stalker-like model: partitions `legs` (0), `torso` (1), `head` (2),
/// one bone each, with the named cycles (name, flags, partition, keys).
fn model(cycles: &[(&str, u16, u16, u32)]) -> Arc<Motions> {
    let definitions = (cycles.iter())
        .enumerate()
        .map(|(i, &(name, flags, partition, _))| MotionDefinition {
            name: name.into(),
            flags,
            bone_or_partition: partition,
            motion_index: i as u16,
            speed: 1.0,
            power: 1.0,
            accrue: 2.0,
            falloff: 2.0,
        })
        .collect();
    let motions = (cycles.iter())
        .map(|&(_, _, _, keys)| Motion {
            keys,
            tracks: vec![track(), track(), track()],
        })
        .collect();
    let partition = |name: &str, bone: &str, t| Partition {
        name: name.into(),
        bones: vec![(bone.into(), t)],
    };
    let params = MotionParams {
        version: 3,
        partitions: vec![
            partition("legs", "pelvis", 0),
            partition("torso", "spine", 1),
            partition("head", "head", 2),
        ],
        definitions,
    };
    Arc::new(Motions::new(
        &["pelvis", "spine", "head"],
        &[None, Some(0), Some(1)],
        &[IDENTITY; 3],
        vec![(params, motions)],
    ))
}

const STOP: u16 = STOP_AT_END;

fn campfire() -> Arc<Motions> {
    model(&[
        ("head_idle_0", 0, 2, 30),
        ("head_talk_0", 0, 2, 30),
        ("norm_idle_1", 0, 0, 30),
        ("norm_torso_0_idle_1", 0, 1, 30),
        ("norm_torso_0_idle_0", 0, 1, 30),
        ("idle_0_to_sit_1", STOP, 0xFFFF, 10),
        ("sit_1_idle_0", 0, 0xFFFF, 30),
        ("norm_walk_fwd_0", 0, 0, 40),
        ("norm_walk_fwd_1", 8, 0, 40),
        ("norm_torso_0_walk_0", 0, 1, 40),
        ("norm_torso_0_walk_1", 8, 1, 40),
        ("norm_turn_left_1", STOP, 0, 20),
        ("norm_turn_right_1", STOP, 0, 20),
        ("norm_escape_0", 0, 0xFFFF, 20),
    ])
}

fn id(motions: &Motions, name: &str) -> MotionId {
    motions.cycle(name).unwrap()
}

fn clock(ms: u32) -> Clock {
    Clock {
        time_global: ms,
        frame: ms,
    }
}

/// A free stalker standing still at yaw 0.
fn free() -> StalkerState {
    StalkerState::default()
}

/// The motions playing on each partition, latest last, without the fading.
fn playing(model: &AnimatedModel, partition: usize) -> Vec<MotionId> {
    (model.partition_playbacks(partition))
        .filter(|p| p.fade == crate::motion_playback::Fade::In)
        .map(|p| p.motion)
        .collect()
}

#[test]
fn lists_skip_gaps_below_ten_and_end_from_ten() {
    let names: Vec<String> = (0..13)
        .filter(|i| *i != 1 && *i != 11)
        .map(|i| format!("norm_torso_0_aim_{i}"))
        .collect();
    let cycles: Vec<(&str, u16, u16, u32)> = names.iter().map(|n| (n.as_str(), 0, 1, 10)).collect();
    let motions = model(&cycles);
    let lists = MotionLists::build(&motions);
    let aim = &lists.torso[1][0][AIM];
    // 0, 2 … 10 found; 11 missing ends the scan, so 12 is left out.
    let want: Vec<MotionId> = (0..10).map(|k| motions.cycle(&names[k]).unwrap()).collect();
    assert_eq!(aim, &want);
    assert_eq!(global_names().len(), GLOBAL_LISTS);
    assert_eq!(global_names()[22], "panic_stand_");
    assert_eq!(global_names()[4], "1_critical_hit_head_");
}

#[test]
fn the_mask_leaves_out_the_head_partition() {
    let motions = campfire();
    let mut s = StalkerMotions::new();
    s.reload(Some(StalkerModel::new(motions.clone())), true, -1);
    assert_eq!(s.mask(), [0, 1]);
    s.reload(Some(StalkerModel::new(motions)), false, -1);
    assert!(s.mask().is_empty());
}

#[test]
fn add_animation_takes_only_the_models_cycles() {
    let mut s = StalkerMotions::new();
    s.reload(Some(StalkerModel::new(campfire())), true, -1);
    assert!(s.add_animation(b"sit_1_idle_0", true, false));
    assert!(!s.add_animation(b"no_such", true, false));
    assert_eq!(s.animation_count(), 1);
    s.clear_animations();
    assert_eq!(s.animation_count(), 0);
}

/// 3.3, 8: the queue's motions play one after the other on legs and torso,
/// the head goes on; the end marks callback 29; an empty queue goes back
/// to the normal selection.
#[test]
fn the_queue_takes_over_the_model() {
    let motions = campfire();
    let mut model = AnimatedModel::new(motions.clone());
    let mut s = StalkerMotions::new();
    s.reload(Some(StalkerModel::new(motions.clone())), true, -1);
    let mut draw = || 0;
    let state = free();
    // The normal selection first: still and free.
    s.update(&mut model, &state, true, clock(10), &mut draw)
        .unwrap();
    assert_eq!(playing(&model, 0), [id(&motions, "norm_idle_1")]);
    assert_eq!(playing(&model, 1), [id(&motions, "norm_torso_0_idle_1")]);
    assert_eq!(playing(&model, 2), [id(&motions, "head_idle_0")]);
    assert!(s.add_animation(b"idle_0_to_sit_1", true, false));
    assert!(s.add_animation(b"sit_1_idle_0", true, false));
    let into = id(&motions, "idle_0_to_sit_1");
    let sit = id(&motions, "sit_1_idle_0");
    let mut t = 20;
    assert!(!s.take_script_animation_end(&mut model, t));
    let out = s
        .update(&mut model, &state, true, clock(t), &mut draw)
        .unwrap();
    assert_eq!(out.steps, [into]);
    assert_eq!(playing(&model, 0), [into]);
    assert_eq!(playing(&model, 1), [into]);
    assert_eq!(playing(&model, 2), [id(&motions, "head_idle_0")]);
    assert_eq!(s.slot(Slot::Torso), (None, None));
    // It ends 1/30 s before its 10 keys' length: the front entry goes.
    let mut ended = false;
    while !ended {
        t += 20;
        assert!(t < 2_000, "the transfer ends");
        ended = s.take_script_animation_end(&mut model, t);
        if !ended {
            s.update(&mut model, &state, true, clock(t), &mut draw)
                .unwrap();
        }
    }
    assert_eq!(s.animation_count(), 1);
    s.update(&mut model, &state, true, clock(t), &mut draw)
        .unwrap();
    assert_eq!(playing(&model, 0), [sit]);
    assert_eq!(playing(&model, 1), [sit]);
    // The looping idle stays.
    for _ in 0..100 {
        t += 20;
        assert!(!s.take_script_animation_end(&mut model, t));
        s.update(&mut model, &state, true, clock(t), &mut draw)
            .unwrap();
    }
    assert_eq!(s.animation_count(), 1);
    assert_eq!(playing(&model, 0), [sit]);
    // Cleared: the normal selection starts afresh.
    s.clear_animations();
    s.update(&mut model, &state, true, clock(t + 20), &mut draw)
        .unwrap();
    assert_eq!(playing(&model, 0), [id(&motions, "norm_idle_1")]);
    assert_eq!(playing(&model, 1), [id(&motions, "norm_torso_0_idle_1")]);
}

/// 5: the head talks while a sound plays.
#[test]
fn the_head_talks() {
    let motions = campfire();
    let mut model = AnimatedModel::new(motions.clone());
    let mut s = StalkerMotions::new();
    s.reload(Some(StalkerModel::new(motions.clone())), true, -1);
    let state = StalkerState {
        talking: true,
        ..free()
    };
    s.update(&mut model, &state, false, clock(10), &mut || 0)
        .unwrap();
    assert_eq!(playing(&model, 2), [id(&motions, "head_talk_0")]);
}

/// 7.3: a free stalker whose head is turned more than π/6 turns toward
/// the head's target, left or right by the sine.
#[test]
fn still_and_free_turns_toward_the_head() {
    let motions = campfire();
    let mut model = AnimatedModel::new(motions.clone());
    let mut s = StalkerMotions::new();
    s.reload(Some(StalkerModel::new(motions.clone())), true, -1);
    let state = StalkerState {
        head_yaw: 1.0,
        head_target_yaw: 1.0,
        ..free()
    };
    let out = s
        .update(&mut model, &state, false, clock(10), &mut || 0)
        .unwrap();
    assert_eq!(out.body_target_yaw, Some(1.0));
    // sin(0 − 1) ≤ 0: right.
    assert_eq!(playing(&model, 0), [id(&motions, "norm_turn_right_1")]);
    let state = StalkerState {
        head_yaw: -1.0,
        head_target_yaw: -1.0,
        ..free()
    };
    s.update(&mut model, &state, false, clock(20), &mut || 0)
        .unwrap();
    assert_eq!(playing(&model, 0), [id(&motions, "norm_turn_left_1")]);
}

/// 7.1, 6.1, 3.6, 7.4: walking free plays the walk on legs and torso, the
/// torso kept in time; the speed follows the walk's weight.
#[test]
fn walking_free() {
    let motions = campfire();
    let mut model = AnimatedModel::new(motions.clone());
    let mut s = StalkerMotions::new();
    s.reload(Some(StalkerModel::new(motions.clone())), true, -1);
    let mut speeds = [0.0; SPEEDS];
    speeds[16] = 1.5;
    let state = StalkerState {
        movement_type: WALK,
        speed: 1.0,
        speeds,
        ..free()
    };
    let out = s
        .update(&mut model, &state, false, clock(10), &mut || 0)
        .unwrap();
    let walk = id(&motions, "norm_walk_fwd_1");
    assert_eq!(playing(&model, 0), [walk]);
    assert_eq!(playing(&model, 1), [id(&motions, "norm_torso_0_walk_1")]);
    assert_eq!(out.steps, [walk]);
    // The update of the change writes no speed (7.4 step 3).
    assert_eq!(out.speed, None);
    let mut last = 0.0;
    for t in 1..30 {
        model.update_tracks(clock(10 + 20 * t));
        let out = s
            .update(&mut model, &state, false, clock(10 + 20 * t), &mut || 0)
            .unwrap();
        let v = out.speed.unwrap();
        assert!(v >= last && v <= 1.5, "{v}");
        last = v;
        let torso = s.slot(Slot::Torso).1.unwrap();
        let legs = s.slot(Slot::Legs).1.unwrap();
        let time = |p| model.playback(p).unwrap().time;
        assert_eq!(time(torso), time(legs));
    }
    assert!((last - 1.5).abs() < 1e-3, "{last}");
}

/// 4: a panicking stalker on the move plays an escape motion on legs and
/// torso; its torso and legs slots stay empty.
#[test]
fn panic_on_the_move_escapes() {
    let motions = campfire();
    let mut model = AnimatedModel::new(motions.clone());
    let mut s = StalkerMotions::new();
    s.reload(Some(StalkerModel::new(motions.clone())), true, -1);
    let state = StalkerState {
        mental_state: PANIC,
        movement_type: RUN,
        speed: 3.0,
        ..free()
    };
    s.update(&mut model, &state, false, clock(10), &mut || 0)
        .unwrap();
    let escape = id(&motions, "norm_escape_0");
    assert_eq!(playing(&model, 0), [escape]);
    assert_eq!(playing(&model, 1), [escape]);
    assert_eq!(s.slot(Slot::Legs), (None, None));
}

#[test]
fn directions() {
    assert_eq!(direction(0.1, 0.0), FORWARD);
    assert_eq!(direction(PI, 0.0), BACK);
    assert_eq!(direction(-FRAC_PI_2, 0.0), 2);
    assert_eq!(direction(FRAC_PI_2, 0.0), 3);
    assert!(opposite(0, 1) && opposite(3, 2) && !opposite(0, 2));
    assert!((wrap(3.0 * PI) - PI).abs() < 1e-5);
    assert!((wrap(-1.5 * PI) - FRAC_PI_2).abs() < 1e-5);
}
