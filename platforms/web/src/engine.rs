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
    /// The user configuration file waiting to be written, and whether a
    /// write is under way.
    static USER_CONFIG: RefCell<(Option<Vec<u8>>, bool)> = const { RefCell::new((None, false)) };
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
    let status = serde_json::to_value(crate::game::status().await?).unwrap_or_default();
    let flag = |name: &str| status.get(name).and_then(|v| v.as_bool()).unwrap_or(false);
    if !flag("imported") {
        return Err(error("game not imported"));
    }
    // The browser's own error if the files can't be opened here.
    crate::game::ensure_mounted().await?;
    // The user configuration file stored before; without one the
    // installer's copy in the game root applies (startup.md section 4).
    let user = crate::game::read_user_config().await?;
    // The saved games the menu's screens list; a folder that can't be read
    // lists none (our choice), saying why on the console.
    let saves = crate::game::saved_games().await.unwrap_or_else(|e| {
        web_sys::console::error_1(&e);
        Default::default()
    });
    let splash = crate::game::with_trees(|_, root| app::splash(root))?.map_err(error)?;
    let size = splash.as_ref().map_or((0, 0), |s| (s.width, s.height));
    if let Some(image) = &splash {
        crate::gpu::set_splash(image);
    }
    SPLASH.with(|s| *s.borrow_mut() = Some(size));
    crate::input::set_capture(true);
    // Let the frame loop show the splash before loading blocks the thread.
    crate::opfs::sleep(100).await?;
    let native = RESOLUTION.with(|r| *r.borrow());
    let started = js_sys::Date::now();
    let loaded = crate::game::with_trees(|tree, root| GameData::load(tree, Some(root), native))
        .and_then(|r| r.map_err(error))
        .and_then(|mut data| {
            if let Some(user) = user {
                data.set_user_config(user);
            }
            let mut app = App::new(data).map_err(error)?;
            // A GPU without BC gets the level's textures transcoded while
            // it loads (#113).
            app.set_prepare_textures(crate::textures::wanted());
            app.resized(native);
            app.set_saved_games(saves);
            Ok(app)
        });
    SPLASH.with(|s| *s.borrow_mut() = None);
    let app = match loaded {
        Ok(app) => app,
        Err(e) => {
            crate::input::set_capture(false);
            return Err(e);
        }
    };
    crate::input::set_render(Some(app.video_mode()));
    RUNNING.with(|r| {
        *r.borrow_mut() = Some(Running {
            app,
            voices: Vec::new(),
        })
    });
    Ok(json!({ "loading_ms": js_sys::Date::now() - started, "splash": splash.is_some() }))
}

/// Ends the game, stopping its sounds and giving the screen back. The
/// app's master gain ends with it (it is 0 while the loading image covers
/// the level, docs/behavior/loading-screen.md section 6).
pub fn stop() {
    let running = RUNNING.with(|r| r.borrow_mut().take());
    if let Some(Running { voices, .. }) = running {
        crate::audio::with_mixer_opt(|mixer| {
            if let Some(mixer) = mixer {
                for voice in voices {
                    mixer.stop(voice);
                }
                mixer.set_master_gain(1.0);
            }
        });
        crate::input::set_capture(false);
        crate::input::set_render(None);
        crate::game::close_saved_games();
    }
}

/// Stores the user configuration file. Writes go one at a time; a file
/// asked for while one is written replaces any still waiting, so the latest
/// content is written last and an older write never lands after a newer one.
fn store_user_config(bytes: Vec<u8>) {
    let start = USER_CONFIG.with(|q| {
        let mut q = q.borrow_mut();
        q.0 = Some(bytes);
        !std::mem::replace(&mut q.1, true)
    });
    if !start {
        return;
    }
    wasm_bindgen_futures::spawn_local(async {
        while let Some(bytes) = USER_CONFIG.with(|q| q.borrow_mut().0.take()) {
            if let Err(e) = crate::game::write_user_config(bytes).await {
                web_sys::console::error_1(&e);
            }
        }
        USER_CONFIG.with(|q| q.borrow_mut().1 = false);
    });
}

/// The size the game draws at while it runs (its video mode, #49), `None`
/// otherwise (the splash and the page draw at the canvas's size).
pub fn render_size() -> Option<Resolution> {
    RUNNING.with(|r| r.borrow().as_ref().map(|running| running.app.video_mode()))
}

/// The texture files of the level shown, if any.
pub fn level_texture_paths() -> Option<Vec<String>> {
    RUNNING.with(|r| r.borrow().as_ref()?.app.level_texture_paths())
}

/// The level's texture files, while its loading waits for them to be
/// prepared ([`App::textures_to_prepare`]).
pub fn textures_to_prepare() -> Option<Vec<String>> {
    RUNNING.with(|r| Some(r.borrow().as_ref()?.app.textures_to_prepare()?.to_vec()))
}

