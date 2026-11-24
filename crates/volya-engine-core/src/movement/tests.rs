// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The movement rules on synthetic worlds, against the numbers of
//! player-movement.md (sections 5, 6.6, 7, 8, 12).

// Numbers are written as the data writes them (`rot_speed 3.14`).
#![allow(clippy::approx_constant)]

use super::*;
use crate::collision::test_world::Builder;

/// The shipped values (section 12).
fn config() -> MovementConfig {
    MovementConfig {
        walk_accel: 17.0,
        run_coef: 2.1,
        run_back_coef: 1.8,
        walk_back_coef: 0.8,
        crouch_coef: 0.45,
        sprint_koef: 2.6,
        walk_strafe_coef: 1.0,
        run_strafe_coef: 1.0,
        jump_speed: 6.0,
        air_control_param: 0.1,
        camera_height_factor: 0.85,
        crash_speed_min: 14.0,
        crash_speed_max: 30.0,
        body: BodyConfig {
            boxes: [[0.35, 0.9, 0.35], [0.35, 0.75, 0.35], [0.35, 0.6, 0.35]],
            mass: 80.0,
        },
        lim_pitch: [-1.5, 1.5],
        rot_speed: [3.14, 3.14, 0.0],
        bobbing: Bobbing {
            run_amplitude: 0.0075,
            run_speed: 10.0,
            walk_amplitude: 0.005,
            walk_speed: 7.0,
        },
    }
}

/// The shipped `user.ltx` settings (`cam_inert 0.`).
fn settings() -> Settings {
    Settings::read(&[b"g_always_run on\nmouse_sens 0.12\nmouse_invert off\ncam_inert 0.\n"])
}

fn floor() -> CollisionWorld {
    let mut b = Builder::default();
    b.floor([-100.0, 100.0], [-100.0, 100.0], 0.0, true);
    b.world()
}

/// A player at the origin looking along +z (yaw 0), settled on the floor.
fn player(world: &CollisionWorld) -> (Player, u64) {
    let mut p = Player::new(config(), settings(), [0.0; 3], Angles::default(), 0);
    let now = run(&mut p, world, 0, &Request::default(), 0.3);
    (p, now)
}

/// Frames of 16 ms for `seconds` with the same held `request`; presses
/// act on the first frame only. Returns the clock after.
fn run(
    p: &mut Player,
    world: &CollisionWorld,
    mut now: u64,
    request: &Request,
    seconds: f32,
) -> u64 {
    let held = Request {
        crouch_toggle: false,
        sprint_toggle: false,
        jump_pressed: false,
        jump_released: false,
        crouch_released: false,
        fire: false,
        mouse: [0.0; 2],
        ..*request
    };
    let frames = (seconds / 0.016).round() as u32;
    for i in 0..frames {
        now += 16;
        p.frame(world, now, if i == 0 { request } else { &held });
    }
    now
}

fn horizontal_speed(p: &Player) -> f32 {
    let v = p.state().velocity;
    v[0].hypot(v[2])
}

const FORWARD: Request = Request {
    forward: true,
    back: false,
    left: false,
    right: false,
    accel: false,
    crouch: false,
    crouch_released: false,
    crouch_toggle: false,
    sprint_toggle: false,
    jump_pressed: false,
    jump_released: false,
    fire: false,
    mouse: [0.0; 2],
    turn: [0.0; 2],
};

#[test]
fn settings_and_their_built_in_values() {
    assert_eq!(
        Settings::default(),
        Settings {
            always_run: true,
            mouse_sens: 0.12,
            mouse_invert: false,
            cam_inert: 0.7
        }
    );
    let s = Settings::read(&[
        b"g_always_run off\nmouse_sens 5\nbind jump kSPACE\n",
        b"mouse_invert on",
    ]);
    assert_eq!(
        (s.always_run, s.mouse_sens, s.mouse_invert),
        (false, 0.6, true)
    );
    assert_eq!(settings().cam_inert, 0.0);
}

