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
use std::sync::Arc;

use serde_json::json;
use volya_engine::app::{self, App};
use volya_engine::audio::{MixerOut, Silent};
use volya_engine::core::audio::VoiceId;
use volya_engine::data::GameData;
use volya_engine::formats::ui::Resolution;
use volya_engine::level::LevelWorld;
use volya_engine::present::Presenter;
use volya_engine::render::{DrawList, Renderer};
use volya_engine::scene::Scene;
use wasm_bindgen::JsValue;

struct Running {
    app: App,
    /// Voices the app started, for the mixer adapter.
    voices: Vec<VoiceId>,
    /// The console's generations (all, untyped) the `settings` event last
    /// looked at, and the values then.
    settings: (u64, u64, Vec<String>),
}

/// The console's settings the diagnostic log's `settings` event records:
/// those that change the picture or a frame's cost
/// (docs/architecture/diagnostics.md).
const SETTINGS: &[&str] = &[
    "renderer",
    "vid_mode",
    "r__supersample",
    "_preset",
    "r__detail_density",
    "r__geometry_lod",
    "texture_lod",
    "r__tf_aniso",
    "r1_tf_mipbias",
    "r1_dlights",
    "r1_dlights_clip",
    "r1_glows_per_frame",
    "r2_sun_lumscale",
    "r1_pps_u",
    "r1_pps_v",
    "rs_c_gamma",
    "rs_c_contrast",
    "rs_c_brightness",
    "rs_vis_distance",
    "r1_ssa_lod_a",
    "r1_ssa_lod_b",
    "r1_lmodel_lerp",
    "rs_stats",
];

/// The [`SETTINGS`] as the console holds them, empty where a name holds no
/// value (its built-in default applies).
fn settings(app: &App) -> Vec<String> {
    let console = app.console();
    SETTINGS
        .iter()
        .map(|name| console.status(name).unwrap_or_default())
        .collect()
}

