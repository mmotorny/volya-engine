// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Page side: owns the DOM, starts the engine worker and talks to it only by
// message (docs/architecture/web-platform.md).

import { blockZoom } from "./zoom.js";
import { startInput } from "./input.js";
import { TOUCH_MODES, createTouchControls } from "./touch-controls.js";
import { createFullscreenOffer, fullScreenSupported } from "./fullscreen.js";
import { createTextEntry } from "./text-entry.js";
import { createConsoleTouch } from "./console-touch.js";
// Early: Chrome offers installing the app once, soon after load.
import "./install.js";
import { saveSettings, settingsStore, startUpdates, versionText } from "./update.js";
import * as diagnostics from "./diagnostics.js";
import { ensureIsolated } from "./isolation.js";
import { createRing, ringTarget } from "./audio-ring.js";

// No browser zoom anywhere in the app, first (#345).
blockZoom();

// Installable app: offline start and updates (sw.js, update.js). Registered
// relative to this module so the scope is the app's base path.
const registration = navigator.serviceWorker?.register(new URL("./sw.js", import.meta.url)).catch((error) => {
  console.warn(`service worker: ${error.message}`);
});
const updates = navigator.serviceWorker ? startUpdates(registration) : null;
// Audio output needs a cross-origin isolated page (isolation.js): one
// reload when that makes it so, before anything starts; otherwise the
// interface says so (game-ui.js).
const isolation = await ensureIsolated({ registered: registration });
if (isolation === "reloading") await new Promise(() => {});
const isolated = isolation === "isolated";

let worker;
const pending = new Map();
diagnostics.connect((message) => worker.postMessage(message));
// Uncaught errors on the page end the engine as far as anyone knows (#356).
addEventListener("error", (event) =>
  diagnostics.end("page_error", `${event.message} (${event.filename}:${event.lineno}:${event.colno})`),
);
addEventListener("unhandledrejection", (event) =>
  diagnostics.end("page_error", `unhandled rejection: ${event.reason?.stack ?? event.reason?.message ?? event.reason}`),
);
let nextId = 0;
// Set once the canvas exists, below; capture starts off, and so do the
// level's controls.
let setCapture = () => {};
let setLevel = () => {};
let setTrip = () => {};
let setTextField = () => {};
let setCursor = () => {};
let setConsole = () => {};
let setFullScreen = () => {};
/** Tells the engine whether the page is in full screen, or can't be. */
const reportFullScreen = () =>
  worker?.postMessage({ type: "full_screen", on: Boolean(document.fullscreenElement), supported: fullScreenSupported() });
/** The running engine's build id (engine-worker.js). */
let engineBuild = null;

/** Engine interface (docs/testing.md): runs `op` with `args` in the engine. */
function call(op, args = {}) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    worker.postMessage({ type: "call", id, op, args });
  });
}

// The page's live audio context, once made (below), and its connection to
// the engine's mixer.
let audio = null;
let output = null;
/** Contexts holding the AudioWorklet's module (added once per context). */
const worklets = new WeakSet();
// Mixes on the output's schedule for the AudioWorklet, from the engine
// worker's commands (audio-worker.js); made with the first output.
let audioWorker = null;
/** Outputs connected so far, naming each to the audio worker. */
let outputs = 0;

/**
 * Connects an audio context (live or offline) to the engine's mixer: an
 * AudioWorklet playing what the audio worker mixes, which the engine worker
 * drives, all over the output's shared memory (audio-ring.js). Needs a
 * cross-origin isolated page.
 */
async function connectAudio(context) {
  if (!worklets.has(context)) {
    await context.audioWorklet.addModule(new URL("./audio-worklet.js", import.meta.url));
    worklets.add(context);
  }
  const memory = createRing();
  const sampleRate = context.sampleRate;
  const target = ringTarget(sampleRate);
  const node = new AudioWorkletNode(context, "volya-output", {
    numberOfInputs: 0,
    outputChannelCount: [2],
    processorOptions: { memory, target },
  });
  // The processor failed (its constructor too; errors it caught it reports itself).
  node.onprocessorerror = () => diagnostics.end("worklet_error", "the audio worklet's processor failed");
  node.connect(context.destination);
  audioWorker ??= new Worker(new URL("./audio-worker.js", import.meta.url), { type: "module" });
  const link = new MessageChannel();
  // The audio worker measures the time between its services only while
  // the output is expected to run (#496).
  const id = ++outputs;
  const running = () => document.visibilityState === "visible" && context.state === "running";
  const tell = () => audioWorker?.postMessage({ type: "running", id, on: running() });
  context.addEventListener("statechange", tell);
  document.addEventListener("visibilitychange", tell);
  audioWorker.postMessage({ type: "connect", id, sampleRate, target, memory, engine: link.port1, running: running() }, [link.port1]);
  worker.postMessage({ type: "audio", memory, port: link.port2, worklet: node.port, sampleRate }, [link.port2, node.port]);
  return node;
}

