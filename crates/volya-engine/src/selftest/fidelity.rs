// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The `fidelity` suite's views (#521, `tests/fidelity/views.rs`), drawn as
//! the other render checks draw: each a check named after its view, keeping
//! its frame under the view's name, which the suite compares with the
//! original's (`//tests/fidelity:frames`). Each starts from the user
//! configuration file the original's run of the view read
//! (`<view>.user.ltx`, which the suite gives), applies its settings as the app
//! does, and keeps the file our console then writes, as the original
//! rewrites its file when it quits (options-screens.md 12.3), as
//! `<view>.saved.ltx`: the suite compares the two before any pixel (#561).

use super::title_menu::{key, step};
use super::{Context, Outcome, Problem, Run, capture, ensure};
use crate::app::{self, App, AppState};
use crate::core::camera::{self, Angles};
use crate::core::console::Console;
use crate::core::display::DisplayColour;
use crate::core::input::{Input, Key};
use crate::core::texture_quality::{SamplerStates, TextureQuality};
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
use list::{
    Camera, FOV_DEGREES, FRAME_MS, HEIGHT, PLAY_GLOBAL_MS, Place, SKIP_EVERY, TITLE_MENU_FRAME,
    VIEWS, WIDTH,
};

/// Frames the startup sequence may take before the title menu (the
/// capture's `MENU_TIMEOUT`).
const MENU_TIMEOUT: u32 = 5000;

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
    let Some(user) = cx.user_config() else {
        return Err(Problem::Skip(format!(
            "no {}.user.ltx of the original's run (//tests/fidelity:frames)",
            view.name
        )));
    };
    let user = user.to_vec();
    match view.place {
        Place::TitleMenu => {
            let mut data = GameData::load(cx.tree, cx.root, SIZE).map_err(game_data)?;
            data.set_user_config(user);
            title_menu(cx, data, view.name).await
        }
        Place::NewGame { camera, .. } => {
            let at = match camera {
                Camera::Start => None,
                Camera::Track { eye, yaw, pitch } => {
                    let roll = 0.0;
                    Some((eye, Angles { yaw, pitch, roll }))
                }
            };
            let mut data = GameData::read(cx.tree, cx.root, SIZE).map_err(game_data)?;
            data.set_user_config(user);
            new_game(cx, &data, view.name, at, view.place.since_first_ms()).await
        }
    }
}

/// The title menu as the original's capture reaches it, in its frames of
/// [`FRAME_MS`]: the quit key every [`SKIP_EVERY`] frames while the startup
/// sequence plays, then the menu's frame [`TITLE_MENU_FRAME`] counted from
/// its first, so the videos and the bar are at the capture's moment.
async fn title_menu(cx: &Context<'_>, data: GameData, name: &str) -> Outcome {
    let tree = cx.tree;
    let mut app = App::new(data).map_err(|e| format!("app: {e}"))?;
    let mut input = Input::new();
    let in_menu = |app: &App| matches!(app.state(), AppState::Menu { .. });
    let frame_ms = f64::from(FRAME_MS);
    let mut frame = 0;
    while !in_menu(&app) {
        ensure!(frame < MENU_TIMEOUT, "no title menu within {frame} frames");
        if frame % SKIP_EVERY == 0 {
            key(&mut input, Key::Escape);
        }
        frame += 1;
        step(&mut app, &mut input, f64::from(frame) * frame_ms);
    }
    // Every menu frame drawn, as the app draws them: the videos decode a
    // few frames per draw toward the frame due (`Presenter`), so a single
    // draw at the end would show their first frames.
    let mut presenter = Presenter::new();
    let mut gpu = cx.renderer();
    let first = app.clock();
    let mut f = frame;
    let list = loop {
        let s = app
            .scene(tree, SIZE)
            .map_err(|e| format!("menu scene: {e}"))?;
        let list = presenter.draw_list(&mut gpu, tree, &s, app.clock());
        if f + 1 == frame + TITLE_MENU_FRAME {
            break list;
        }
        f += 1;
        step(&mut app, &mut input, f64::from(f) * frame_ms);
        ensure!(in_menu(&app), "not in the menu");
    };
    let age = app.clock() - first;
    ensure!(
        age == Place::TitleMenu.since_first_ms(),
        "the menu is {age} ms old, the capture's {} ms",
        Place::TitleMenu.since_first_ms()
    );
    let pixels = capture(&mut gpu, &list, WIDTH, HEIGHT).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump(name, WIDTH, HEIGHT, &pixels);
    cx.keep_file(&format!("{name}.saved.ltx"), app.console().save());
    Ok(format!("{} quads at {} ms", list.quads.len(), app.clock()))
}