/// Writes the `settings` event: `values` (of [`SETTINGS`]).
fn log_settings(values: &[String]) {
    use volya_engine::core::diagnostics::Value;
    let args: Vec<(String, Value)> = SETTINGS
        .iter()
        .zip(values)
        .map(|(name, v)| ((*name).to_owned(), Value::Text(v.clone())))
        .collect();
    crate::diagnostics::instant("settings", 1, &args);
}

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
    /// The splash picture's size while it shows (loading).
    static SPLASH: RefCell<Option<(u32, u32)>> = const { RefCell::new(None) };
    /// The user configuration file waiting to be written, and whether a
    /// write is under way.
    static USER_CONFIG: Queue = const { RefCell::new((None, false)) };
    /// The game's log file the same way.
    static LOG_FILE: Queue = const { RefCell::new((None, false)) };
    /// Why writes of the game's log file failed, for the log to say
    /// (console-log.md 3.9) at the next frame.
    static LOG_FILE_FAILED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The output size in pixels, as the canvas was last sized.
    static RESOLUTION: RefCell<Resolution> = const { RefCell::new(Resolution::new(1024, 768)) };
    /// Whether the game's resolution follows the canvas's size (the
    /// ergonomic mobile mode, #806).
    static FOLLOW_SCREEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The game's resolution follows the canvas's own size, `on`, or not (the
/// page's ergonomic mobile mode, #806): each change of size applied as a
/// change of `vid_mode` is (display-mode.md A.9). Turned off, the video
/// mode it had before is applied again; that mode is the one stored
/// meanwhile (#819), so a game started later has it too.
pub fn set_follow_screen(on: bool) {
    if FOLLOW_SCREEN.replace(on) == on {
        return;
    }
    let native = RESOLUTION.with(|r| *r.borrow());
    RUNNING.with(|r| {
        if let Some(running) = r.borrow_mut().as_mut() {
            if on {
                running.app.follow_screen(native);
            } else {
                running.app.stop_following_screen();
            }
        }
    });
}

/// `app` at the screen's size `native` while the resolution follows it.
fn follow(app: &mut App, native: Resolution) {
    if FOLLOW_SCREEN.get() {
        app.follow_screen(native);
    }
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
        return Err(error("game files not imported"));
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
    // The renderer released at the last game's exit, made again.
    crate::gpu::restore();
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
            // Program start's startup of the game's scripts
            // (script-api-engine-entry-points.md 3.1).
            app.start_scripts(std::rc::Rc::new(crate::game::GameTrees))
                .map_err(error)?;
            // Its normal exit releases what the renderer holds
            // (docs/behavior/console-diagnostics.md 2.5).
            app.set_device_release(Box::new(crate::gpu::release));
            // The level's textures (transcoded on a GPU without BC) and
            // geometry are uploaded while it loads, a piece at a time
            // (#113, #184, #355).
            app.set_prepare_textures(crate::textures::wanted());
            // The objects' own geometry is kept by the renderer as it is
            // made, a chunk at a time (#788).
            app.set_geometry_keeper(Some(std::sync::Arc::new(|c| {
                crate::gpu::with_renderer(|r| {
                    r.keep_geometry(c.set, c.index, &c.vertices, &c.indices)
                })
                .is_none()
                .then_some(c)
            })));
            app.resized(native);
            follow(&mut app, native);
            app.set_saved_games(saves);
            // The page's full screen as it last said (`rs_fullscreen`).
            if let Some((on, supported)) = PAGE_FULL_SCREEN.get() {
                app.set_full_screen(on, supported);
            }
            Ok(app)
        });
    SPLASH.with(|s| *s.borrow_mut() = None);
    let app = match loaded {
        Ok(app) => app,
        Err(e) => {
            crate::input::set_capture(false);
            let message = e.as_string().unwrap_or_default();
            volya_engine::core::diagnostics::end("load_failed", &message);
            crate::diagnostics::store_if_ended();
            return Err(e);
        }
    };
    tracing::info!(loading_ms = js_sys::Date::now() - started, "game started");
    let values = settings(&app);
    log_settings(&values);
    crate::input::set_render(Some(app.video_mode()));
    RUNNING.with(|r| {
        *r.borrow_mut() = Some(Running {
            settings: (
                app.console().generation(),
                app.console().untyped_generation(),
                values,
            ),
            app,
            voices: Vec::new(),
        })
    });
    Ok(json!({ "loading_ms": js_sys::Date::now() - started, "splash": splash.is_some() }))
}

/// Ends the game, stopping its sounds and giving the screen back; the
/// diagnostic log records why (`reason`, `message`). The app's master gain
/// ends with it (it is 0 while the loading image covers the level,
/// docs/behavior/loading-screen.md section 6).
pub fn stop(reason: &str, message: &str) {
    let running = RUNNING.with(|r| r.borrow_mut().take());
    if let Some(Running {
        mut app, voices, ..
    }) = running
    {
        // The sound system outlives the app: the level's sounds leave it.
        crate::audio::with_sounds(|host| app.end_sounds(&mut host.system));
        if let Some(bytes) = app.take_log_file() {
            store_log_file(bytes);
        }
        drop(app);
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
        volya_engine::core::diagnostics::end(reason, message);
        crate::diagnostics::store_if_ended();
    }
}

/// A file's writes, one at a time: the content waiting to be written, and
/// whether a write is under way.
type Queue = RefCell<(Option<Vec<u8>>, bool)>;

/// Stores `bytes` through `queue` with `write`. Writes go one at a time; a
/// file asked for while one is written replaces any still waiting, so the
/// latest content is written last and an older write never lands after a
/// newer one.
fn store_latest(
    queue: &'static std::thread::LocalKey<Queue>,
    bytes: Vec<u8>,
    write: fn(Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
) {
    let start = queue.with(|q| {
        let mut q = q.borrow_mut();
        q.0 = Some(bytes);
        !std::mem::replace(&mut q.1, true)
    });
    if !start {
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        while let Some(bytes) = queue.with(|q| q.borrow_mut().0.take()) {
            write(bytes).await;
        }
        queue.with(|q| q.borrow_mut().1 = false);
    });
}

