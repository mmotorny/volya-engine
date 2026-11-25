// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Audio output: plays stereo blocks mixed by the engine worker, which talks to
// this processor directly over its port. Under the target it asks, one
// request at a time, for what tops the queue up to the target plus a
// request's worth plus what plays while the reply is on its way (the last
// reply's wait, at most the target): the worker answers between its frames,
// so a reply has to last a whole frame however long frames are (#343: a
// fixed 50 ms per reply starved the output once frames took longer). Each
// request also says how many frames played as silence for want of mixed
// ones since the last (underrun) and how many are queued; for the diagnostic
// log (#346), also the last reply's wait, the queue's least, summed and
// greatest fill over the process calls since the last request, and whether
// the last reply left it over-full (overrun: over OVERFULL_SECONDS). An
// error here is sent as `{ error }` before it ends the processor, for the
// log's `engine.end`.

const TARGET_SECONDS = 0.15;
const REQUEST_SECONDS = 0.05;
// More than the most a reply asks for (TARGET + REQUEST + TARGET).
const OVERFULL_SECONDS = 0.5;

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
    this.overrun = 0; // replies that left the queue over-full
    this.resetFill();
    this.port.onmessage = ({ data }) => {
      try {
        this.queue.push(data);
        this.queued += data.length / 2;
        this.pending = false;
        this.wait = currentFrame - this.sent;
        if (this.queued > sampleRate * OVERFULL_SECONDS) this.overrun++;
      } catch (error) {
        this.report(error);
        throw error;
      }
    };
  }

  /** Tells the engine worker of an error (its diagnostic log). */
  report(error) {
    try {
      this.port.postMessage({ error: String(error?.stack ?? error) });
    } catch {
      // The port is gone.
    }
  }

  process(inputs, outputs) {
    try {
      return this.play(inputs, outputs);
    } catch (error) {
      this.report(error);
      throw error;
    }
  }

  resetFill() {
    this.fillMin = Infinity;
    this.fillMax = 0;
    this.fillSum = 0;
    this.fillN = 0;
  }

  play(_inputs, outputs) {
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
    this.fillMin = Math.min(this.fillMin, this.queued);
    this.fillMax = Math.max(this.fillMax, this.queued);
    this.fillSum += this.queued;
    this.fillN++;
    const target = sampleRate * TARGET_SECONDS;
    if (!this.pending && this.queued < target) {
      this.pending = true;
      this.sent = currentFrame;
      const need = Math.round(target + sampleRate * REQUEST_SECONDS + Math.min(this.wait, target) - this.queued);
      this.port.postMessage({
        need,
        underrun: this.underrun,
        queued: this.queued,
        wait: this.wait,
        fill: [this.fillMin, this.fillSum, this.fillMax, this.fillN],
        overrun: this.overrun,
      });
      this.underrun = 0;
      this.overrun = 0;
      this.resetFill();
    }
    return true;
  }
}

registerProcessor("volya-output", VolyaOutput);
