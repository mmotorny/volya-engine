// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine for web browsers. Runs inside the engine worker
//! (`static/engine-worker.js`); the page talks to it only by message
//! (docs/architecture.md). Builds to nothing on other targets.

#![cfg(target_arch = "wasm32")]

mod audio;
mod engine;
mod game;
mod gpu;
mod import;
mod input;
mod opfs;
mod textures;

use serde::Serialize;
use volya_engine::core::audio::{Category, Playback};
use volya_engine::core::vfs::conformance;
use volya_engine::formats::ui::Resolution;
use wasm_bindgen::prelude::*;
use web_sys::{OffscreenCanvas, WorkerGlobalScope};

/// Starts the engine on the canvas transferred from the page. Without
/// WebGPU the engine still starts; `status` reports why graphics are
/// unavailable.
#[wasm_bindgen]
pub async fn start(canvas: OffscreenCanvas) {
    gpu::init(canvas).await;
}

/// The canvas's size in CSS pixels and the page's devicePixelRatio, on start
/// and whenever they change.
#[wasm_bindgen]
pub fn resize(width: f64, height: f64, dpr: f64) {
    gpu::resize(width, height, dpr);
    input::resize(width, height);
    let pixels = |css: f64| ((css * dpr).round() as u32).max(1);
    // The canvas's own size: what the resolution list offers (#49).
    engine::resized(gpu::resolution().unwrap_or(Resolution::new(pixels(width), pixels(height))));
}

/// Frames whose timings [`FrameStats`] averages.
const STATS_FRAMES: usize = 60;

/// Timings of the latest frames, milliseconds.
#[derive(Default)]
struct FrameStats {
    /// Time spent in [`frame`] (engine update, scene, draw submission).
    cpu: std::collections::VecDeque<f64>,
    /// Animation-frame time between consecutive frames.
    interval: std::collections::VecDeque<f64>,
    /// Time spent in the sound system's update (occlusion included).
    sound: std::collections::VecDeque<f64>,
    last: Option<f64>,
    /// Frames run so far.
    frames: u64,
}

thread_local! {
    static FRAME_STATS: std::cell::RefCell<FrameStats> = Default::default();
    /// While on (tests: the `heap_log` op), each frame's loading caption
    /// and wasm heap size at its end.
    static HEAP_LOG: std::cell::RefCell<Option<Vec<serde_json::Value>>> = Default::default();
}

/// The wasm heap (linear memory) size, bytes.
pub(crate) fn heap_bytes() -> f64 {
    wasm_bindgen::memory()
        .dyn_into::<js_sys::WebAssembly::Memory>()
        .map(|m| {
            m.buffer()
                .unchecked_into::<js_sys::ArrayBuffer>()
                .byte_length() as f64
        })
        .unwrap_or(0.0)
}

/// `performance.now()` of the worker, milliseconds.
pub(crate) fn now() -> f64 {
    js_sys::global()
        .dyn_into::<WorkerGlobalScope>()
        .ok()
        .and_then(|g| g.performance())
        .map_or_else(js_sys::Date::now, |p| p.now())
}

/// Runs one animation frame; `time` in milliseconds.
#[wasm_bindgen]
pub fn frame(time: f64) {
    let start = now();
    engine::frame(time);
    let sound_start = now();
    audio::sound_frame(time);
    let sound = now() - sound_start;
    gpu::frame(time);
    input::end_frame();
    textures::poll();
    let cpu = now() - start;
    if HEAP_LOG.with(|l| l.borrow().is_some()) {
        let state = engine::state();
        let entry = serde_json::json!({
            "phase": state.get("phase"),
            "caption": state.get("loading").and_then(|l| l.get("caption")),
            "wasm_heap_bytes": heap_bytes(),
        });
        HEAP_LOG.with(|l| {
            if let Some(log) = l.borrow_mut().as_mut() {
                log.push(entry);
            }
        });
    }
    FRAME_STATS.with(|s| {
        let mut s = s.borrow_mut();
        let push = |q: &mut std::collections::VecDeque<f64>, v: f64| {
            if q.len() == STATS_FRAMES {
                q.pop_front();
            }
            q.push_back(v);
        };
        push(&mut s.cpu, cpu);
        push(&mut s.sound, sound);
        if let Some(last) = s.last {
            push(&mut s.interval, time - last);
        }
        s.last = Some(time);
        s.frames += 1;
    });
}