#[test]
fn maximum_speeds_per_mode() {
    let world = floor();
    // Section 5's table: the cap is 0.1 × |a|.
    let cases: [(&str, Request, bool, f32); 9] = [
        ("run", FORWARD, false, 3.57),
        (
            "walk",
            Request {
                accel: true,
                ..FORWARD
            },
            false,
            1.7,
        ),
        (
            "run back",
            Request {
                forward: false,
                back: true,
                ..FORWARD
            },
            false,
            3.06,
        ),
        (
            "walk back",
            Request {
                forward: false,
                back: true,
                accel: true,
                ..FORWARD
            },
            false,
            1.36,
        ),
        (
            "run diagonal",
            Request {
                right: true,
                ..FORWARD
            },
            false,
            3.57,
        ),
        (
            "run back + right",
            Request {
                forward: false,
                back: true,
                right: true,
                ..FORWARD
            },
            false,
            3.06,
        ),
        (
            "sprint",
            Request {
                sprint_toggle: true,
                ..FORWARD
            },
            false,
            9.282,
        ),
        (
            "crouch",
            Request {
                crouch: true,
                ..FORWARD
            },
            true,
            1.6065,
        ),
        (
            "low crouch",
            Request {
                crouch: true,
                accel: true,
                ..FORWARD
            },
            true,
            0.765,
        ),
    ];
    for (name, request, crouched, speed) in cases {
        let (mut p, now) = player(&world);
        run(&mut p, &world, now, &request, 1.5);
        let got = horizontal_speed(&p);
        assert!(
            (got - speed).abs() < 0.01,
            "{name}: {got} m/s, expected {speed}"
        );
        let s = p.state();
        assert_eq!(s.flags.crouch, crouched, "{name}");
        assert!(!s.airborne, "{name}");
        assert!(s.position[1].abs() < 0.01, "{name}: {:?}", s.position);
        // Forward at yaw 0 is +z; "right" is +x.
        if name == "run" {
            assert!(
                s.position[2] > 4.0 && s.position[0].abs() < 1e-3,
                "{:?}",
                s.position
            );
        }
    }
}

#[test]
fn accelerates_at_the_driving_force_and_stops_on_friction() {
    let world = floor();
    let (mut p, now) = player(&world);
    // 25 m/s² on flat ground: 0.143 s from rest to 3.57 m/s.
    let now = run(&mut p, &world, now, &FORWARD, 0.096);
    let v = horizontal_speed(&p);
    assert!(v > 1.5 && v < 3.0, "{v}");
    let now = run(&mut p, &world, now, &FORWARD, 0.3);
    assert!((horizontal_speed(&p) - 3.57).abs() < 0.01);
    // Shift (walk): the lower cap within one frame.
    let walk = Request {
        accel: true,
        ..FORWARD
    };
    let now = run(&mut p, &world, now, &walk, 0.016);
    assert!(
        (horizontal_speed(&p) - 1.7).abs() < 0.01,
        "{}",
        horizontal_speed(&p)
    );
    // Released: contact friction stops the body.
    run(&mut p, &world, now, &Request::default(), 1.0);
    assert!(horizontal_speed(&p) < 0.01, "{}", horizontal_speed(&p));
    assert!(
        !p.state().flags.forward,
        "the state stops moving below 0.2 m/s"
    );
}

#[test]
fn the_hand_over_ignores_close_values() {
    let world = floor();
    let (mut p, _) = player(&world);
    // Section 12: 13.6 → 13.77 keeps the vector, the limit follows.
    p.hand_over([0.0, 0.0, 13.6]);
    p.hand_over([0.0, 0.0, 13.77]);
    assert_eq!(p.accepted, [0.0, 0.0, 13.6]);
    assert!((p.speed_limit - 1.377).abs() < 1e-6);
    p.hand_over([0.0, 0.0, 35.7]);
    assert_eq!(p.accepted, [0.0, 0.0, 35.7]);
    // A turn of 0.05 rad from an axis is accepted, 0.04 is not.
    let turned = |r: f32| [35.7 * r.sin(), 0.0, 35.7 * r.cos()];
    p.hand_over(turned(0.04));
    assert_eq!(p.accepted, [0.0, 0.0, 35.7]);
    p.hand_over(turned(0.0501));
    assert_eq!(p.accepted, turned(0.0501));
    p.hand_over([0.0; 3]);
    assert_eq!(p.accepted, [0.0; 3]);
}

