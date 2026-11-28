// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The trader's door at a New Game's start on the Russian edition's data
//! (BYOA), against docs/behavior/trader-door.md: the door found with its
//! locked section, its leaf's box (1), the scheme's start and motor (2.2),
//! the player run at it stopped (3) and the door-locked hint while it is
//! under the crosshair within 2 m (5.1). Skips when `VOLYA_GOG_INSTALLER`
//! is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::controls::MoveAction;
use volya_engine::core::camera::Angles;
use volya_engine::core::collision::geometry::{Vec3, sub};
use volya_engine::core::input::{Control, Event, Input, Key};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);
const FRAME_MS: u64 = 16;

struct Run {
    level: LevelScreen,
    input: Input,
    now: u64,
}

impl Run {
    fn frames(&mut self, n: u32) {
        for _ in 0..n {
            self.now += FRAME_MS;
            self.level.set_global_clock(self.now);
            self.level.update(self.now, &self.input);
            self.input.end_frame();
        }
    }

    fn feet(&self) -> Vec3 {
        self.level.player().state().position
    }

    fn hint(&self) -> Option<String> {
        self.level.state().player.hint
    }
}

/// The horizontal distance from `p` to the leaf's plane through `centre`
/// with normal `n` (unit, horizontal), signed.
fn across(p: Vec3, centre: Vec3, n: Vec3) -> f32 {
    let d = sub(p, centre);
    d[0] * n[0] + d[2] * n[2]
}

#[test]
fn the_locked_door_stops_the_player_and_shows_its_hint() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let level = LevelScreen::load(&tree, &data, 0).unwrap();

    // Section 1, 2.1: one door, the record `trader_door`, in its locked
    // section; no motor and no hint before its first update.
    let doors = level.state().doors;
    assert_eq!(doors.len(), 1, "{doors:?}");
    let d = &doors[0];
    assert_eq!(
        (
            d.name.as_str(),
            d.section.as_str(),
            d.locked,
            d.motor,
            d.tip.as_deref()
        ),
        ("trader_door", "ph_door@locked", true, None, None)
    );
    // Its leaf: one box, half sizes as the model's bone `door`, of
    // `objects\large_metal_trash`; 1.7 m from the start (level-objects.md).
    let boxes = level.player().body().bodies().to_vec();
    assert_eq!(boxes.len(), 1);
    let leaf = boxes[0];
    for (got, want) in leaf.half_size.into_iter().zip([0.690, 1.322, 0.125]) {
        assert!((got - want).abs() < 1e-3, "{:?}", leaf.half_size);
    }
    let material = &level.collision().materials()[leaf.material as usize];
    assert_eq!(material.name, "objects\\large_metal_trash");
    let start = level.player().state().position;
    // The leaf's thin axis is its normal; which side the player starts on
    // decides its sign.
    let k = (0..3)
        .min_by(|&a, &b| leaf.half_size[a].total_cmp(&leaf.half_size[b]))
        .unwrap();
    let mut n = leaf.axes[k];
    if across(start, leaf.centre, n) < 0.0 {
        n = n.map(|v| -v);
    }
    let gap = across(start, leaf.centre, n) - leaf.half_size[k];
    eprintln!(
        "leaf centre {:?}, normal {n:?}; start {start:?}, {gap:.3} m from its face",
        leaf.centre
    );

    let mut run = Run {
        level,
        input: Input::new(),
        now: 0,
    };
    // 2.2: the first update closes it (200 / 3), the next physics step
    // holds it (10000 / 1).
    run.frames(1);
    let d = &run.level.state().doors[0];
    assert!(d.motor.is_some(), "{d:?}");
    run.frames(2);
    let d = &run.level.state().doors[0];
    assert_eq!(
        (d.motor, d.tip.as_deref()),
        (Some([10_000.0, 1.0]), Some("tip_door_locked"))
    );

    // Face the leaf's centre, level, and run at it.
    let to = sub(leaf.centre, start);
    let p = run.level.player();
    let h = p.config.eye_height(p.body().box_index());
    let angles = Angles {
        yaw: to[0].atan2(to[2]),
        pitch: 0.0,
        roll: 0.0,
    };
    run.level
        .set_camera([start[0], start[1] + h, start[2]], angles);
    run.frames(5);
    let hint_at_start = run.hint();
    run.input.handle(Event::Key {
        key: Key::W,
        pressed: true,
    });
    let mut closest = f32::INFINITY;
    for _ in 0..150 {
        run.frames(1);
        closest = closest.min(across(run.feet(), leaf.centre, n) - leaf.half_size[k]);
    }
    let feet = run.feet();
    let left = across(feet, leaf.centre, n) - leaf.half_size[k];
    eprintln!(
        "hint at the start {hint_at_start:?}; after 2.4 s running: feet {feet:?}, {left:.3} m from the face (closest {closest:.3}), hint {:?}",
        run.hint()
    );
    // 3: stopped in front of the leaf: the body's radius from its face,
    // give or take the contacts' softness.
    assert!(
        closest > 0.25 && left < 0.5,
        "{closest} … {left} m from the face"
    );
    // 5.1: the leaf under the crosshair, well within 2 m: the hint.
    assert_eq!(run.hint().as_deref(), Some("tip_door_locked"));
    assert_eq!(data.strings.get("tip_door_locked"), "Дверь заперта");
    // Turned away, none.
    run.input.handle(Event::Key {
        key: Key::W,
        pressed: false,
    });
    let feet = run.feet();
    run.level.set_camera(
        [feet[0], feet[1] + h, feet[2]],
        Angles {
            yaw: angles.yaw + std::f32::consts::PI,
            ..angles
        },
    );
    run.frames(2);
    assert_eq!(run.hint(), None);
    // A use while turned away does nothing.
    let press = |run: &mut Run, control: Control| {
        let Control::Key(key) = control else {
            panic!("{control:?}")
        };
        for pressed in [true, false] {
            run.input.handle(Event::Key { key, pressed });
        }
        run.frames(1);
    };
    let use_key = run
        .level
        .controls()
        .bindings
        .controls(MoveAction::Use)
        .find(|c| matches!(c, Control::Key(_)))
        .expect("a key bound to use");
    eprintln!("use is bound to {use_key:?}");
    press(&mut run, use_key);
    assert!(run.level.state().doors[0].locked);

    // 2.5: facing the door again, a use leaves it locked and in the way.
    run.level
        .set_camera([feet[0], feet[1] + h, feet[2]], angles);
    run.frames(2);
    assert_eq!(run.hint().as_deref(), Some("tip_door_locked"));
    press(&mut run, use_key);
    let d = &run.level.state().doors[0];
    assert!(d.locked && d.section == "ph_door@locked", "{d:?}");
    assert_eq!(run.hint().as_deref(), Some("tip_door_locked"));
    run.input.handle(Event::Key {
        key: Key::W,
        pressed: true,
    });
    run.frames(60);
    let stopped = across(run.feet(), leaf.centre, n) - leaf.half_size[k];
    assert!(stopped > 0.25, "{stopped} m from the face");
}