/// The page was hidden (`hidden`) or shown again (our design, #93): the
/// engine stops while hidden; the worker's frame loop idles meanwhile and
/// the page suspends audio output.
#[wasm_bindgen]
pub fn visibility(hidden: bool) {
    engine::set_hidden(hidden);
    audio::with_sounds(|host| host.set_hidden(hidden));
    // The interval spanning the hidden time is not a frame time.
    FRAME_STATS.with(|s| s.borrow_mut().last = None);
}

#[derive(Serialize)]
struct LevelStats {
    /// The wasm heap (linear memory) size.
    wasm_heap_bytes: f64,
    /// GPU memory of the static world (buffers and textures).
    #[serde(flatten)]
    gpu: serde_json::Value,
    /// The latest level's texture preparation (#113).
    textures: textures::Stats,
    /// Averages over the latest frames, milliseconds.
    frame_cpu_ms: f64,
    frame_cpu_max_ms: f64,
    frame_interval_ms: f64,
    /// The sound system's update per frame: average and worst.
    sound_update_ms: f64,
    sound_update_max_ms: f64,
}

fn level_stats() -> LevelStats {
    let heap = heap_bytes();
    let average =
        |q: &std::collections::VecDeque<f64>| q.iter().sum::<f64>() / q.len().max(1) as f64;
    FRAME_STATS.with(|s| {
        let s = s.borrow();
        LevelStats {
            wasm_heap_bytes: heap,
            gpu: gpu::world_stats(),
            textures: textures::stats(),
            frame_cpu_ms: average(&s.cpu),
            frame_cpu_max_ms: s.cpu.iter().copied().fold(0.0, f64::max),
            frame_interval_ms: average(&s.interval),
            sound_update_ms: average(&s.sound),
            sound_update_max_ms: s.sound.iter().copied().fold(0.0, f64::max),
        }
    })
}

/// One input message from the page (format in the `input` module).
#[wasm_bindgen]
pub fn input(message: js_sys::Array) -> Result<(), JsValue> {
    input::handle(&message)
}

/// Whether the engine owns the screen, so the page should forward input to
/// it. Off by default; the `input_capture` op sets it.
#[wasm_bindgen]
pub fn input_capture() -> bool {
    input::capture()
}

/// The level's controls for the page: bit 1 while they are live,
/// plus 2 while the crouch toggle is engaged, 4 while a sprint is
/// requested, 8 while the toggled crouch is the low crouch and 16 while the
/// sprint lock holds the touch stick; 32 while the character takes keys
/// (the level's controls live, or the inventory screen over them, #110).
#[wasm_bindgen]
pub fn level_controls() -> u32 {
    let keys = u32::from(engine::character_takes_keys()) << 5;
    keys | engine::level_controls().map_or(0, |c| {
        1 | u32::from(c.crouch) << 1
            | u32::from(c.sprint) << 2
            | u32::from(c.low_crouch) << 3
            | u32::from(c.sprint_lock) << 4
    })
}

/// Whether the game shows its mouse cursor: the page's touch is a
/// touchpad then, elsewhere direct (#108).
#[wasm_bindgen]
pub fn cursor_shown() -> bool {
    engine::cursor_shown()
}

/// The save screen's name field while it shows, for the page's text entry
/// (#94): `[x, y, width, height, focused, over]`, canvas CSS pixels, then 1
/// or 0 for its focus and for the cursor over it (#108).
#[wasm_bindgen]
pub fn text_field() -> Option<Vec<f32>> {
    let flag = |b: bool| f32::from(u8::from(b));
    engine::text_field()
        .map(|([x, y, w, h], focused, over)| vec![x, y, w, h, flag(focused), flag(over)])
}

