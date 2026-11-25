// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Climbing ladders on the Russian edition's data (BYOA), against
//! docs/behavior/climbing.md (S12): the ladders of `l01_escape` from its
//! records (section 5.1), the worked cases of section 5.3 on `clmbl#1`,
//! then a New Game whose player climbs that ladder, with its positions,
//! leg cycles and (absent) step sounds, and gets off it at the top,
//! sideways and by a jump. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::camera::Angles;
use volya_engine::core::climbing::{
    ClimbState, Climber, Drive, Ladder, StepView, intent, jump_velocity,
};
use volya_engine::core::collision::geometry::{Vec3, add, dot, length, scale, sub};
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);
const FRAME_MS: u64 = 16;
/// The body radius (level-cform.md 5.1).
const R: f32 = 0.35;

/// Section 5.1: name, axis/side/normal rows, raised, negated, length,
/// width, B, T, N̂.
type Row = (
    &'static str,
    [usize; 3],
    bool,
    bool,
    f32,
    f32,
    Vec3,
    Vec3,
    Vec3,
);
#[rustfmt::skip]
const L01_LADDERS: [Row; 14] = [
    ("clmbl#0", [0, 1, 2], true, false, 7.4548, 1.1297, [-113.386, -30.453, -362.589], [-113.358, -24.024, -358.814], [-0.0137, 0.5064, -0.8622]),
    ("clmbl#1", [0, 2, 1], true, true, 7.2228, 1.1309, [-137.362, -30.163, -396.856], [-141.086, -23.974, -396.854], [0.8569, 0.5155, -0.0047]),
    ("clmbl#2", [0, 1, 2], true, true, 3.3328, 0.8813, [111.241, -2.834, 4.935], [111.240, 0.448, 4.358], [-0.0006, 0.1733, 0.9849]),
    ("clmbl#3", [0, 1, 2], true, false, 4.3985, 0.7951, [114.545, -7.176, -34.215], [114.545, -2.844, -33.451], [0.0051, 0.1736, -0.9848]),
    ("clmbl#4", [0, 1, 2], true, false, 4.4686, 0.7948, [109.317, -7.497, -10.301], [109.317, -3.096, -9.525], [-0.0013, 0.1736, -0.9848]),
    ("clmbl#5", [0, 1, 2], true, true, 3.3962, 0.7954, [136.920, 0.235, 341.164], [136.918, 3.579, 340.571], [-0.0609, 0.1743, 0.9828]),
    ("clmbl#6", [0, 2, 1], true, true, 5.5504, 0.7227, [26.590, 15.645, 659.211], [23.772, 20.427, 659.211], [0.8611, 0.5073, 0.0348]),
    ("clmbl#7", [0, 1, 2], true, true, 3.8320, 0.6591, [-220.359, -19.627, -131.601], [-220.364, -15.857, -132.285], [0.0932, 0.1778, 0.9796]),
    ("clmbl#8", [0, 2, 1], true, true, 3.2438, 0.7950, [-198.995, -19.694, -134.260], [-198.149, -16.563, -134.262], [-0.9653, 0.2610, 0.0027]),
    ("clmbl#9", [0, 1, 2], true, false, 3.2442, 0.7940, [-220.382, -20.134, -170.983], [-220.384, -17.002, -170.136], [0.0027, 0.2611, -0.9653]),
    ("clmbl#10", [0, 1, 2], true, true, 2.2075, 0.4977, [-185.404, -21.616, -132.995], [-185.406, -19.490, -133.592], [-0.0761, 0.2696, 0.9600]),
    ("clmbl#11", [0, 2, 1], true, true, 2.2066, 0.4960, [-219.125, -22.055, -157.392], [-219.719, -19.930, -157.392], [0.9632, 0.2687, -0.0000]),
    ("clmbl#12", [0, 1, 2], true, true, 3.4175, 0.8967, [136.919, 0.213, 341.185], [136.928, 3.578, 340.585], [0.0873, 0.1745, 0.9808]),
    ("clmbl#13", [0, 2, 1], true, false, 15.2500, 0.2532, [130.008, 0.169, 342.977], [145.258, 0.169, 342.981], [0.0000, 1.0000, -0.0002]),
];

fn near(a: Vec3, b: Vec3, tolerance: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tolerance)
}

