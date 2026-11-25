// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The audio output's shared memory (#354), one `SharedArrayBuffer` per
//! output made by the page (`static/audio-ring.js`, which holds the same
//! layout): control words, a ring of mixer commands from the engine worker
//! to the audio worker, and a ring of mixed stereo frames from the audio
//! worker to the AudioWorklet. Each ring has one writer and one reader;
//! its positions only grow (wrapping at 2^32, a multiple of the ring's
//! power-of-two size) and are published with `Atomics.store` after the data
//! is written, so the reader sees whole data.

use js_sys::{Atomics, Float32Array, Int32Array, Uint8Array};
use wasm_bindgen::JsValue;

/// Control words (`Int32Array` at byte 0).
pub const OUT_WRITE: u32 = 0;
pub const OUT_READ: u32 = 1;
/// Frames the AudioWorklet played as silence for want of mixed ones, ever.
pub const UNDERRUN: u32 = 2;
/// Bumped and notified to wake the audio worker: by the AudioWorklet when
/// the ring of frames runs low, by the engine worker after commands.
pub const WAKE: u32 = 3;
pub const CMD_WRITE: u32 = 4;
pub const CMD_READ: u32 = 5;
const CONTROL_WORDS: u32 = 16;

/// Bytes of the ring of commands: many frames' worth (a frame's commands
/// for every voice take a few hundred bytes).
pub const CMD_BYTES: u32 = 1 << 18;
/// Frames of the ring of mixed audio.
pub const OUT_FRAMES: u32 = 4096;

/// The views of one output's shared memory.
pub struct Ring {
    pub control: Int32Array,
    pub commands: Uint8Array,
    pub frames: Float32Array,
}

impl Ring {
    pub fn new(buffer: &JsValue) -> Self {
        let commands_at = CONTROL_WORDS * 4;
        let frames_at = commands_at + CMD_BYTES;
        Self {
            control: Int32Array::new_with_byte_offset_and_length(buffer, 0, CONTROL_WORDS),
            commands: Uint8Array::new_with_byte_offset_and_length(buffer, commands_at, CMD_BYTES),
            frames: Float32Array::new_with_byte_offset_and_length(
                buffer,
                frames_at,
                OUT_FRAMES * 2,
            ),
        }
    }

    pub fn load(&self, word: u32) -> u32 {
        Atomics::load(&self.control, word).unwrap_or(0) as u32
    }

    pub fn store(&self, word: u32, value: u32) {
        let _ = Atomics::store(&self.control, word, value as i32);
    }

    /// Wakes the audio worker.
    pub fn wake(&self) {
        let _ = Atomics::add(&self.control, WAKE, 1);
        let _ = Atomics::notify(&self.control, WAKE);
    }

    /// Appends `bytes` to the ring of commands if they fit whole.
    pub fn write_commands(&self, bytes: &[u8]) -> bool {
        let (w, r) = (self.load(CMD_WRITE), self.load(CMD_READ));
        let free = CMD_BYTES - w.wrapping_sub(r);
        if bytes.len() as u32 > free {
            return false;
        }
        copy_in(&self.commands, CMD_BYTES, w, bytes, |v, s| v.copy_from(s));
        self.store(CMD_WRITE, w.wrapping_add(bytes.len() as u32));
        true
    }

    /// Takes every command written since the last call, appending to `out`.
    pub fn read_commands(&self, out: &mut Vec<u8>) {
        let (w, r) = (self.load(CMD_WRITE), self.load(CMD_READ));
        let n = w.wrapping_sub(r) as usize;
        if n == 0 {
            return;
        }
        let start = out.len();
        out.resize(start + n, 0);
        let at = r % CMD_BYTES;
        let first = n.min((CMD_BYTES - at) as usize);
        self.commands
            .subarray(at, at + first as u32)
            .copy_to(&mut out[start..start + first]);
        if first < n {
            self.commands
                .subarray(0, (n - first) as u32)
                .copy_to(&mut out[start + first..]);
        }
        self.store(CMD_READ, w);
    }

    /// Mixed frames the AudioWorklet has yet to play.
    pub fn queued(&self) -> u32 {
        self.load(OUT_WRITE).wrapping_sub(self.load(OUT_READ))
    }

    /// Appends mixed `samples` (stereo, interleaved; they must fit).
    pub fn write_frames(&self, samples: &[f32]) {
        let w = self.load(OUT_WRITE);
        copy_in(
            &self.frames,
            OUT_FRAMES * 2,
            (w % OUT_FRAMES) * 2,
            samples,
            |v, s| v.copy_from(s),
        );
        self.store(OUT_WRITE, w.wrapping_add((samples.len() / 2) as u32));
    }
}

/// A view type of a ring.
trait View {
    fn sub(&self, begin: u32, end: u32) -> Self;
}

impl View for Uint8Array {
    fn sub(&self, begin: u32, end: u32) -> Self {
        self.subarray(begin, end)
    }
}

impl View for Float32Array {
    fn sub(&self, begin: u32, end: u32) -> Self {
        self.subarray(begin, end)
    }
}

/// Copies `data` into the ring `view` of `len` elements from position `at`
/// (any position; taken modulo `len`), wrapping at its end.
fn copy_in<V: View, T>(view: &V, len: u32, at: u32, data: &[T], copy: impl Fn(&V, &[T])) {
    let at = at % len;
    let first = data.len().min((len - at) as usize);
    copy(&view.sub(at, at + first as u32), &data[..first]);
    if first < data.len() {
        copy(&view.sub(0, (data.len() - first) as u32), &data[first..]);
    }
}
