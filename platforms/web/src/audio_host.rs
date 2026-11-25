// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The audio worker's side (`static/audio-worker.js`, #354): a mixer
//! [`Host`] applying the engine's commands as they arrive and keeping the
//! AudioWorklet's ring of mixed frames filled (`crate::audio_ring`), on the
//! output's schedule rather than between the engine's frames. The same
//! wasm build as the engine's, in its own instance: nothing else of it runs
//! here.

use std::cell::RefCell;

use volya_engine::core::audio::VoiceId;
use volya_engine::core::audio::remote::Host;
use wasm_bindgen::prelude::*;

use crate::audio_ring::{OUT_FRAMES, Ring, UNDERRUN};

/// Frames mixed at a time: a gain change ramps over one block.
const BLOCK: u32 = 256;

/// The least, sum, greatest and count of values.
#[derive(Clone, Copy)]
struct Series([f64; 4]);

impl Default for Series {
    fn default() -> Self {
        Self([f64::INFINITY, 0.0, 0.0, 0.0])
    }
}

impl Series {
    fn add(&mut self, v: f64) {
        let s = &mut self.0;
        s[0] = s[0].min(v);
        s[1] += v;
        s[2] = s[2].max(v);
        s[3] += 1.0;
    }

    fn take(&mut self) -> [f64; 4] {
        let mut s = std::mem::take(self).0;
        if s[3] == 0.0 {
            s[0] = 0.0;
        }
        s
    }
}

#[derive(Default)]
struct State {
    host: Host,
    ring: Option<Ring>,
    /// Commands read from the ring and not yet applied (waiting for a file).
    pending: Vec<u8>,
    sample_rate: f64,
    /// The fill kept, frames.
    target: u32,
    /// Time spent mixing (ms), frames mixed and frames played as silence,
    /// since the worker started; the ring's underrun count last seen.
    mix_ms: f64,
    frames: f64,
    underrun: f64,
    underrun_seen: u32,
    /// Frames mixed since the last report.
    unreported: f64,
    /// The heap size the last report gave.
    reported_heap: f64,
    /// Since the last report, for the diagnostic log: the fill when woken
    /// and the mixing time per block.
    fill: Series,
    mix: Series,
    underrun_reported: f64,
    /// Blocks recorded for the engine's capture (`Remote::watch`).
    blocks: Vec<serde_json::Value>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// A new output on `memory` (`crate::audio_ring`), whose ring of frames
/// is kept filled to `target` frames (`static/audio-ring.js`): a fresh
/// mixer from the engine's first commands.
#[wasm_bindgen]
pub fn audio_host_connect(sample_rate: f64, target: u32, memory: JsValue) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let ring = Ring::new(&memory);
        s.underrun_seen = ring.load(UNDERRUN);
        s.ring = Some(ring);
        s.host = Host::default();
        s.pending.clear();
        s.blocks.clear();
        s.sample_rate = sample_rate;
        s.target = target.clamp(BLOCK, OUT_FRAMES - BLOCK);
    });
}

/// File `number` arrived from the engine worker (`audio_flush`). It
/// stays in JavaScript memory, read a piece at a time ([`JsFile`]), so the
/// wasm heap, which never shrinks, holds none of it.
#[wasm_bindgen]
pub fn audio_host_file(number: u32, bytes: js_sys::Uint8Array) {
    let file = JsFile { bytes, pos: 0 };
    STATE.with(|s| s.borrow_mut().host.add_file(number, Box::new(file)));
}

/// A compressed file in a JavaScript `Uint8Array`.
struct JsFile {
    bytes: js_sys::Uint8Array,
    pos: u64,
}

impl std::io::Read for JsFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = u64::from(self.bytes.length());
        let n = (len.saturating_sub(self.pos) as usize).min(buf.len());
        if n > 0 {
            let at = self.pos as u32;
            self.bytes
                .subarray(at, at + n as u32)
                .copy_to(&mut buf[..n]);
            self.pos += n as u64;
        }
        Ok(n)
    }
}

