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
    const { BUILD, BUILT, versionText } = await import(`${base}update.js`);
    assert(/^[0-9a-f]{7,}\+?$/.test(BUILD), `build ${BUILD}`);
    assert(!Number.isNaN(new Date(BUILT).getTime()), `built ${BUILT}`);
    assert(/^\d{4}-\d\d-\d\d \d\d:\d\d · [0-9a-f]{7,}\+?$/.test(versionText()), `version ${versionText()}`);
    // One version: the build's time and commit (missing graphics is said in the page).
    assertEqual(el.textContent, `Volya Engine · ${versionText()}`, "text");
  },

  "updates: automatic applies at a safe moment, opted out only on request": async () => {
    const { startUpdates } = await import(`${base}update.js`);
    const fake = ({ auto }) => {
      const registration = Object.assign(new EventTarget(), { waiting: null, installing: null, updates: 0, scope: "https://example.test/" });
      registration.update = async () => registration.updates++;
      const container = Object.assign(new EventTarget(), { controller: {} });
      const worker = Object.assign(new EventTarget(), { state: "installing", messages: [] });
      worker.postMessage = (m) => worker.messages.push(m);
      let saved = { auto };
      const store = { read: async () => saved, write: async (s) => (saved = s) };
      const session = new Map();
      const sessionStore = { getItem: (k) => session.get(k) ?? null, setItem: (k, v) => session.set(k, v), removeItem: (k) => session.delete(k) };
      let safe = true;
      const updates = startUpdates(Promise.resolve(registration), { container, store, session: sessionStore, safe: () => safe });
      const waitFor = async () => {
        registration.installing = worker;
        registration.dispatchEvent(new Event("updatefound"));
        registration.installing = null;
        registration.waiting = worker;
        worker.state = "installed";
        worker.dispatchEvent(new Event("statechange"));
      };
      return { updates, registration, container, worker, store: () => saved, setSafe: (v) => (safe = v), waitFor };
    };
    const tick = () => new Promise((r) => setTimeout(r));

    // Automatic: unsafe (in a game) waits; applies once safe.
    const a = fake({ auto: true });
    await tick();
    assert(a.updates.auto, "automatic by default");
    a.setSafe(false);
    await a.waitFor();
    assertEqual(a.updates.status, "ready", "status");
    assertEqual(a.worker.messages.length, 0, "applied during a game");
    a.setSafe(true);
    await a.updates.check();
    assertEqual(JSON.stringify(a.worker.messages), '[{"type":"take-over"}]', "not applied once safe");

    // Opted out: an offer with its size; nothing until asked.
    const b = fake({ auto: false });
    await tick();
    assert(!b.updates.auto, "opt-out not read");
    await b.updates.check();
    assertEqual(b.registration.updates, 1, "check");
    b.container.dispatchEvent(new MessageEvent("message", { data: { type: "update-available", build: "c0ffee1", bytes: 1234 } }));
    assertEqual(`${b.updates.status} ${b.updates.bytes} ${b.updates.build}`, "available 1234 c0ffee1", "offer");
    await b.updates.download();
    assertEqual(b.store().consent, "c0ffee1", "consent recorded for that build");
    assertEqual(b.registration.updates, 2, "download asks the browser to install");
    await b.waitFor();
    assertEqual(JSON.stringify(b.worker.messages), '[{"type":"take-over"}]', "requested update not applied");
    // Turning automatic updates back on is remembered.
    await b.updates.setAuto(true);
    assertEqual(b.store().auto, true, "setting saved");
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

  "an update downloads only changed files, with consent, waits, and drops removed ones": async () => {
    const source = await (await fetch(`${base}sw.js`, { cache: "no-store" })).text();
    const scope = "https://example.test/app/";
    const store = new Map();
    let downloaded = [];
    let told = [];
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
    const setSettings = (settings) => store.set(`${scope}__settings.json`, new Response(JSON.stringify(settings)));
    // Runs the real sw.js (with a given build and PRECACHE) against fakes.
    const worker = (build, precache) => {
      const listeners = {};
      const state = { skipped: 0, claimed: 0 };
      const client = { postMessage: (m) => told.push(m) };
      const self = {
        addEventListener: (type, f) => (listeners[type] = f),
        skipWaiting: () => state.skipped++,
        registration: { scope },
        clients: { claim: async () => state.claimed++, matchAll: async () => [client] },
      };
      const code = source
        .replace(/const BUILD = ".*?";/, `const BUILD = ${JSON.stringify(build)};`)
        .replace(/const PRECACHE = \[.*?\];/s, `const PRECACHE = ${JSON.stringify(precache)};`);
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
    const install = async (build, precache) => {
      downloaded = [];
      told = [];
      const w = worker(build, precache);
      let error = null;
      await w.run("install").catch((e) => (error = e));
      return { ...w, error };
    };
    const kept = () =>
      [...store.keys()]
        .filter((u) => !u.includes("__"))
        .map((u) => u.slice(scope.length))
        .sort()
        .join(" ");

    // The first install downloads even when opted out.
    setSettings({ auto: false });
    await install("b1", [["a.js", "aaaa", 10], ["b.wasm", "bbbb", 20]]);
    assertEqual(downloaded.sort().join(" "), "a.js b.wasm", "first install");
    assertEqual(told.length, 0, "a first install announced as an update");
    // Opted out: no download; the pages hear the size; the install fails.
    const v2 = [["a.js", "aaaa", 10], ["b.wasm", "cccc", 2000], ["c.css", "dddd", 300]];
    const refused = await install("b2", v2);
    assertEqual(downloaded.join(" "), "", "downloaded without consent");
    assert(refused.error, "install succeeded without consent");
    assertEqual(JSON.stringify(told), '[{"type":"update-available","build":"b2","bytes":2300,"files":2}]', "told");
    // Consent for that build: only the changed files download; it waits.
    setSettings({ auto: false, consent: "b2" });
    const update = await install("b2", v2);
    assertEqual(downloaded.sort().join(" "), "b.wasm c.css", "update");
    assertEqual(told[0]?.type, "update-downloading", "told downloading");
    assertEqual(update.state.skipped, 0, "took over without being asked");
    await update.run("message", { type: "something else" });
    assertEqual(update.state.skipped, 0, "took over on another message");
    await update.run("message", { type: "take-over" });
    assertEqual(update.state.skipped, 1, "did not take over when asked");
    // Consent was for b2 only.
    assert((await install("b3", [...v2, ["d.js", "eeee", 5]])).error, "b3 installed on b2's consent");
    // Automatic: no changes, nothing downloads; an evicted copy comes back.
    setSettings({ auto: true });
    await install("b2", v2);
    assertEqual(downloaded.join(" "), "", "no changes");
    store.delete(`${scope}a.js`);
    await install("b2", v2);
    assertEqual(downloaded.join(" "), "a.js", "evicted copy");
    // Activation drops files that left the build, keeping the settings.
    const removal = worker("b4", [["c.css", "dddd", 300]]);
    await removal.run("install");
    await removal.run("activate");
    assertEqual(kept(), "c.css", "removed files are dropped on activation");
    assert(store.has(`${scope}__settings.json`), "settings dropped");
    assertEqual(removal.state.claimed, 1, "claims once active");
  },

  "settings: updates, storage and a danger zone, reachable from the bottom": async () => {
    const settings = [...document.querySelectorAll("#game .bottom button")].find((b) => b.textContent === "Settings");
    assert(settings, "no Settings button at the bottom");
    // Scrolled down to reach it, as on a short screen: Settings opens at its top.
    document.body.style.minHeight = "300vh";
    scrollTo(0, document.documentElement.scrollHeight);
    settings.click();
    for (let i = 0; i < 50 && !document.querySelector(".screen-head"); i++) await new Promise((r) => setTimeout(r, 20));
    const labels = [...document.querySelectorAll("#game .label")].map((l) => l.textContent);
    assert(labels.includes("Updates") && labels.includes("Storage"), `labels ${labels}`);
    for (let i = 0; i < 50 && scrollY !== 0; i++) await new Promise((r) => setTimeout(r, 20));
    assertEqual(scrollY, 0, "Settings opened scrolled");
    document.body.style.minHeight = "";
    const toggle = document.querySelector("#game .switch input");
    assert(toggle && toggle.checked, "automatic updates not on by default");
    assertEqual(document.activeElement?.textContent, "‹ Back", "focus starts on Back (keyboard)");
    document.activeElement.click();
    for (let i = 0; i < 50 && document.querySelector(".screen-head"); i++) await new Promise((r) => setTimeout(r, 20));
    assert(!document.querySelector(".screen-head"), "Back did not leave Settings");
  },

  "the UX audit page renders every state on every device": async () => {
    const { DEVICES, STATES } = await import(`${base}ux/states.js`);
    const failed = [];
    for (const device of DEVICES) {
      for (const state of STATES) {
        const frame = Object.assign(document.createElement("iframe"), { width: device.width, height: device.height });
        frame.style.cssText = "position: fixed; left: -5000px; top: 0";
        frame.src = `${base}ux/frame.html#${device.id}/${state.id}`;
        document.body.append(frame);
        let ok = false;
        for (let i = 0; i < 100 && !ok; i++) {
          await new Promise((r) => setTimeout(r, 20));
          const doc = frame.contentDocument;
          ok = doc?.documentElement.dataset.ready === "true" && (doc.getElementById("game").children.length > 0 || state.startError);
        }
        if (!ok) failed.push(`${device.id}/${state.id}`);
        frame.remove();
      }
    }
    assertEqual(failed.join(" "), "", "frames that didn't render");
  },

  "layout fits the viewport": () => {
    const overflow = document.documentElement.scrollWidth - window.innerWidth;
    assert(overflow <= 0, `horizontal overflow ${overflow}px`);
  },
};