/// Whether a level's loading waits for its textures.
pub fn preparing_textures() -> bool {
    RUNNING.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.app.textures_to_prepare().is_some())
    })
}

/// The level's textures are prepared: its loading goes on.
pub fn textures_prepared() {
    RUNNING.with(|r| {
        if let Some(running) = r.borrow_mut().as_mut() {
            running.app.textures_prepared();
        }
    });
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
                crate::audio::with_sounds(|host| {
                    crate::audio::with_mixer_opt(|mixer| match mixer {
                        Some(mixer) => app.update(
                            tree,
                            time,
                            input,
                            &mut MixerOut {
                                mixer,
                                tree,
                                voices,
                                sounds: Some(&mut host.system),
                            },
                        ),
                        None => app.update(tree, time, input, &mut Silent),
                    })
                })
            })
        });
        if let Some(bytes) = app.take_save() {
            store_user_config(bytes);
        }
        for path in app.take_deleted_saves() {
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = crate::game::delete_saved_file(&path).await {
                    web_sys::console::error_1(&e);
                }
            });
        }
        crate::input::set_render(Some(app.video_mode()));
        // The game was unmounted under the app.
        updated.is_err() || app.quit_requested()
    });
    if quit {
        stop();
    }
}

thread_local! {
    /// Whether the page is hidden (#93), for work outside the frame loop.
    static PAGE_HIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the page is hidden: work outside the frame loop waits meanwhile
/// (#93).
pub fn page_hidden() -> bool {
    PAGE_HIDDEN.with(|h| h.get())
}

/// The page was hidden or shown again ([`App::set_hidden`]; our design,
/// #93): in a level the menu opens; the frame loop idles meanwhile.
pub fn set_hidden(hidden: bool) {
    PAGE_HIDDEN.with(|h| h.set(hidden));
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let Some(Running { app, voices }) = r.as_mut() else {
            return;
        };
        let _ = crate::game::with_trees(|tree, _| {
            crate::input::with_input(|input| {
                crate::audio::with_mixer_opt(|mixer| match mixer {
                    Some(mixer) => app.set_hidden(
                        hidden,
                        input,
                        &mut MixerOut {
                            mixer,
                            tree,
                            voices,
                            sounds: None,
                        },
                    ),
                    None => app.set_hidden(hidden, input, &mut Silent),
                })
            })
        });
    });
}

/// The draw list of the engine's screen at `resolution`, `None` when the
/// game does not run.
pub fn draw_list(
    renderer: &mut Renderer,
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

/// The level's controls for the page while they are live
/// ([`App::level_controls`]): the page shows its touch controls and takes
/// pointer lock only then.
pub fn level_controls() -> Option<app::LevelControls> {
    RUNNING.with(|r| r.borrow().as_ref()?.app.level_controls())
}

/// Whether the character takes keyboard keys ([`App::character_takes_keys`]).
pub fn character_takes_keys() -> bool {
    RUNNING.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.app.character_takes_keys())
    })
}

/// Whether the game shows its mouse cursor ([`App::cursor_shown`]): the
/// page's touch is a touchpad then (#108).
pub fn cursor_shown() -> bool {
    RUNNING.with(|r| r.borrow().as_ref().is_some_and(|r| r.app.cursor_shown()))
}

/// The save screen's name field while it shows ([`App::text_field`]), in
/// canvas CSS pixels, whether it has focus, and whether the cursor is over
/// it (a click now would focus it: the page raises the device's keyboard
/// in the tap, #108).
pub fn text_field() -> Option<([f32; 4], bool, bool)> {
    let (rect, focused) = RUNNING.with(|r| r.borrow().as_ref()?.app.text_field())?;
    let [x, y] = crate::input::with_input(|i| i.cursor_ui());
    let over =
        (rect.x..rect.x + rect.width).contains(&x) && (rect.y..rect.y + rect.height).contains(&y);
    Some((crate::input::css_from_ui(rect), focused, over))
}

/// The `bound_keys` op: the names of the keys bound to any action, which
/// the page's full screen locks (#85); empty while the game is not running.
pub fn bound_keys() -> Vec<&'static str> {
    RUNNING.with(|r| {
        let r = r.borrow();
        let Some(running) = r.as_ref() else {
            return Vec::new();
        };
        let console = running.app.console();
        let mut keys: Vec<&'static str> = (0..volya_engine::core::console::ACTIONS.len())
            .flat_map(|a| console.keys(a))
            .flatten()
            .filter_map(volya_engine::core::input::Key::from_dik_name)
            .map(|k| k.name())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    })
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

