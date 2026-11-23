// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Page side: owns the DOM, starts the engine worker and talks to it only by
// message (docs/architecture.md).

// Installable app: offline start and updates (sw.js). Registered relative to
// this module so the scope is the app's base path.
navigator.serviceWorker?.register(new URL("./sw.js", import.meta.url)).catch((error) => {
  console.warn(`service worker: ${error.message}`);
});

const worker = new Worker(new URL("./engine-worker.js", import.meta.url), { type: "module" });
const pending = new Map();
let nextId = 0;

const started = new Promise((resolve, reject) => {
  worker.onmessage = ({ data }) => {
    if (data.type === "started") resolve();
    else if (data.type === "error") reject(new Error(data.message));
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

globalThis.volya = { call, status: () => call("status") };

const canvas = document.getElementById("view").transferControlToOffscreen();
worker.postMessage({ type: "start", canvas }, [canvas]);

const status = document.getElementById("status");
try {
  await started;
  status.textContent = `Volya ${(await volya.status()).version}`;
  status.dataset.state = "ready";
  await import("./game-ui.js").then((ui) => ui.start(call));
} catch (error) {
  status.textContent = `Failed to start: ${error.message}`;
  status.dataset.state = "error";
  throw error;
}