#[test]
fn jumps_to_the_spec_height_and_lands() {
    let world = floor();
    let (mut p, mut now) = player(&world);
    let start = p.state().position[1];
    let press = Request {
        jump_pressed: true,
        ..Request::default()
    };
    let (mut top, mut air_time, mut was_airborne) = (start, 0.0f32, false);
    let mut landed_at = None;
    let idle = Request::default();
    for i in 0..60 {
        now += 16;
        p.frame(&world, now, if i == 0 { &press } else { &idle });
        let s = p.state();
        top = top.max(s.position[1]);
        if s.airborne {
            air_time += 0.016;
            was_airborne = true;
        } else if was_airborne && landed_at.is_none() {
            landed_at = Some(i);
        }
        if i == 0 {
            assert!(s.flags.jump, "the jump flag on take-off");
        }
    }
    // 6² / (2 × 19.62) = 0.917 m, 0.612 s in the air; fixed steps of
    // 0.01 s reach about v·h/2 = 0.03 m less.
    assert!((top - start - 0.917).abs() < 0.04, "apex {}", top - start);
    assert!((air_time - 0.612).abs() < 0.05, "air time {air_time}");
    assert!(landed_at.is_some(), "landed");
    let s = p.state();
    assert!(!s.flags.jump && !s.flags.falling, "{:?}", s.flags);
    // The floor's soft contact (level-cform.md 5.4) gives way a few
    // centimetres on landing and recovers slowly.
    assert!((s.position[1] - start).abs() < 0.06, "{:?}", s.position);
    // The key was held through the landing: no jump until it is released
    // and pressed again.
    now = run(&mut p, &world, now, &Request::default(), 0.5);
    assert!(!p.state().airborne && !p.state().flags.jump);
    let release = Request {
        jump_released: true,
        ..Request::default()
    };
    now = run(&mut p, &world, now, &release, 0.016);
    assert!(!p.state().flags.jump);
    now = run(&mut p, &world, now, &press, 0.016);
    assert!(
        p.state().flags.jump && p.state().airborne,
        "a new press jumps"
    );
    // Released in the air, pressed again right after landing: the 0.3 s
    // cooldown holds it back (4.4).
    now = run(&mut p, &world, now, &release, 0.016);
    let mut frames_to_land = 0;
    while p.state().airborne {
        now = run(&mut p, &world, now, &Request::default(), 0.016);
        frames_to_land += 1;
        assert!(frames_to_land < 100);
    }
    now = run(&mut p, &world, now, &press, 0.016);
    assert!(!p.state().flags.jump, "jumped within the cooldown");
    // The request stays while the key is held: it jumps once it expires.
    run(&mut p, &world, now, &Request::default(), 0.35);
    assert!(p.state().flags.jump, "jumps after the cooldown");
}

#[test]
fn a_running_jump_keeps_its_speed() {
    let world = floor();
    let (mut p, now) = player(&world);
    let mut now = run(&mut p, &world, now, &FORWARD, 0.5);
    now += 16;
    p.frame(
        &world,
        now,
        &Request {
            jump_pressed: true,
            ..FORWARD
        },
    );
    run(&mut p, &world, now, &FORWARD, 0.1);
    // 1.2 × 3.57 + 0.2 = 4.484 at take-off; the jump assistance pushes at
    // 12.5 m/s² for the first 0.3 m (about 0.9 m/s more), air control
    // adds little.
    let v = horizontal_speed(&p);
    assert!(p.state().airborne);
    assert!(v > 4.45 && v < 5.6, "{v}");
}

#[test]
fn postures_set_the_eye_height_at_once() {
    let world = floor();
    let (mut p, now) = player(&world);
    let eye = |p: &Player| p.view().eye[1] - p.state().position[1];
    assert!((eye(&p) - 1.53).abs() < 1e-4, "{}", eye(&p));
    let crouch = Request {
        crouch: true,
        ..Request::default()
    };
    let now = run(&mut p, &world, now, &crouch, 0.016);
    assert_eq!(p.state().posture, 1);
    assert!((eye(&p) - 1.275).abs() < 1e-4, "{}", eye(&p));
    // Shift while crouched: the low crouch.
    let low = Request {
        accel: true,
        ..crouch
    };
    let now = run(&mut p, &world, now, &low, 0.016);
    assert_eq!(p.state().posture, 2);
    assert!((eye(&p) - 1.02).abs() < 1e-4, "{}", eye(&p));
    let now = run(&mut p, &world, now, &Request::default(), 0.016);
    assert_eq!(p.state().posture, 0);
    assert!(!p.state().flags.crouch);
    // The toggle holds the crouch; releasing the crouch key releases it.
    let now = run(
        &mut p,
        &world,
        now,
        &Request {
            crouch_toggle: true,
            ..Request::default()
        },
        0.2,
    );
    assert!(p.state().flags.crouch && p.state().crouch_toggled);
    run(
        &mut p,
        &world,
        now,
        &Request {
            crouch_released: true,
            ..Request::default()
        },
        0.016,
    );
    assert!(!p.state().flags.crouch);
}