/// Engine interface for the page, tests and debugging: runs `op` with JSON
/// `args` and returns its JSON result.
#[wasm_bindgen]
pub async fn call(op: String, args: String) -> Result<String, JsValue> {
    let args: serde_json::Value = serde_json::from_str(&args).unwrap_or_default();
    let text = |key: &str| args.get(key).and_then(|v| v.as_str()).unwrap_or_default();
    match op.as_str() {
        "status" => to_json(&status()),
        "vfs_selftest" => vfs_selftest().await.map(|()| "null".into()),
        "script_selftest" => script_selftest(),
        "game_status" => to_json(&game::status().await?),
        "unmount_game" => {
            game::unmount();
            Ok("null".into())
        }
        "forget_game" => game::forget().await.map(|()| "null".into()),
        "read_dir" => to_json(&game::read_dir(text("path"))?),
        "cover" => to_json(&game::cover()?),
        "play_sound" => {
            let number = |key: &str, default: f64| {
                args.get(key).and_then(|v| v.as_f64()).unwrap_or(default) as f32
            };
            let playback = Playback {
                looping: args.get("loop").and_then(|v| v.as_bool()).unwrap_or(false),
                gain: number("gain", 1.0),
                pan: number("pan", 0.0),
                category: category(args.get("category"))?,
            };
            to_json(&game::play_sound(text("path"), playback)?)
        }
        "play_music" => to_json(&game::play_music(text("track"))?),
        "stop_music" => {
            audio::stop_music();
            Ok("null".into())
        }
        "set_volume" => {
            let volume = args.get("volume").and_then(|v| v.as_f64());
            let volume = volume.ok_or_else(|| JsValue::from_str("`volume` must be a number"))?;
            audio::set_volume(category(args.get("category"))?, volume as f32)?;
            Ok("null".into())
        }
        "stop_sounds" => {
            audio::stop_all();
            Ok("null".into())
        }
        "debug_draw" => to_json(&gpu::debug_draw(args.get("path").and_then(|v| v.as_str()))?),
        "debug_capture" => {
            let size = |key| args.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            to_json(&gpu::debug_capture(size("width"), size("height")).await?)
        }
        "read_head" => {
            let max = args.get("max").and_then(|v| v.as_u64()).unwrap_or(4096);
            to_json(&game::read_head(text("path"), max)?)
        }
        "input_state" => to_json(&input::state()),
        "level_stats" => to_json(&level_stats()),
        "debug_transcode" => to_json(&textures::bench(&args)?),
        "ui_images" => to_json(&engine::ui_images(&args)?),
        // `{"on": true}` starts logging each frame's loading caption and
        // wasm heap; `{"on": false}` stops and returns the log.
        "heap_log" => {
            let on = args.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            let log = HEAP_LOG.with(|l| std::mem::replace(&mut *l.borrow_mut(), on.then(Vec::new)));
            to_json(&log.unwrap_or_default())
        }
        "launch" => to_json(&engine::launch().await?),
        "menu_state" => to_json(&engine::state()),
        "bound_keys" => to_json(&engine::bound_keys()),
        #[cfg(feature = "test-ops")]
        "debug_give_item" => to_json(&engine::give_item(text("section"))?),
        // `{name, position: [x, y, z], category, looped, two_d, delay}`.
        #[cfg(feature = "test-ops")]
        "debug_play_sound" => {
            let flag = |key: &str| args.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
            let position = args
                .get("position")
                .and_then(|v| serde_json::from_value::<[f32; 3]>(v.clone()).ok())
                .unwrap_or_default();
            let flags = volya_engine::core::sound::system::PlayFlags {
                looped: flag("looped"),
                two_d: flag("two_d"),
                delay: args.get("delay").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            };
            let category = category(args.get("category"))?;
            game::with_trees(|tree, _| {
                audio::debug_play_sound(tree, text("name"), category, position, flags)
            })??;
            Ok("null".into())
        }
        #[cfg(feature = "test-ops")]
        "sound_state" => to_json(&audio::sound_state()),
        "menu_scene" => {
            let size = |key| args.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            to_json(&engine::scene(Resolution::new(
                size("width").max(1),
                size("height").max(1),
            ))?)
        }
        "user_config" => {
            let bytes = game::read_user_config().await?;
            to_json(&bytes.map(|b| volya_engine::formats::cp1251::decode(&b)))
        }
        "forget_user_config" => game::forget_user_config().await.map(|()| "null".into()),
        "quit_game" => {
            engine::stop();
            Ok("null".into())
        }
        "input_capture" => {
            input::set_capture(args.get("on").and_then(|v| v.as_bool()).unwrap_or(false));
            Ok("null".into())
        }
        _ => Err(JsValue::from_str(&format!("unknown op {op:?}"))),
    }
}

