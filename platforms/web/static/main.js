// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Page side: owns the DOM, starts the engine worker and talks to it only by
// message (docs/architecture.md).

// `?test=<suite>` runs a browser test suite (tests/browser/).
const params = new URLSearchParams(location.search);
const suite = params.get("test");
const harness = suite ? await import("./__test/harness.js") : null;
harness?.install(params.get("run"));

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

/** Engine test interface (docs/testing.md): runs `op` in the engine. */
function call(op) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    worker.postMessage({ type: "call", id, op });
  });
}

globalThis.volya = { call, status: () => call("status") };

const canvas = document.getElementById("view").transferControlToOffscreen();
worker.postMessage({ type: "start", canvas }, [canvas]);

const status = document.getElementById("status");
let startupError = null;
try {
  await started;
  status.textContent = `Volya ${(await volya.status()).version}`;
  status.dataset.state = "ready";
} catch (error) {
  startupError = error;
  status.textContent = `Failed to start: ${error.message}`;
  status.dataset.state = "error";
}

if (harness) {
  await harness.run(suite, startupError);
} else if (startupError) {
  throw startupError;
}
