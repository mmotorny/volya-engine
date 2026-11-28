// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The trader's door leaf against the player's body at a New Game's start
//! on `l01_escape` on the Russian edition's data (BYOA), against
//! docs/behavior/physics-contacts.md 7: the player run at the locked leaf
//! meets it through the physics world's contacts, kept by the player's
//! rules, which push the leaf (its motor holding it closed) and stop the
//! player; walking away, none. Measures the physics step's cost with the
//! player's body in the world. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::camera::Angles;
use volya_engine::core::collision::geometry::{Vec3, sub};
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::objects::level_objects;
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

    fn contacts(&self) -> u64 {
        self.level.physics().player_contacts()
    }

    fn key(&mut self, key: Key, pressed: bool) {
        self.input.handle(Event::Key { key, pressed });
    }
}

#[test]
fn the_leaf_meets_the_players_body() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let level = LevelScreen::load(&tree, &data, 0).unwrap();
    // 5: the body's shapes carry the actor section's material.
    let materials = level.collision().materials();
    let m = &materials[level.player().body().material() as usize];
    assert_eq!(m.name, "creatures\\actor");
    assert_eq!((m.friction, m.damping, m.spring), (0.8, 1.0, 0.8));
    let door = level_objects(level.simulation().unwrap())
        .into_iter()
        .find(|o| o.record.header.name.as_str() == "trader_door")
        .expect("the trader's door")
        .id;
    let leaf = level.doors().boxes()[0];
    let start = level.player().state().position;
    let k = (0..3)
        .min_by(|&a, &b| leaf.half_size[a].total_cmp(&leaf.half_size[b]))
        .unwrap();
    let mut n = leaf.axes[k];
    let across = |p: Vec3, n: Vec3| {
        let d = sub(p, leaf.centre);
        d[0] * n[0] + d[2] * n[2]
    };
    if across(start, n) < 0.0 {
        n = n.map(|v| -v);
    }
    let mut run = Run {
        level,
        input: Input::new(),
        now: 0,
    };
    // The scheme's start wakes the leaf and its motor holds it.
    run.frames(3);
    assert_eq!(run.contacts(), 0, "nothing near at the start");
    let p = run.level.player();
    let h = p.config.eye_height(p.body().box_index());
    let to = sub(leaf.centre, start);
    let angles = Angles {
        yaw: to[0].atan2(to[2]),
        pitch: 0.0,
        roll: 0.0,
    };
    run.level
        .set_camera([start[0], start[1] + h, start[2]], angles);
    run.frames(2);
    run.key(Key::W, true);
    run.level.take_physics_timers();
    let mut closest = f32::INFINITY;
    let mut most = 0.0f32;
    for _ in 0..150 {
        run.frames(1);
        closest = closest.min(across(run.feet(), n) - leaf.half_size[k]);
        let angle = run.level.physics().hinge_angle(door, "door").unwrap();
        most = most.max(angle.abs());
    }
    let timers = run.level.take_physics_timers();
    let met = run.contacts();
    let left = across(run.feet(), n) - leaf.half_size[k];
    eprintln!(
        "running 2.4 s at the leaf: {met} contacts kept, closest {closest:.3} m from the face, {left:.3} at the end; the hinge's angle at most {most:.2e} rad"
    );
    eprintln!(
        "physics while pushing: {} steps, {:.4} ms each (collider {:.4}, solver {:.4})",
        timers.steps,
        timers.physics / f64::from(timers.steps.max(1)),
        timers.collider / f64::from(timers.steps.max(1)),
        timers.solver / f64::from(timers.steps.max(1)),
    );
    // 7: the contacts met it, the player stopped in front of it (the
    // contacts' softness aside), the motor held it closed.
    assert!(met > 0);
    assert!(closest > 0.2 && left < 0.5, "{closest} … {left}");
    assert!(most < 0.05, "{most}");
    // Turned round and walking off: no more contacts.
    run.key(Key::W, false);
    let feet = run.feet();
    run.level.set_camera(
        [feet[0], feet[1] + h, feet[2]],
        Angles {
            yaw: angles.yaw + std::f32::consts::PI,
            ..angles
        },
    );
    run.key(Key::W, true);
    run.frames(40);
    let after = run.contacts();
    run.frames(40);
    assert_eq!(run.contacts(), after, "away from it");
    let away = across(run.feet(), n) - leaf.half_size[k];
    assert!(away > left + 1.0, "{away}");
}
