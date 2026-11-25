// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Page side: owns the DOM, starts the engine worker and talks to it only by
// message (docs/architecture.md).

import { blockZoom } from "./zoom.js";
import { startInput } from "./input.js";
import { createTouchControls } from "./touch-controls.js";
import { createFullscreenOffer } from "./fullscreen.js";
import { createTextEntry } from "./text-entry.js";
import { choose } from "./graphics.js";
// Early: Chrome offers installing the app once, soon after load.
import "./install.js";
import { startUpdates, versionText } from "./update.js";
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

/**
 * The Graphics choice (graphics.js), stored only when the player made one.
 * localStorage: the page alone reads it, and tells the engine worker.
 * Namespaced: the github.io origin is shared.
 */
const GRAPHICS = "volya-engine.graphics";
const storedGraphics = () => {
  try {
    return localStorage.getItem(GRAPHICS);
  } catch {
    return null;
  }
};

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
let setTextField = () => {};
let setCursor = () => {};
/** What the browser supports and the backend the engine started with (graphics.js). */
let available = {};
let backend = null;

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
  audioWorker.postMessage({ type: "connect", sampleRate, target, memory, engine: link.port1 }, [link.port1]);
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
addEventListener("pagehide", () => {
  const message = "the page was closed";
  diagnostics.keepEnd("page_closed", message);
  worker.postMessage({ type: "end", reason: "page_closed", message });
  worker.terminate();
  audioWorker?.terminate();
});
addEventListener("pageshow", (event) => event.persisted && location.reload());

// The engine sizes the canvas in device pixels from these. A canvas gives
// its drawing to one worker for good, and keeps the first kind of context it
// gives: an engine started anew gets a new one.
let view = document.getElementById("view");
const size = () => ({ width: view.clientWidth, height: view.clientHeight, dpr: devicePixelRatio });
const resized = () => worker.postMessage({ type: "resize", ...size() });
const sizes = new ResizeObserver(resized);

/** Starts an engine worker on the canvas; resolves once it runs. */
function startEngine() {
  worker = new Worker(new URL("./engine-worker.js", import.meta.url), { type: "module" });
  const started = new Promise((resolve, reject) => {
    worker.onmessage = ({ data }) => {
      if (data.type === "started") {
        ({ available, backend } = data);
        diagnostics.setStatus(data.diagnostics);
        resolve();
      } else if (data.type === "error") reject(new Error(data.message));
      else if (data.type === "capture") setCapture(data.on);
      else if (data.type === "controls") setLevel(data);
      else if (data.type === "text_field") setTextField(data);
      else if (data.type === "cursor") setCursor(data.shown);
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
  // The diagnostic log, when the player keeps one (#346), and how an
  // earlier session's page ended (#356).
  const logging = diagnostics.enabled();
  const log = logging ? diagnostics.session() : null;
  const previousEnd = logging ? diagnostics.takeKeptEnd() : null;
  worker.postMessage({ type: "start", canvas, size: size(), graphics: storedGraphics(), diagnostics: log, previousEnd }, [canvas]);
  if (document.visibilityState === "hidden") visibilityChanged();
  sizes.observe(view);
  return started;
}

/**
 * Starts the engine anew, on a new canvas, so the Graphics choice applies:
 * only outside a game (the app's own interface, where Settings is).
 */
async function restartEngine() {
  // The imported files' locks released first, for the next engine.
  await call("unmount_game").catch(() => {});
  // Why, in the diagnostic log (#356): handled before the call after it.
  worker.postMessage({ type: "end", reason: "restart", message: "the engine was restarted for the Graphics setting" });
  await call("status").catch(() => {});
  worker.terminate();
  for (const { reject } of pending.values()) reject(new Error("the engine was restarted"));
  pending.clear();
  output?.disconnect();
  sizes.unobserve(view);
  const fresh = document.createElement("canvas");
  fresh.id = "view";
  view.replaceWith(fresh);
  view = fresh;
  await startEngine();
  if (audio && isolated) output = await connectAudio(audio);
}

/**
 * The Graphics setting for the interface (game-ui.js): what the browser
 * supports, the stored choice, storing one, and applying it before the game
 * starts.
 */
/** The Graphics choice being applied (`graphics.apply`), while it is. */
let applying = null;
const graphics = {
  get available() {
    return available;
  },
  get chosen() {
    return storedGraphics();
  },
  choose(choice) {
    try {
      localStorage.setItem(GRAPHICS, choice);
    } catch (error) {
      console.warn(`graphics setting: ${error.message}`);
    }
  },
  /**
   * Starts the engine anew where the choice differs from the backend it
   * runs. One restart at a time: `backend` changes only once the new engine
   * runs, so a second restart would end the one awaited (#369).
   */
  apply() {
    applying ??= (async () => {
      if (choose(storedGraphics(), available) !== backend) await restartEngine();
    })().finally(() => (applying = null));
    return applying;
  },
};

globalThis.volya = { call, status: () => call("status"), connectAudio, audioState: () => audio?.state, diagnostics, graphics };

// Input goes to the engine only while it owns the screen (input.js); the
// page's own interface hides meanwhile.
const send = (event) => worker.postMessage({ type: "input", event });
// The device's keyboard for the save screen's name field on touch screens
// (#94), raised in the touchpad's click on it (#108).
const entry = createTextEntry(send);
const forward = startInput(() => view, send, { onClick: () => entry.click() });
setCursor = (shown) => forward.setCursor(shown);
const touch = createTouchControls(send, { call });
document.body.append(touch.element);
// Full screen with Keyboard Lock, offered in the level (#85).
const offer = createFullscreenOffer(() => call("bound_keys"));
document.body.append(offer.element);
document.body.append(entry.element);
setTextField = (field) => entry.update(field);
setCapture = (on) => {
  forward.setCapture(on);
  document.querySelector("main").hidden = on;
  if (!on) offer.leave();
};
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
  // One version: the build's time and commit (docs/architecture.md, Hosting).
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
  await import("./game-ui.js").then((ui) => ui.start(call, updates, graphics, { isolated }));
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