/// Stores the user configuration file.
fn store_user_config(bytes: Vec<u8>) {
    store_latest(&USER_CONFIG, bytes, |bytes| {
        Box::pin(async move {
            if let Err(e) = crate::game::write_user_config(bytes).await {
                web_sys::console::error_1(&e);
            }
        })
    });
}

/// Stores the game's log file (console-log.md 3.3). A write that fails
/// leaves its line in the log while the game runs (3.9).
/// TODO(#650): a write that fails once the file is open is fatal (3.9).
fn store_log_file(bytes: Vec<u8>) {
    store_latest(&LOG_FILE, bytes, |bytes| {
        Box::pin(async move {
            if let Err(e) = crate::game::write_log_file(&bytes).await {
                web_sys::console::error_1(&e);
                let text = js_sys::Reflect::get(&e, &"message".into())
                    .ok()
                    .and_then(|m| m.as_string())
                    .or_else(|| e.as_string())
                    .unwrap_or_default();
                if active() {
                    LOG_FILE_FAILED.with(|f| f.borrow_mut().push(text));
                }
            }
        })
    });
}

/// The size the game draws at while it runs (its video mode, #49), `None`
/// otherwise (the splash and the page draw at the canvas's size).
pub fn render_size() -> Option<Resolution> {
    RUNNING.with(|r| r.borrow().as_ref().map(|running| running.app.video_mode()))
}

/// Sets `r__supersample` as a video restart applies it (tests).
#[cfg(feature = "test-ops")]
pub fn set_supersample(value: u32) -> Result<(), JsValue> {
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut().ok_or_else(|| error("the game is not running"))?;
        running.app.set_supersample(value);
        Ok(())
    })
}

/// The `debug_video_mode` op (tests): draws at `mode` from now on
/// ([`App::set_video_mode`]).
#[cfg(feature = "test-ops")]
pub fn set_video_mode(mode: Resolution) -> Result<(), JsValue> {
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut().ok_or_else(|| error("the game is not running"))?;
        running.app.set_video_mode(mode);
        Ok(())
    })
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

/// `texture_lod` and its list as a texture loaded now uses them
/// ([`App::texture_quality`]).
pub fn texture_quality() -> Option<volya_engine::core::texture_quality::TextureQuality> {
    RUNNING.with(|r| Some(r.borrow().as_ref()?.app.texture_quality()))
}

/// The New Game load in progress or that made the level shown
/// ([`App::load`]).
pub fn load() -> Option<u64> {
    RUNNING.with(|r| r.borrow().as_ref()?.app.load())
}

/// Whether the loading waits for the prefetched models to be made
/// ([`App::models_to_prefetch`]).
pub fn models_to_prefetch() -> bool {
    RUNNING.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.app.models_to_prefetch())
    })
}

/// Makes the next prefetched models, each chunk of their geometry kept by
/// the renderer as it is made ([`App::prefetch_models`]); whether the
/// loading waits for more.
pub fn prefetch_models() -> Result<bool, JsValue> {
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let Some(running) = r.as_mut() else {
            return Ok(false);
        };
        crate::game::with_trees(|tree, _| {
            running.app.prefetch_models(tree, &mut |c| {
                crate::gpu::with_renderer(|r| {
                    r.keep_geometry(c.set, c.index, &c.vertices, &c.indices)
                });
            })
        })
    })
}

/// Whether the textures to prepare are the prefetched models', kept while
/// the game type stays ([`App::prefetching_textures`]).
pub fn prefetching_textures() -> bool {
    RUNNING.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.app.prefetching_textures())
    })
}

/// Whether a level's loading waits for its textures.
pub fn preparing_textures() -> bool {
    RUNNING.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.app.textures_to_prepare().is_some())
    })
}

/// The level's world, while its loading waits for it to be prepared for
/// the GPU ([`App::world_to_prepare`]).
pub fn world_to_prepare() -> Option<Arc<LevelWorld>> {
    RUNNING.with(|r| Some(r.borrow().as_ref()?.app.world_to_prepare()?.clone()))
}

