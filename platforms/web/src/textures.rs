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
//! uploaded a few pieces per task turn (#184): wgpu's GL backend keeps what
//! is written in the wasm heap until WebGL reports the work done, which it
//! does only after the turn ends.
//!
//! On WebGPU, after each turn the next waits until the browser reports the
//! GPU done with what was submitted (#355; where WebGL 2 draws, the turns
//! wait for frames instead, which WebGL's reports need anyway): the browser's GPU process copies the
//! uploads after the engine has handed them over, so without the wait a
//! level's uploads all reach it with the level's first frame, and the
//! engine worker waits at that frame's presentation for all of them
//! (believed from the iPhone's diagnostic log: a 6.2 s gap after the
//! level's first frame, outside every span of the frame, with no audio
//! mixed meanwhile).

use std::cell::RefCell;
use std::sync::Arc;

use serde::Serialize;
use volya_engine::level::LevelWorld;
use volya_engine::render::world::UPLOAD_PIECE;
use volya_engine::render::{TexturePath, Transcoded, transcode, transcode_into};
use volya_engine_formats::dds::Dds;
use volya_engine_formats::etc2::Etc2Format;
use wasm_bindgen::JsValue;

/// The latest preparation, for `level_stats`.
#[derive(Clone, Default, Serialize)]
pub struct Stats {
    pub textures: u32,
    pub transcoded: u32,
    /// Milliseconds: in all (frames between included), reading the files,
    /// transcoding, and uploading.
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
}

/// Milliseconds of preparation per task turn, about a frame of the loading
/// screen at 30 Hz.
const TURN_MS: f64 = 30.0;

/// Bytes of texture data written while WebGL 2 draws before the
/// preparation waits for their staging to be freed: what the wasm heap
/// holds of them at most (#357; with a 30 ms turn alone, up to 40 MiB of BC
/// textures in desktop Chrome, measured).
const STAGING_BYTES: u64 = 4 << 20;

/// The texture bytes written as frames were seen, to tell which writes
/// WebGL may still hold: those made since the frame two frames back.
struct Staging {
    /// (frames run, bytes written then), oldest first; the first is the
    /// latest at least two frames old.
    seen: std::collections::VecDeque<(u64, u64)>,
}

impl Staging {
    /// Counting from `written` bytes written, after `frames` frames.
    fn new(frames: u64, written: u64) -> Self {
        Self {
            seen: [(frames.saturating_sub(2), written)].into(),
        }
    }

    /// The bytes that may still be staged, `written` written after
    /// `frames` frames.
    fn staged(&mut self, frames: u64, written: u64) -> u64 {
        if self.seen.back().is_some_and(|&(f, _)| f < frames) {
            self.seen.push_back((frames, written));
        }
        while self.seen.len() > 1 && self.seen[1].0 + 2 <= frames {
            self.seen.pop_front();
        }
        written - self.seen.front().map_or(written, |&(_, w)| w)
    }
}

/// Bytes of the level's geometry uploaded per task turn: what the wasm heap
/// holds of it at most per turn.
const WORLD_TURN_BYTES: u64 = 2 * UPLOAD_PIECE as u64;

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

