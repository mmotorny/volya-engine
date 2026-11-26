// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level prepared while it loads (#113, #355): while a New Game's loading
//! waits under `st_loading_textures`
//! ([`volya_engine::app::App::textures_to_prepare`]), each texture is
//! uploaded (on a GPU without BC transcoded to ETC2 first), a frame's worth
//! per task turn so frames run in between (the loading screen keeps
//! drawing). Nothing is stored: every load transcodes again (decided by the
//! user: a level's transcodings would take as much storage as its BC
//! textures, on top of the game). Then, while it waits under its last
//! caption ([`volya_engine::app::App::world_to_prepare`]), its geometry is
//! uploaded a few pieces per task turn (#184). The textures' writes are
//! waited for every [`STAGING_BYTES`] and the geometry's every piece, so
//! the browser's GPU process takes them in a little at a time (#443).
//!
//! After each turn the next waits until the browser reports the GPU done
//! with what was submitted (#355): the browser's GPU process copies the
//! uploads after the engine has handed them over, so without the wait a
//! level's uploads all reach it with the level's first frame, and the
//! engine worker waits at that frame's presentation for all of them
//! (believed from the iPhone's diagnostic log: a 6.2 s gap after the
//! level's first frame, outside every span of the frame, with no audio
//! mixed meanwhile).

use std::cell::RefCell;
use std::sync::Arc;

use serde::Serialize;
use volya_engine::core::diagnostics::Value;
use volya_engine::level::LevelWorld;
use volya_engine::render::transcode;
use volya_engine::render::world::UPLOAD_PIECE;
use volya_engine_formats::dds::Dds;
use wasm_bindgen::JsValue;

/// The latest preparation, for `level_stats`.
#[derive(Clone, Default, Serialize)]
pub struct Stats {
    pub textures: u32,
    pub transcoded: u32,
    /// Milliseconds: in all (frames between included), reading the files,
    /// transcoding (with the upload of what is transcoded, done band by
    /// band with it), and uploading the rest.
    pub total_ms: f64,
    pub read_ms: f64,
    pub transcode_ms: f64,
    pub upload_ms: f64,
    /// Megapixels transcoded (every face and level).
    pub transcoded_mpx: f64,
    /// Milliseconds uploading the level's geometry (frames between
    /// included), and the task turns it took.
    pub world_ms: f64,
    pub world_turns: u32,
    /// Milliseconds waiting for the GPU to be done with the uploads, in
    /// all (included in the totals above).
    pub gpu_wait_ms: f64,
    /// Milliseconds drawing the start view unseen (GPU waits included),
    /// and the longest interval between the engine worker's frames
    /// meanwhile.
    pub warm_up_ms: f64,
    pub warm_up_frame_max_ms: f64,
}