fn horizontal_unit(v: Vec3) -> Vec3 {
    let h = v[0].hypot(v[2]);
    [v[0] / h, 0.0, v[2] / h]
}

/// Section 5.3's set-up on ladder `l`: P = B + 0.5·Â + 0.36·N̂, the camera
/// level facing the horizontal direction of −N̂, running towards the
/// ladder at 24.99, at clock 1.
fn worked_view(l: &Ladder) -> StepView {
    let p = add(
        add(l.lower, scale(l.axis_dir, 0.5)),
        scale(l.normal_dir, 0.36),
    );
    let camera = scale(horizontal_unit(l.normal_dir), -1.0);
    StepView {
        centre: p,
        r: R,
        a: scale(camera, 24.99),
        camera,
        velocity: [0.0; 3],
        clock: 1,
        mass: 80.0,
        gravity: 19.62,
    }
}

/// A climber that found `l[0]` at clock 0 (section 5.3) and is near it.
fn near_climber(l: &[Ladder]) -> Climber {
    let mut c = Climber::default();
    let at_zero = StepView {
        clock: 0,
        ..worked_view(&l[0])
    };
    c.found(l, 0, &at_zero);
    assert_eq!(c.state(), ClimbState::Near);
    c
}

/// Section 5.3 (M4 – M8) on `clmbl#1`.
fn worked_cases(l: &Ladder) {
    let l = std::slice::from_ref(l);
    let v = worked_view(&l[0]);
    let m = l[0].measure(v.centre);
    assert!((m.delta + 0.56).abs() < 1e-4, "{m:?}");
    assert!(m.front(&l[0], R, 0.0) && m.in_range(&l[0], R));
    // M4: the intent and the state taken.
    let k = intent(v.a, v.camera, m.v_n);
    assert!((k - 4.1014).abs() < 1e-3, "{k}");
    let mut c = near_climber(l);
    c.rules(l, &v);
    assert_eq!(c.state(), ClimbState::ClimbingUp);
    let (s, co) = 30f32.to_radians().sin_cos();
    let down = [v.camera[0] * co, -s, v.camera[2] * co];
    let k = intent(v.a, down, m.v_n);
    assert!((k + 1.8945).abs() < 1e-3, "{k}");
    let mut c = near_climber(l);
    c.rules(l, &StepView { camera: down, ..v });
    assert_eq!(c.state(), ClimbState::ClimbingDown);
    assert_eq!(intent([0.0; 3], v.camera, m.v_n), 0.0);
    let mut c = near_climber(l);
    c.rules(l, &StepView { a: [0.0; 3], ..v });
    assert_eq!(c.state(), ClimbState::ClimbingDown);
    // M5: input along Ŝ lets go; → free refused 1,000 and 3,000 ms after
    // with the body moved 1 m, accepted at 3,001.
    let mut c = near_climber(l);
    c.rules(l, &v);
    let sideways = StepView {
        a: scale(l[0].side_dir, 24.99),
        clock: 2,
        ..v
    };
    c.rules(l, &sideways);
    assert_eq!(c.state(), ClimbState::Leaving);
    let moved = add(v.centre, scale(l[0].normal_dir, 1.0));
    for (after, state) in [
        (1_000, ClimbState::Leaving),
        (3_000, ClimbState::Leaving),
        (3_001, ClimbState::Free),
    ] {
        let later = StepView {
            centre: moved,
            clock: 2 + after,
            ..v
        };
        c.rules(l, &later);
        assert_eq!(c.state(), state, "{after}");
    }
    // M6: climbing up with P at h_top 0.2, −0.2 stays; −0.3 at the top.
    for (h_top, state) in [
        (0.2, ClimbState::ClimbingUp),
        (-0.2, ClimbState::ClimbingUp),
        (-0.3, ClimbState::AtTop),
    ] {
        let mut c = near_climber(l);
        c.rules(l, &v);
        let at = StepView {
            centre: add(v.centre, scale(l[0].axis_dir, m.h_top - h_top)),
            clock: 2,
            ..v
        };
        c.rules(l, &at);
        assert_eq!(c.state(), state, "{h_top}");
    }
    // M7: the drive while climbing up.
    let mut c = near_climber(l);
    c.rules(l, &v);
    let Some(Drive::Climb(d)) = c.drive(l, &v) else {
        panic!("{:?}", c.drive(l, &v));
    };
    assert!(near(d, [-0.9704, 0.2414, 0.0035], 1e-4), "{d:?}");
    // M8: the jump with input along Ŝ.
    let j = jump_velocity(&l[0], sideways.a);
    assert!(near(j, [1.8090, 1.0887, -2.1312], 1e-4), "{j:?}");
}

