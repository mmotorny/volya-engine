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

/** Services `current` whenever woken, until another output replaces it. */
async function run(e, current) {
  while (output === current) {
    const seen = Atomics.load(current.control, WAKE);
    service(e, current);
    const wait = Atomics.waitAsync(current.control, WAKE, seen);
    if (wait.async) await wait.value;
  }
}

function service(e, current) {
  e.audio_host_service();
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
