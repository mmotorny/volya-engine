// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Audio output: plays stereo blocks mixed by the engine worker, which talks to
// this processor directly over its port. Asks for more when under the target.

const TARGET_SECONDS = 0.15;
const REQUEST_SECONDS = 0.05;

class VolyaOutput extends AudioWorkletProcessor {
  constructor() {
    super();
    this.queue = []; // Float32Array blocks, interleaved stereo
    this.offset = 0; // in the first block, in samples
    this.queued = 0; // frames
    this.pending = false;
    this.port.onmessage = ({ data }) => {
      this.queue.push(data);
      this.queued += data.length / 2;
      this.pending = false;
    };
  }

  process(_inputs, outputs) {
    const [left, right = left] = outputs[0];
    for (let i = 0; i < left.length; i++) {
      const block = this.queue[0];
      if (!block) {
        left[i] = right[i] = 0; // underrun: silence
        continue;
      }
      left[i] = block[this.offset];
      right[i] = block[this.offset + 1];
      this.offset += 2;
      this.queued--;
      if (this.offset >= block.length) {
        this.queue.shift();
        this.offset = 0;
      }
    }
    if (!this.pending && this.queued < sampleRate * TARGET_SECONDS) {
      this.pending = true;
      this.port.postMessage({ need: Math.round(sampleRate * REQUEST_SECONDS) });
    }
    return true;
  }
}

registerProcessor("volya-output", VolyaOutput);
