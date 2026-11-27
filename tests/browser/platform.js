// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Platform capabilities the engine-worker architecture relies on (#11).

import { assert, assertEqual } from "./harness.js";

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

/** Standard base64 of `bytes`. */
function base64Of(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

export const tests = {
  "wasm runs in a module worker": async () => {
    const status = await inWorker("wasm");
    assertEqual(status.state, "ready", "state");
  },

  // The renderer's name says whether WebGL runs on the GPU (ANGLE over
  // Metal on the Mac) or in software (SwiftShader).
  "WebGL 2 renders from a worker to an OffscreenCanvas": async () => {
    const el = document.createElement("canvas");
    el.width = el.height = 16;
    document.body.append(el);
    const canvas = el.transferControlToOffscreen();
    const { renderer, rgba } = await inWorker("webgl2", { canvas }, [canvas]);
    const expected = [255, 128, 0, 255];
    assert(
      rgba.every((v, i) => Math.abs(v - expected[i]) <= 2),
      `pixel ${rgba} != ${expected}`,
    );
    return renderer;
  },

  "engine OPFS VFS passes the VFS conformance check": async () => {
    await volya.call("vfs_selftest");
  },

  "script runtime passes its self-test in the engine worker": async () => {
    assertEqual(await volya.call("script_selftest"), "Lua 5.1 (alpha) + LuaJIT 1.0.3");
  },

  // Before any other world is made: the first scenario reads the values the
  // library loads with.
  "physics comparison's scenarios compute in the engine worker as natively": async () => {
    // Each scenario of tests/odeharness against the native arm64 build's
    // output (//tests/odeharness:platform_references), bit for bit.
    const names = await volya.call("physics_scenarios");
    const base = `${location.pathname.slice(0, location.pathname.indexOf("/__test/"))}/__test/reference/platform/odeharness/`;
    const differences = [];
    let words = 0;
    for (const [index, name] of names.entries()) {
      const response = await fetch(`${base}${name}.bin`);
      if (!response.ok) throw new Error(`${name}: no native output (//tests/odeharness:platform_references)`);
      const native = new Uint8Array(await response.arrayBuffer());
      const result = await volya.call("physics_scenario", { index, native_base64: base64Of(native) });
      words += result.words;
      if (result.difference) differences.push(`${name}: ${result.difference}`);
    }
    if (differences.length) throw new Error(differences.join("\n"));
    return `${names.length} scenarios, ${words} words identical`;
  },

  "physics solver steps a falling body as natively in the engine worker": async () => {
    // physics::ode::fall_check: within 0.1 mm of the closed form at each
    // step; 10 - 19.62 * 0.01^2 * 100 * 101 / 2 at the last.
    const y = await volya.call("physics_selftest");
    assert(Math.abs(y - 0.0919) < 1e-4, `y ${y}`);
  },

  "OPFS sync access handle works in a worker": async () => {
    assertEqual(await inWorker("opfs"), "volya");
  },
};
