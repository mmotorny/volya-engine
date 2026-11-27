// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The engine thread (docs/architecture/web-platform.md). Messages from the page:
//   { type: "start", canvas, size, diagnostics, previousEnd }
//     → { type: "started", diagnostics, build } | { type: "error", message }
//   (`diagnostics`: the session as diagnostics.js tells it when the player keeps a
//   diagnostic log, else null, and in the reply the log's status;
//   `previousEnd`: how an earlier session's page ended, kept by
//   diagnostics.js; `build`: the engine's build id, stamped by the site's build (tools/site))
//   { type: "diag", kind, name, … }: the page's events for the diagnostic
//   log (diagnostics.js), dropped while it is off.
//   { type: "end", reason, message }: how the page ended (an uncaught
//   error, closed), recorded as `engine.end` and stored at once.
//   { type: "resize", width, height, dpr }: the canvas's CSS size and the
//   page's devicePixelRatio (`size` in "start" has the same fields).
//   { type: "call", id, op, args }  → { type: "result", id, result } | { type: "result", id, error }
//   { type: "audio", memory, port, sampleRate } connects a new output:
//   `memory` is its shared memory (audio-ring.js), `port` goes to the audio
//   worker (audio-worker.js), which mixes for the AudioWorklet. After every
//   frame, call and visibility change this worker writes the mixer's
//   commands to the ring of commands and posts the files they play to
//   `port`; it takes the audio worker's reports from it. `worklet` is the
//   AudioWorklet's port, which reports its errors.
//   { type: "input", event }: one input event (format in src/input.rs).
//   { type: "visibility", hidden }: the page was hidden or shown again; the
//   engine stops while hidden (our design, #93).
// To the page, besides replies: { type: "capture", on } whenever the engine
// starts or stops owning the screen; the page forwards input only while on.
// { type: "controls", level, crouch, sprint, low, lock, keys, torch,
// nightVision, pda } whenever the level's controls go live or stop (level)
// or their toggles change (crouch toggle engaged, sprint requested, the
// toggled crouch low, the sprint lock holding; the torch and night vision
// on, or null while their touch buttons are hidden): the page's touch
// controls and pointer lock follow it.
// { type: "trip", at } whenever a tap-to-move trip's destination moves in
// the picture (from its centre, in half its height, +y down), or goes
// (null): the touch controls mark it (#772).
// { type: "text_field", rect, focused, over } whenever the save screen's
// name field shows, moves, gains or loses focus, the cursor comes over it or
// leaves it, or the field goes (rect null): canvas CSS pixels [x, y, width,
// height]; the page's text entry follows it (#94, #108).
// { type: "cursor", shown } whenever the game's mouse cursor shows or hides:
// the page's touch controls drive the cursor while it shows (#856, #807).
// { type: "console", open, menu } whenever the console opens or closes,
// or whether it would open over the main menu changes: the page reads the
// clipboard for Insert while it is open (#639) and shows its Console
// button for touch screens over the menu (#642).
// { type: "full_screen", on } when a video restart asks the page to enter
// or leave the browser's full screen (`rs_fullscreen`); the page answers,
// as at start and whenever its full screen changes, with
// { type: "full_screen", on, supported }.
// Once started, the worker runs a frame loop on requestAnimationFrame where
// workers have it, otherwise on a 60 Hz timer; it idles while the page is
// hidden, drawing nothing.

import init, * as wasm from "./pkg/volya_engine_web.js";

/** The engine's exports, once "start" initialized them. */
let engine = null;
/** The engine's exports; throws until they are loaded. */
const e = () => {
  if (!engine) throw new Error("the engine is not started");
  return engine;
};

const message = (error) => String(error?.message ?? error);

/**
 * The engine's compiled wasm, once it is known to be the glue's build
 * (the site's build (`tools/site`) stamps both with one id): mixed, the glue would call
 * exports that aren't there.
 */
async function engineModule() {
  const module = await WebAssembly.compileStreaming(fetch(new URL("./pkg/volya_engine_web_bg.wasm", import.meta.url)));
  const [section] = WebAssembly.Module.customSections(module, "volya-engine-build");
  const id = section ? new TextDecoder().decode(section) : "none";
  if (id !== wasm.BUILD_ID) throw new Error(`the engine's code and wasm come from different builds (${wasm.BUILD_ID ?? "none"}, ${id})`);
  return module;
}

/**
 * Records how the engine ended in the diagnostic log (`engine.end`, stored
 * at once; nothing while it is off), on thread `tid` (1: this worker, 2:
 * the page, 3: the audio worklet).
 */
function ended(reason, text, tid = 1) {
  try {
    engine?.diagnostics_end(reason, String(text), tid);
  } catch {
    // The engine can't record any more.
  }
}
// Uncaught errors here end the engine as far as anyone knows.
addEventListener("error", (event) =>
  ended("worker_error", `${event.message} (${event.filename}:${event.lineno}:${event.colno})`),
);
addEventListener("unhandledrejection", (event) =>
  ended("worker_error", `unhandled rejection: ${event.reason?.stack ?? message(event.reason)}`),
);

const nextFrame = self.requestAnimationFrame
  ? (callback) => self.requestAnimationFrame(callback)
  : (callback) => setTimeout(() => callback(performance.now()), 1000 / 60);

