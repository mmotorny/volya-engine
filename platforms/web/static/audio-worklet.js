// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Audio output: plays stereo blocks mixed by the audio worker
// (audio-worker.js), which talks to this processor directly over its port.
// Under the target it asks, one request at a time, for what tops the queue
// up to the target plus a request's worth plus what plays while the reply
// is on its way (the last reply's wait, at most the target). The audio
// worker mixes on this schedule, not between the engine's frames (#354), so
// replies come within milliseconds and the target only has to cover its
// scheduling. Each request also says how many frames played as silence for
// want of mixed ones since the last (underrun) and how many are queued.

const TARGET_SECONDS = 0.06;
const REQUEST_SECONDS = 0.02;

class VolyaOutput extends AudioWorkletProcessor {
  constructor() {
    super();
    this.queue = []; // Float32Array blocks, interleaved stereo
    this.offset = 0; // in the first block, in samples
    this.queued = 0; // frames
    this.underrun = 0; // frames
    this.pending = false;
    this.sent = 0; // currentFrame when the pending request was sent
    this.wait = 0; // frames the last reply took
    this.port.onmessage = ({ data }) => {
      this.queue.push(data);
      this.queued += data.length / 2;
      this.pending = false;
      this.wait = currentFrame - this.sent;
    };
  }

  process(_inputs, outputs) {
    const [left, right = left] = outputs[0];
    for (let i = 0; i < left.length; i++) {
      const block = this.queue[0];
      if (!block) {
        left[i] = right[i] = 0; // underrun: silence
        this.underrun++;
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
    const target = sampleRate * TARGET_SECONDS;
    if (!this.pending && this.queued < target) {
      this.pending = true;
      this.sent = currentFrame;
      const need = Math.round(target + sampleRate * REQUEST_SECONDS + Math.min(this.wait, target) - this.queued);
      this.port.postMessage({ need, underrun: this.underrun, queued: this.queued });
      this.underrun = 0;
    }
    return true;
  }
}

registerProcessor("volya-output", VolyaOutput);
