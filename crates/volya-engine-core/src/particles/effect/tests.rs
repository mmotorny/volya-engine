// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The test vectors of particle-lifecycle.md section 9.2 and
//! particle-motion-actions.md section 6.4.

use super::super::fixture::{action, effect, muhi, muhi_vortex, orbit, sphere_03};
use super::*;

/// *M* of P2 9.2.
const M: Matrix = [
    [0.0, 0.0, -1.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0, 0.0],
    [10.0, 2.0, -3.0, 1.0],
];

fn close(a: Vec3, b: Vec3, tolerance: f32) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= tolerance)
}

#[test]
fn a_playing_instance_runs_whole_steps_at_most_three_an_update() {
    let mut e = EffectInstance::new(0, effect("empty", 1, 0, 0.0, Vec::new()).record());
    e.play();
    let mut random = Random::default();
    let mut steps = Vec::new();
    for ms in [16, 16, 16, 16, 200, 0, 40] {
        let before = e.accumulator;
        e.update(ms, &mut random);
        let total = before + ms;
        steps.push(if total < 33 { 0 } else { (total / 33).min(3) });
    }
    assert_eq!(steps, [0, 0, 1, 0, 3, 0, 1]);
}

/// P2 8.1: a new instance's box is empty and its sphere at the origin
/// with radius 0; record, play, stop and placement leave them, and so do
/// steps without live particles.
#[test]
fn a_new_instance_has_the_empty_box_and_the_origin_sphere() {
    let origin = Sphere {
        centre: [0.0; 3],
        radius: 0.0,
    };
    let mut e = EffectInstance::new(0, effect("empty", 1, 0, 0.0, Vec::new()).record());
    let unchanged = |e: &EffectInstance| e.bounds() == Bounds::EMPTY && e.sphere() == origin;
    assert!(unchanged(&e));
    e.place(&M, [1.0, 2.0, 3.0], Mode::World);
    e.play();
    assert!(unchanged(&e));
    let mut random = Random::default();
    e.update(99, &mut random);
    assert!(unchanged(&e));
    e.stop_deferred();
    e.stop_immediate();
    assert!(unchanged(&e));
    e.update(33, &mut random);
    assert_eq!(e.bounds(), Bounds::around(e.position()));
}

#[test]
fn muhi_is_placed_as_p2_says() {
    let mut e = EffectInstance::new(0, muhi().record());
    e.place(&M, [1.0, 2.0, 3.0], Mode::World);
    assert_eq!(e.position, [10.0, 2.0, -3.0]);
    let ActionKind::Source(s) = &e.actions[0].placed else {
        panic!()
    };
    assert_eq!(s.position.p1, [10.0, 2.0, -3.0]);
    assert_eq!(s.position.p2, [0.0, 1.0, 0.0]);
    assert_eq!(s.position.u, [0.0, 0.0, -1.0]);
    assert_eq!(s.position.v, [-1.0, 0.0, 0.0]);
    assert_eq!(s.position.r[..3], [1.0, 0.0, 0.0]);
    assert_eq!(s.velocity.p1, [0.0; 3]);
    assert_eq!(s.velocity.p2, [0.0, 0.1, 0.0]);
    assert_eq!(s.velocity.u, [0.0, 0.0, -1.0]);
    assert_eq!(s.velocity.v, [-1.0, 0.0, 0.0]);
    assert_eq!(s.parent_velocity, [0.0; 3]);
    let centre = |e: &EffectInstance, i: usize| match &e.actions[i].placed {
        ActionKind::OrbitPoint { centre, .. } => *centre,
        ActionKind::Vortex { centre, .. } => *centre,
        ActionKind::RandomDisplace { region } => region.p1,
        _ => panic!(),
    };
    assert_eq!(centre(&e, 2), [10.0, 2.0, -3.0]);
    assert!(close(centre(&e, 3), [10.3, 2.0, -3.3], 1e-6));
    assert_eq!(centre(&e, 4), [0.0; 3]);
    assert_eq!(centre(&e, 5), [10.0, 5.0, -3.0]);
    let ActionKind::Vortex { axis, .. } = e.actions[5].placed else {
        panic!()
    };
    assert_eq!(axis, [0.0, 1.0, 0.0]);
    // Placing again starts from the stored values.
    e.place(&IDENTITY, [0.0; 3], Mode::World);
    assert_eq!(centre(&e, 2), [0.0; 3]);
}

#[test]
fn the_parent_velocity_follows_the_effects_velocity() {
    let mut record = muhi();
    let ActionKind::Source(s) = &mut record.actions[0].kind else {
        panic!()
    };
    s.parent_motion = 0.5;
    let mut e = EffectInstance::new(0, record.record());
    e.place(&IDENTITY, [2.0, 4.0, 6.0], Mode::World);
    let ActionKind::Source(s) = &e.actions[0].placed else {
        panic!()
    };
    assert_eq!(s.parent_velocity, [1.0, 2.0, 3.0]);
    // Local mode places nothing and keeps the matrix.
    e.place(&M, [9.0; 3], Mode::Local);
    assert_eq!(e.local(), Some(&M));
    assert_eq!(e.position, [0.0; 3]);
}

