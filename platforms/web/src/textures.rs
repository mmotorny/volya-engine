// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level prepared for a GPU without BC (#113), where WebGL 2 draws: while
//! a New Game's loading waits under `st_loading_textures`
//! ([`volya_engine::app::App::textures_to_prepare`]), each texture is
//! transcoded to ETC2 and uploaded, a frame's worth per task turn so frames
//! run in between (the loading screen keeps drawing). Nothing is stored:
//! every load transcodes again (decided by the user: a level's transcodings
//! would take as much storage as its BC textures, on top of the game). Then,
//! while it waits under its last caption
//! ([`volya_engine::app::App::world_to_prepare`]), its geometry is uploaded
//! a few pieces per task turn (#184): wgpu's GL backend keeps what is
//! written in the wasm heap until WebGL reports the work done, which it
//! does only after the turn ends.

use std::cell::RefCell;
use std::sync::Arc;

use serde::Serialize;
use volya_engine::level::LevelWorld;
use volya_engine::render::world::UPLOAD_PIECE;
use volya_engine::render::{TexturePath, transcode};
use volya_engine_formats::dds::Dds;
use wasm_bindgen::JsValue;

/// The latest preparation, for `level_stats`.
#[derive(Clone, Default, Serialize)]
pub struct Stats {
    pub textures: u32,
    pub transcoded: u32,
    /// Milliseconds: in all (frames between included), and transcoding.
    pub total_ms: f64,
    pub transcode_ms: f64,
    /// Megapixels transcoded (every face and level).
    pub transcoded_mpx: f64,
    /// Milliseconds uploading the level's geometry (frames between
    /// included), and the task turns it took.
    pub world_ms: f64,
    pub world_turns: u32,
}

/// Milliseconds of preparation per task turn, about a frame of the loading
/// screen at 30 Hz.
const TURN_MS: f64 = 30.0;

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

/// Whether this GPU's textures are prepared while a level loads: when they
/// are transcoded.
pub fn wanted() -> bool {
    crate::gpu::with_renderer(|r| r.texture_path() == TexturePath::Etc2).unwrap_or(false)
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
            let result = prepare_world(level).await;
            RUNNING.with(|r| *r.borrow_mut() = false);
            if let Err(e) = result {
                web_sys::console::error_1(&e);
            }
            // What is left is uploaded, or the failure reported, when the
            // level is first drawn.
            crate::engine::world_prepared();
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
    let mut turn = now();
    for path in paths {
        // The game was quit or unmounted meanwhile.
        if !crate::engine::preparing_textures() {
            break;
        }
        let Ok(bytes) = crate::game::read(&path) else {
            continue;
        };
        let Ok(dds) = Dds::parse(&bytes) else {
            continue;
        };
        let t = now();
        let transcoded = transcode(&dds);
        stats.transcode_ms += now() - t;
        if transcoded.is_some() {
            stats.transcoded += 1;
            stats.transcoded_mpx += megapixels(&dds);
        }
        let texture = crate::gpu::with_renderer(|r| match &transcoded {
            Some(t) => r.create_transcoded_texture(&dds, t),
            None => r.create_texture(&dds),
        });
        if let Some(Ok(texture)) = texture {
            crate::gpu::with_renderer(|r| {
                r.add_prepared(&path, texture);
                r.flush_uploads();
            });
        }
        drop(transcoded);
        drop(bytes);
        stats.total_ms = now() - start;
        STATS.with(|s| *s.borrow_mut() = stats.clone());
        // A task turn for frames to run once this one has taken its share;
        // none while the page is hidden (#93). Yielding after every texture
        // cost as much as the transcoding itself (each turn waits for a
        // timer and frames; measured in the iOS Simulator).
        if now() - turn >= TURN_MS {
            crate::opfs::sleep(0).await?;
            while crate::engine::page_hidden() {
                crate::opfs::sleep(100).await?;
            }
            turn = now();
        }
    }
    stats.total_ms = now() - start;
    STATS.with(|s| *s.borrow_mut() = stats);
    Ok(())
}

async fn prepare_world(level: Arc<LevelWorld>) -> Result<(), JsValue> {
    let start = now();
    let mut turns = 0;
    // Until done, or the game was quit or unmounted meanwhile.
    while crate::engine::world_to_prepare().is_some_and(|w| Arc::ptr_eq(&w, &level)) {
        turns += 1;
        let done = crate::gpu::prepare_world(&level, WORLD_TURN_BYTES)?;
        STATS.with(|s| {
            let mut s = s.borrow_mut();
            s.world_ms = now() - start;
            s.world_turns = turns;
        });
        if done {
            break;
        }
        // None while the page is hidden (#93).
        crate::opfs::sleep(0).await?;
        while crate::engine::page_hidden() {
            crate::opfs::sleep(100).await?;
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