// Our design (#93): every browser throttles or suspends hidden pages, so the
// engine stops while the page is hidden (another tab, a minimised window, an
// app sent to the background): in a level the menu opens, the engine clock
// stands still, the worker's frame loop idles and audio is suspended; the
// player resumes from the menu. `visibilitychange` is the one event every
// browser fires on all of these (documented, MDN); Chrome's `freeze` only
// ever follows it (documented, Page Lifecycle API) and `pagehide` ends the
// worker anyway (below).
function visibilityChanged() {
  const hidden = document.visibilityState === "hidden";
  // The engine records the end and stores it (a hidden page may be ended
  // without notice); kept here too should the worker be ended first.
  if (hidden) diagnostics.keepEnd("page_hidden", "the page was hidden");
  else diagnostics.forgetEnd();
  worker.postMessage({ type: "visibility", hidden });
  if (!audio) return;
  if (hidden) audio.suspend().catch(() => {});
  // Resume unconditionally: a suspend just requested may not show in `state`
  // yet. iOS may refuse without a gesture; the next gesture resumes it (below).
  else audio.resume().catch(() => {});
}
document.addEventListener("visibilitychange", visibilityChanged);

// The engine worker holds OPFS file locks. Browsers keep left pages alive in
// the back-forward cache, which would keep those locks from the next page:
// end the worker when the page is hidden, and reload if it is restored.
// The worker is told why first, and the page keeps the end itself, as the
// worker may be ended before it stores it (#356).
addEventListener("pagehide", (event) => {
  const message = "the page was closed";
  // `persisted`: the page may be kept in the back-forward cache.
  diagnostics.event("pagehide", { persisted: event.persisted });
  diagnostics.keepEnd("page_closed", message);
  worker.postMessage({ type: "end", reason: "page_closed", message });
  worker.terminate();
  audioWorker?.terminate();
});
addEventListener("pageshow", (event) => {
  diagnostics.event("pageshow", { persisted: event.persisted });
  if (!event.persisted) return;
  diagnostics.restoredReload();
  location.reload();
});

// The engine sizes the canvas in device pixels from these.
const view = document.getElementById("view");
const size = () => ({ width: view.clientWidth, height: view.clientHeight, dpr: devicePixelRatio });
const resized = () => worker.postMessage({ type: "resize", ...size() });
const sizes = new ResizeObserver(resized);

/** Starts an engine worker on the canvas; resolves once it runs. */
function startEngine() {
  worker = new Worker(new URL("./engine-worker.js", import.meta.url), { type: "module" });
  const started = new Promise((resolve, reject) => {
    worker.onmessage = ({ data }) => {
      if (data.type === "started") {
        ({ build: engineBuild } = data);
        diagnostics.setStatus(data.diagnostics);
        resolve();
      } else if (data.type === "error") reject(new Error(data.message));
      else if (data.type === "capture") setCapture(data.on);
      else if (data.type === "controls") setLevel(data);
      else if (data.type === "trip") setTrip(data.at);
      else if (data.type === "text_field") setTextField(data);
      else if (data.type === "cursor") setCursor(data.shown);
      else if (data.type === "console") setConsole(data);
      else if (data.type === "full_screen") setFullScreen(data.on);
      else if (data.type === "result") {
        const { resolve, reject } = pending.get(data.id);
        pending.delete(data.id);
        data.error === undefined ? resolve(data.result) : reject(new Error(data.error));
      }
    };
    worker.onerror = (event) => {
      event.preventDefault();
      reject(new Error(`engine worker: ${event.message ?? "failed to load"}`));
    };
  });
  const canvas = view.transferControlToOffscreen();
  // The diagnostic log, always on (#346, #669), and how an earlier
  // session's page ended (#356).
  const logging = diagnostics.enabled();
  const log = logging ? diagnostics.session() : null;
  const previousEnd = logging ? diagnostics.takeKeptEnd() : null;
  worker.postMessage({ type: "start", canvas, size: size(), diagnostics: log, previousEnd }, [canvas]);
  reportFullScreen();
  if (document.visibilityState === "hidden") visibilityChanged();
  sizes.observe(view);
  return started;
}

globalThis.volya = {
  call,
  get touchControls() {
    return touchControls;
  },
  status: () => call("status"),
  connectAudio,
  audioState: () => audio?.state,
  diagnostics,
  updates,
  get engineBuild() {
    return engineBuild;
  },
};

