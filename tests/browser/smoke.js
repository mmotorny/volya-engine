// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import { assert, assertEqual } from "./harness.js";

// The app's base path (tests are served under it, in __test/).
const base = location.pathname.slice(0, location.pathname.indexOf("/__test/") + 1);

/** The `[path, hash]` list injected into a deployed sw.js. */
const precacheOf = (source) => JSON.parse(source.match(/const PRECACHE = (\[.*?\]);/s)[1]);

export const tests = {
  "engine reports ready": async () => {
    const status = await volya.status();
    assertEqual(status.state, "ready", "state");
    assert(/^\d+\.\d+\.\d+$/.test(status.version), `version ${status.version}`);
  },

  "engine runs in a worker": async () => {
    assertEqual((await volya.status()).thread, "worker");
  },

  "status is shown": async () => {
    const el = document.getElementById("status");
    assertEqual(el.dataset.state, "ready", "data-state");
    const { version, gpu } = await volya.status();
    const { BUILD } = await import(`${base}update.js`);
    assert(/^[0-9a-f]{7,}\+?$/.test(BUILD), `build ${BUILD}`);
    // Without WebGPU the line says why graphics are unavailable.
    const name = `Volya Engine ${version} (${BUILD})`;
    const expected = gpu.state === "ready" ? name : `${name} · graphics unavailable: ${gpu.message}`;
    assertEqual(el.textContent, expected, "text");
  },

  "a waiting new version is offered, and takes over only when asked": async () => {
    const { startUpdateCheck } = await import(`${base}update.js`);
    const registration = Object.assign(new EventTarget(), { waiting: null, installing: null, updates: 0 });
    registration.update = async () => registration.updates++;
    const container = Object.assign(new EventTarget(), { controller: {} });
    const footer = document.createElement("footer");
    const { button, check } = startUpdateCheck(footer, registration, container);
    assertEqual(button.parentElement, footer, "placement");
    await check();
    assertEqual(registration.updates, 1, "asks the browser for an update");
    assert(button.hidden, "offered without a new version");
    // A new version installs and waits.
    const worker = Object.assign(new EventTarget(), { state: "installing", messages: [] });
    worker.postMessage = (m) => worker.messages.push(m);
    registration.installing = worker;
    registration.dispatchEvent(new Event("updatefound"));
    registration.installing = null;
    registration.waiting = worker;
    worker.state = "installed";
    worker.dispatchEvent(new Event("statechange"));
    assert(!button.hidden, "not offered once a new version waits");
    assertEqual(worker.messages.length, 0, "took over without being asked");
    // Tapping asks it to take over (the reload follows its controllerchange).
    button.click();
    assertEqual(JSON.stringify(worker.messages), '[{"type":"take-over"}]', "messages");
    // A first install (no controller yet) is not offered: it takes over itself.
    const first = startUpdateCheck(document.createElement("footer"), registration, Object.assign(new EventTarget(), { controller: null }));
    assert(first.button.hidden, "offered on first install");
  },

  // GitHub Pages cannot send COOP/COEP; the engine must not depend on them.
  "page is not cross-origin isolated": () => {
    assertEqual(self.crossOriginIsolated, false);
  },

  "service worker controls the app scope": async () => {
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 10000));
    const registration = await Promise.race([navigator.serviceWorker.ready, timeout]);
    assert(registration.scope.endsWith("/volya-engine/"), `scope ${registration.scope}`);
  },

  "the whole app works offline once installed": async () => {
    const registration = await navigator.serviceWorker.ready;
    // The page must be controlled to use the kept copies.
    if (!navigator.serviceWorker.controller) {
      await new Promise((resolve) => navigator.serviceWorker.addEventListener("controllerchange", resolve, { once: true }));
    }
    assert(registration.active, "no active service worker");
    const files = precacheOf(await (await fetch(`${base}sw.js`, { cache: "no-store" })).text()).map(([path]) => path);
    assert(files.includes("./") && files.includes("index.html") && files.some((f) => f.endsWith(".wasm")), `files ${files.join(" ")}`);
    const offline = (on) => fetch(`${base}__test/offline/${on ? "on" : "off"}`, { method: "POST" });
    await offline(true);
    try {
      const unreachable = await fetch(`${base}not-in-the-build`).then(() => false, () => true);
      assert(unreachable, "the network is still reachable");
      const missing = [];
      for (const file of files) {
        const response = await fetch(`${base}${file === "./" ? "" : file}`).catch(() => null);
        if (!response?.ok || (await response.arrayBuffer()).byteLength === 0) missing.push(file);
      }
      assertEqual(missing.join(" "), "", "files unavailable offline");
    } finally {
      await offline(false);
    }
  },

  "an update downloads only changed files, waits, and drops removed ones": async () => {
    const source = await (await fetch(`${base}sw.js`, { cache: "no-store" })).text();
    const scope = "https://example.test/app/";
    const store = new Map();
    let downloaded = [];
    const cache = {
      match: async (key) => store.get(typeof key === "string" ? key : key.url)?.clone(),
      put: async (key, response) => store.set(typeof key === "string" ? key : key.url, response),
      addAll: async (requests) => {
        downloaded.push(...requests.map((r) => r.url.slice(scope.length)));
        for (const r of requests) store.set(r.url, new Response(`body of ${r.url}`));
      },
      keys: async () => [...store.keys()].map((url) => ({ url })),
      delete: async (request) => store.delete(request.url),
    };
    // Runs the real sw.js (with a given PRECACHE) against a fake cache.
    const worker = (precache) => {
      const listeners = {};
      const state = { skipped: 0, claimed: 0 };
      const self = {
        addEventListener: (type, f) => (listeners[type] = f),
        skipWaiting: () => state.skipped++,
        registration: { scope },
        clients: { claim: async () => state.claimed++ },
      };
      const code = source.replace(/const PRECACHE = \[.*?\];/s, `const PRECACHE = ${JSON.stringify(precache)};`);
      class FakeRequest {
        constructor(url) {
          this.url = url;
        }
      }
      new Function("self", "caches", "Request", code)(self, { open: async () => cache }, FakeRequest);
      const run = async (type, data) => {
        let done;
        listeners[type]({ data, waitUntil: (p) => (done = p) });
        await done;
      };
      return { state, run };
    };
    const install = async (precache) => {
      downloaded = [];
      const w = worker(precache);
      await w.run("install");
      return w;
    };
    const kept = () => [...store.keys()].filter((u) => !u.endsWith("__precache.json")).map((u) => u.slice(scope.length)).sort().join(" ");

    await install([["a.js", "aaaa"], ["b.wasm", "bbbb"]]);
    assertEqual(downloaded.sort().join(" "), "a.js b.wasm", "first install");
    const update = await install([["a.js", "aaaa"], ["b.wasm", "cccc"], ["c.css", "dddd"]]);
    assertEqual(downloaded.sort().join(" "), "b.wasm c.css", "update");
    assertEqual(update.state.skipped, 0, "took over without being asked");
    await update.run("message", { type: "something else" });
    assertEqual(update.state.skipped, 0, "took over on another message");
    await update.run("message", { type: "take-over" });
    assertEqual(update.state.skipped, 1, "did not take over when asked");
    await install([["a.js", "aaaa"], ["b.wasm", "cccc"], ["c.css", "dddd"]]);
    assertEqual(downloaded.join(" "), "", "no changes");
    store.delete(`${scope}a.js`);
    await install([["b.wasm", "cccc"], ["c.css", "dddd"], ["a.js", "aaaa"]]);
    assertEqual(downloaded.join(" "), "a.js", "evicted copy");
    const removal = worker([["c.css", "dddd"]]);
    await removal.run("install");
    await removal.run("activate");
    assertEqual(kept(), "c.css", "removed files are dropped on activation");
    assertEqual(removal.state.claimed, 1, "claims once active");
  },

  "layout fits the viewport": () => {
    const overflow = document.documentElement.scrollWidth - window.innerWidth;
    assert(overflow <= 0, `horizontal overflow ${overflow}px`);
  },
};
