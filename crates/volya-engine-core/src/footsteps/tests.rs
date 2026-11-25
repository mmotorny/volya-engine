// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use volya_engine_formats::ltx;
use volya_engine_formats::skeletal_motions::{MotionDefinition, MotionParams, MotionSet};

use super::*;
use crate::sound::random::SharedRandom;

/// The cycles of S10 5.1 the player's table names (N, flags, speed).
fn cycles() -> Cycles {
    let defs: &[(&str, u16, f32, u32)] = &[
        ("norm_walk_fwd_0", 8, 1.0, 26),
        ("norm_run_fwd_0", 8, 1.0, 26),
        ("cr_walk_fwd_0", 8, 1.0, 55),
        ("cr_walk_ls_0", 8, 1.0, 39),
        ("norm_escape_00", 8, 1.0, 20),
        ("norm_jump_end", 0, 1.0, 10),
        ("norm_idle_0", 0, 1.0, 60),
        ("dmg_norm_turn_left_0", 2, 1.0, 25),
    ];
    Cycles::new(&[MotionSet {
        params: MotionParams {
            version: 3,
            partitions: Vec::new(),
            definitions: defs
                .iter()
                .enumerate()
                .map(|(k, &(name, flags, speed, _))| MotionDefinition {
                    name: name.into(),
                    flags,
                    bone_or_partition: 0xFFFF,
                    motion_index: k as u16,
                    speed,
                    power: 1.0,
                    accrue: 2.0,
                    falloff: 2.0,
                })
                .collect(),
        },
        key_counts: defs.iter().map(|d| d.3).collect(),
    }])
}

const SYSTEM: &[u8] = b"[actor]\nmaterial = creatures\\actor\nstep_params = stalker_step_manager\n\
LegsCount = 2\n\
[stalker_step_manager]\n\
norm_walk_fwd_0 = 1, 0.2, 0.2, 0.65, 0.2\n\
norm_run_fwd_0 = 1, 0.2, 1, 0.65, 1\n\
cr_walk_fwd_0 = 1, 0.2, 0.1, 0.65, 0.1\n\
cr_walk_ls_0 = 1, 0.2, 0.1, 0.65, 0.1\n\
norm_escape_00 = 1, 0.2, 1.3, 0.65, 1.3\n\
norm_jump_end = 1, 0.2, 1.3, 0.21, 1.3\n\
dmg_norm_turn_left_0 = 1, 0.2, 1, 0.65, 1\n\
not_a_cycle = 1, 0.2, 1, 0.65, 1\n";

fn table() -> StepTable {
    StepTable::read(&ltx::parse(SYSTEM).unwrap(), "actor", &cycles()).unwrap()
}

/// The frames at which a leg steps, from `from` to `to` every `every` ms,
/// for cycle `name` started at `from`.
fn steps(name: &str, from: u32, to: u32, every: u32, paired: bool) -> Vec<(u32, Step)> {
    let (cycles, table) = (cycles(), table());
    let mut f = Footsteps::default();
    f.choose(name, from, &cycles, &table);
    let mut out = Vec::new();
    let mut now = from;
    while now <= to {
        let mut s = Vec::new();
        f.update(now, paired, &mut s);
        out.extend(s.into_iter().map(|s| (now, s)));
        now += every;
    }
    out
}

fn times(s: &[(u32, Step)]) -> Vec<u32> {
    s.iter().map(|(t, _)| *t).collect()
}

