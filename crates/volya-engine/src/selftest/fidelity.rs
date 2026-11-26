// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The `fidelity` suite's views (#521, `tests/fidelity/views.rs`), drawn as
//! the other render checks draw: each a check named after its view, keeping
//! its frame under the view's name, which the suite compares with the
//! original's (Bazel's capture actions (`//tests/fidelity:frames`)). The game reads its settings
//! as the app does, from the root folder's `user.ltx` where there is one,
//! as the original reads the shipped `user.ltx`.

use super::title_menu::{key, step};
use super::{Context, Outcome, Run, capture, ensure};
use crate::app::{App, AppState};
use crate::core::camera::{self, Angles};
use crate::core::input::{Input, Key};
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use volya_engine_formats::ui::Resolution;

#[allow(dead_code)]
mod list {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fidelity/views.rs"
    ));
}
use list::{Camera, FOV_DEGREES, HEIGHT, Place, VIEWS, WIDTH};

/// One check per view, in the list's order.
pub const CHECKS: &[(&str, Run)] = &[
    (VIEWS[0].name, |c| Box::pin(view(c, 0))),
    (VIEWS[1].name, |c| Box::pin(view(c, 1))),
    (VIEWS[2].name, |c| Box::pin(view(c, 2))),
    (VIEWS[3].name, |c| Box::pin(view(c, 3))),
    (VIEWS[4].name, |c| Box::pin(view(c, 4))),
];

const SIZE: Resolution = Resolution::new(WIDTH, HEIGHT);

/// Milliseconds between the level's updates up to the view's moment: the
/// longest step the level takes whole (`MAX_STEP`, 0.1 s); the last second
/// at 16 ms, as frames come.
const COARSE_MS: u64 = 100;
const FINE_MS: u64 = 16;

async fn view(cx: &Context<'_>, index: usize) -> Outcome {
    let view = VIEWS[index];
    let game_data = |e| format!("game data: {e}");
    match view.place {
        Place::TitleMenu => {
            let data = GameData::load(cx.tree, cx.root, SIZE).map_err(game_data)?;
            title_menu(cx, data, view.name).await
        }
        Place::NewGame { camera, seconds } => {
            let at = match camera {
                Camera::Start => None,
                Camera::Track { eye, yaw, pitch } => {
                    let roll = 0.0;
                    Some((eye, Angles { yaw, pitch, roll }))
                }
            };
            let data = GameData::read(cx.tree, cx.root, SIZE).map_err(game_data)?;
            new_game(cx, &data, view.name, at, seconds).await
        }
    }
}

/// The title menu once its startup sequence has ended, no item moved to
/// (as `title_menu`'s check reaches it, Esc through the items).
async fn title_menu(cx: &Context<'_>, data: GameData, name: &str) -> Outcome {
    let tree = cx.tree;
    let mut app = App::new(data).map_err(|e| format!("app: {e}"))?;
    let mut input = Input::new();
    step(&mut app, &mut input, 1000.0);
    step(&mut app, &mut input, 3500.0);
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 3600.0);
    key(&mut input, Key::Enter);
    step(&mut app, &mut input, 3700.0);
    key(&mut input, Key::Escape);
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 4000.0);
    step(&mut app, &mut input, 6000.0);
    key(&mut input, Key::Escape);
    for t in [7000.0, 12_000.0, 12_600.0, 13_000.0, 13_300.0, 13_700.0] {
        step(&mut app, &mut input, t);
    }
    ensure!(
        matches!(app.state(), AppState::Menu { .. }),
        "not in the menu"
    );
    let s = app
        .scene(tree, SIZE)
        .map_err(|e| format!("menu scene: {e}"))?;
    let mut presenter = Presenter::new();
    let mut gpu = cx.renderer();
    let list = presenter.draw_list(&mut gpu, tree, &s, app.clock());
    let pixels = capture(&mut gpu, &list, WIDTH, HEIGHT).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump(name, WIDTH, HEIGHT, &pixels);
    Ok(format!("{} quads at {} ms", list.quads.len(), app.clock()))
}

/// A New Game's level after `seconds` of updates, seen by the player where
/// the game put it (`at` `None`), or from an eye and angles the camera is
/// placed at before each update (the player would fall from a view in the
/// air).
async fn new_game(
    cx: &Context<'_>,
    data: &GameData,
    name: &str,
    at: Option<([f32; 3], Angles)>,
    seconds: u32,
) -> Outcome {
    ensure!(
        (camera::FOV_Y_DEGREES - FOV_DEGREES).abs() < 1e-4,
        "our vertical field of view is {}°, the view's {FOV_DEGREES}°",
        camera::FOV_Y_DEGREES
    );
    let tree = cx.tree;
    let mut screen = LevelScreen::load(tree, data, 0).map_err(|e| format!("level: {e}"))?;
    let input = Input::new();
    let end = u64::from(seconds) * 1000;
    let fine = end.saturating_sub(1000);
    let mut now = 0;
    while now < end {
        now = (now + if now < fine { COARSE_MS } else { FINE_MS }).min(end);
        if let Some((eye, angles)) = at {
            screen.set_camera(eye, angles);
        }
        screen.update(now, &input);
    }
    if let Some((eye, angles)) = at {
        screen.set_camera(eye, angles);
    }
    let scene = screen.scene(SIZE, now);
    let mut presenter = Presenter::new();
    let mut gpu = cx.renderer();
    let list = presenter.draw_list(&mut gpu, tree, &scene, now);
    let pixels = capture(&mut gpu, &list, WIDTH, HEIGHT).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    ensure!(screen.take_fatal().is_none(), "fatal");
    cx.dump(name, WIDTH, HEIGHT, &pixels);
    Ok(format!("at {seconds} s"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_per_view() {
        assert_eq!(CHECKS.len(), VIEWS.len());
        for ((name, _), view) in CHECKS.iter().zip(VIEWS) {
            assert_eq!(*name, view.name);
        }
    }
}