#[test]
fn a_low_ceiling_keeps_the_crouch() {
    let mut b = Builder::default();
    b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
    b.floor([-10.0, 10.0], [-10.0, 10.0], 1.6, false);
    let world = b.world();
    let mut p = Player::new(config(), settings(), [0.0; 3], Angles::default(), 0);
    let crouch = Request {
        crouch: true,
        ..Request::default()
    };
    let now = run(&mut p, &world, 0, &crouch, 0.3);
    assert_eq!(p.state().posture, 1);
    run(&mut p, &world, now, &Request::default(), 1.0);
    assert_eq!(p.state().posture, 1, "no room to stand");
    assert!(p.state().flags.crouch);
}

#[test]
fn sprint_needs_forward_running() {
    let world = floor();
    let (mut p, now) = player(&world);
    // Toggled while standing still: cancelled at once.
    let now = run(
        &mut p,
        &world,
        now,
        &Request {
            sprint_toggle: true,
            ..Request::default()
        },
        0.05,
    );
    assert!(!p.state().sprint_requested);
    let now = run(
        &mut p,
        &world,
        now,
        &Request {
            sprint_toggle: true,
            ..FORWARD
        },
        0.2,
    );
    assert!(p.state().flags.sprint);
    // A sideways key stops the sprint but keeps the request.
    let now = run(
        &mut p,
        &world,
        now,
        &Request {
            right: true,
            ..FORWARD
        },
        0.05,
    );
    assert!(!p.state().flags.sprint && p.state().sprint_requested);
    let now = run(&mut p, &world, now, &FORWARD, 0.05);
    assert!(p.state().flags.sprint);
    // Walking cancels it.
    run(
        &mut p,
        &world,
        now,
        &Request {
            accel: true,
            ..FORWARD
        },
        0.05,
    );
    assert!(!p.state().flags.sprint && !p.state().sprint_requested);
}

#[test]
fn looking_follows_the_mouse_and_keys_within_limits() {
    let world = floor();
    let (mut p, mut now) = player(&world);
    // Section 12: 0.0024 rad of yaw and 0.0018 of pitch per count.
    assert!((p.mouse_scale() - 0.0024).abs() < 1e-7);
    now += 16;
    p.frame(
        &world,
        now,
        &Request {
            mouse: [10.0, 10.0],
            ..Request::default()
        },
    );
    let a = p.angles();
    assert!(
        (a.yaw - 0.024).abs() < 1e-6 && (a.pitch - 0.018).abs() < 1e-6,
        "{a:?}"
    );
    now += 16;
    p.frame(
        &world,
        now,
        &Request {
            mouse: [0.0, 1e5],
            ..Request::default()
        },
    );
    assert_eq!(p.angles().pitch, 1.5);
    now += 16;
    p.frame(
        &world,
        now,
        &Request {
            mouse: [0.0, -1e5],
            ..Request::default()
        },
    );
    assert_eq!(p.angles().pitch, -1.5);
    // Inverted, moving the mouse toward the user looks up.
    p.settings.mouse_invert = true;
    let pitch = p.angles().pitch;
    now += 16;
    p.frame(
        &world,
        now,
        &Request {
            mouse: [0.0, -10.0],
            ..Request::default()
        },
    );
    assert!(p.angles().pitch > pitch);
    // Keys: π rad/s by the display frame time (which follows 16 ms frames).
    let yaw = p.angles().yaw;
    run(
        &mut p,
        &world,
        now,
        &Request {
            turn: [1.0, 0.0],
            ..Request::default()
        },
        1.0,
    );
    let turned = p.angles().yaw - yaw;
    assert!((turned - 3.14).abs() < 0.05, "{turned}");
    // Moving follows the yaw: a quarter turn right walks along +x.
    let (mut p, now) = player(&world);
    p.angles.yaw = std::f32::consts::FRAC_PI_2;
    run(&mut p, &world, now, &FORWARD, 0.5);
    let s = p.state().position;
    assert!(s[0] > 1.0 && s[2].abs() < 0.05, "{s:?}");
}

#[test]
fn walking_bobs_the_camera_slightly() {
    let world = floor();
    let (mut p, mut now) = player(&world);
    let base = p.view().eye[1] - p.state().position[1];
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for _ in 0..120 {
        now += 16;
        p.frame(&world, now, &FORWARD);
        let d = p.view().eye[1] - p.state().position[1] - 1.53;
        lo = lo.min(d);
        hi = hi.max(d);
    }
    // At most 7.5 mm up while running, never down.
    assert!(
        lo > -1e-4 && hi <= 0.0075 + 1e-4 && hi > 0.005,
        "{lo} … {hi}"
    );
    assert!((base - 1.53).abs() < 1e-4, "{base}");
}

