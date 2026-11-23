// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine for web browsers. Runs inside the engine worker
//! (`static/engine-worker.js`); the page talks to it only by message
//! (docs/architecture.md). Builds to nothing on other targets.

#![cfg(target_arch = "wasm32")]

mod audio;
mod game;
mod import;
mod opfs;

use std::cell::RefCell;

use serde::Serialize;
use volya_engine::core::vfs::conformance;
use wasm_bindgen::prelude::*;
use web_sys::{OffscreenCanvas, WorkerGlobalScope};

thread_local! {
    /// The page's canvas, transferred to this worker; the renderer's target.
    static CANVAS: RefCell<Option<OffscreenCanvas>> = const { RefCell::new(None) };
}

/// Starts the engine on the canvas transferred from the page.
#[wasm_bindgen]
pub fn start(canvas: OffscreenCanvas) {
    CANVAS.with(|c| *c.borrow_mut() = Some(canvas));
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
        "game_status" => to_json(&game::status().await?),
        "unmount_game" => {
            game::unmount();
            Ok("null".into())
        }
        "forget_game" => game::forget().await.map(|()| "null".into()),
        "read_dir" => to_json(&game::read_dir(text("path"))?),
        "play_sound" => {
            let looping = args.get("loop").and_then(|v| v.as_bool()).unwrap_or(false);
            let gain = args.get("gain").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
            to_json(&game::play_sound(text("path"), looping, gain)?)
        }
        "stop_sounds" => {
            audio::stop_all();
            Ok("null".into())
        }
        "read_head" => {
            let max = args.get("max").and_then(|v| v.as_u64()).unwrap_or(4096);
            to_json(&game::read_head(text("path"), max)?)
        }
        _ => Err(JsValue::from_str(&format!("unknown op {op:?}"))),
    }
}

fn to_json(value: &impl Serialize) -> Result<String, JsValue> {
    serde_json::to_string(value).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[derive(Serialize)]
struct Status {
    #[serde(flatten)]
    engine: volya_engine::test_api::Status,
    /// `worker` when running in a worker, as the architecture requires.
    thread: &'static str,
}

fn status() -> Status {
    Status {
        engine: volya_engine::test_api::status(),
        thread: if js_sys::global().is_instance_of::<WorkerGlobalScope>() {
            "worker"
        } else {
            "main"
        },
    }
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
