// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The ladder's frame and the climbing rules on synthetic ladders, against
//! the rules and worked cases of docs/behavior/climbing.md (sections 1.2,
//! 2, 3.2, 3.5, 3.6, 5.3).

use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

use super::*;

/// A thin upright ladder facing +z (angles 0: the rows are the axes),
/// 0.8 m wide and 4 m long, its record at (0, 2, 0): B = (0, 0, 0),
/// T = (0, 4, 0), C = (0, 2, −0.2).
fn upright() -> Ladder {
    Ladder::new("test", [0.0, 2.0, 0.0], [0.0; 3], [0.4, 2.0, 0.02])
}

fn close(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-5)
}

const R: f32 = 0.35;

/// Section 5.3's set-up on [`upright`]: P = B + 0.5·Â + 0.36·N̂, the camera
/// level and facing −N̂, running towards the ladder at 24.99.
fn view(l: &Ladder) -> StepView {
    let p = add(
        add(l.lower, scale(l.axis_dir, 0.5)),
        scale(l.normal_dir, 0.36),
    );
    StepView {
        centre: p,
        r: R,
        a: scale(l.normal_dir, -24.99),
        camera: scale(l.normal_dir, -1.0),
        velocity: [0.0; 3],
        clock: 1,
        mass: 80.0,
        gravity: 19.62,
    }
}

/// A climber that found `l` at clock 0 (2.4) and is in `state`.
fn climber(l: &[Ladder], state: ClimbState) -> Climber {
    let mut c = Climber::default();
    let v = view(&l[0]);
    c.found(l, 0, &StepView { clock: 0, ..v });
    assert_eq!((c.ladder(), c.state()), (Some(0), ClimbState::Near));
    if state != ClimbState::Near {
        c.state = state;
    }
    c
}

#[test]
fn roles_follow_the_extents() {
    // 1.2 step 2: each row of the table.
    for (e, roles) in [
        ([1.0, 2.0, 0.5], [1, 0, 2]),
        ([1.0, 3.0, 2.0], [1, 2, 0]),
        ([1.0, 2.0, 3.0], [2, 1, 0]),
        ([3.0, 2.0, 1.0], [0, 1, 2]),
        ([3.0, 1.0, 2.0], [0, 2, 1]),
        ([2.0, 1.0, 3.0], [2, 0, 1]),
        // Ties: e0 = e1 counts as e0 ≥ e1, e2 = e1 as e2 ≤ e1.
        ([2.0, 2.0, 2.0], [0, 1, 2]),
    ] {
        assert_eq!(Ladder::new("", [0.0; 3], [0.0; 3], e).roles, roles, "{e:?}");
    }
}

#[test]
fn the_frame_of_a_thin_ladder() {
    let l = upright();
    assert_eq!(l.roles, [1, 0, 2]);
    // The normal extent 0.02 < 0.2: raised, the centre moved back.
    assert!(l.raised && !l.flipped);
    assert!(close(l.centre, [0.0, 2.0, -0.2]));
    assert!(close(l.axis, [0.0, 2.0, 0.0]) && close(l.side, [0.4, 0.0, 0.0]));
    assert!(close(l.normal, [0.0, 0.0, 0.2]));
    assert!(close(l.lower, [0.0, 0.0, 0.0]) && close(l.upper, [0.0, 4.0, 0.0]));
    assert_eq!(l.radius, 2.0);
    // The box: half-sizes with the raised extent, at C.
    assert_eq!(l.half, [0.4, 2.0, 0.2]);
    assert!(close(l.min, [-0.4, 0.0, -0.4]) && close(l.max, [0.4, 4.0, 0.0]));
    // Its twelve triangles face out.
    for t in &l.triangles {
        let mid = scale(add(add(t[0], t[1]), t[2]), 1.0 / 3.0);
        let n = front_normal(t).unwrap();
        assert!(dot(n, sub(mid, l.centre)) > 0.0, "{t:?}");
    }
    // Upside down (a1 = π about y keeps it upright; a0 = π turns it over):
    // A.y < 0, so A and S are negated and B stays at the lower end.
    let over = Ladder::new("", [0.0, 2.0, 0.0], [PI, 0.0, 0.0], [0.4, 2.0, 0.3]);
    assert!(over.flipped && !over.raised);
    assert!(over.axis[1] > 0.0);
    assert!(over.lower[1] < over.upper[1]);
    // N is never negated: row 2 of a0 = π is (0, 0, −1).
    assert!(close(over.normal_dir, [0.0, 0.0, -1.0]));
    // The minimum half-size.
    let flat = Ladder::new("", [0.0; 3], [0.0; 3], [1.0, 3.0, 0.0]);
    assert!(flat.half.iter().all(|&h| h >= MIN_HALF_SIZE));
}

