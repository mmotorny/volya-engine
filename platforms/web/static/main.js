// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Page side: owns the DOM, starts the engine worker and talks to it only by
// message (docs/architecture.md).

import { startInput } from "./input.js";
import { createTouchControls } from "./touch-controls.js";
import { createFullscreenOffer } from "./fullscreen.js";
import { createTextEntry } from "./text-entry.js";
// Early: Chrome offers installing the app once, soon after load.
import "./install.js";
import { startUpdates, versionText } from "./update.js";

// Installable app: offline start and updates (sw.js, update.js). Registered
// relative to this module so the scope is the app's base path.
const registration = navigator.serviceWorker?.register(new URL("./sw.js", import.meta.url)).catch((error) => {
  console.warn(`service worker: ${error.message}`);
});
const updates = navigator.serviceWorker ? startUpdates(registration) : null;

const worker = new Worker(new URL("./engine-worker.js", import.meta.url), { type: "module" });
const pending = new Map();
let nextId = 0;
// Set once the canvas exists, below; capture starts off, and so do the
// level's controls.
let setCapture = () => {};
let setLevel = () => {};
let setTextField = () => {};
let setCursor = () => {};

const started = new Promise((resolve, reject) => {
  worker.onmessage = ({ data }) => {
    if (data.type === "started") resolve();
    else if (data.type === "error") reject(new Error(data.message));
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

/** Engine interface (docs/testing.md): runs `op` with `args` in the engine. */
function call(op, args = {}) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    worker.postMessage({ type: "call", id, op, args });
  });
}

/**
 * Connects an audio context (live or offline) to the engine's mixer through
 * an AudioWorklet whose port goes straight to the engine worker.
 */
async function connectAudio(context) {
  await context.audioWorklet.addModule(new URL("./audio-worklet.js", import.meta.url));
  const node = new AudioWorkletNode(context, "volya-output", { numberOfInputs: 0, outputChannelCount: [2] });
  node.connect(context.destination);
  worker.postMessage({ type: "audio", port: node.port, sampleRate: context.sampleRate }, [node.port]);
  return node;
}

// The page's live audio context, once made (below).
let audio = null;

globalThis.volya = { call, status: () => call("status"), connectAudio, audioState: () => audio?.state };

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
addEventListener("pagehide", () => worker.terminate());
addEventListener("pageshow", (event) => event.persisted && location.reload());

// The engine sizes the canvas in device pixels from these.
const view = document.getElementById("view");
const size = () => ({ width: view.clientWidth, height: view.clientHeight, dpr: devicePixelRatio });
// Input goes to the engine only while it owns the screen (input.js); the
// page's own interface hides meanwhile.
const send = (event) => worker.postMessage({ type: "input", event });
// The device's keyboard for the save screen's name field on touch screens
// (#94), raised in the touchpad's click on it (#108).
const entry = createTextEntry(send);
const forward = startInput(view, send, { onClick: () => entry.click() });
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
const canvas = view.transferControlToOffscreen();
worker.postMessage({ type: "start", canvas, size: size() }, [canvas]);
if (document.visibilityState === "hidden") visibilityChanged();
const resized = () => worker.postMessage({ type: "resize", ...size() });
new ResizeObserver(resized).observe(view);
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
  // Made while hidden: suspended until shown.
  if (document.visibilityState === "hidden") audio.suspend().catch(() => {});
  await connectAudio(audio);
  const resume = () => document.visibilityState === "visible" && audio.state !== "running" && audio.resume();
  for (const event of ["pointerdown", "keydown", "touchend"]) addEventListener(event, resume);
  await import("./game-ui.js").then((ui) => ui.start(call, updates));
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