/// The level's start view while its world is prepared
/// ([`App::level_to_prepare_scene`]).
pub fn level_to_prepare_scene(resolution: Resolution) -> Option<Scene> {
    RUNNING.with(|r| r.borrow().as_ref()?.app.level_to_prepare_scene(resolution))
}

/// The level's world is prepared: the level shows.
pub fn world_prepared() {
    RUNNING.with(|r| {
        if let Some(running) = r.borrow_mut().as_mut() {
            running.app.world_prepared();
        }
    });
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
            follow(&mut running.app, resolution);
        }
    });
}

/// One frame at page time `time` (ms): the app takes this frame's input and
/// plays its sounds; quitting ends the game.
pub fn frame(time: f64) {
    let quit = RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut()?;
        let Running {
            app,
            voices,
            settings: logged,
        } = running;
        let updated = crate::game::with_trees(|tree, _| {
            crate::input::with_input_mut(|input| {
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
        for error in LOG_FILE_FAILED.with(|f| std::mem::take(&mut *f.borrow_mut())) {
            app.log_file_not_written(crate::game::LOG_FILE_PATH, &error);
        }
        if let Some(bytes) = app.take_log_file() {
            store_log_file(bytes);
        }
        for path in app.take_deleted_saves() {
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = crate::game::delete_saved_file(&path).await {
                    web_sys::console::error_1(&e);
                }
            });
        }
        // A command took effect: the settings again where they changed,
        // unless only typed commands changed them, which the game's log
        // records (`~ <line>`, docs/architecture/diagnostics.md).
        let console = app.console();
        let (generation, untyped) = (console.generation(), console.untyped_generation());
        if logged.0 != generation {
            let values = settings(app);
            if values != logged.2 && logged.1 != untyped {
                log_settings(&values);
            }
            *logged = (generation, untyped, values);
        }
        crate::input::set_render(Some(app.video_mode()));
        // The game was unmounted under the app.
        match updated {
            Err(e) => Some(("unmounted", e.as_string().unwrap_or_default())),
            Ok(()) if app.quit_requested() => Some(("quit", "left the game".to_owned())),
            Ok(()) => None,
        }
    });
    if let Some((reason, message)) = quit {
        stop(reason, &message);
    }
}

thread_local! {
    /// Whether the page is hidden (#93), for work outside the frame loop.
    static PAGE_HIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Whether the page is in full screen and can be, as it last said,
    /// for the games started later.
    static PAGE_FULL_SCREEN: std::cell::Cell<Option<(bool, bool)>> =
        const { std::cell::Cell::new(None) };
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
        let Some(Running { app, voices, .. }) = r.as_mut() else {
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
            let scene: Scene = tracing::info_span!("render.app_scene")
                .in_scope(|| running.app.scene(tree, resolution))
                .map_err(error)?;
            Ok::<_, JsValue>(
                tracing::info_span!("render.draw_list")
                    .in_scope(|| presenter.draw_list(renderer, tree, &scene, running.app.clock())),
            )
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

/// Where a tap-to-move trip's destination shows in the picture
/// ([`App::trip_marker`]).
pub fn trip_marker() -> Option<[f32; 2]> {
    RUNNING.with(|r| r.borrow().as_ref()?.app.trip_marker())
}

/// Whether the character takes keyboard keys ([`App::character_takes_keys`]).
pub fn character_takes_keys() -> bool {
    RUNNING.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.app.character_takes_keys())
    })
}

/// Whether the PDA window is open over the running level with nothing over
/// it ([`App::pda_over_level`]).
pub fn pda_over_level() -> bool {
    RUNNING.with(|r| r.borrow().as_ref().is_some_and(|r| r.app.pda_over_level()))
}

/// The page is in the browser's full screen or not, or can't be
/// ([`App::set_full_screen`]).
pub fn set_full_screen(on: bool, supported: bool) {
    PAGE_FULL_SCREEN.set(Some((on, supported)));
    RUNNING.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            r.app.set_full_screen(on, supported);
        }
    });
}

/// The platform's measurements of the frame just drawn
/// ([`App::set_frame_times`]).
pub fn set_frame_times(times: volya_engine::statistics_overlay::FrameTimes) {
    RUNNING.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            r.app.set_frame_times(times);
        }
    });
}