/// The level with a clock and input, frame by frame.
struct Run {
    level: LevelScreen,
    input: Input,
    now: u64,
}

/// One frame's record: the climbing state and flag, the position (feet),
/// the leg cycle and the number of step sounds heard.
#[derive(Clone, Debug)]
struct Frame {
    state: ClimbState,
    climbing: bool,
    position: Vec3,
    cycle: Option<String>,
    sounds: usize,
}

impl Run {
    fn key(&mut self, key: Key, pressed: bool) {
        self.input.handle(Event::Key { key, pressed });
    }

    fn frames(&mut self, n: u32) -> Vec<Frame> {
        (0..n)
            .map(|_| {
                self.now += FRAME_MS;
                // The global clock, which is the step clock, as the app gives
                // it: nothing paused.
                self.level.set_global_clock(self.now);
                self.level.update(self.now, &self.input);
                self.input.end_frame();
                self.level.footsteps_frame(self.now as u32).unwrap();
                let p = self.level.player();
                let f = self.level.footsteps().unwrap();
                Frame {
                    state: p.climber().state(),
                    climbing: p.state().flags.climbing,
                    position: p.state().position,
                    cycle: f.leg_cycle().map(str::to_owned),
                    sounds: f.sounds().len(),
                }
            })
            .collect()
    }

    /// Stands the player 0.6 m in front of ladder `l`'s foot along its
    /// normal's horizontal direction, facing it with the camera level, and
    /// lets it settle.
    fn stand_before(&mut self, l: &Ladder) {
        let out = horizontal_unit(l.normal_dir);
        let feet = add(l.lower, add(scale(out, 0.6), [0.0, 0.1, 0.0]));
        let p = self.level.player();
        let h = p.config.eye_height(p.body().box_index());
        let angles = Angles {
            yaw: (-out[0]).atan2(-out[2]),
            pitch: 0.0,
            roll: 0.0,
        };
        self.level
            .set_camera([feet[0], feet[1] + h, feet[2]], angles);
        self.frames(10);
    }
}