/// The volume category named by `value` (`"effects"`, the default, or
/// `"music"`).
fn category(value: Option<&serde_json::Value>) -> Result<Category, JsValue> {
    match value.map(|v| v.as_str()) {
        None | Some(Some("effects")) => Ok(Category::Effects),
        Some(Some("music")) => Ok(Category::Music),
        _ => Err(JsValue::from_str(&format!(
            "unknown sound category {}",
            value.map(|v| v.to_string()).unwrap_or_default()
        ))),
    }
}

fn to_json(value: &impl Serialize) -> Result<String, JsValue> {
    serde_json::to_string(value).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[derive(Serialize)]
struct Status {
    #[serde(flatten)]
    engine: volya_engine::test_api::Status,
    /// The size the game draws at while it runs (#49).
    #[serde(skip_serializing_if = "Option::is_none")]
    render: Option<[u32; 2]>,
    /// `worker` when running in a worker, as the architecture requires.
    thread: &'static str,
    /// Frames the worker's loop has run.
    frames: u64,
    gpu: gpu::Status,
    /// The wasm build running (static/wasm-build.js, #186): `webgpu`, or
    /// `webgl` with WebGL 2 too.
    build: &'static str,
}

/// Frames the worker's loop has run.
pub fn frames() -> u64 {
    FRAME_STATS.with(|s| s.borrow().frames)
}

fn status() -> Status {
    Status {
        engine: volya_engine::test_api::status(),
        render: engine::render_size().map(|r| [r.width, r.height]),
        thread: if js_sys::global().is_instance_of::<WorkerGlobalScope>() {
            "worker"
        } else {
            "main"
        },
        frames: frames(),
        gpu: gpu::status(),
        build: if cfg!(feature = "webgl") {
            "webgl"
        } else {
            "webgpu"
        },
    }
}

/// Runs the script runtime's shared self-test, then a chunk in a new state;
/// returns that chunk's result, the runtime's `_VERSION`.
fn script_selftest() -> Result<String, JsValue> {
    use volya_engine::script::{Lua, selftest};
    selftest::check().map_err(|e| JsValue::from_str(&e))?;
    let error = |e: volya_engine::script::Error| JsValue::from_str(&e.message);
    let version = Lua::new()
        .map_err(error)?
        .exec("=version", b"return _VERSION")
        .map_err(error)?;
    to_json(&version.first().and_then(|v| v.as_str()))
}

/// Runs the shared VFS conformance check against [`opfs::OpfsVfs`] on a
/// scratch directory inside the engine's OPFS namespace.
async fn vfs_selftest() -> Result<(), JsValue> {
    let namespace = opfs::namespace().await?;
    let name = format!("selftest-{}", js_sys::Date::now() as u64);
    let result = async {
        let dir = opfs::create_dir(&namespace, &name).await?;
        for (path, contents) in conformance::FILES {
            opfs::write_file(&dir, path, contents).await?;
        }
        opfs::write_file(&dir, conformance::BIG_PATH, &conformance::big_contents()).await?;
        let vfs = opfs::OpfsVfs::mount(&dir).await?;
        conformance::check(&vfs).map_err(|e| JsValue::from_str(&e))
        // `vfs` drops here, releasing its file locks before cleanup.
    }
    .await;
    opfs::remove(&namespace, &name).await?;
    result
}
