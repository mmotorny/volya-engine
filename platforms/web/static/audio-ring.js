// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The audio output's shared memory (#354), as src/audio_ring.rs lays it
// out: control words, the ring of mixer commands (engine worker → audio
// worker) and the ring of mixed stereo frames (audio worker → AudioWorklet).

export const OUT_WRITE = 0;
export const OUT_READ = 1;
export const UNDERRUN = 2;
export const WAKE = 3;
const CONTROL_WORDS = 16;
const CMD_BYTES = 1 << 18;
export const OUT_FRAMES = 4096;

// The ring of frames' fill the audio worker keeps: its share of the
// output's latency, and what covers the audio worker's waking.
const TARGET_SECONDS = 0.03;

/** The fill kept at `sampleRate`, frames. */
export const ringTarget = (sampleRate) => Math.round(sampleRate * TARGET_SECONDS);

/** A new output's shared memory. */
export function createRing() {
  return new SharedArrayBuffer(CONTROL_WORDS * 4 + CMD_BYTES + OUT_FRAMES * 2 * 4);
}

/** The views of `memory`: `{ control, frames }`. */
export function ringViews(memory) {
  return {
    control: new Int32Array(memory, 0, CONTROL_WORDS),
    frames: new Float32Array(memory, CONTROL_WORDS * 4 + CMD_BYTES, OUT_FRAMES * 2),
  };
}
