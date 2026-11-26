// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The audio worker (docs/architecture/audio.md; #354): mixes on the
// output's own schedule, as an audio device mixes apart from the game's
// frames. It runs the engine's wasm build in an instance of its own
// (src/audio_host.rs) over the output's shared memory (audio-ring.js): it
// sleeps until woken (WAKE: the AudioWorklet when its ring of frames runs
// low, the engine worker after writing commands to the ring of commands),
// then applies the commands and fills the ring of frames, however long
// the engine's frames take. Messages from the page:
//   { type: "connect", sampleRate, target, memory, engine }: a new
//   output; `engine` is a port to the engine worker, which sends it the
//   files the commands play as `[number, Uint8Array]`, and to which it
//   posts its reports (audio_host_report). An earlier output gets nothing
//   more.

import { WAKE, ringViews } from "./audio-ring.js";
import init, * as wasm from "./pkg/volya_engine_web.js";

let ready = null;
/** The current output: `{ engine, control }`. */
let output = null;

// How long the worker sleeps at most (ms), well within the ring's target
// (30 ms): a wake that never arrives can't starve the output. On an iPhone
// the worker slept up to 15 s with the output starving and the page and
// the engine running (#452), until a file's message woke it; the wait's
// own timeout and a timer both end the sleep, so it ends with either.
const POLL_MS = 10;

/** Services `current` whenever woken, until another output replaces it. */
async function run(e, current) {
  let polled = false;
  while (output === current) {
    const seen = Atomics.load(current.control, WAKE);
    service(e, current, polled);
    const wait = Atomics.waitAsync(current.control, WAKE, seen, POLL_MS);
    if (!wait.async) {
      polled = false;
      continue;
    }
    polled = await new Promise((resolve) => {
      const timer = setTimeout(() => resolve(true), POLL_MS + 5);
      wait.value.then((result) => {
        clearTimeout(timer);
        resolve(result === "timed-out");
      });
    });
  }
}

/** `polled`: woken by a timeout, not a wake. */
function service(e, current, polled = false) {
  e.audio_host_service(polled);
  const report = e.audio_host_report();
  if (report !== undefined) current.engine.postMessage(report);
}

onmessage = async ({ data }) => {
  if (data.type !== "connect") return;
  ready ??= init().then(() => wasm);
  const e = await ready;
  const previous = output;
  previous?.engine.close();
  const current = { engine: data.engine, control: ringViews(data.memory).control };
  output = current;
  // The replaced output's loop ends.
  if (previous) Atomics.notify(previous.control, WAKE);
  e.audio_host_connect(data.sampleRate, data.target, data.memory);
  current.engine.onmessage = ({ data: [number, bytes] }) => {
    if (output !== current) return;
    e.audio_host_file(number, bytes);
    service(e, current);
  };
  run(e, current);
};