#[test]
fn muhis_source_makes_p2s_particle() {
    // P2 9.2 with `tests\muhi`'s flags 0x1401: seven draws for the
    // particle, then the birth notice's eighth for its start frame (P7 3.2).
    let only_source = vec![muhi().actions.remove(0)];
    let mut e = EffectInstance::new(0, effect("muhi", 7, 0x1401, 0.0, only_source).record());
    let mut random = Random::new(1);
    e.play();
    e.update(33, &mut random);
    let [p] = e.particles() else {
        panic!("{:?}", e.particles())
    };
    assert!(close(p.position, [-0.178_081, 0.0, 0.075_190], 1e-6));
    assert_eq!(p.previous, p.position);
    assert!(close(p.velocity, [-0.060_986, 0.058_501, -0.007_754], 1e-6));
    assert!(close(p.rotation, [84.5949, 0.6175, 0.6175], 1e-4));
    assert_eq!(p.size, [0.01, 0.01, 0.0]);
    assert_eq!(p.colour, 0xFF7F_7F7F);
    assert_eq!(p.age, 0.0);
    assert_eq!(p.frame, 510);
    assert_eq!(p.frame_flags, 0);
    assert_eq!(random.uniform.get().state, 1_924_036_713);
}

#[test]
fn muhis_motion_records_are_p3s_6_4() {
    let motion = vec![
        action(14, 2, orbit([0.0; 3])),
        action(14, 2, orbit([0.3, 0.0, 0.3])),
        action(
            16,
            2,
            ActionKind::RandomDisplace {
                region: sphere_03(),
            },
        ),
        action(29, 2, muhi_vortex()),
        action(12, 0, ActionKind::Move),
    ];
    let mut e = EffectInstance::new(0, effect("muhi", 7, 0, 0.0, motion).record());
    let p = |x: Vec3, w: Vec3, age: f32| Particle {
        position: x,
        previous: x,
        velocity: w,
        age,
        ..Particle::default()
    };
    e.particles = vec![
        p([1.0, 2.0, 3.0], [0.5, -1.0, 2.0], 0.25),
        p([-2.0, 0.5, 1.0], [3.0, 0.0, -1.0], 1.0),
        p([0.5, -1.0, -0.5], [0.0; 3], 0.0),
    ];
    e.play();
    e.update(33, &mut Random::new(1));
    let want = [
        (
            [1.12492, 1.9676, 3.01896],
            [1.10853, 2.00086, 2.95334],
            [0.496622, -1.00806, 1.98856],
            0.283,
        ),
        (
            [-1.86881, 0.498834, 1.04605],
            [-1.96839, 0.49897, 1.07928],
            [3.01769, -0.00413191, -1.0071],
            1.033,
        ),
        (
            [0.476914, -0.995208, -0.511129],
            [0.477187, -0.995973, -0.511621],
            [-0.00827028, 0.0231912, 0.0149209],
            0.033,
        ),
    ];
    for (p, (x, xp, w, age)) in e.particles().iter().zip(want) {
        assert!(close(p.position, x, 2e-5), "{:?}", p.position);
        assert!(close(p.previous, xp, 2e-5), "{:?}", p.previous);
        assert!(close(p.velocity, w, 2e-5), "{:?}", p.velocity);
        assert!((p.age - age).abs() < 1e-6);
    }
}

#[test]
fn the_time_limit_stops_in_its_step_and_replays_with_the_full_limit() {
    assert_eq!(time_limit_step(0.1), 4);
    assert_eq!(time_limit_step(0.2), 7);
    assert_eq!(time_limit_step(1.0), 31);
    assert_eq!(time_limit_step(0.5), 16);
    let move_only = vec![action(12, 0, ActionKind::Move)];
    let mut e = EffectInstance::new(0, effect("t", 1, 0x4000, 0.1, move_only).record());
    e.play();
    let mut random = Random::default();
    for _ in 0..3 {
        e.update(33, &mut random);
        assert!(!e.stopping);
    }
    e.update(33, &mut random);
    assert!(e.stopping);
    assert_eq!(e.remaining, 0.1);
    // With no particles the deferred stop ends at the next step.
    e.update(33, &mut random);
    assert!(!e.is_playing());
}

#[test]
fn an_immediate_stop_drops_every_particle() {
    let mut e = EffectInstance::new(0, muhi().record());
    let mut random = Random::default();
    e.play();
    e.update(99, &mut random);
    assert!(!e.particles().is_empty());
    let b = e.bounds();
    assert!(b.is_set());
    e.stop_immediate();
    assert!(e.particles().is_empty() && !e.is_playing());
    e.update(33, &mut random);
    assert_eq!(e.bounds(), Bounds::around(e.position));
}

#[test]
fn a_deferred_stop_lets_particles_live_on_without_births() {
    let mut e = EffectInstance::new(0, muhi().record());
    let mut random = Random::default();
    e.play();
    e.update(99, &mut random);
    let n = e.particles().len();
    e.stop_deferred();
    e.update(33, &mut random);
    assert!(e.is_playing() && e.particles().len() <= n);
    // `killold` at age 2 ends them.
    for _ in 0..70 {
        e.update(33, &mut random);
    }
    assert!(!e.is_playing() && !e.is_stopping());
}

#[test]
fn frames_animate_as_p7s_vector() {
    // particle-drawing.md 8.2, vector 4.
    let mut p = vec![Particle::default()];
    let mut got = Vec::new();
    for _ in 0..6 {
        super::super::actions::animate_frames(&mut p, 24.0, 4, STEP);
        got.push(p[0].frame);
    }
    assert_eq!(got, [201, 402, 603, 804, 1005, 186]);
    let mut back = vec![Particle {
        frame_flags: 1,
        ..Particle::default()
    }];
    super::super::actions::animate_frames(&mut back, 24.0, 4, STEP);
    assert_eq!(back[0].frame, 818);
}