/// Whether a level is prepared while it loads: wherever it draws (#355).
/// Where WebGL 2 draws, its uploads stay in the wasm heap until WebGL
/// reports them done; where textures are transcoded, that is done there;
/// on WebGPU, the GPU process copies them while the loading image shows.
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
    let etc2 =
        crate::gpu::with_renderer(|r| r.texture_path() == TexturePath::Etc2).unwrap_or(false);
    let compat = crate::gpu::with_renderer(|r| r.compat()).unwrap_or(false);
    let written = || crate::gpu::with_renderer(|r| r.texture_bytes_written()).unwrap_or(0);
    let mut staging = Staging::new(crate::frames(), written());
    // Every file is read into this one, and transcoded into the other.
    let mut bytes = Vec::new();
    let mut transcoding = Transcoded {
        format: Etc2Format::Rgb8,
        data: Vec::new(),
    };
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
        let transcoded = (etc2 && transcode_into(&dds, &mut transcoding)).then_some(&transcoding);
        stats.transcode_ms += now() - t;
        let t = now();
        if transcoded.is_some() {
            stats.transcoded += 1;
            stats.transcoded_mpx += megapixels(&dds);
        }
        let texture = crate::gpu::with_renderer(|r| match transcoded {
            Some(t) => r.create_transcoded_texture(&dds, t),
            None => r.create_texture(&dds),
        });
        if let Some(Ok(texture)) = texture {
            crate::gpu::with_renderer(|r| {
                r.add_prepared(&path, texture);
                r.flush_uploads();
            });
        }
        stats.upload_ms += now() - t;
        stats.total_ms = now() - start;
        STATS.with(|s| *s.borrow_mut() = stats.clone());
        // A task turn for frames to run once this one has taken its share;
        // none while the page is hidden (#93). Yielding after every texture
        // cost as much as the transcoding itself (each turn waits for a
        // timer and frames; measured in the iOS Simulator).
        let staged = if compat {
            staging.staged(crate::frames(), written())
        } else {
            0
        };
        if now() - turn >= TURN_MS || staged >= STAGING_BYTES {
            drop(_span);
            if !compat {
                gpu_done(&mut stats.gpu_wait_ms).await?;
            }
            crate::opfs::sleep(0).await?;
            // Until WebGL can have reported the writes done (two frames,
            // as for the geometry below) and their staging is freed.
            let since = crate::frames();
            while (staged >= STAGING_BYTES && crate::frames() < since + 2)
                || crate::engine::page_hidden()
            {
                crate::opfs::sleep(if crate::engine::page_hidden() { 100 } else { 4 }).await?;
            }
            turn = now();
        }
    }
    if !compat {
        gpu_done(&mut stats.gpu_wait_ms).await?;
    }
    stats.total_ms = now() - start;
    STATS.with(|s| *s.borrow_mut() = stats);
    Ok(())
}

async fn prepare_world(level: Arc<LevelWorld>) -> Result<(), JsValue> {
    let start = now();
    let mut turns = 0;
    let mut wait_ms = 0.0;
    let compat = crate::gpu::with_renderer(|r| r.compat()).unwrap_or(false);
    // Until done, or the game was quit or unmounted meanwhile.
    while crate::engine::world_to_prepare().is_some_and(|w| Arc::ptr_eq(&w, &level)) {
        turns += 1;
        let done = tracing::info_span!("prepare.world")
            .in_scope(|| crate::gpu::prepare_world(&level, WORLD_TURN_BYTES))?;
        if !compat {
            gpu_done(&mut wait_ms).await?;
        }
        STATS.with(|s| {
            let mut s = s.borrow_mut();
            s.world_ms = now() - start;
            s.world_turns = turns;
            s.gpu_wait_ms += std::mem::take(&mut wait_ms);
        });
        if done {
            // The level's start view drawn once, unseen, and the GPU's
            // work on it waited for.
            let warmed = tracing::info_span!("prepare.warm_up").in_scope(crate::gpu::warm_up)?;
            if warmed {
                gpu_done(&mut wait_ms).await?;
                STATS.with(|s| {
                    let mut s = s.borrow_mut();
                    s.world_ms = now() - start;
                    s.gpu_wait_ms += std::mem::take(&mut wait_ms);
                });
            }
            break;
        }
        // Where WebGL 2 draws, until two frames ran, so WebGL can report the
        // pieces written done and their staging goes (it may not in the
        // frame they were written in, documented by the WebGL 2
        // specification); on WebGPU a task turn. None while the page is
        // hidden (#93).
        let since = crate::frames();
        let frames = if compat { 2 } else { 0 };
        crate::opfs::sleep(0).await?;
        while crate::frames() < since + frames || crate::engine::page_hidden() {
            crate::opfs::sleep(if crate::engine::page_hidden() { 100 } else { 4 }).await?;
        }
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