#[test]
fn quantities_and_predicates() {
    let l = upright();
    let v = view(&l);
    let m = l.measure(v.centre);
    // 5.3: in front, in range, δ = −0.56.
    assert!((m.delta + 0.56).abs() < 1e-5);
    assert!((m.d_axis - 0.56).abs() < 1e-5 && close(m.towards_axis, [0.0, 0.0, -1.0]));
    assert!(m.front(&l, R, 0.0) && m.in_range(&l, R));
    assert!((m.h_low - 0.5).abs() < 1e-5 && (m.h_top - 3.5).abs() < 1e-5);
    assert!(m.lat.abs() < 1e-6);
    assert!(facing(v.camera, m.v_n));
    // front(0) needs −δ > |N| + r/2 = 0.375, in range |δ| < 0.6, with the
    // axis 0.2 behind the front face at z = 0.
    let at = |dz: f32| l.measure([0.0, 0.5, dz]);
    assert!(!at(0.17).front(&l, R, 0.0) && at(0.18).front(&l, R, 0.0));
    assert!(at(0.39).in_range(&l, R) && !at(0.41).in_range(&l, R));
    // Beside the ladder, below its foot, above its top.
    assert!(!l.measure([0.41, 0.5, 0.2]).in_range(&l, R));
    assert!(!l.measure([0.0, -0.21, 0.2]).in_range(&l, R));
    assert!(l.measure([0.0, 4.3, 0.2]).in_range(&l, R));
    assert!(!l.measure([0.0, 4.36, 0.2]).in_range(&l, R));
    // facing: within 22.5°, and never for a vertical direction.
    let (s, c) = (0.39f32).sin_cos();
    assert!(facing([s, 0.0, -c], m.v_n));
    let (s, c) = (0.4f32).sin_cos();
    assert!(!facing([s, 0.0, -c], m.v_n));
    assert!(!facing([0.0, -1.0, 0.0], m.v_n));
    assert_eq!(hcos([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]), None);
}

#[test]
fn the_intent() {
    let l = upright();
    let v = view(&l);
    let v_n = l.measure(v.centre).v_n;
    // Running towards the ladder, the camera level: up.
    let k = intent(v.a, v.camera, v_n);
    assert!((k - INTENT_PITCH * 24.99 * 0.56).abs() < 1e-3, "{k}");
    // The camera 30° down: down; no input: 0; away: negative.
    assert!(intent(v.a, [0.0, -0.5, -0.866], v_n) < 0.0);
    assert_eq!(intent([0.0; 3], v.camera, v_n), 0.0);
    assert!(intent(scale(v.a, -1.0), v.camera, v_n) < 0.0);
}