#[test]
fn climbing_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let level = LevelScreen::load(&tree, &data, 0).unwrap();

    // Section 5.1: the 14 ladders of the level, in its collision world,
    // with their material (1.2).
    let ladders = level.collision().ladders().to_vec();
    assert_eq!(ladders.len(), 14);
    for (l, want) in ladders.iter().zip(L01_LADDERS) {
        let (name, roles, raised, flipped, len, width, b, t, n) = want;
        assert_eq!(
            (l.name.as_str(), l.roles, l.raised, l.flipped),
            (name, roles, raised, flipped)
        );
        assert!((2.0 * length(l.axis) - len).abs() < 1e-4, "{name}");
        assert!((2.0 * length(l.side) - width).abs() < 1e-4, "{name}");
        assert!(near(l.lower, b, 6e-4) && near(l.upper, t, 6e-4), "{name}");
        assert!(near(l.normal_dir, n, 6e-5), "{name}: {:?}", l.normal_dir);
    }
    let index = level.collision().ladder_material() as usize;
    let m = &level.collision().materials()[index];
    assert_eq!(
        (m.name.as_str(), m.id, index),
        ("materials\\fake_ladders", 82, 47)
    );
    let ladder = ladders[1].clone();
    worked_cases(&ladder);

    let mut run = Run {
        level,
        input: Input::new(),
        now: 0,
    };
    run.frames(10);

    // `clmbl#1` from its foot, running towards it with the camera level.
    run.stand_before(&ladder);
    run.key(Key::W, true);
    let frames = run.frames(300);
    let first = frames.iter().position(|f| f.climbing).expect("never on it");
    let top = (frames.iter())
        .position(|f| f.state == ClimbState::AtTop)
        .expect("never at the top");
    eprintln!(
        "clmbl#1: on the ladder at frame {first}, at the top at frame {top} ({:.2} s)",
        (top - first) as f32 * 0.016
    );
    assert!(frames[first..top].iter().all(|f| f.climbing));
    // On the ladder (from the frame after the flag is set, whose leg cycle
    // was chosen before): the climbing cycle (S11 2.1), which has no steps
    // (section 4), the body in front of the ladder and within its range.
    for f in &frames[first + 1..top] {
        assert_eq!(f.cycle.as_deref(), Some("cl_run_fwd_0"), "{f:?}");
        assert_eq!(f.sounds, 0, "{f:?}");
        let m = ladder.measure(add(f.position, [0.0, R, 0.0]));
        assert!(
            m.front(&ladder, R, 0.0) && m.in_range(&ladder, R),
            "{f:?} {m:?}"
        );
    }
    // Rising along the axis at the speed cap 0.1 · 24.99 (3.3).
    let (a, b) = (&frames[first + 40], &frames[first + 100]);
    let along = dot(sub(b.position, a.position), ladder.axis_dir);
    let speed = length(sub(b.position, a.position)) / (60.0 * 0.016);
    eprintln!("clmbl#1: {along:.3} m along the axis in 0.96 s, {speed:.3} m/s");
    assert!((speed - 2.499).abs() < 0.05, "{speed}");
    assert!(along > 0.95 * speed * 0.96, "{along}");
    // Off at the top onto what lies behind it, walking with steps again.
    let after = &frames[top..];
    let last = after.last().unwrap();
    assert!(!last.climbing, "{last:?}");
    assert!((last.position[1] - ladder.upper[1]).abs() < 0.2, "{last:?}");
    let behind = dot(
        sub(last.position, ladder.upper),
        horizontal_unit(ladder.normal_dir),
    );
    assert!(behind < -0.5, "{behind}");
    assert!(after.iter().any(|f| f.sounds > 0), "no steps after the top");
    run.key(Key::W, false);
    run.frames(30);

    // Halfway up, the yaw is limited about the ladder's (3.6); right lets
    // go: leaving, sliding down its face to its foot (2.5).
    run.stand_before(&ladder);
    run.key(Key::W, true);
    let on = run.frames(80);
    assert!(on.last().unwrap().climbing, "{:?}", on.last());
    let p = run.level.player();
    let [lo, hi] = p.yaw_limits().expect("yaw not limited");
    let psi = (-ladder.normal_dir[0]).atan2(-ladder.normal_dir[2]);
    assert!((lo + 1.0 - psi).abs() < 0.01 && (hi - 1.0 - psi).abs() < 0.01);
    assert!(lo < p.angles().yaw && p.angles().yaw < hi);
    run.key(Key::W, false);
    run.key(Key::D, true);
    let off = run.frames(60);
    run.key(Key::D, false);
    let off = [off, run.frames(150)].concat();
    let leaving = off.iter().position(|f| f.state == ClimbState::Leaving);
    assert!(leaving.is_some_and(|i| i < 3), "{:?}", &off[..3]);
    let last = off.last().unwrap();
    assert!(!last.climbing && run.level.player().yaw_limits().is_none());
    assert!((last.position[1] - ladder.lower[1]).abs() < 0.3, "{last:?}");
    assert!(
        off[2..]
            .iter()
            .all(|f| f.cycle.as_deref() != Some("cl_run_fwd_0"))
    );

    // A jump off it: 3 m/s along N̂ (no sideways input), airborne and
    // leaving (3.5); landing in front of it.
    run.stand_before(&ladder);
    run.key(Key::W, true);
    run.frames(80);
    run.key(Key::W, false);
    run.frames(1);
    run.key(Key::Space, true);
    let jump = run.frames(2);
    run.key(Key::Space, false);
    let s = run.level.player().state();
    assert_eq!(jump[1].state, ClimbState::Leaving, "{jump:?}");
    assert!(s.airborne && !s.flags.climbing, "{s:?}");
    assert!(dot(s.velocity, ladder.normal_dir) > 2.5, "{s:?}");
    let landed = run.frames(120);
    assert!(!run.level.player().state().airborne && !landed.last().unwrap().climbing);
    let at = landed.last().unwrap().position;
    let away = dot(
        sub(at, jump[0].position),
        horizontal_unit(ladder.normal_dir),
    );
    eprintln!("clmbl#1: landed {away:.2} m out from where it jumped");
    assert!(
        away > 1.0 && at[1] < jump[0].position[1] - 1.0,
        "{away} {at:?}"
    );
}