/// Full screen to enter or leave, once a video restart asked for it
/// ([`App::take_full_screen_request`]).
pub fn take_full_screen_request() -> Option<bool> {
    RUNNING.with(|r| r.borrow_mut().as_mut()?.app.take_full_screen_request())
}

/// Whether the console is open ([`App::console_open`]).
pub fn console_open() -> bool {
    RUNNING.with(|r| r.borrow().as_ref().is_some_and(|r| r.app.console_open()))
}

/// Whether the console would open over the main menu now
/// ([`App::console_opens_over_menu`]).
pub fn console_opens_over_menu() -> bool {
    RUNNING.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.app.console_opens_over_menu())
    })
}

/// Whether the game shows its mouse cursor ([`App::cursor_shown`]): the
/// page's touch drives the cursor then (#856, #807).
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

/// The `load_memory` op: the latest load's memory figures
/// ([`volya_engine::app::LoadMemory`]), or null while no game runs.
pub fn load_memory() -> serde_json::Value {
    RUNNING.with(|r| match r.borrow().as_ref() {
        Some(running) => serde_json::to_value(running.app.load_memory()).unwrap_or_default(),
        None => serde_json::Value::Null,
    })
}

/// The `menu_state` op: the app's state, or `{"phase": "none"}` (or
/// `"loading"` during the splash).
pub fn state() -> serde_json::Value {
    if SPLASH.with(|s| s.borrow().is_some()) {
        return json!({ "phase": "loading" });
    }
    RUNNING.with(|r| match r.borrow().as_ref() {
        Some(running) => {
            let mut state = serde_json::to_value(running.app.state()).unwrap_or_default();
            // The console window and its input line (console-window.md).
            let console = running.app.console();
            state["console"] = json!({
                "open": running.app.console_open(),
                "line": console.line(),
                "log": console.log().last(),
            });
            state
        }
        None => json!({ "phase": "none" }),
    })
}