#[test]
fn falling_off_a_ledge_lands_with_the_effect() {
    let mut b = Builder::default();
    b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
    b.floor([-10.0, 10.0], [-10.0, -1.0], 10.0, true);
    let world = b.world();
    // From 10 m: the extra down force before the body counts as airborne
    // (6.2) shortens a lower fall below the falling delay.
    let mut p = Player::new(
        config(),
        settings(),
        [0.0, 10.0, -1.5],
        Angles::default(),
        0,
    );
    let mut now = run(&mut p, &world, 0, &Request::default(), 0.2);
    let (mut falling, mut landing, mut raised) = (false, false, false);
    for _ in 0..150 {
        now += 16;
        p.frame(&world, now, &FORWARD);
        let s = p.state();
        falling |= s.flags.falling;
        landing |= s.flags.landing;
        raised |= p.landing_effect.is_some_and(|(t, _)| t > 0.0 && t < 1.0);
    }
    assert!(falling && landing && raised, "{falling} {landing} {raised}");
    assert!(
        p.state().position[1].abs() < 0.1 && !p.state().airborne,
        "{:?}",
        p.state()
    );
}

#[test]
fn releasing_the_low_crouch_under_a_ceiling_rises_to_the_crouch() {
    // Room for the crouch box (1.44 m) but not for standing (1.74 m).
    let mut b = Builder::default();
    b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
    b.floor([-10.0, 10.0], [-10.0, 10.0], 1.6, false);
    let world = b.world();
    let mut p = Player::new(config(), settings(), [0.0; 3], Angles::default(), 0);
    let low = Request {
        crouch: true,
        accel: true,
        ..Request::default()
    };
    let now = run(&mut p, &world, 0, &low, 0.3);
    assert_eq!(p.state().posture, 2);
    // Ctrl and Shift released together: the request runs, so the height
    // changes to box 1 (4.2), and standing up is refused.
    let released = Request {
        crouch_released: true,
        ..Request::default()
    };
    run(&mut p, &world, now, &released, 0.5);
    assert_eq!(p.state().posture, 1);
    assert!(p.state().flags.crouch);
}

#[test]
fn a_jump_against_a_wall_lands_and_stops_falling() {
    let mut b = Builder::default();
    b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
    b.wall_x(0.34, [0.0, 3.0], [-10.0, 10.0], true);
    let world = b.world();
    let mut p = Player::new(config(), settings(), [0.0; 3], Angles::default(), 0);
    let mut now = run(&mut p, &world, 0, &Request::default(), 0.3);
    assert!(!p.body().contacts().is_empty());
    let press = Request {
        jump_pressed: true,
        ..Request::default()
    };
    now = run(&mut p, &world, now, &press, 0.016);
    assert!(p.state().airborne);
    let mut falling = false;
    for _ in 0..80 {
        now = run(&mut p, &world, now, &Request::default(), 0.016);
        falling |= p.state().flags.falling;
    }
    let s = p.state();
    assert!(falling, "it fell");
    assert!(!s.airborne && !s.flags.falling && !s.flags.jump, "{s:?}");
}

#[test]
fn a_jump_pressed_again_in_the_air_waits_for_a_release() {
    let world = floor();
    let (mut p, now) = player(&world);
    let press = Request {
        jump_pressed: true,
        ..Request::default()
    };
    let release = Request {
        jump_released: true,
        ..Request::default()
    };
    let mut now = run(&mut p, &world, now, &press, 0.016);
    now = run(&mut p, &world, now, &release, 0.1);
    // Released, then pressed and held while still in the air.
    now = run(&mut p, &world, now, &press, 0.016);
    assert!(p.state().airborne);
    while p.state().airborne {
        now = run(&mut p, &world, now, &Request::default(), 0.016);
    }
    // Held through the landing: no jump, however long after the cooldown.
    now = run(&mut p, &world, now, &Request::default(), 1.0);
    assert!(!p.state().airborne && !p.state().flags.jump);
    now = run(&mut p, &world, now, &release, 0.016);
    run(&mut p, &world, now, &press, 0.016);
    assert!(p.state().flags.jump && p.state().airborne);
}