impl std::io::Seek for JsFile {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        let len = i128::from(self.bytes.length());
        let pos = match to {
            std::io::SeekFrom::Start(p) => i128::from(p),
            std::io::SeekFrom::End(d) => len + i128::from(d),
            std::io::SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
        };
        if pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start",
            ));
        }
        self.pos = pos as u64;
        Ok(self.pos)
    }
}

/// Applies the commands the ring holds and fills the ring of frames to its
/// target. Run whenever the audio worker wakes.
#[wasm_bindgen]
pub fn audio_host_service() {
    STATE.with(|s| {
        let s = &mut *s.borrow_mut();
        let Some(ring) = s.ring.as_ref() else {
            return;
        };
        ring.read_commands(&mut s.pending);
        if !s.pending.is_empty() {
            match s.host.apply(&s.pending) {
                Ok(applied) => drop(s.pending.drain(..applied)),
                Err(e) => {
                    web_sys::console::error_1(&format!("audio: {e}").into());
                    s.pending.clear();
                }
            }
        }
        let underrun = ring.load(UNDERRUN);
        let new = underrun.wrapping_sub(s.underrun_seen);
        s.underrun_seen = underrun;
        s.underrun += f64::from(new);
        let mut queued = ring.queued();
        s.fill.add(f64::from(queued));
        let mut gap = new;
        // The target leaves a block's room in the ring.
        while queued < s.target {
            mix(s, queued, gap);
            gap = 0;
            queued += BLOCK;
        }
    });
}

/// Mixes a block into the ring; `queued` frames were ahead of it, `gap`
/// played as silence since the last block.
fn mix(s: &mut State, queued: u32, gap: u32) {
    let watched = s.host.watched();
    let probe = |s: &State| {
        let id: VoiceId = watched.flatten()?;
        s.host.mixer_ref()?.probe(id)
    };
    let before = probe(s);
    let t = crate::epoch_now();
    let out = s.host.mix(BLOCK as usize);
    if let Some(ring) = s.ring.as_ref() {
        ring.write_frames(&out);
    }
    let took = crate::epoch_now() - t;
    s.mix_ms += took;
    s.mix.add(took);
    s.frames += f64::from(BLOCK);
    s.unreported += f64::from(BLOCK);
    if watched.is_some() {
        let after = probe(s);
        let voice = watched.flatten().map_or(-1, |v| i64::from(v.index()));
        s.blocks.push(serde_json::json!([
            t,
            BLOCK,
            took,
            gap,
            queued,
            voice,
            before.map_or(0.0, |p| p.advanced),
            after.map_or(0.0, |p| p.advanced),
            after.map_or(0.0, |p| p.step),
            before.map_or(0.0, |p| p.gains.0),
            after.map_or(0.0, |p| p.gains.0),
        ]));
    }
}

/// A report for the engine (`audio_report`), JSON, when there is news: the
/// voices that ended, the recorded blocks, a heap that grew, and every
/// second of output the counters and the diagnostic log's numbers.
/// `undefined` otherwise.
#[wasm_bindgen]
pub fn audio_host_report() -> Option<String> {
    STATE.with(|s| {
        let s = &mut *s.borrow_mut();
        let ended = s.host.take_ended();
        let heap = crate::heap_bytes();
        let second = s.unreported >= s.sample_rate;
        if ended.is_empty() && s.blocks.is_empty() && !second && heap == s.reported_heap {
            return None;
        }
        s.reported_heap = heap;
        let mut report = serde_json::json!({
            "ended": ended,
            "blocks": std::mem::take(&mut s.blocks),
            "heap": heap,
            "mix_ms": s.mix_ms,
            "frames": s.frames,
            "underrun": s.underrun,
            "rate": s.sample_rate,
        });
        if second {
            s.unreported = 0.0;
            let underrun = s.underrun - s.underrun_reported;
            s.underrun_reported = s.underrun;
            let [f, m] = [s.fill.take(), s.mix.take()];
            report["diagnostics"] =
                serde_json::json!([underrun, f[0], f[1], f[2], f[3], m[0], m[1], m[2], m[3]]);
        }
        Some(report.to_string())
    })
}
