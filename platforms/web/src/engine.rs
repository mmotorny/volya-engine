// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game on the engine's screen: launch (splash, loading), then
//! [`volya_engine::app::App`] on the frame loop with the page's input and the
//! mixer, until the player quits.
//!
//! While the game runs the engine owns the screen: input capture is on, so
//! the page hides its own interface and forwards input. Quitting (the Q key
//! or the menu's quit item) ends the game and gives the screen back to the
//! page's import/browser interface.

use std::cell::RefCell;

use serde_json::json;
use volya_engine::app::{self, App};
use volya_engine::audio::{MixerOut, Silent};
use volya_engine::core::audio::VoiceId;
use volya_engine::data::GameData;
use volya_engine::formats::ui::Resolution;
use volya_engine::present::Presenter;
use volya_engine::render::{DrawList, Renderer};
use volya_engine::scene::Scene;
use wasm_bindgen::JsValue;

struct Running {
    app: App,
    /// Voices the app started, for the mixer adapter.
    voices: Vec<VoiceId>,
}

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
    /// The splash picture's size while it shows (loading).
    static SPLASH: RefCell<Option<(u32, u32)>> = const { RefCell::new(None) };
    /// The output size in pixels, as the canvas was last sized.
    static RESOLUTION: RefCell<Resolution> = const { RefCell::new(Resolution::new(1024, 768)) };
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

pub fn active() -> bool {
    RUNNING.with(|r| r.borrow().is_some()) || SPLASH.with(|s| s.borrow().is_some())
}

/// Launches the game: the splash shows while the data loads (startup.md
/// sections 3–5), then the logo sequence starts on the next frame. Fails,
/// giving the screen back, when the game is not imported or its data is
/// incomplete.
pub async fn launch() -> Result<serde_json::Value, JsValue> {
    if active() {
        return Err(error("the game is already running"));
    }
    let status = crate::game::status().await?;
    if !serde_json::to_value(&status)
        .ok()
        .and_then(|s| s.get("imported")?.as_bool())
        .unwrap_or(false)
    {
        return Err(error("game not imported"));
    }
    let splash = crate::game::with_trees(|_, root| app::splash(root))?.map_err(error)?;
    let size = splash.as_ref().map_or((0, 0), |s| (s.width, s.height));
    if let Some(image) = &splash {
        crate::gpu::set_splash(image);
    }
    SPLASH.with(|s| *s.borrow_mut() = Some(size));
    crate::input::set_capture(true);
    // Let the frame loop show the splash before loading blocks the thread.
    crate::opfs::sleep(100).await?;
    let resolution = RESOLUTION.with(|r| *r.borrow());
    let started = js_sys::Date::now();
    let loaded = crate::game::with_trees(|tree, root| GameData::load(tree, Some(root), resolution))
        .and_then(|r| r.map_err(error))
        .and_then(|data| App::new(data).map_err(error));
    SPLASH.with(|s| *s.borrow_mut() = None);
    let app = match loaded {
        Ok(app) => app,
        Err(e) => {
            crate::input::set_capture(false);
            return Err(e);
        }
    };
    RUNNING.with(|r| {
        *r.borrow_mut() = Some(Running {
            app,
            voices: Vec::new(),
        })
    });
    Ok(json!({ "loading_ms": js_sys::Date::now() - started, "splash": splash.is_some() }))
}

/// Ends the game, stopping its sounds and giving the screen back.
pub fn stop() {
    let running = RUNNING.with(|r| r.borrow_mut().take());
    if let Some(running) = running {
        crate::audio::with_mixer_opt(|mixer| {
            if let Some(mixer) = mixer {
                for voice in running.voices {
                    mixer.stop(voice);
                }
            }
        });
        crate::input::set_capture(false);
    }
}

/// The output was resized to `resolution` pixels.
pub fn resized(resolution: Resolution) {
    RESOLUTION.with(|r| *r.borrow_mut() = resolution);
    RUNNING.with(|r| {
        if let Some(running) = r.borrow_mut().as_mut() {
            running.app.resized(resolution);
        }
    });
}

/// One frame at page time `time` (ms): the app takes this frame's input and
/// plays its sounds; quitting ends the game.
pub fn frame(time: f64) {
    let quit = RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let Some(running) = r.as_mut() else {
            return false;
        };
        let Running { app, voices } = running;
        let updated = crate::game::with_trees(|tree, _| {
            crate::input::with_input(|input| {
                crate::audio::with_mixer_opt(|mixer| match mixer {
                    Some(mixer) => app.update(
                        time,
                        input,
                        &mut MixerOut {
                            mixer,
                            tree,
                            voices,
                        },
                    ),
                    None => app.update(time, input, &mut Silent),
                })
            })
        });
        // The game was unmounted under the app.
        updated.is_err() || app.quit_requested()
    });
    if quit {
        stop();
    }
}

/// The draw list of the engine's screen at `resolution`, `None` when the
/// game does not run.
pub fn draw_list(
    renderer: &Renderer,
    presenter: &mut Presenter,
    resolution: Resolution,
) -> Option<DrawList> {
    if let Some(size) = SPLASH.with(|s| *s.borrow()) {
        let scene = app::splash_scene(resolution, size);
        let empty = volya_engine::core::vfs::db::DbVfs::<std::io::Empty>::new();
        return Some(presenter.draw_list(renderer, &empty, &scene, 0));
    }
    let list = RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut()?;
        let list = crate::game::with_trees(|tree, _| {
            let scene: Scene = running.app.scene(tree, resolution).map_err(error)?;
            Ok::<_, JsValue>(presenter.draw_list(renderer, tree, &scene, running.app.clock()))
        });
        Some(list.and_then(|l| l))
    })?;
    for e in presenter.take_errors() {
        web_sys::console::error_1(&format!("drawing: {e}").into());
    }
    match list {
        Ok(list) => Some(list),
        Err(e) => {
            web_sys::console::error_1(&e);
            Some(DrawList::default())
        }
    }
}

/// The `menu_state` op: the app's state, or `{"phase": "none"}` (or
/// `"loading"` during the splash).
pub fn state() -> serde_json::Value {
    if SPLASH.with(|s| s.borrow().is_some()) {
        return json!({ "phase": "loading" });
    }
    RUNNING.with(|r| match r.borrow().as_ref() {
        Some(running) => serde_json::to_value(running.app.state()).unwrap_or_default(),
        None => json!({ "phase": "none" }),
    })
}

/// The `menu_scene` op: the sprites of the current frame at `resolution`.
pub fn scene(resolution: Resolution) -> Result<serde_json::Value, JsValue> {
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut().ok_or_else(|| error("the game is not running"))?;
        let scene = crate::game::with_trees(|tree, _| running.app.scene(tree, resolution))?
            .map_err(error)?;
        serde_json::to_value(scene).map_err(error)
    })
}
