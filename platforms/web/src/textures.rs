// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's textures prepared for a GPU without BC (#113): while a New
//! Game's loading waits under `st_loading_textures`
//! ([`volya_engine::app::App::textures_to_prepare`]), each texture is
//! transcoded to ETC2, or read from the stored transcodings, and uploaded,
//! one texture per task turn so frames run in between (the loading screen
//! keeps drawing). Stored transcodings live in `cache/etc2/` of the OPFS
//! namespace, derived data beside the game, never in its files
//! ([`volya_engine::render::texture_cache`]); a write that fails (another
//! copy of the app writing the same entry) is skipped.

use std::cell::RefCell;

use serde::Serialize;
use volya_engine::render::{TexturePath, texture_cache, transcode};
use volya_engine_formats::dds::Dds;
use wasm_bindgen::JsValue;

use crate::opfs;

/// Where stored transcodings live, under the namespace.
pub const CACHE_DIR: &str = "cache";
const ETC2_DIR: &str = "etc2";

/// The latest preparation, for `level_stats`.
#[derive(Clone, Default, Serialize)]
pub struct Stats {
    pub textures: u32,
    /// Transcoded now, and read from stored transcodings.
    pub transcoded: u32,
    pub stored: u32,
    /// Milliseconds: in all (frames between included), transcoding,
    /// reading and writing stored transcodings.
    pub total_ms: f64,
    pub transcode_ms: f64,
    pub cache_read_ms: f64,
    pub cache_write_ms: f64,
    /// Megapixels transcoded (every face and level).
    pub transcoded_mpx: f64,
}

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
    let namespace = opfs::namespace().await?;
    let cache = opfs::create_dir(&opfs::create_dir(&namespace, CACHE_DIR).await?, ETC2_DIR).await?;
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
        let name = texture_cache::name(&path);
        let t = now();
        let stored = opfs::read_file(&cache, &name)
            .await
            .ok()
            .flatten()
            .and_then(|entry| texture_cache::read(&path, &bytes, &entry));
        stats.cache_read_ms += now() - t;
        let transcoded = match stored {
            Some(t) => {
                stats.stored += 1;
                Some(t)
            }
            None => {
                let t = now();
                let transcoded = transcode(&dds);
                stats.transcode_ms += now() - t;
                if let Some(transcoded) = &transcoded {
                    stats.transcoded += 1;
                    stats.transcoded_mpx += (0..dds.mip_levels)
                        .map(|m| {
                            let (w, h) = dds.mip_size(m);
                            f64::from(w * h) * f64::from(dds.faces) / 1e6
                        })
                        .sum::<f64>();
                    let entry = texture_cache::entry(&path, &bytes, transcoded);
                    let t = now();
                    if let Err(e) = opfs::write_file(&cache, &name, &entry).await {
                        web_sys::console::warn_1(&e);
                    }
                    stats.cache_write_ms += now() - t;
                }
                transcoded
            }
        };
        let texture = crate::gpu::with_renderer(|r| match &transcoded {
            Some(t) => r.create_transcoded_texture(&dds, t),
            None => r.create_texture(&dds),
        });
        if let Some(Ok(texture)) = texture {
            crate::gpu::with_renderer(|r| r.add_prepared(&path, texture));
        }
        drop(transcoded);
        drop(bytes);
        stats.total_ms = now() - start;
        STATS.with(|s| *s.borrow_mut() = stats.clone());
        // A task turn for frames to run.
        opfs::sleep(0).await?;
    }
    stats.total_ms = now() - start;
    STATS.with(|s| *s.borrow_mut() = stats);
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
        let megapixels = (0..dds.mip_levels)
            .map(|m| {
                let (w, h) = dds.mip_size(m);
                f64::from(w * h) * f64::from(dds.faces) / 1e6
            })
            .sum::<f64>();
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
