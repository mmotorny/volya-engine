// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Page side: owns the DOM, starts the engine worker and talks to it only by
// message (docs/architecture.md).

import { startInput } from "./input.js";
import { BUILD, startUpdateCheck } from "./update.js";

// Installable app: offline start and updates (sw.js, update.js). Registered
// relative to this module so the scope is the app's base path.
const registration = navigator.serviceWorker?.register(new URL("./sw.js", import.meta.url)).catch((error) => {
  console.warn(`service worker: ${error.message}`);
});

const worker = new Worker(new URL("./engine-worker.js", import.meta.url), { type: "module" });
const pending = new Map();
let nextId = 0;
// Set once the canvas exists, below; capture starts off.
let setCapture = () => {};

const started = new Promise((resolve, reject) => {
  worker.onmessage = ({ data }) => {
    if (data.type === "started") resolve();
    else if (data.type === "error") reject(new Error(data.message));
    else if (data.type === "capture") setCapture(data.on);
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

globalThis.volya = { call, status: () => call("status"), connectAudio };

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
const forward = startInput(view, (event) => worker.postMessage({ type: "input", event }));
setCapture = (on) => {
  forward(on);
  document.querySelector("main").hidden = on;
};
const canvas = view.transferControlToOffscreen();
worker.postMessage({ type: "start", canvas, size: size() }, [canvas]);
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
  const { version, gpu } = await volya.status();
  registration?.then((r) => r && startUpdateCheck(status.parentElement, r));
  const name = `Volya Engine ${version} (${BUILD})`;
  status.textContent = gpu.state === "ready" ? name : `${name} · graphics unavailable: ${gpu.message}`;
  status.dataset.state = "ready";
  // Browsers start audio suspended until a user gesture (iOS especially).
  const audio = new AudioContext();
  await connectAudio(audio);
  const resume = () => audio.state !== "running" && audio.resume();
  for (const event of ["pointerdown", "keydown", "touchend"]) addEventListener(event, resume);
  await import("./game-ui.js").then((ui) => ui.start(call));
} catch (error) {
  status.textContent = `Failed to start: ${error.message}`;
  status.dataset.state = "error";
  throw error;
}
