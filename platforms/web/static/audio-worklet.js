// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Audio output: plays the stereo frames the audio worker (audio-worker.js)
// mixes into a ring in shared memory (audio-ring.js, #354), reading on each
// render quantum without allocating, so nothing here waits for a message
// or a garbage collector. A quantum the ring can't fill plays the rest as
// silence and counts it (UNDERRUN). When the ring holds under the audio
// worker's target after a read, the processor wakes it.

import { OUT_FRAMES, OUT_READ, OUT_WRITE, UNDERRUN, WAKE, ringViews } from "./audio-ring.js";

const MASK = OUT_FRAMES - 1;

class VolyaOutput extends AudioWorkletProcessor {
  constructor({ processorOptions }) {
    super();
    ({ control: this.control, frames: this.frames } = ringViews(processorOptions.memory));
    // The fill under which the audio worker is woken: its target.
    this.low = processorOptions.target;
  }

  process(_inputs, outputs) {
    const [left, right = left] = outputs[0];
    const { control, frames } = this;
    const read = Atomics.load(control, OUT_READ);
    const queued = (Atomics.load(control, OUT_WRITE) - read) | 0;
    const n = Math.min(left.length, queued);
    for (let i = 0; i < n; i++) {
      const at = ((read + i) & MASK) * 2;
      left[i] = frames[at];
      right[i] = frames[at + 1];
    }
    for (let i = n; i < left.length; i++) left[i] = right[i] = 0;
    Atomics.store(control, OUT_READ, (read + n) | 0);
    if (n < left.length) Atomics.add(control, UNDERRUN, left.length - n);
    if (queued - n < this.low) {
      Atomics.add(control, WAKE, 1);
      Atomics.notify(control, WAKE);
    }
    return true;
  }
}

registerProcessor("volya-output", VolyaOutput);