thread_local! {
    /// Whether the start view is being drawn unseen.
    static WARMING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// An interval of `ms` between two frames: kept while warming up.
pub fn frame_interval(ms: f64) {
    if WARMING.get() {
        STATS.with(|s| {
            let mut s = s.borrow_mut();
            s.warm_up_frame_max_ms = s.warm_up_frame_max_ms.max(ms);
        });
    }
}

/// Milliseconds of preparation per task turn, about a frame of the loading
/// screen at 30 Hz.
const TURN_MS: f64 = 30.0;

/// Bytes of texture data written before the preparation waits for WebGL to
/// report them done (#357, #440).
const STAGING_BYTES: u64 = 2 << 20;

/// Bytes of the level's geometry uploaded per task turn: one piece, waited
/// for before the next (#440).
const WORLD_TURN_BYTES: u64 = UPLOAD_PIECE as u64;

thread_local! {
    static RUNNING: RefCell<bool> = const { RefCell::new(false) };
    static STATS: RefCell<Stats> = RefCell::new(Stats::default());
}

pub fn stats() -> Stats {
    STATS.with(|s| s.borrow().clone())
}

fn now() -> f64 {
    crate::now()
}

/// Megapixels of every face and level of `dds`.
fn megapixels(dds: &Dds) -> f64 {
    (0..dds.mip_levels)
        .map(|m| {
            let (w, h) = dds.mip_size(m);
            f64::from(w * h) * f64::from(dds.faces) / 1e6
        })
        .sum()
}

/// Whether a level is prepared while it loads: wherever it draws (#355);
/// where textures are transcoded, that is done there.
pub fn wanted() -> bool {
    crate::gpu::with_renderer(|_| ()).is_some()
}

/// Waits until the browser reports the GPU done with all work submitted so
/// far ([`crate::gpu::submitted_work_done`]) and records how long it took:
/// the metric `load.gpu_wait_ms`, and in `wait_ms`.
async fn gpu_done(wait_ms: &mut f64) -> Result<(), JsValue> {
    let t = now();
    crate::gpu::submitted_work_done().await?;
    let ms = now() - t;
    *wait_ms += ms;
    volya_engine::core::diagnostics::metric("load.gpu_wait_ms", ms);
    Ok(())
}

/// Starts preparing the textures the loading waits for, if it does and no
/// preparation runs (called after each frame).
pub fn poll() {
    if RUNNING.with(|r| *r.borrow()) {
        return;
    }
    if let Some(level) = crate::engine::world_to_prepare() {
        RUNNING.with(|r| *r.borrow_mut() = true);
        wasm_bindgen_futures::spawn_local(async move {
            let result = prepare_world(level.clone()).await;
            RUNNING.with(|r| *r.borrow_mut() = false);
            if let Err(e) = result {
                web_sys::console::error_1(&e);
            }
            // What is left is uploaded, or the failure reported, when the
            // level is first drawn; unless the game was quit meanwhile (a
            // later load's hold is its own task's to release).
            if crate::engine::world_to_prepare().is_some_and(|w| Arc::ptr_eq(&w, &level)) {
                crate::engine::world_prepared();
            }
        });
        return;
    }
    let Some(paths) = crate::engine::textures_to_prepare() else {
        return;
    };
    RUNNING.with(|r| *r.borrow_mut() = true);
    wasm_bindgen_futures::spawn_local(async move {
        let result = prepare(paths).await;
        RUNNING.with(|r| *r.borrow_mut() = false);
        if let Err(e) = result {
            web_sys::console::error_1(&e);
        }
        // Whatever failed is loaded the ordinary way, and reported, at the
        // world's upload.
        crate::engine::textures_prepared();
    });
}

async fn prepare(paths: Vec<String>) -> Result<(), JsValue> {
    let start = now();
    let mut stats = Stats {
        textures: paths.len() as u32,
        ..Default::default()
    };
    let written = || crate::gpu::with_renderer(|r| r.texture_bytes_written()).unwrap_or(0);
    // The bytes written since WebGL last reported the writes done.
    let mut staged_from = written();
    // Every file is read into this one, and each band transcoded into the
    // other.
    let mut bytes = Vec::new();
    let mut scratch = Vec::new();
    let mut turn = now();
    for path in paths {
        // The game was quit or unmounted meanwhile.
        if !crate::engine::preparing_textures() {
            break;
        }
        let _span = tracing::info_span!("prepare.texture").entered();
        let t = now();
        if crate::game::read_into(&path, &mut bytes).is_err() {
            continue;
        }
        stats.read_ms += now() - t;
        let Ok(dds) = Dds::parse(&bytes) else {
            continue;
        };
        let t = now();
        // Each texture's making and upload a step in the diagnostic log
        // (#443; one per texture, as one per band filled the log).
        let size = [dds.width, dds.height].map(|n| Value::Number(f64::from(n)));
        let [width, height] = size;
        let step = [
            ("path", Value::Text(path.clone())),
            ("width", width),
            ("height", height),
        ];
        crate::gpu::with_renderer(|r| crate::diagnostics::gpu_step("gpu.texture", Some(r), &step));
        let Some(Ok(mut upload)) = crate::gpu::with_renderer(|r| r.begin_texture(&dds)) else {
            continue;
        };
        let transcoded = upload.transcoded();
        if transcoded {
            stats.transcoded += 1;
            stats.transcoded_mpx += megapixels(&dds);
        }
        let mut waited = 0.0;
        loop {
            // The writes before are waited for when the next band would
            // take the staging past its bound.
            let staged = written() - staged_from;
            if staged > 0
                && upload
                    .next_bytes()
                    .is_some_and(|n| staged + n > STAGING_BYTES)
            {
                let w = now();
                gpu_done(&mut stats.gpu_wait_ms).await?;
                waited += now() - w;
                staged_from = written();
            }
            let budget = STAGING_BYTES.saturating_sub(written() - staged_from);
            let done = crate::gpu::with_renderer(|r| {
                let done = upload.step(r, &dds, &mut scratch, budget);
                r.flush_uploads();
                done
            });
            if done != Some(false) {
                break;
            }
        }
        if let Some(texture) = crate::gpu::with_renderer(|r| upload.finish(r)) {
            crate::gpu::with_renderer(|r| r.add_prepared(&path, texture));
        }
        // Transcoding and uploading go band by band: a transcoded texture's
        // time is the transcoding's.
        let took = now() - t - waited;
        if transcoded {
            stats.transcode_ms += took;
        } else {
            stats.upload_ms += took;
        }
        stats.total_ms = now() - start;
        STATS.with(|s| *s.borrow_mut() = stats.clone());
        // A task turn for frames to run once this one has taken its share;
        // none while the page is hidden (#93). Yielding after every texture
        // cost as much as the transcoding itself (each turn waits for a
        // timer and frames; measured in the iOS Simulator).
        if now() - turn >= TURN_MS {
            drop(_span);
            crate::opfs::sleep(0).await?;
            while crate::engine::page_hidden() {
                crate::opfs::sleep(100).await?;
            }
            turn = now();
        }
    }
    gpu_done(&mut stats.gpu_wait_ms).await?;
    stats.total_ms = now() - start;
    STATS.with(|s| *s.borrow_mut() = stats);
    Ok(())
}

async fn prepare_world(level: Arc<LevelWorld>) -> Result<(), JsValue> {
    let start = now();
    let mut turns = 0;
    let mut wait_ms = 0.0;
    // Until done, or the game was quit or unmounted meanwhile.
    while crate::engine::world_to_prepare().is_some_and(|w| Arc::ptr_eq(&w, &level)) {
        turns += 1;
        crate::gpu::with_renderer(|r| {
            crate::diagnostics::gpu_step(
                "gpu.geometry",
                Some(r),
                &[("turn", Value::Number(f64::from(turns)))],
            )
        });
        let done = tracing::info_span!("prepare.world")
            .in_scope(|| crate::gpu::prepare_world(&level, WORLD_TURN_BYTES))?;
        // Until WebGL reports the pieces written done (it may not in the
        // task they were written in, documented by the WebGL 2
        // specification).
        gpu_done(&mut wait_ms).await?;
        STATS.with(|s| {
            let mut s = s.borrow_mut();
            s.world_ms = now() - start;
            s.world_turns = turns;
            s.gpu_wait_ms += std::mem::take(&mut wait_ms);
        });
        if done {
            let warm_start = now();
            STATS.with(|s| s.borrow_mut().warm_up_frame_max_ms = 0.0);
            WARMING.set(true);
            let warmed = warm_up().await;
            WARMING.set(false);
            STATS.with(|s| s.borrow_mut().warm_up_ms = now() - warm_start);
            return warmed;
        }
        // A task turn; none while the page is hidden (#93).
        crate::opfs::sleep(0).await?;
        while crate::engine::page_hidden() {
            crate::opfs::sleep(100).await?;
        }
    }
    Ok(())
}

/// The level's start view drawn unseen, whole, and the GPU's work on it
/// waited for. In 40-call pieces (#437) it took 410–425 ms against 72–76 in
/// headless Chrome, the longest frame meanwhile 50–67 ms against 67 (#448).
async fn warm_up() -> Result<(), JsValue> {
    let mut wait_ms = 0.0;
    let drawn = tracing::info_span!("prepare.warm_up").in_scope(crate::gpu::warm_up)?;
    if drawn {
        gpu_done(&mut wait_ms).await?;
        STATS.with(|s| s.borrow_mut().gpu_wait_ms += wait_ms);
    }
    Ok(())
}

/// The `debug_transcode` op (tests: the cost of transcoding): transcodes
/// the BC textures at `paths` (the level shown's when absent) to ETC2 and
/// drops the results. Reports the count, megapixels (every face and
/// level), milliseconds in all and per texture of `paths`, and the wasm
/// heap before and at most during.
pub fn bench(args: &serde_json::Value) -> Result<serde_json::Value, JsValue> {
    let listed = args.get("paths").and_then(|p| p.as_array()).map(|paths| {
        paths
            .iter()
            .filter_map(|p| p.as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    });
    let each = listed.is_some();
    let paths = listed
        .or_else(crate::engine::level_texture_paths)
        .ok_or_else(|| JsValue::from_str("no paths and no level"))?;
    let heap_before = crate::heap_bytes();
    let mut heap_max = heap_before;
    let (mut textures, mut mpx, mut ms) = (0u32, 0f64, 0f64);
    let mut per = Vec::new();
    for path in &paths {
        let bytes = crate::game::read(path)?;
        let dds = Dds::parse(&bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let t = now();
        let Some(transcoded) = transcode(&dds) else {
            continue;
        };
        let took = now() - t;
        heap_max = heap_max.max(crate::heap_bytes());
        drop(transcoded);
        let megapixels = megapixels(&dds);
        textures += 1;
        mpx += megapixels;
        ms += took;
        if each {
            per.push(serde_json::json!({
                "path": path,
                "size": [dds.width, dds.height],
                "format": format!("{:?}", dds.format),
                "ms": took,
                "ms_per_mpx": took / megapixels,
            }));
        }
    }
    Ok(serde_json::json!({
        "textures": textures,
        "mpx": mpx,
        "ms": ms,
        "ms_per_mpx": ms / mpx.max(1e-9),
        "heap_before": heap_before,
        "heap_max": heap_max,
        "per": per,
    }))
}
