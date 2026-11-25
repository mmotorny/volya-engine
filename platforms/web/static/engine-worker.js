// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The engine thread (docs/architecture.md). Messages from the page:
//   { type: "start", canvas, size, graphics }
//     → { type: "started", available, backend } | { type: "error", message }
//   (`graphics`: the stored Graphics choice or null; `available`: what the
//   browser supports, `backend`: the one chosen, graphics.js)
//   { type: "resize", width, height, dpr }: the canvas's CSS size and the
//   page's devicePixelRatio (`size` in "start" has the same fields).
//   { type: "call", id, op, args }  → { type: "result", id, result } | { type: "result", id, error }
//   { type: "audio", port, sampleRate } connects the page's AudioWorklet; it
//   then asks this worker for mixed blocks directly over `port`.
//   { type: "input", event }: one input event (format in src/input.rs).
//   { type: "visibility", hidden }: the page was hidden or shown again; the
//   engine stops while hidden (our design, #93).
// To the page, besides replies: { type: "capture", on } whenever the engine
// starts or stops owning the screen; the page forwards input only while on.
// { type: "controls", level, crouch, sprint, low, lock } whenever the
// level's controls go live or stop (level) or their toggles change (crouch
// toggle engaged, sprint requested, the toggled crouch low, the sprint lock
// holding): the page's touch controls and pointer lock follow it.
// { type: "text_field", rect, focused, over } whenever the save screen's
// name field shows, moves, gains or loses focus, the cursor comes over it or
// leaves it, or the field goes (rect null): canvas CSS pixels [x, y, width,
// height]; the page's text entry follows it (#94, #108).
// { type: "cursor", shown } whenever the game's mouse cursor shows or hides:
// the page's touch is a touchpad while it shows (#108).
// Once started, the worker runs a frame loop on requestAnimationFrame where
// workers have it, otherwise on a 60 Hz timer; it idles while the page is
// hidden, drawing nothing.

import init, * as wasm from "./pkg/volya_engine_web.js";
import { choose, supported } from "./graphics.js";

/** The engine's exports, once "start" initialized them. */
let engine = null;
/** The engine's exports; throws until they are loaded. */
const e = () => {
  if (!engine) throw new Error("the engine is not started");
  return engine;
};

const message = (error) => String(error?.message ?? error);

const nextFrame = self.requestAnimationFrame
  ? (callback) => self.requestAnimationFrame(callback)
  : (callback) => setTimeout(() => callback(performance.now()), 1000 / 60);

let capture = false;
let controls = 0;
let field = "";
let cursor = false;
/** Tells the page when input capture or the level's controls changed (after calls and frames). */
function syncCapture() {
  const on = e().input_capture();
  if (on !== capture) {
    capture = on;
    postMessage({ type: "capture", on });
  }
  const bits = e().level_controls();
  if (bits !== controls) {
    controls = bits;
    postMessage({
      type: "controls",
      level: (bits & 1) !== 0,
      crouch: (bits & 2) !== 0,
      sprint: (bits & 4) !== 0,
      low: (bits & 8) !== 0,
      lock: (bits & 16) !== 0,
      keys: (bits & 32) !== 0,
    });
  }
  const shown = e().cursor_shown();
  if (shown !== cursor) {
    cursor = shown;
    postMessage({ type: "cursor", shown });
  }
  const f = e().text_field();
  const key = f ? Array.from(f).join() : "";
  if (key !== field) {
    field = key;
    postMessage({
      type: "text_field",
      rect: f ? Array.from(f.subarray(0, 4)) : null,
      focused: f ? f[4] === 1 : false,
      over: f ? f[5] === 1 : false,
    });
  }
}

let hidden = false;
let scheduled = false;
let broken = false;
/** Asks for the next frame unless one is asked for, the page is hidden or the engine broke. */
function schedule() {
  if (scheduled || hidden || broken) return;
  scheduled = true;
  nextFrame(loop);
}

function loop(time) {
  scheduled = false;
  // Hidden after this frame was asked for: idle until shown.
  if (hidden) return;
  try {
    e().frame(time);
    syncCapture();
  } catch (error) {
    // A broken engine stops drawing; calls report their own errors.
    console.error(`frame: ${message(error)}`);
    broken = true;
    return;
  }
  schedule();
}

let started;
/** The port of the AudioWorklet pulling mixed blocks, once connected. */
let audioPort = null;

onmessage = async ({ data }) => {
  if (data.type === "start") {
    let available, backend;
    started = (async () => {
      available = await supported();
      backend = choose(data.graphics, available);
      await init();
      engine = wasm;
      await engine.start(data.canvas, backend === "webgl2");
      engine.resize(data.size.width, data.size.height, data.size.dpr);
    })();
    try {
      await started;
      postMessage({ type: "started", available, backend });
      schedule();
    } catch (error) {
      postMessage({ type: "error", message: message(error) });
    }
  } else if (data.type === "resize") {
    await started;
    e().resize(data.width, data.height, data.dpr);
  } else if (data.type === "audio") {
    e().audio_init(data.sampleRate);
    // One output pulls from the mixer: a replaced one gets nothing more.
    audioPort?.close();
    audioPort = data.port;
    data.port.onmessage = ({ data: request }) => {
      const block = e().audio_mix(request.need, request.underrun, request.queued);
      data.port.postMessage(block, [block.buffer]);
    };
  } else if (data.type === "visibility") {
    await started;
    hidden = data.hidden;
    try {
      e().visibility(hidden);
      syncCapture();
    } catch (error) {
      console.warn(`visibility: ${message(error)}`);
    }
    schedule();
  } else if (data.type === "input") {
    try {
      e().input(data.event);
    } catch (error) {
      console.warn(`input: ${message(error)}`);
    }
  } else if (data.type === "call") {
    let reply;
    try {
      reply = { type: "result", id: data.id, result: JSON.parse(await e().call(data.op, JSON.stringify(data.args ?? {}))) };
    } catch (error) {
      reply = { type: "result", id: data.id, error: message(error) };
    }
    // Before the reply, so a caller turning capture on finds the page forwarding.
    try {
      syncCapture();
    } catch {
      // The engine is not initialized: capture is still off.
    }
    postMessage(reply);
  }
};