/// The `debug_nightvision_outfit` op (tests): an outfit with night vision
/// worn, or taken off and away ([`App::nightvision_outfit`]); its section.
#[cfg(feature = "test-ops")]
pub fn nightvision_outfit(wear: bool) -> Result<Option<String>, JsValue> {
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut().ok_or_else(|| error("the game is not running"))?;
        running.app.nightvision_outfit(wear).map_err(error)
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

/// The `debug_hit` op (tests): a hit on the player character
/// ([`App::hit_player`]).
#[cfg(feature = "test-ops")]
pub fn hit(kind: &str, power: f32, bone: Option<&str>, ap: f32) -> Result<(), JsValue> {
    RUNNING.with(|r| {
        let mut r = r.borrow_mut();
        let running = r.as_mut().ok_or_else(|| error("the game is not running"))?;
        running.app.hit_player(kind, power, bone, ap).map_err(error)
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

/// The `zones_state` op (tests): the level's zones and the weather's
/// ambient effect (docs/behavior/zone-idle-effects.md,
/// weather-ambient-effects.md).
#[cfg(feature = "test-ops")]
pub fn zones_state() -> Result<serde_json::Value, JsValue> {
    RUNNING.with(|r| {
        let r = r.borrow();
        let running = r.as_ref().ok_or_else(|| error("the game is not running"))?;
        let level = running.app.level().ok_or_else(|| error("no level"))?;
        serde_json::to_value(level.zones_report()).map_err(error)
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
            .copy_of(volya_engine::core::particles::placed::Origin::Placed);
        let camera = effects
            .effects()
            .next()
            .ok_or_else(|| error("no placed effects"))?
            .instance
            .sphere()
            .centre;
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

/// The `debug_rays_bench` op (tests, #396): `count` pseudo-random rays
/// from within 1 m of the player's sky-ray origin against the level's
/// collision world, short (a particle's step, 1 mm to 0.5 m) and long
/// (5 to 100 m), each cast 5 times: the mean CPU time of a ray
/// (microseconds) by `ray_nearest` under the particles' test and by
/// `ray_any` under the sound occlusion's, and how many hit.
#[cfg(feature = "test-ops")]
pub fn rays_bench(count: u32) -> Result<serde_json::Value, JsValue> {
    use volya_engine::core::particles::collision::front_hit;
    use volya_engine::core::sound::occlusion::crosses;
    RUNNING.with(|r| {
        let r = r.borrow();
        let running = r.as_ref().ok_or_else(|| error("the game is not running"))?;
        let level = running.app.level().ok_or_else(|| error("no level"))?;
        let world = level.collision();
        let p = level.sky_ray_origin();
        let mut state = 396u64;
        let mut unit = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 40) as f32 / (1u64 << 24) as f32
        };
        let mut rays = |short: bool| {
            (0..count)
                .map(|_| {
                    let o = [
                        p[0] + unit() * 2.0 - 1.0,
                        p[1] + unit() * 2.0 - 1.0,
                        p[2] + unit() * 2.0 - 1.0,
                    ];
                    let d = loop {
                        let v = [unit() * 2.0 - 1.0, unit() * 2.0 - 1.0, unit() * 2.0 - 1.0];
                        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                        if l > 0.1 && l <= 1.0 {
                            break v.map(|x| x / l);
                        }
                    };
                    let length = if short {
                        1e-3 * 500f32.powf(unit())
                    } else {
                        5.0 + 95.0 * unit()
                    };
                    (o, d, length)
                })
                .collect::<Vec<_>>()
        };
        let mut result = serde_json::Map::new();
        for (name, rays) in [("short", rays(true)), ("long", rays(false))] {
            let time = |f: &dyn Fn(([f32; 3], [f32; 3], f32)) -> bool| {
                let begin = crate::now();
                let mut hits = 0;
                for _ in 0..5 {
                    hits = rays.iter().filter(|&&r| f(r)).count();
                }
                let us = (crate::now() - begin) * 1000.0 / (5.0 * f64::from(count.max(1)));
                (us, hits)
            };
            let (nearest_us, nearest_hits) = time(&|(o, d, l)| {
                world
                    .ray_nearest(o, d, l, |i| {
                        front_hit(o, d, &world.corners(i)).filter(|&t| t < l)
                    })
                    .is_some()
            });
            let (any_us, any_hits) =
                time(&|(o, d, l)| world.ray_any(o, d, l, |t| crosses(o, d, l, t, true)));
            result.insert(
                name.into(),
                json!({
                    "nearest_us": nearest_us,
                    "nearest_hits": nearest_hits,
                    "any_us": any_us,
                    "any_hits": any_hits,
                }),
            );
        }
        result.insert("rays".into(), json!(count));
        result.insert("index_bytes".into(), json!(world.memory_bytes()));
        Ok(serde_json::Value::Object(result))
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
/// page's own controls, each `{path}` (a texture path), `{id}` (a texture
/// description id) or `{item}` (an item section's inventory icon), with an
/// optional `region` `[x, y, width,
/// height]` and `silhouette` (the HUD icon transform of
/// [`volya_engine::ui_image::silhouette`]) or `engraving`
/// ([`volya_engine::ui_image::engraving`]). Answers `{images, cross_color,
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
            let source = match (text("path"), text("id"), text("item")) {
                (Some(path), _, _) => Source::Path(path.into()),
                (None, Some(id), _) => Source::Id(id.into()),
                (None, None, Some(item)) => Source::Item(item.into()),
                _ => return json!({ "error": "none of `path`, `id` and `item`" }),
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
                match ui_image::load(
                tree,
                &data.aliases,
                &data.descriptions,
                &data.system,
                &source,
                region,
            ) {
                    Ok(image) => image,
                    Err(e) => return json!({ "error": e.to_string() }),
                };
            let flag = |key| request.get(key).and_then(|v| v.as_bool()) == Some(true);
            let (image, mean) = if flag("silhouette") {
                ui_image::silhouette(&image)
            } else if flag("engraving") {
                let mean = ui_image::mean_color(&image);
                (ui_image::engraving(&image), mean)
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
