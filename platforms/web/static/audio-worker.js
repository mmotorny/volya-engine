// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The audio worker (docs/architecture.md, Audio; #354): mixes on the
// output's own schedule, as an audio device mixes apart from the game's
// frames. It runs the engine's wasm build in an instance of its own
// (src/audio_host.rs): the engine worker sends it, after each of its frames,
// the mixer's commands and the compressed files they play; the AudioWorklet
// asks it for mixed blocks, which it answers at once, however long the
// engine's frames take. Messages from the page:
//   { type: "connect", build, sampleRate, worklet, engine }: a new output.
//   `worklet` is the AudioWorklet's port, `engine` a port to the engine
//   worker; any earlier output gets nothing more.
// From `engine`: { commands, files } (audio_take). To `engine`: a report
// string (audio_host_report) whenever there is news.

import { loadEngine } from "./wasm-build.js";

let ready = null;
let link = null;

onmessage = async ({ data }) => {
  if (data.type !== "connect") return;
  ready ??= loadEngine(data.build).then(({ module }) => module);
  const e = await ready;
  link?.worklet.close();
  link?.engine.close();
  link = data;
  const { worklet, engine } = data;
  e.audio_host_reset(data.sampleRate);
  // Messages wait in the ports until these are set.
  const report = () => {
    const news = e.audio_host_report();
    if (news !== undefined) engine.postMessage(news);
  };
  engine.onmessage = ({ data: message }) => {
    try {
      e.audio_host_apply(message.commands, message.files);
    } catch (error) {
      console.error(`audio: ${error?.message ?? error}`);
    }
    report();
  };
  worklet.onmessage = ({ data: request }) => {
    const block = e.audio_host_mix(request.need, request.underrun, request.queued);
    worklet.postMessage(block, [block.buffer]);
    report();
  };
};