let capture = false;
let controls = 0;
let field = "";
let cursor = false;
let consoleOpen = false;
let consoleMenu = false;
let trip = "";
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
      torch: bits & 64 ? (bits & 128) !== 0 : null,
      nightVision: bits & 256 ? (bits & 512) !== 0 : null,
      pda: (bits & 1024) !== 0,
    });
  }
  const at = e().trip_marker();
  const place = at ? Array.from(at, (v) => v.toFixed(4)).join() : "";
  if (place !== trip) {
    trip = place;
    postMessage({ type: "trip", at: at ? Array.from(at) : null });
  }
  const full = e().full_screen_request();
  if (full >= 0) postMessage({ type: "full_screen", on: full === 1 });
  const shown = e().cursor_shown();
  if (shown !== cursor) {
    cursor = shown;
    postMessage({ type: "cursor", shown });
  }
  const open = e().console_open();
  const menu = e().console_opens_over_menu();
  if (open !== consoleOpen || menu !== consoleMenu) {
    consoleOpen = open;
    consoleMenu = menu;
    postMessage({ type: "console", open, menu });
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

let started;
/** The port to the audio worker, once an output is connected. */
let audioPort = null;
/** Sends the audio worker the mixer's commands since the last time, and the files they play. */
function flushAudio() {
  if (!audioPort || !engine) return;
  const files = engine.audio_flush();
  if (files) for (const file of files) audioPort.postMessage(file, [file[1].buffer]);
}

function loop(time) {
  scheduled = false;
  // Hidden after this frame was asked for: idle until shown.
  if (hidden) return;
  try {
    e().frame(time);
    flushAudio();
    syncCapture();
  } catch (error) {
    // A broken engine stops drawing; calls report their own errors.
    console.error(`frame: ${message(error)}`);
    broken = true;
    ended("engine_error", error?.stack ?? message(error));
    return;
  }
  schedule();
}

/** Whether the diagnostic log is on (the `diagnostics_set` op's result). */
let logging = false;

onmessage = async ({ data }) => {
  if (data.type === "start") {
    let diagnostics = null;
    started = (async () => {
      await init({ module_or_path: await engineModule() });
      engine = wasm;
      // The player's setting, before anything else runs (#346).
      if (data.diagnostics) {
        const args = { on: true, session: data.diagnostics, previous_end: data.previousEnd };
        diagnostics = JSON.parse(await engine.call("diagnostics_set", JSON.stringify(args)));
        logging = diagnostics.on;
      }
      await engine.start(data.canvas);
      engine.resize(data.size.width, data.size.height, data.size.dpr);
    })();
    try {
      await started;
      postMessage({ type: "started", diagnostics, build: wasm.BUILD_ID });
      schedule();
    } catch (error) {
      postMessage({ type: "error", message: message(error) });
    }
  } else if (data.type === "resize") {
    await started;
    e().resize(data.width, data.height, data.dpr);
  } else if (data.type === "follow_screen") {
    // The canvas's size as the mode lays it out, then the mode (#806).
    await started;
    e().resize(data.width, data.height, data.dpr);
    e().follow_screen(data.on);
  } else if (data.type === "audio") {
    e().audio_init(data.sampleRate, data.memory);
    // One output: a replaced one gets nothing more.
    audioPort?.close();
    audioPort = data.port;
    data.port.onmessage = ({ data: report }) => {
      try {
        e().audio_report(report);
      } catch (error) {
        console.warn(`audio: ${message(error)}`);
      }
    };
    // The worklet's processor failed (audio-worklet.js).
    data.worklet.onmessage = ({ data: { error } }) => ended("worklet_error", error, 3);
    flushAudio();
  } else if (data.type === "visibility") {
    await started;
    hidden = data.hidden;
    try {
      e().visibility(hidden);
      flushAudio();
      syncCapture();
    } catch (error) {
      console.warn(`visibility: ${message(error)}`);
    }
    schedule();
  } else if (data.type === "full_screen") {
    await started;
    try {
      e().full_screen(data.on, data.supported);
    } catch (error) {
      console.warn(`full screen: ${message(error)}`);
    }
  } else if (data.type === "end") {
    ended(data.reason, data.message, 2);
  } else if (data.type === "diag") {
    // The page's events for the diagnostic log (diagnostics.js).
    if (!logging) return;
    try {
      if (data.kind === "event") e().diagnostics_event(data.name, 2, JSON.stringify(data.args ?? {}));
      else if (data.kind === "metrics") e().diagnostics_metrics(data.name, data.n, data.min, data.sum, data.max);
      else if (data.kind === "count") e().diagnostics_count(data.name, data.n);
    } catch (error) {
      console.warn(`diagnostics: ${message(error)}`);
    }
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
      // Captures carry their pixels as base64 (gpu.rs, Capture), the logs
      // their archive (lib.rs, logs): handed over as bytes.
      for (const key of ["rgba", "zip"]) {
        if (typeof reply.result?.[`${key}_base64`] !== "string") continue;
        const text = atob(reply.result[`${key}_base64`]);
        const bytes = new Uint8Array(text.length);
        for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
        delete reply.result[`${key}_base64`];
        reply.result[key] = bytes;
      }
      if (data.op === "diagnostics_set") logging = reply.result.on;
    } catch (error) {
      reply = { type: "result", id: data.id, error: message(error) };
    }
    // Before the reply, so what the call played is on its way.
    flushAudio();
    // Before the reply, so a caller turning capture on finds the page forwarding.
    try {
      syncCapture();
    } catch {
      // The engine is not initialized: capture is still off.
    }
    const bytes = reply.result?.rgba ?? reply.result?.zip;
    postMessage(reply, bytes instanceof Uint8Array ? [bytes.buffer] : []);
  }
};