#[test]
fn reads_the_actor_s_table() {
    let system = ltx::parse(SYSTEM).unwrap();
    assert_eq!(own_material(&system, "actor").unwrap(), "creatures\\actor");
    let t = table();
    assert_eq!(t.legs, 2);
    // `not_a_cycle` is skipped.
    assert_eq!(t.len(), 7);
    assert_eq!(
        t.line("norm_jump_end"),
        Some(&StepLine {
            passes: 1,
            legs: vec![(0.2, 1.3), (0.21, 1.3)]
        })
    );
    assert!(t.line("not_a_cycle").is_none());
    // No `material` is fatal; a pass count of 0 too; more than 4 legs is
    // not supported.
    let no_material = ltx::parse(b"[actor]\nLegsCount = 2\n").unwrap();
    assert!(own_material(&no_material, "actor").is_err());
    let zero = ltx::parse(b"[actor]\nstep_params = t\nLegsCount = 2\n[t]\nnorm_idle_0 = 0, 1, 1\n")
        .unwrap();
    assert!(StepTable::read(&zero, "actor", &cycles()).is_err());
    let five = ltx::parse(b"[actor]\nLegsCount = 5\n").unwrap();
    assert!(StepTable::read(&five, "actor", &cycles()).is_err());
    // Without `step_params` or its section: no lines. A short line's
    // missing items are 0.
    let none = ltx::parse(b"[actor]\nstep_params = missing\nLegsCount = 2\n").unwrap();
    assert!(
        StepTable::read(&none, "actor", &cycles())
            .unwrap()
            .is_empty()
    );
    let short = ltx::parse(b"[actor]\nstep_params = t\nLegsCount = 2\n[t]\nnorm_idle_0 = 2, 0.5\n")
        .unwrap();
    let short = StepTable::read(&short, "actor", &cycles()).unwrap();
    assert_eq!(
        short.line("norm_idle_0").unwrap().legs,
        [(0.5, 0.0), (0.0, 0.0)]
    );
}

#[test]
fn walking_with_16_ms_frames() {
    // M1.
    let s = steps("norm_walk_fwd_0", 10_000, 12_400, 16, true);
    assert_eq!(times(&s), [10_176, 10_576, 11_040, 11_440, 11_920, 12_304]);
    let legs: Vec<usize> = s.iter().map(|(_, s)| s.leg).collect();
    assert_eq!(legs, [0, 1, 0, 1, 0, 1]);
    assert!(s.iter().all(|(_, s)| s.volume == 0.2));
}

#[test]
fn walking_with_1_ms_frames() {
    // M2: passes restart at 10,867 and 11,734.
    let s = steps("norm_walk_fwd_0", 10_000, 12_400, 1, true);
    assert_eq!(times(&s), [10_173, 10_563, 11_040, 11_430, 11_907, 12_297]);
}

#[test]
fn landing() {
    // M3: both steps (due +66, +70) in the frame at +80 ms, volume 1.3.
    let s = steps("norm_jump_end", 10_000, 10_090, 16, true);
    assert_eq!(times(&s), [10_080, 10_080]);
    assert!(s.iter().all(|(_, s)| s.volume == 1.3));
}

#[test]
fn crouching_and_sprinting_with_1_ms_frames() {
    // M4.
    let s = steps("cr_walk_fwd_0", 10_000, 11_833, 1, true);
    assert_eq!(times(&s), [10_366, 11_192]);
    assert!(s.iter().all(|(_, s)| s.volume == 0.1));
    let s = steps("norm_escape_00", 10_000, 11_200, 1, true);
    assert_eq!(times(&s), [10_133, 10_433, 10_800, 11_100]);
    // S11 3.3's table: the sideways crouched walk.
    let s = steps("cr_walk_ls_0", 10_000, 11_299, 1, true);
    assert_eq!(times(&s), [10_260, 10_845]);
}

#[test]
fn no_line_or_no_pair_gives_no_steps() {
    // M5: no table line; and without a pair not even the pass restarts.
    assert!(steps("norm_idle_0", 10_000, 13_000, 16, true).is_empty());
    let (cycles, table) = (cycles(), table());
    let mut f = Footsteps::default();
    f.choose("norm_walk_fwd_0", 10_000, &cycles, &table);
    let mut out = Vec::new();
    f.update(12_000, false, &mut out);
    assert!(out.is_empty());
    // The pair found again: the first pass's steps are overdue at once, then
    // the pass restarts from 10,867 only.
    f.update(12_000, true, &mut out);
    assert_eq!(out.len(), 2);
    out.clear();
    f.update(12_001, true, &mut out);
    assert_eq!(out.len(), 2);
}

#[test]
fn a_cycle_that_stops_at_the_end_never_restarts() {
    let s = steps("dmg_norm_turn_left_0", 10_000, 13_000, 1, true);
    assert_eq!(times(&s), [10_166, 10_541]);
}

#[test]
fn choosing_the_same_cycle_keeps_its_timing() {
    let (cycles, table) = (cycles(), table());
    let mut f = Footsteps::default();
    f.choose("norm_walk_fwd_0", 10_000, &cycles, &table);
    f.choose("norm_walk_fwd_0", 10_100, &cycles, &table);
    let mut out = Vec::new();
    f.update(10_173, true, &mut out);
    assert_eq!(out.len(), 1);
    // A change restarts the timing from the moment of the change.
    f.choose("norm_run_fwd_0", 10_200, &cycles, &table);
    out.clear();
    f.update(10_372, true, &mut out);
    assert!(out.is_empty());
    f.update(10_373, true, &mut out);
    assert_eq!(
        out,
        [Step {
            leg: 0,
            volume: 1.0
        }]
    );
    assert_eq!(f.playing(), Some("norm_run_fwd_0"));
}

