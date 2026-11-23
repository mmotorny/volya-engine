// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Platform capabilities the engine-worker architecture relies on (#11).

import { assert, assertEqual, env, skip } from "./harness.js";

// Verified: the iOS Simulator exposes navigator.gpu but never returns an
// adapter, on the main thread or in workers. WebGPU needs a real device.
function skipWithoutWebGpu() {
  if (env === "ios-simulator") skip("iOS Simulator has no WebGPU adapter; check on a real device");
}

/** Runs `op` in a fresh module worker and resolves with its result. */
function inWorker(op, extra = {}, transfer = []) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("./platform-worker.js", import.meta.url), { type: "module" });
    const done = (fn, value) => {
      clearTimeout(timer);
      worker.terminate();
      fn(value);
    };
    const timer = setTimeout(() => done(reject, new Error(`${op}: timeout`)), 15000);
    worker.onmessage = ({ data }) =>
      data.error ? done(reject, new Error(data.error)) : done(resolve, data.ok);
    worker.onerror = (e) => {
      e.preventDefault();
      done(reject, new Error(`worker error: ${e.message}`));
    };
    worker.postMessage({ op, ...extra }, transfer);
  });
}

export const tests = {
  "wasm runs in a module worker": async () => {
    const status = await inWorker("wasm");
    assertEqual(status.state, "ready", "state");
  },

  // Diagnostic: separates "no WebGPU here" from "no WebGPU in workers".
  "WebGPU adapter on the main thread": async () => {
    skipWithoutWebGpu();
    assert(navigator.gpu, "navigator.gpu missing");
    const adapter = await navigator.gpu.requestAdapter();
    assert(adapter, "no WebGPU adapter");
    return adapter.info?.vendor ?? "?";
  },

  "WebGPU renders from a worker to an OffscreenCanvas": async () => {
    skipWithoutWebGpu();
    const el = document.createElement("canvas");
    el.width = el.height = 16;
    document.body.append(el);
    const canvas = el.transferControlToOffscreen();
    const { format, vendor, rgba } = await inWorker("webgpu", { canvas }, [canvas]);
    const expected = [255, 128, 0, 255];
    assert(
      rgba.every((v, i) => Math.abs(v - expected[i]) <= 2),
      `pixel ${rgba} != ${expected}`,
    );
    return `${format}, ${vendor}`;
  },

  "engine OPFS VFS passes the VFS conformance check": async () => {
    await volya.call("vfs_selftest");
  },

  "OPFS sync access handle works in a worker": async () => {
    assertEqual(await inWorker("opfs"), "volya");
  },
};
