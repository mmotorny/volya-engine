// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The trader's first talk at a New Game on the Russian edition's data
//! (BYOA), against docs/behavior/talk.md and docs/behavior/use-action.md
//! 6.2: the use action within 2 m of the trader, aimed at him, starts a
//! talk (2.2); its set-up opens with his start dialog (3.1, 3.2, 8); the
//! view turns toward him (4); more than 3 m apart the window closes, and
//! hiding it ends the player character's side, then his (5.1, 5.2). Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::io;

use volya_engine::core::camera::Angles;
use volya_engine::core::collision::geometry::Vec3;
use volya_engine::core::dialogs::conditions::DialogFunctions;
use volya_engine::core::info_portions::Event;
use volya_engine::core::input::{Event as InputEvent, Input, Key};
use volya_engine::core::talk::{Talks, scheduled_update};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

/// The scripts' dialog functions, not run here: every precondition holds
/// (the continuations of the trader's phrase 0 call some), every action
/// does nothing.
struct NoScripts;

impl DialogFunctions for NoScripts {
    fn call_precondition(&mut self, _: &str, _: u16, _: u16, _: [&str; 3]) -> io::Result<bool> {
        Ok(true)
    }

    fn call_action(&mut self, _: &str, _: u16, _: u16, _: &str, _: &str) -> io::Result<()> {
        Ok(())
    }

    fn portion_events(&mut self, _: Vec<Event>) -> io::Result<()> {
        Ok(())
    }
}

fn frames(level: &mut LevelScreen, input: &mut Input, now: &mut u64, n: u32) {
    for _ in 0..n {
        *now += 16;
        level.set_global_clock(*now);
        level.update(*now, input);
        input.end_frame();
    }
}

#[test]
fn using_the_trader_starts_a_talk() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    let (mut input, mut now) = (Input::new(), 0);
    frames(&mut level, &mut input, &mut now, 2);
    let start = level.player().state().position;
    let world = std::sync::Arc::clone(level.world());
    let trader = (world.objects.instances.iter())
        .find(|i| i.name == "escape_trader")
        .expect("the trader drawn");
    let (sphere, trader) = (trader.sphere, trader.id);
    eprintln!(
        "the player at {start:?}, yaw {}; the trader's sphere {sphere:?}; picked {:?}",
        level.player().angles().yaw,
        level.state().talk.picked
    );

    // Stand 1.5 m from the sphere's centre, on the start's side, facing it.
    let c: Vec3 = [sphere[0], sphere[1], sphere[2]];
    let (dx, dz) = (start[0] - c[0], start[2] - c[2]);
    let k = 1.5 / (dx * dx + dz * dz).sqrt();
    let eye_h = level.player().config.eye_height(0);
    let feet = [c[0] + dx * k, start[1], c[2] + dz * k];
    let eye = [feet[0], feet[1] + eye_h, feet[2]];
    let to = [c[0] - eye[0], c[1] - eye[1], c[2] - eye[2]];
    let angles = Angles {
        yaw: to[0].atan2(to[2]),
        pitch: -(to[1] / (to[0] * to[0] + to[2] * to[2]).sqrt()).atan(),
        roll: 0.0,
    };
    level.set_camera(eye, angles);
    frames(&mut level, &mut input, &mut now, 2);
    assert_eq!(
        level.state().talk.picked,
        Some(trader),
        "under the crosshair"
    );

    // use-action.md 6.2: F, its default key.
    for pressed in [true, false] {
        input.handle(InputEvent::Key {
            key: Key::F,
            pressed,
        });
    }
    frames(&mut level, &mut input, &mut now, 1);
    assert_eq!(level.take_used(), Some(trader), "used through the scripts");
    // Script API R's offer (4.2) and CH-7's start dialog, here on talk
    // sides of our own.
    let player = level.character().unwrap().owner.id;
    let mut sides = Talks::default();
    assert!(sides.offer(trader, 1.0, player, 1.0));
    sides.come_online(trader, "escape_trader_start_dialog");
    level.talk_sync(&sides);
    assert_eq!(level.state().talk.talking, [player, trader]);

    // 3, 8: his start dialog opens the talk; phrase 0 has no text.
    let start = (level.talk_set_up(&tree, &data, trader, &sides, &mut NoScripts))
        .unwrap()
        .unwrap();
    assert!(!start.special_mode);
    assert!(
        start.said.iter().all(|s| s.text.is_empty()),
        "{:?}",
        start.said
    );
    let t = level.state().talk;
    assert_eq!(
        (t.dialog.as_deref(), t.first_speaker),
        (Some("escape_trader_start_dialog"), Some(trader))
    );
    eprintln!("partner {:?}", start.partner);
    assert!(!start.partner.name.is_empty());
    assert!(
        !level.talk_window_closes(&sides, trader, false),
        "within 3 m"
    );

    // 4: turned away, the view comes back to within π/6 at once.
    let yaw = level.player().angles().yaw;
    level.set_camera(
        eye,
        Angles {
            yaw: yaw + 2.0,
            ..angles
        },
    );
    level.talk_turn_camera(trader, 0.016);
    let back = level.player().angles().yaw;
    let off = (back - yaw).rem_euclid(std::f32::consts::TAU);
    let off = off.min(std::f32::consts::TAU - off);
    assert!(
        off <= std::f32::consts::FRAC_PI_6 + 0.2,
        "{off} rad from facing him"
    );

    // 5.2 rule 3: more than 3 m apart the window closes.
    let far = [c[0] + dx * 4.0 * k, start_y(&level), c[2] + dz * 4.0 * k];
    level.set_camera([far[0], far[1] + eye_h, far[2]], angles);
    assert!(level.talk_window_closes(&sides, trader, false), "6 m apart");
    assert!(
        !level.talk_window_closes(&sides, trader, true),
        "never with the special mode"
    );
    level.talk_hidden(&mut sides);
    level.talk_sync(&sides);
    assert_eq!(level.state().talk.talking, [trader], "his side goes on");
    scheduled_update(&mut sides, trader, 1.0);
    level.talk_sync(&sides);
    assert!(level.state().talk.talking.is_empty(), "until his update");
}

fn start_y(level: &LevelScreen) -> f32 {
    level.player().state().position[1]
}
