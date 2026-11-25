// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The audio worker's side (`static/audio-worker.js`, #354): a mixer
//! [`Host`] applying the engine's commands as they arrive and mixing
//! whenever the AudioWorklet asks, on the output's schedule rather than
//! between the engine's frames. The same wasm build as the engine's, in its
//! own instance: nothing else of it runs here.

use std::cell::RefCell;

use volya_engine::core::audio::VoiceId;
use volya_engine::core::audio::remote::Host;
use wasm_bindgen::prelude::*;

#[derive(Default)]
struct State {
    host: Host,
    /// Time spent mixing (ms), frames mixed and frames played as silence,
    /// since the worker started.
    mix_ms: f64,
    frames: f64,
    underrun: f64,
    /// Frames mixed since the last report.
    unreported: f64,
    /// The heap size the last report gave.
    reported_heap: f64,
    sample_rate: f64,
    /// Blocks recorded for the engine's capture (`Remote::watch`).
    blocks: Vec<serde_json::Value>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// A new connection: a fresh mixer from the engine's next message.
#[wasm_bindgen]
pub fn audio_host_reset(sample_rate: f64) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.host = Host::default();
        s.sample_rate = sample_rate;
        s.blocks.clear();
    });
}

/// Applies one message of the engine's commands with the files they play
/// (`audio_take`).
#[wasm_bindgen]
pub fn audio_host_apply(commands: &[u8], files: js_sys::Array) -> Result<(), JsValue> {
    let files = files
        .iter()
        .map(|f| f.unchecked_into::<js_sys::Uint8Array>().to_vec())
        .collect();
    STATE
        .with(|s| s.borrow_mut().host.apply(commands, files))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Mixes the next `frames` frames (stereo, interleaved). `underrun` is how
/// many frames the AudioWorklet played as silence for want of mixed ones
/// since its last request, `queued` how many it holds.
#[wasm_bindgen]
pub fn audio_host_mix(frames: u32, underrun: u32, queued: u32) -> Vec<f32> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let watched = s.host.watched();
        let probe = |s: &State| {
            let id: VoiceId = watched.flatten()?;
            s.host.mixer_ref()?.probe(id)
        };
        let before = probe(&s);
        let t = crate::epoch_now();
        let out = s.host.mix(frames as usize);
        let took = crate::epoch_now() - t;
        s.mix_ms += took;
        s.frames += f64::from(frames);
        s.unreported += f64::from(frames);
        s.underrun += f64::from(underrun);
        if watched.is_some() {
            let after = probe(&s);
            let voice = watched.flatten().map_or(-1, |v| i64::from(v.index()));
            s.blocks.push(serde_json::json!([
                t,
                frames,
                took,
                underrun,
                queued,
                voice,
                before.map_or(0.0, |p| p.advanced),
                after.map_or(0.0, |p| p.advanced),
                after.map_or(0.0, |p| p.step),
                before.map_or(0.0, |p| p.gains.0),
                after.map_or(0.0, |p| p.gains.0),
            ]));
        }
        out
    })
}

/// A report for the engine (`audio_report`), JSON, when there is news: the
/// voices that ended, the recorded blocks, a heap that grew, and every
/// second of output the counters. `undefined` otherwise.
#[wasm_bindgen]
pub fn audio_host_report() -> Option<String> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let ended = s.host.take_ended();
        let heap = crate::heap_bytes();
        if ended.is_empty()
            && s.blocks.is_empty()
            && s.unreported < s.sample_rate
            && heap == s.reported_heap
        {
            return None;
        }
        s.unreported = 0.0;
        s.reported_heap = heap;
        let blocks = std::mem::take(&mut s.blocks);
        Some(
            serde_json::json!({
                "ended": ended,
                "blocks": blocks,
                "heap": heap,
                "mix_ms": s.mix_ms,
                "frames": s.frames,
                "underrun": s.underrun,
            })
            .to_string(),
        )
    })
}