/// The `debug_give_item` op (tests): an item of `section` put into the
/// player's inventory ([`App::give_item`]); its id.
#[cfg(feature = "test-ops")]
pub fn give_item(section: &str) -> Result<u16, JsValue> {
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut().ok_or_else(|| error("the game is not running"))?;
        running.app.give_item(section).map_err(error)
    })
}

/// The `particles_state` op (tests): the level's placed effects and their
/// particles (docs/behavior/level-placed-effects.md).
#[cfg(feature = "test-ops")]
pub fn particles_state() -> Result<serde_json::Value, JsValue> {
    RUNNING.with(|r| {
        let r = r.borrow();
        let running = r.as_ref().ok_or_else(|| error("the game is not running"))?;
        let level = running.app.level().ok_or_else(|| error("no level"))?;
        serde_json::to_value(level.effects_report()).map_err(error)
    })
}

/// The `debug_particles_bench` op (tests): a copy of the level's placed
/// effects runs `frames` frames of 16 ms with the camera at the first
/// record; the CPU time of a frame's update by `performance.now()`
/// (microseconds: mean and max), and the particles at the end.
#[cfg(feature = "test-ops")]
pub fn particles_bench(frames: u32) -> Result<serde_json::Value, JsValue> {
    RUNNING.with(|r| {
        let r = r.borrow();
        let running = r.as_ref().ok_or_else(|| error("the game is not running"))?;
        let level = running.app.level().ok_or_else(|| error("no level"))?;
        let mut effects = level
            .effects()
            .ok_or_else(|| error("no placed effects"))?
            .clone();
        let camera = effects.effects()[0].instance.sphere().centre;
        // The timer's resolution (coarsened without cross-origin
        // isolation) is far above a frame's update: the mean is the whole
        // run's time over its frames, the max a single frame's.
        let mut max = 0.0f64;
        let begin = crate::now();
        for _ in 0..frames {
            let start = crate::now();
            effects.frame(16, camera);
            max = max.max(crate::now() - start);
        }
        let total = crate::now() - begin;
        Ok(serde_json::json!({
            "frames": frames,
            "mean_us": total * 1000.0 / f64::from(frames.max(1)),
            "max_us": max * 1000.0,
            "particles": effects.particle_count(),
        }))
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

/// The `ui_images` op (#105): pictures of the game's UI textures for the
/// page's own controls, each `{path}` (a texture path) or `{id}` (a
/// texture description id), with an optional `region` `[x, y, width,
/// height]` and `silhouette` (the HUD icon transform of
/// [`volya_engine::ui_image::silhouette`]). Answers `{images, cross_color,
/// ms}`: per request `{width, height, rgba, mean}` (RGBA as base64, `mean`
/// the alpha-weighted mean colour) or `{error}`; the crosshair's colour
/// (`[hud_cursor] cross_color`, 0 to 1, or null); the decoding time.
pub fn ui_images(args: &serde_json::Value) -> Result<serde_json::Value, JsValue> {
    use volya_engine::ui_image::{self, Source};
    let started = crate::now();
    let requests = args
        .get("images")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    RUNNING.with(|r| {
        let r = r.borrow();
        let running = r.as_ref().ok_or_else(|| error("the game is not running"))?;
        let data = running.app.data();
        let picture = |tree: &dyn volya_engine::core::vfs::Vfs, request: &serde_json::Value| {
            let text = |key| request.get(key).and_then(|v| v.as_str());
            let source = match (text("path"), text("id")) {
                (Some(path), _) => Source::Path(path.into()),
                (None, Some(id)) => Source::Id(id.into()),
                _ => return json!({ "error": "neither `path` nor `id`" }),
            };
            let region = match request.get("region") {
                None | Some(serde_json::Value::Null) => None,
                Some(v) => match serde_json::from_value::<[u32; 4]>(v.clone()) {
                    Ok(region) => Some(region),
                    Err(_) => {
                        return json!({ "error": format!("`region` must be [x, y, width, height]: {v}") });
                    }
                },
            };
            let image =
                match ui_image::load(tree, &data.aliases, &data.descriptions, &source, region) {
                    Ok(image) => image,
                    Err(e) => return json!({ "error": e.to_string() }),
                };
            let (image, mean) = if request.get("silhouette").and_then(|v| v.as_bool()) == Some(true)
            {
                ui_image::silhouette(&image)
            } else {
                let mean = ui_image::mean_color(&image);
                (image, mean)
            };
            json!({
                "width": image.width,
                "height": image.height,
                "rgba": crate::game::base64(&image.pixels),
                "mean": mean,
            })
        };
        let images = crate::game::with_trees(|tree, _| {
            requests
                .iter()
                .map(|request| picture(tree, request))
                .collect::<Vec<_>>()
        })?;
        Ok(json!({
            "images": images,
            "cross_color": ui_image::cross_color(&data.system),
            "ms": crate::now() - started,
        }))
    })
}