#[test]
fn the_generator_from_state_0_chooses_sounds_2_3_2_1() {
    // M6: sound r mod m of a 4-sound list.
    let mut r = SharedRandom::new(0);
    let chosen: Vec<u32> = (0..4).map(|_| r.draw() % 4).collect();
    assert_eq!(chosen, [2, 3, 2, 1]);
}

fn leg(flags: Flags, runs: bool) -> String {
    leg_cycle(&LegState {
        flags,
        runs,
        ..LegState::default()
    })
}

#[test]
fn leg_cycles_by_movement_state() {
    let f = Flags::default;
    assert_eq!(leg(f(), true), "norm_idle_0");
    let fwd = Flags {
        forward: true,
        ..f()
    };
    assert_eq!(leg(fwd, true), "norm_run_fwd_0");
    assert_eq!(leg(fwd, false), "norm_walk_fwd_0");
    let back_left = Flags {
        back: true,
        left: true,
        ..f()
    };
    assert_eq!(leg(back_left, true), "norm_run_back_0");
    let right = Flags { right: true, ..f() };
    assert_eq!(leg(right, false), "norm_walk_rs_0");
    // Crouched: the low crouch (walking) and the crouch (running).
    let cr = Flags {
        crouch: true,
        ..f()
    };
    assert_eq!(leg(Flags { left: true, ..cr }, false), "cr_walk_ls_0");
    assert_eq!(
        leg(
            Flags {
                forward: true,
                ..cr
            },
            true
        ),
        "cr_run_fwd_0"
    );
    assert_eq!(leg(cr, true), "cr_idle_0");
    assert_eq!(leg(cr, false), "cr_idle_1");
    // Landing comes first, then the hard landing, falling, jumping.
    let landing = Flags {
        landing: true,
        forward: true,
        falling: true,
        ..f()
    };
    assert_eq!(leg(landing, true), "norm_jump_end");
    assert_eq!(
        leg(
            Flags {
                crouch: true,
                ..landing
            },
            true
        ),
        "cr_jump_end"
    );
    let hard = Flags {
        hard_landing: true,
        ..landing
    };
    assert_eq!(leg(hard, true), "norm_jump_end_1");
    assert_eq!(
        leg(
            Flags {
                falling: true,
                jump: true,
                ..fwd
            },
            true
        ),
        "norm_jump_idle"
    );
    assert_eq!(leg(Flags { jump: true, ..fwd }, true), "norm_jump_begin");
    // Sprint replaces the choice, but with only back.
    let sprint = Flags {
        sprint: true,
        ..f()
    };
    assert_eq!(
        leg(
            Flags {
                forward: true,
                jump: true,
                ..sprint
            },
            true
        ),
        "norm_escape_00"
    );
    assert_eq!(
        leg(
            Flags {
                left: true,
                crouch: true,
                ..sprint
            },
            true
        ),
        "norm_escape_ls_00"
    );
    assert_eq!(
        leg(
            Flags {
                right: true,
                ..sprint
            },
            true
        ),
        "norm_escape_rs_00"
    );
    assert_eq!(
        leg(
            Flags {
                back: true,
                ..sprint
            },
            true
        ),
        "norm_run_back_0"
    );
    // Turning while standing, and climbing.
    let turning = LegState {
        turning: true,
        ..LegState::default()
    };
    assert_eq!(leg_cycle(&turning), "norm_turn");
    let climbing = LegState {
        climbing: true,
        ..LegState::default()
    };
    assert_eq!(leg_cycle(&climbing), "cl_idle_1");
    assert_eq!(
        leg_cycle(&LegState {
            flags: Flags { back: true, ..f() },
            ..climbing
        }),
        "cl_run_back_0"
    );
    assert_eq!(
        leg_cycle(&LegState {
            turning: true,
            ..climbing
        }),
        "cl_idle_1"
    );
    assert_eq!(
        leg_cycle(&LegState {
            flags: landing,
            ..climbing
        }),
        "norm_jump_end"
    );
}