#[test]
fn near_takes_the_ladder() {
    // 5.3, M4: towards the ladder → climbing up; the camera 30° down or no
    // input → climbing down; gravity off in both.
    let l = [upright()];
    let v = view(&l[0]);
    let mut c = climber(&l, ClimbState::Near);
    c.rules(&l, &v);
    assert_eq!(c.state(), ClimbState::ClimbingUp);
    assert!(c.state().climbing());
    let mut c = climber(&l, ClimbState::Near);
    c.rules(
        &l,
        &StepView {
            camera: [0.0, -0.5, -0.866_025_4],
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::ClimbingDown);
    let mut c = climber(&l, ClimbState::Near);
    c.rules(&l, &StepView { a: [0.0; 3], ..v });
    assert_eq!(c.state(), ClimbState::ClimbingDown);
    // At clock 0, where it was found, the change is refused (2.3).
    let mut c = climber(&l, ClimbState::Near);
    c.rules(&l, &StepView { clock: 0, ..v });
    assert_eq!(c.state(), ClimbState::Near);
    // Not facing it: near its foot, at the bottom.
    let mut c = climber(&l, ClimbState::Near);
    let p = [0.0, 0.36, 0.5];
    c.rules(
        &l,
        &StepView {
            centre: p,
            camera: [1.0, 0.0, 0.0],
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::AtBottom);
}

#[test]
fn leaving_and_the_refusal() {
    // 5.3, M5: climbing up with input along Ŝ → leaving; → free refused
    // 1,000 and 3,000 ms after with the body moved 1 m, accepted at 3,001.
    let l = [upright()];
    let v = view(&l[0]);
    let mut c = climber(&l, ClimbState::ClimbingUp);
    c.rules(
        &l,
        &StepView {
            a: [24.99, 0.0, 0.0],
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::Leaving);
    assert!(!c.state().climbing());
    let moved = add(v.centre, [0.0, 0.0, 1.0]);
    for (clock, state) in [
        (1_001, ClimbState::Leaving),
        (3_001, ClimbState::Leaving),
        (3_002, ClimbState::Free),
    ] {
        c.rules(
            &l,
            &StepView {
                centre: moved,
                clock,
                ..v
            },
        );
        assert_eq!(c.state(), state, "{clock}");
    }
    // Free: the ladder is dropped.
    c.rules(&l, &StepView { clock: 3_003, ..v });
    assert_eq!(c.ladder(), None);
    // Moved more than 2 m, accepted at once.
    let mut c = climber(&l, ClimbState::ClimbingUp);
    c.rules(
        &l,
        &StepView {
            a: [24.99, 0.0, 0.0],
            ..v
        },
    );
    let far = add(v.centre, [0.0, 0.0, 2.1]);
    c.rules(
        &l,
        &StepView {
            centre: far,
            clock: 2,
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::Free);
}

#[test]
fn the_top_end() {
    // 5.3, M6: climbing up with P at h_top = 0.2, −0.2 stays; at −0.3 → at
    // the top (more than 0.25 above T along the axis).
    let l = [upright()];
    let v = view(&l[0]);
    for (h_top, state) in [
        (0.2, ClimbState::ClimbingUp),
        (-0.2, ClimbState::ClimbingUp),
        (-0.3, ClimbState::AtTop),
    ] {
        let mut c = climber(&l, ClimbState::ClimbingUp);
        let p = [0.0, 4.0 - h_top, 0.56];
        c.rules(&l, &StepView { centre: p, ..v });
        assert_eq!(c.state(), state, "{h_top}");
    }
    // At the top, looking down by more than 9° in front of it: climbing
    // down; looking level: not.
    let p = [0.0, 4.2, 0.35];
    let mut c = climber(&l, ClimbState::AtTop);
    c.rules(&l, &StepView { centre: p, ..v });
    assert_eq!(c.state(), ClimbState::AtTop);
    let down = [0.0, -0.2, -0.98];
    c.rules(
        &l,
        &StepView {
            centre: p,
            camera: down,
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::ClimbingDown);
    // Far from its top: free.
    let mut c = climber(&l, ClimbState::AtTop);
    c.rules(
        &l,
        &StepView {
            centre: [0.0, 4.2, 2.5],
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::Free);
}

#[test]
fn the_bottom_end() {
    let l = [upright()];
    let v = view(&l[0]);
    // At the bottom, facing B, pushing towards it with the camera level:
    // climbing up.
    let p = [0.0, 0.35, 0.3];
    let to_b = unit(sub(l[0].lower, p));
    let mut c = climber(&l, ClimbState::AtBottom);
    c.rules(
        &l,
        &StepView {
            centre: p,
            camera: [0.0, 0.0, -1.0],
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::ClimbingUp);
    assert!(to_b[2] < 0.0);
    // Climbing down to the foot: at the bottom.
    let mut c = climber(&l, ClimbState::ClimbingDown);
    c.rules(
        &l,
        &StepView {
            centre: [0.0, 0.44, 0.5],
            a: [0.0; 3],
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::AtBottom);
}

#[test]
fn held_on_the_ladder() {
    let l = [upright()];
    let v = view(&l[0]);
    // No input: towards the axis at m·g (2.5, the common rule).
    let mut c = climber(&l, ClimbState::ClimbingUp);
    let f = c.rules(&l, &StepView { a: [0.0; 3], ..v });
    assert_eq!(c.state(), ClimbState::ClimbingUp);
    assert!(close(f, [0.0, 0.0, -80.0 * 19.62]), "{f:?}");
    // Climbing down and moving up: pulled down as well.
    let mut c = climber(&l, ClimbState::ClimbingDown);
    let f = c.rules(
        &l,
        &StepView {
            a: [0.0; 3],
            velocity: [0.0, 0.5, 0.0],
            ..v
        },
    );
    assert!(close(f, [0.0, -80.0 * 19.62, -80.0 * 19.62]), "{f:?}");
    // With input: no holding force.
    let mut c = climber(&l, ClimbState::ClimbingUp);
    assert_eq!(c.rules(&l, &v), [0.0; 3]);
    // Pushed away from the axis by more than 1.5 + r: free.
    let mut c = climber(&l, ClimbState::ClimbingUp);
    c.rules(
        &l,
        &StepView {
            centre: [0.0, 1.0, 1.9],
            ..v
        },
    );
    assert_eq!(c.state(), ClimbState::Free);
}

#[test]
fn the_climbing_drive() {
    // 5.3, M7: climbing up, the direction of Â + û.
    let l = [upright()];
    let v = view(&l[0]);
    let c = climber(&l, ClimbState::ClimbingUp);
    let Some(Drive::Climb(d)) = c.drive(&l, &v) else {
        panic!()
    };
    assert!(close(d, [0.0, FRAC_1_SQRT_2, -FRAC_1_SQRT_2]), "{d:?}");
    // Climbing down in front: û − Â; not in front: the ground rules
    // without the step assistance's factor.
    let c = climber(&l, ClimbState::ClimbingDown);
    let Some(Drive::Climb(d)) = c.drive(&l, &v) else {
        panic!()
    };
    assert!(close(d, [0.0, -FRAC_1_SQRT_2, -FRAC_1_SQRT_2]), "{d:?}");
    let behind = StepView {
        centre: [0.0, 1.0, -0.1],
        ..v
    };
    assert_eq!(
        c.drive(&l, &behind),
        Some(Drive::Ground {
            steer: None,
            no_assist_factor: true
        })
    );
    // At the top, facing T and pushing towards it: steered onto it.
    let c = climber(&l, ClimbState::AtTop);
    let top = StepView {
        centre: [0.0, 4.3, 0.6],
        ..v
    };
    let Some(Drive::Ground { steer: Some(w), .. }) = c.drive(&l, &top) else {
        panic!("{:?}", c.drive(&l, &top));
    };
    assert!(close(w, unit([0.0, -0.3, -0.6])));
    // Facing away: the ground rules unchanged.
    let away = StepView {
        camera: [0.0, 0.0, 1.0],
        a: [0.0, 0.0, 24.99],
        ..top
    };
    assert_eq!(c.drive(&l, &away), None);
}

#[test]
fn finding_a_ladder() {
    // 2.4: within 1.5 m of the axis; a nearer ladder replaces the player's.
    let l = [
        upright(),
        Ladder::new("b", [1.0, 2.0, 0.0], [0.0; 3], [0.4, 2.0, 0.02]),
    ];
    let v = view(&l[0]);
    let mut c = Climber::default();
    c.found(
        &l,
        1,
        &StepView {
            centre: [2.6, 1.0, 0.0],
            ..v
        },
    );
    assert_eq!(c.ladder(), None);
    c.found(
        &l,
        1,
        &StepView {
            centre: [0.7, 1.0, 0.36],
            ..v
        },
    );
    assert_eq!(c.ladder(), Some(1));
    c.found(
        &l,
        0,
        &StepView {
            centre: [0.7, 1.0, 0.36],
            ..v
        },
    );
    assert_eq!(c.ladder(), Some(1));
    c.found(
        &l,
        0,
        &StepView {
            centre: [0.3, 1.0, 0.36],
            ..v
        },
    );
    assert_eq!(c.ladder(), Some(0));
    // The ladder is taken even when the change is refused (2.3: the same
    // position at the same clock).
    let mut c = Climber::default();
    let at = StepView {
        centre: [0.3, 1.0, 0.36],
        ..v
    };
    assert!(c.change(ClimbState::ClimbingUp, at.clock, at.centre));
    c.found(&l, 0, &at);
    assert_eq!((c.ladder(), c.state()), (Some(0), ClimbState::ClimbingUp));
}

#[test]
fn jumping_off() {
    // 5.3, M8: input along Ŝ jumps along N̂ + Ŝ at 3 m/s; otherwise along N̂.
    let l = upright();
    let j = jump_velocity(&l, [24.99, 0.0, 0.0]);
    assert!(
        close(j, [3.0 * FRAC_1_SQRT_2, 0.0, 3.0 * FRAC_1_SQRT_2]),
        "{j:?}"
    );
    let j = jump_velocity(&l, [-24.99, 0.0, 0.0]);
    assert!(
        close(j, [-3.0 * FRAC_1_SQRT_2, 0.0, 3.0 * FRAC_1_SQRT_2]),
        "{j:?}"
    );
    assert!(close(
        jump_velocity(&l, [0.0, 0.0, -24.99]),
        [0.0, 0.0, 3.0]
    ));
    assert!(close(jump_velocity(&l, [0.0; 3]), [0.0, 0.0, 3.0]));
}

#[test]
fn yaw_limits_on_the_ladder() {
    // 3.6: ψ is the yaw of −N̂; facing it, ±1 rad about it.
    let n = [0.0, 0.0, 1.0];
    let [lo, hi] = yaw_limits(n, 0.0, PI).unwrap();
    assert!((lo - (PI - 1.0)).abs() < 1e-5 && (hi - (PI + 1.0)).abs() < 1e-5);
    // The stored yaw is kept as it is, its wrapped value compared.
    let [lo, hi] = yaw_limits(n, 0.0, 3.0 * PI - 0.5).unwrap();
    assert!((lo - (3.0 * PI - 1.0)).abs() < 1e-4, "{lo}");
    assert!((hi - (3.0 * PI + 1.0)).abs() < 1e-4, "{hi}");
    let y = -PI + 0.5;
    let [lo, hi] = yaw_limits(n, 0.0, y).unwrap();
    assert!((lo - (y - 1.5)).abs() < 1e-5 && (hi - (y + 0.5)).abs() < 1e-5);
    // More than 1 rad away: not limited.
    assert_eq!(yaw_limits(n, 0.0, 0.0), None);
    assert_eq!(yaw_limits(n, 0.0, PI + 1.1), None);
    // A ladder lying flat: the body's heading is ψ.
    assert!(yaw_limits([0.0, 1.0, 0.0], 0.3, 0.5).is_some());
    assert_eq!(yaw_limits([0.0, 1.0, 0.0], 2.0, 0.5), None);
    // Wrapping.
    assert_eq!(wrap_angle(1.0), 1.0);
    assert!((wrap_angle(TAU + 1.0) - 1.0).abs() < 1e-5);
    assert!((wrap_angle(-TAU - 1.0) + 1.0).abs() < 1e-5);
    assert!((wrap_angle(4.0) - (4.0 - TAU)).abs() < 1e-5);
}