// Input goes to the engine only while it owns the screen (input.js); the
// page's own interface hides meanwhile.
const send = (event) => worker.postMessage({ type: "input", event });
// The device's keyboard for the save screen's name field on touch screens
// (#94), raised in the lift of the touch controls' Click with the cursor on
// it (#856) or a direct click's lift there (#807).
const entry = createTextEntry(send);
const forward = startInput(() => view, send, { onClick: (at) => entry.click(at), screen: document.getElementById("screen") });
setCursor = (shown) => {
  forward.setCursor(shown);
  touch.setCursor({ shown });
};
// The console on touch screens: its Console buttons, the device's keyboard
// and a key strip (#642).
const consoleTouch = createConsoleTouch(send, forward.consoleKeys, { call });
setConsole = (state) => {
  forward.setConsole(state.open);
  consoleTouch.update(state);
  touch.setCursor({ console: state.open });
};
const touch = createTouchControls(send, { call, onConsole: () => consoleTouch.raise(), onClick: () => entry.click() });
// The touch controls' mode (#802): standard unless the app's settings
// (update.js) say otherwise; Settings changes it (game-ui.js), and the
// touch controls take it at once (in a level and on the screens with the
// cursor, #856).
const appSettings = globalThis.caches ? settingsStore(new URL("./", import.meta.url).href) : null;
let touchMode = "standard";
(async () => {
  const stored = await appSettings?.read().catch(() => ({}));
  if (TOUCH_MODES.includes(stored?.touch)) touchMode = stored.touch;
  forward.setTouchMode(touchMode);
  touch.setMode(touchMode);
})();
const touchControls = {
  get mode() {
    return touchMode;
  },
  /** Whether this device has touch, where the setting applies. */
  available: () => navigator.maxTouchPoints > 0 || matchMedia("(any-pointer: coarse)").matches,
  async set(mode) {
    if (!TOUCH_MODES.includes(mode)) throw new Error(`unknown touch controls: ${mode}`);
    touchMode = mode;
    forward.setTouchMode(mode);
    touch.setMode(mode);
    if (appSettings) await saveSettings(appSettings, { touch: mode });
  },
  /** The mode the level's controls run now. */
  get running() {
    return touch.element.dataset.mode;
  },
};
document.body.append(touch.element, ...consoleTouch.elements);
// Full screen with Keyboard Lock, offered in the level (#85).
const offer = createFullscreenOffer(() => call("bound_keys"));
document.body.append(offer.element);
document.body.append(entry.element);
setTextField = (field) => entry.update(field);
// The options screen's full screen (`rs_fullscreen`) is the page's.
setFullScreen = (on) => offer.setFullScreen(on, reportFullScreen);
document.addEventListener("fullscreenchange", reportFullScreen);
setCapture = (on) => {
  forward.setCapture(on);
  touch.setCursor({ capture: on });
  consoleTouch.setCapture(on);
  document.querySelector("main").hidden = on;
  if (!on) offer.leave();
};
setTrip = (at) => touch.trip(at);
setLevel = (controls) => {
  forward.setLevel(controls.level, controls.keys);
  touch.update(controls);
  offer.update(controls);
};
const started = startEngine();
// devicePixelRatio changes alone (e.g. moving to another display) resize nothing.
(function watchPixelRatio() {
  matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener(
    "change",
    () => {
      resized();
      watchPixelRatio();
    },
    { once: true },
  );
})();

const status = document.getElementById("status");
try {
  await started;
  // One version: the build's time and commit (docs/architecture/web-platform.md).
  const name = `Volya Engine · ${versionText()}`;
  // Missing graphics is explained in the page itself (game-ui.js).
  status.textContent = name;
  status.dataset.text = status.textContent;
  status.dataset.state = "ready";
  // Browsers start audio suspended until a user gesture (iOS especially).
  audio = new AudioContext();
  diagnostics.event("AudioContext state", { state: audio.state, sample_rate: audio.sampleRate });
  audio.addEventListener("statechange", () => diagnostics.event("AudioContext state", { state: audio.state }));
  // Made while hidden: suspended until shown.
  if (document.visibilityState === "hidden") audio.suspend().catch(() => {});
  // Not isolated, nothing plays: the interface says so before anything else.
  if (isolated) output = await connectAudio(audio);
  const resume = () => document.visibilityState === "visible" && audio.state !== "running" && audio.resume();
  for (const event of ["pointerdown", "keydown", "touchend"]) addEventListener(event, resume);
  await import("./game-ui.js").then((ui) => ui.start(call, updates, { isolated }));
} catch (error) {
  // The same panel as the interface's other problems, drawn without the engine.
  status.textContent = `Volya Engine · ${versionText()}`;
  status.dataset.state = "ready";
  await import("./game-ui.js")
    .then((ui) => ui.showProblem(error))
    .catch(() => {
      status.textContent = `Failed to start: ${error.message}`;
      status.dataset.state = "error";
    });
  throw error;
}
