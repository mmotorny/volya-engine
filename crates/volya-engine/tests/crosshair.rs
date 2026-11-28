// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The crosshair's pick and record on the Russian edition's first level
//! (BYOA; docs/behavior/use-action.md 2.1, 2.2): the trader, a character,
//! picked by his bone shapes in his current pose from in front of him and
//! recorded within `take_dist`, not from farther or looking away. The
//! door's pick is the trader door's test (`new_game_scripts`). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::camera::Angles;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::ui::Resolution;

fn with_game(test: impl FnOnce(&dyn Vfs, &GameData)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    test(&tree, &data);
}

#[test]
fn the_crosshair_picks_the_trader() {
    with_game(|tree, data| {
        let mut screen = LevelScreen::load(tree, data, 0).unwrap();
        assert!(
            screen
                .play_cycle(tree, data, "escape_trader", "idle")
                .unwrap()
        );
        let mut now = 0;
        let mut frame = |screen: &mut LevelScreen| {
            now += 20;
            screen.set_global_clock(now);
            screen.update(now, &Default::default());
            now
        };
        frame(&mut screen);
        let trader = (screen.world().objects.instances.iter())
            .find(|i| i.name == "escape_trader")
            .unwrap();
        let (id, x, sphere) = (trader.id, trader.transform, trader.sphere);
        // In front of him (his model's z axis), level with his model's
        // sphere centre, facing him.
        let forward = [x[2][0], 0.0, x[2][2]];
        let at = |d: f32| {
            let eye = [
                sphere[0] + forward[0] * d,
                sphere[1],
                sphere[2] + forward[2] * d,
            ];
            let angles = Angles {
                yaw: (-forward[0]).atan2(-forward[2]),
                pitch: 0.0,
                roll: 0.0,
            };
            (eye, angles)
        };
        let (eye, angles) = at(1.0);
        screen.set_camera(eye, angles);
        let since = frame(&mut screen);
        let o = screen
            .crosshair()
            .object
            .expect("the trader under the crosshair");
        assert_eq!((o.id, o.character, o.since), (id, true, since));
        assert!(screen.usable());
        // Still under it a frame later: the same clock.
        screen.set_camera(eye, angles);
        frame(&mut screen);
        assert_eq!(screen.crosshair().object.map(|o| o.since), Some(since));
        // Looking away: nothing.
        screen.set_camera(
            eye,
            Angles {
                yaw: angles.yaw + 3.0,
                ..angles
            },
        );
        frame(&mut screen);
        assert_eq!(screen.crosshair().id(), None);
        // From 3 m: beyond take_dist (2 m).
        let (eye, angles) = at(3.0);
        screen.set_camera(eye, angles);
        frame(&mut screen);
        assert_eq!(screen.crosshair().id(), None);
    });
}