/// A New Game's level after `end` ms of updates (the view's frame of play,
/// frame 1 at 0), seen by the player where the game put it (`at` `None`), or from an eye and angles the camera is
/// placed at before each update (the player would fall from a view in the
/// air); with the settings of `data`'s user configuration file as the app
/// applies them: the device made with them, reset as the level starts
/// (texture-quality-filtering.md 8.1), the level's and the frame's settings
/// before each update and the frame.
async fn new_game(
    cx: &Context<'_>,
    data: &GameData,
    name: &str,
    at: Option<([f32; 3], Angles)>,
    end: u64,
) -> Outcome {
    ensure!(
        (camera::FOV_Y_DEGREES - FOV_DEGREES).abs() < 1e-4,
        "our vertical field of view is {}°, the view's {FOV_DEGREES}°",
        camera::FOV_Y_DEGREES
    );
    let tree = cx.tree;
    let console: Console = data.console();
    let supersample = app::supersample(&console);
    let mut samplers = SamplerStates::at_creation(&console);
    samplers.reset(&console);
    let mut screen = LevelScreen::load(tree, data, 0).map_err(|e| format!("level: {e}"))?;
    let input = Input::new();
    let fine = end.saturating_sub(1000);
    let mut now = 0;
    while now < end {
        now = (now + if now < fine { COARSE_MS } else { FINE_MS }).min(end);
        samplers.follow(&console);
        let wind = screen.wind();
        app::level_settings(&mut screen, &console, (SIZE, supersample), wind);
        if let Some((eye, angles)) = at {
            screen.set_camera(eye, angles);
        }
        // The global time the capture's frame had (the trees' sway).
        screen.set_global_clock(PLAY_GLOBAL_MS + now);
        screen.update(now, &input);
        // The last second composed frame by frame, as the app composes
        // every frame: the detail pass updates and selects the detail
        // objects only when it ran in the previous frame too
        // (detail-objects.md 5.1), so a single frame at the end would draw
        // none (#557).
        if now >= fine && now < end {
            if let Some((eye, angles)) = at {
                screen.set_camera(eye, angles);
            }
            screen.scene(SIZE);
        }
    }
    if let Some((eye, angles)) = at {
        screen.set_camera(eye, angles);
    }
    let mut scene = screen.scene(SIZE);
    let quality = TextureQuality::new(
        console.number("texture_lod").max(0.0) as u32,
        TextureQuality::read_list(&data.system),
    );
    app::frame_settings(
        &mut scene,
        &console,
        supersample,
        &samplers,
        quality,
        FINE_MS,
    );
    scene.ramp = DisplayColour::of(&console).ramp();
    let mut presenter = Presenter::new();
    let mut gpu = cx.renderer();
    let list = presenter.draw_list(&mut gpu, tree, &scene, now);
    let pixels = capture(&mut gpu, &list, WIDTH, HEIGHT).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    ensure!(screen.take_fatal().is_none(), "fatal");
    cx.dump(name, WIDTH, HEIGHT, &pixels);
    cx.keep_file(&format!("{name}.saved.ltx"), console.save());
    Ok(format!("at {end} ms"))
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
