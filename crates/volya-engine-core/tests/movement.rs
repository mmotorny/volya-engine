// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player walking `l01_escape` from its start on the Russian edition
//! (BYOA), checked against docs/behavior/player-movement.md: the
//! configuration of section 12, and a walk at the run speed of section 5.
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

// Numbers are written as the data writes them.
#![allow(clippy::approx_constant)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::f32::consts::{PI, TAU};
use std::time::Instant;

use volya_engine_core::camera::Angles;
use volya_engine_core::collision::CollisionWorld;
use volya_engine_core::config;
use volya_engine_core::movement::{MovementConfig, Player, PlayerState, Request, Settings};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, read};
use volya_engine_formats::{gamemtl, spawn};

#[test]
fn walks_from_the_start_at_the_run_speed() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let system = config::load(&tree, "gamedata/config/system.ltx").unwrap();
    let c = MovementConfig::read(&system, spawn::ACTOR_SECTION).unwrap();
    // Section 12.
    assert_eq!(
        (
            c.walk_accel,
            c.jump_speed,
            c.run_coef,
            c.run_back_coef,
            c.walk_back_coef
        ),
        (17.0, 6.0, 2.1, 1.8, 0.8)
    );
    assert_eq!(
        (c.crouch_coef, c.sprint_koef, c.air_control_param),
        (0.45, 2.6, 0.1)
    );
    assert_eq!((c.walk_strafe_coef, c.run_strafe_coef), (1.0, 1.0));
    assert_eq!(
        (
            c.camera_height_factor,
            c.body.mass,
            c.crash_speed_min,
            c.crash_speed_max
        ),
        (0.85, 80.0, 14.0, 30.0)
    );
    assert_eq!(c.body.boxes[1], [0.35, 0.75, 0.35]);
    assert_eq!((c.lim_pitch, c.rot_speed), ([-1.5, 1.5], [3.14, 3.14, 0.0]));
    let b = c.bobbing;
    assert_eq!(
        (b.run_amplitude, b.walk_amplitude, b.run_speed, b.walk_speed),
        (0.0075, 0.005, 10.0, 7.0)
    );

    let materials = gamemtl::parse(&read(&tree, "gamedata/gamemtl.xr").unwrap()).unwrap();
    let f = tree.open("gamedata/levels/l01_escape/level.cform").unwrap();
    let world = CollisionWorld::read(f, materials).unwrap();
    let actor = spawn::actor_start(&read(&tree, "gamedata/spawns/all.spawn").unwrap()).unwrap();
    // The shipped user settings' camera inertia (`cam_inert 0.`).
    let settings = Settings {
        cam_inert: 0.0,
        ..Settings::default()
    };
    let angles = Angles::from_spawn(actor.angles);
    let mut player = Player::new(c, settings, actor.position, angles, 0);
    // player-movement.md 7.1: the eye 1.53 m above the stored position.
    let eye = player.view().eye;
    assert!(
        (eye[1] - (actor.position[1] + 1.53)).abs() < 1e-4,
        "{eye:?}"
    );

    // Standing a moment (collision stops a run along the start view after
    // about 0.8 m, measured), then turned half round with the mouse, where
    // the way is clear for more than a metre (measured).
    let mut now = 0;
    let idle = Request::default();
    for _ in 0..30 {
        now += 16;
        player.frame(&world, now, &idle);
    }
    now += 16;
    let half_turn = Request {
        mouse: [PI / player.mouse_scale(), 0.0],
        ..idle
    };
    player.frame(&world, now, &half_turn);
    let yaw = player.angles().yaw;
    assert!((yaw - angles.yaw - PI).abs() < 1e-3, "{yaw}");
    let start = player.state().position;

    let forward = Request {
        forward: true,
        ..Request::default()
    };
    let run = |player: &mut Player, now: &mut u64, frames: u32| -> (PlayerState, f32, f32) {
        for _ in 0..frames {
            *now += 16;
            player.frame(&world, *now, &forward);
        }
        let s = player.state();
        let moved = [s.position[0] - start[0], s.position[2] - start[2]];
        (
            s,
            moved[0].hypot(moved[1]),
            s.velocity[0].atan2(s.velocity[2]),
        )
    };
    // Running forward in 16 ms frames: 25 m/s² up to 3.57 m/s (0.143 s),
    // then the cap. In 22 frames (0.352 s): 3.57 × 0.352 − 3.57 × 0.143 / 2
    // ≈ 1.0 m, short of a 0.16 m step measured 1.3 m ahead.
    let timer = Instant::now();
    let (s, distance, heading) = run(&mut player, &mut now, 22);
    let per_frame = timer.elapsed() / 22;
    let speed = s.velocity[0].hypot(s.velocity[2]);
    eprintln!(
        "l01_escape: {distance:.3} m in 0.352 s, {speed:.3} m/s, height change {:.4} m; \
         {per_frame:.1?} per frame",
        s.position[1] - start[1]
    );
    assert!((speed - 3.57).abs() < 0.01, "{speed} m/s");
    assert!((distance - 1.0).abs() < 0.03, "{distance} m");
    // Moving along the camera's yaw (5.3; the first frame from standing
    // follows the body's heading, within π/4 of it).
    let off = (heading - yaw).rem_euclid(TAU);
    assert!(off.min(TAU - off) < 0.01, "heading {heading}, yaw {yaw}");
    assert!(!s.airborne);
    assert!((s.position[1] - start[1]).abs() < 0.01, "{:?}", s.position);

    // On over the step and beyond: on the ground throughout, back at the
    // run speed.
    let mut highest = 0.0f32;
    for _ in 0..41 {
        let (s, _, _) = run(&mut player, &mut now, 1);
        assert!(!s.airborne, "{s:?}");
        highest = highest.max(s.position[1] - start[1]);
    }
    let (s, distance, _) = run(&mut player, &mut now, 0);
    let speed = s.velocity[0].hypot(s.velocity[2]);
    eprintln!("then {distance:.3} m in 1.008 s, over a step {highest:.3} m high");
    assert!(
        distance > 3.0 && (speed - 3.57).abs() < 0.01,
        "{distance} m, {speed} m/s"
    );
    assert!(highest > 0.1 && highest < 0.3, "{highest}");
    assert!((s.position[1] - start[1]).abs() < 0.05, "{:?}", s.position);
}
