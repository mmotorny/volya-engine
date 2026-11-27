// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import { assert, assertEqual, resumeAfterReload, skip } from "./harness.js";

// The app's base path (tests are served under it, in __test/).
const base = location.pathname.slice(0, location.pathname.indexOf("/__test/") + 1);

/** The `[path, hash]` list injected into a deployed sw.js. */
const precacheOf = (source) => JSON.parse(source.match(/const PRECACHE = (\[.*?\]);/s)[1]);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
/** Waits for `ok` (may be async) to hold, failing with `what`. */
async function until(ok, what, ms = 5000) {
  const end = performance.now() + ms;
  while (!(await ok())) {
    assert(performance.now() < end, what);
    await sleep(20);
  }
}

/** The diagnostic log's Download button in Settings. */
const logDownload = () => [...document.querySelectorAll("#game .panel-action")].find((b) => /diagnostic log|Preparing|log$/.test(b.textContent));

/** Opens Settings; its diagnostic log check box. */
async function openSettingsLog() {
  await volya.showSettings();
  const label = [...document.querySelectorAll("#game .switch")].find((l) => l.textContent.includes("diagnostic log"));
  assert(label, "no diagnostic log setting");
  return label.querySelector("input");
}

/** Presses Download diagnostic log; the saved file's name and its trace. */
async function downloadLog() {
  const click = HTMLAnchorElement.prototype.click;
  let link = null;
  HTMLAnchorElement.prototype.click = function () {
    link = this;
  };
  try {
    logDownload().click();
    await until(() => link, "nothing was saved", 15000);
  } finally {
    HTMLAnchorElement.prototype.click = click;
  }
  const text = await (await fetch(link.href)).text();
  const trace = JSON.parse(text);
  checkTraceFormat(trace);
  return { name: link.download, trace };
}

/**
 * The Trace Event Format's JSON Object Format, as documented: a
 * `traceEvents` array of events, each with `ph`, `name`, `pid`, `tid` and
 * `ts` (µs); `X` with `dur`; `C` with numeric `args`; `i` with a scope.
 */
function checkTraceFormat(trace) {
  assert(Array.isArray(trace.traceEvents), "no traceEvents");
  for (const e of trace.traceEvents) {
    const where = JSON.stringify(e).slice(0, 120);
    assert(["X", "C", "i", "M"].includes(e.ph), `phase: ${where}`);
    assert(typeof e.name === "string" && Number.isInteger(e.pid) && Number.isInteger(e.tid), `fields: ${where}`);
    assert(typeof e.ts === "number", `ts: ${where}`);
    if (e.ph === "X") assert(typeof e.dur === "number" && e.dur >= 0, `dur: ${where}`);
    if (e.ph === "C") assert(Object.values(e.args ?? {}).every((v) => typeof v === "number"), `counter: ${where}`);
    if (e.ph === "i") assert(["t", "p", "g"].includes(e.s), `scope: ${where}`);
  }
}

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

  "updates: never restart by themselves; opted out, nothing downloads unasked": async () => {
    const { startUpdates } = await import(`${base}update.js`);
    const fake = ({ auto }) => {
      const registration = Object.assign(new EventTarget(), { waiting: null, installing: null, updates: 0, scope: "https://example.test/" });
      registration.update = async () => registration.updates++;
      const container = Object.assign(new EventTarget(), { controller: {} });
      const worker = Object.assign(new EventTarget(), { state: "installing", messages: [] });
      worker.postMessage = (m) => worker.messages.push(m);
      let saved = { auto };
      const store = { read: async () => saved, write: async (s) => (saved = s) };
      const reloads = { count: 0 };
      const reload = () => reloads.count++;
      const updates = startUpdates(Promise.resolve(registration), { container, store, reload, takeOverMs: 50 });
      const waitFor = async () => {
        registration.installing = worker;
        registration.dispatchEvent(new Event("updatefound"));
        registration.installing = null;
        registration.waiting = worker;
        worker.state = "installed";
        worker.dispatchEvent(new Event("statechange"));
      };
      return { updates, registration, container, worker, store: () => saved, waitFor, reloads };
    };
    const tick = () => new Promise((r) => setTimeout(r));

    // Automatic downloads: a downloaded update waits; Finish update applies it.
    const a = fake({ auto: true });
    await tick();
    assert(a.updates.auto, "automatic by default");
    await a.waitFor();
    await a.updates.check();
    assertEqual(a.updates.status, "ready", "status");
    assertEqual(a.worker.messages.length, 0, "restarted by itself");
    a.updates.apply();
    assertEqual(JSON.stringify(a.worker.messages), '[{"type":"take-over"}]', "Finish update didn't apply");
    // It reloads once the new worker takes over, and only once.
    a.container.dispatchEvent(new Event("controllerchange"));
    assertEqual(a.reloads.count, 1, "reloads on take-over");
    await sleep(80);
    assertEqual(a.reloads.count, 1, "reloaded twice");
    // Without word of the take-over, it reloads anyway.
    const c = fake({ auto: true });
    await tick();
    await c.waitFor();
    await c.updates.check();
    c.updates.apply();
    assertEqual(c.reloads.count, 0, "reloaded before the take-over could be told");
    await sleep(80);
    assertEqual(c.reloads.count, 1, "no reload without word of the take-over");

    // Opted out: an offer with its size; downloads only when asked; still waits.
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
    assertEqual(b.updates.status, "ready", "downloaded");
    assertEqual(b.worker.messages.length, 0, "restarted by itself after download");
    // Turning automatic downloads back on is remembered.
    await b.updates.setAuto(true);
    assertEqual(b.store().auto, true, "setting saved");
  },

  "updates: Check for updates tells found, none, offline and failed apart": async () => {
    const { startUpdates } = await import(`${base}update.js`);
    const tick = () => new Promise((r) => setTimeout(r));
    const fake = (update) => {
      const registration = Object.assign(new EventTarget(), { waiting: null, installing: null, scope: "https://example.test/" });
      const container = Object.assign(new EventTarget(), { controller: {} });
      registration.update = () => update(registration, container);
      const store = { read: async () => ({}), write: async () => {} };
      return startUpdates(Promise.resolve(registration), { container, store });
    };
    const outcome = async (update) => {
      const updates = fake(update);
      await tick();
      return [await updates.checkNow(), updates.requested];
    };
    // A new worker installs (automatic downloads, or consented).
    const installs = async (registration) => {
      registration.installing = Object.assign(new EventTarget(), { state: "installing" });
      registration.dispatchEvent(new Event("updatefound"));
    };
    assertEqual(String(await outcome(installs)), "found,true", "installing");
    // Opted out: the worker offers it and refuses; nothing is left installing.
    const offers = async (registration, container) => {
      registration.dispatchEvent(new Event("updatefound"));
      container.dispatchEvent(new MessageEvent("message", { data: { type: "update-available", build: "b2", bytes: 1 } }));
      throw new TypeError("install failed");
    };
    assertEqual(String(await outcome(offers)), "found,true", "offered");
    // Already downloaded.
    assertEqual(String(await outcome(async (r) => (r.waiting = {}))), "found,true", "waiting");
    assertEqual(String(await outcome(async () => {})), "none,false", "none");
    const rejects = async () => {
      throw new TypeError("Failed to update a ServiceWorker");
    };
    assertEqual(String(await outcome(rejects)), "failed,false", "failed while the browser says it's online");
    Object.defineProperty(navigator, "onLine", { configurable: true, get: () => false });
    try {
      assertEqual(String(await outcome(rejects)), "offline,false", "offline");
    } finally {
      delete navigator.onLine;
    }
  },

  // Audio output needs SharedArrayBuffer (#354). GitHub Pages can't send
  // COOP/COEP: the service worker adds them; test pages run without it, so
  // the dev server sends them for everything but the app's page.
  "page is cross-origin isolated": () => {
    assertEqual(self.crossOriginIsolated, true);
    assert(typeof SharedArrayBuffer === "function", "no SharedArrayBuffer");
  },

  "a page the service worker didn't serve reloads once, as soon as it can": async () => {
    const { ensureIsolated } = await import(`${base}isolation.js`);
    const tick = () => new Promise((r) => setTimeout(r));
    const env = ({ controller = null, active = null, installing = null, stored = {} } = {}) => {
      const container = Object.assign(new EventTarget(), { controller });
      const items = { ...stored };
      const storage = { getItem: (k) => items[k] ?? null, setItem: (k, v) => (items[k] = v), removeItem: (k) => delete items[k] };
      const registration = { active, installing, waiting: null };
      const e = { container, storage, reloads: 0, items, isolated: false };
      e.run = () => ensureIsolated({ ...e, registered: Promise.resolve(registration), reload: () => e.reloads++ });
      return e;
    };
    // Isolated: nothing to do.
    const ok = env();
    assertEqual(await ensureIsolated({ ...ok, isolated: true, reload: () => ok.reloads++ }), "isolated", "isolated");
    // A hard reload, the worker installed: reloads at once.
    const hard = env({ active: {} });
    assertEqual([await hard.run(), hard.reloads].join(), "reloading,1", "hard reload");
    // The reload didn't isolate it: no second reload in the page session.
    assertEqual([await hard.run(), hard.reloads].join(), "blocked,1", "a second time");
    // Once isolated, a later hard reload reloads again.
    assertEqual(await ensureIsolated({ ...hard, isolated: true }), "isolated", "isolated after all");
    assertEqual([await hard.run(), hard.reloads].join(), "reloading,2", "a later hard reload");
    // First visit: reloads once the worker installed and claimed the page.
    const installing = Object.assign(new EventTarget(), { state: "installing" });
    const first = env({ installing });
    const outcome = first.run();
    await tick();
    assertEqual(first.reloads, 0, "reloaded before the worker controls the page");
    first.container.dispatchEvent(new Event("controllerchange"));
    assertEqual([await outcome, first.reloads].join(), "reloading,1", "first visit");
    // An install that fails: no reload, the player is told.
    const failing = Object.assign(new EventTarget(), { state: "installing" });
    const failed = env({ installing: failing });
    const failure = failed.run();
    await tick();
    failing.state = "redundant";
    failing.dispatchEvent(new Event("statechange"));
    assertEqual([await failure, failed.reloads].join(), "blocked,0", "failed install");
    // Served by the worker and not isolated: the browser ignores the headers.
    const ignored = env({ controller: {}, active: {} });
    assertEqual([await ignored.run(), ignored.reloads].join(), "blocked,0", "headers ignored");
  },

  "service worker controls the app scope": async () => {
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 10000));
    const registration = await Promise.race([navigator.serviceWorker.ready, timeout]);
    assert(registration.scope.endsWith("/volya-engine/"), `scope ${registration.scope}`);
  },

  // The dev server sends the app's page without them, as GitHub Pages does.
  "the service worker serves the app with COOP and COEP": async () => {
    if (!navigator.serviceWorker.controller) {
      await new Promise((resolve) => navigator.serviceWorker.addEventListener("controllerchange", resolve, { once: true }));
    }
    const page = await fetch(base);
    const worker = await fetch(`${base}audio-worker.js`);
    for (const [name, response] of [["page", page], ["audio worker", worker]]) {
      assertEqual(response.headers.get("Cross-Origin-Opener-Policy"), "same-origin", `${name} COOP`);
      assertEqual(response.headers.get("Cross-Origin-Embedder-Policy"), "require-corp", `${name} COEP`);
    }
    for (const response of [page, worker]) await response.arrayBuffer();
  },

  "the whole app works offline once installed": async () => {
    const registration = await navigator.serviceWorker.ready;
    // The page must be controlled to use the kept copies.
    if (!navigator.serviceWorker.controller) {
      await new Promise((resolve) => navigator.serviceWorker.addEventListener("controllerchange", resolve, { once: true }));
    }
    assert(registration.active, "no active service worker");
    const source = await (await fetch(`${base}sw.js`, { cache: "no-store" })).text();
    const files = precacheOf(source).map(([path]) => path);
    assert(files.includes("./") && files.includes("index.html") && files.some((f) => f.endsWith(".wasm")), `files ${files.join(" ")}`);
    const name = source.match(/const CACHE = "(volya-engine-build-[0-9a-f]+)";/)?.[1];
    assert(name, "no build cache name injected");
    const cache = await caches.open(name);
    const kept = async (path) => Boolean(await cache.match(new URL(path, registration.scope).href));
    for (let i = 0; i < 100 && !(await Promise.all(files.map(kept))).every(Boolean); i++) {
      await new Promise((r) => setTimeout(r, 100));
    }
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

  "an update installs into a cache of its own, with consent, verified, and waits; the running build is untouched": async () => {
    const source = await (await fetch(`${base}sw.js`, { cache: "no-store" })).text();
    const scope = "https://example.test/app/";
    const hashOf = async (text) =>
      Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text))).slice(0, 8), (b) => b.toString(16).padStart(2, "0")).join("");
    // A fake CacheStorage: name → (url → response).
    const storage = new Map();
    const caches = {
      open: async (name) => {
        if (!storage.has(name)) storage.set(name, new Map());
        const entries = storage.get(name);
        return { match: async (url) => entries.get(url)?.clone(), put: async (url, response) => void entries.set(url, response) };
      },
      has: async (name) => storage.has(name),
      keys: async () => [...storage.keys()],
      delete: async (name) => storage.delete(name),
    };
    const SETTINGS = "volya-engine-settings";
    const setSettings = async (settings) => (await caches.open(SETTINGS)).put(`${scope}__settings.json`, new Response(JSON.stringify(settings)));
    // The network: path → body; what install fetched.
    let network = {};
    let downloaded = [];
    const fakeFetch = async (url) => {
      const path = (url.url ?? url).slice(scope.length);
      downloaded.push(path);
      const body = network[path];
      return body === undefined ? new Response("", { status: 404 }) : new Response(body, { headers: { "Content-Type": "text/plain" } });
    };
    let told = [];
    const registration = { scope, active: null };
    // Runs the real sw.js for a build of `files` (path → body) against fakes.
    const worker = async (build, files) => {
      const precache = await Promise.all(Object.entries(files).map(async ([path, body]) => [path, await hashOf(body), body.length]));
      const listeners = {};
      const state = { skipped: 0, claimed: 0 };
      const client = { postMessage: (m) => told.push(m) };
      const self = {
        addEventListener: (type, f) => (listeners[type] = f),
        skipWaiting: () => state.skipped++,
        registration,
        clients: { claim: async () => state.claimed++, matchAll: async () => [client] },
      };
      const code = source
        .replace(/const CACHE = ".*?";/, `const CACHE = "volya-engine-build-${build}";`)
        .replace(/const BUILD = ".*?";/, `const BUILD = ${JSON.stringify(build)};`)
        .replace(/const PRECACHE = \[.*?\];/s, `const PRECACHE = ${JSON.stringify(precache)};`);
      new Function("self", "caches", "fetch", code)(self, caches, fakeFetch);
      const run = async (type, data) => {
        let done;
        listeners[type]({ data, waitUntil: (p) => (done = p) });
        await done;
      };
      const install = async () => {
        downloaded = [];
        told = [];
        let error = null;
        await run("install").catch((e) => (error = e));
        return error;
      };
      const activate = async () => {
        await run("activate");
        registration.active = self;
      };
      // What this worker serves for `path`, or null when it leaves it to the network.
      const serve = async (path) => {
        let response = null;
        listeners.fetch({ request: { method: "GET", url: `${scope}${path}` }, respondWith: (p) => (response = p) });
        return response && (await response).text();
      };
      return { state, run, install, activate, serve };
    };
    const files = (name) => [...(storage.get(`volya-engine-build-${name}`)?.keys() ?? [])].map((u) => u.slice(scope.length)).sort().join(" ");

    // The first install downloads even when opted out, and serves its build.
    await setSettings({ auto: false });
    const v1 = { "a.js": "a1", "b.wasm": "b1" };
    network = { ...v1 };
    const b1 = await worker("b1", v1);
    assertEqual(await b1.install(), null, "first install failed");
    assertEqual(downloaded.sort().join(" "), "a.js b.wasm", "first install");
    assertEqual(told.length, 0, "a first install announced as an update");
    await b1.activate();
    assertEqual(await b1.serve("b.wasm"), "b1", "served");
    assertEqual(await b1.serve("elsewhere.js"), null, "a file outside the build served from the cache");
    // Opted out: no download; the pages hear the size; the install fails.
    const v2 = { "a.js": "a1", "b.wasm": "b2-wasm", "c.css": "c2" };
    network = { ...v2 };
    const b2 = await worker("b2", v2);
    assert(await b2.install(), "install succeeded without consent");
    assertEqual(downloaded.join(" "), "", "downloaded without consent");
    assertEqual(JSON.stringify(told), '[{"type":"update-available","build":"b2","bytes":9,"files":2}]', "told");
    // Consent for that build: only the changed files download, the
    // unchanged one is copied; it waits.
    await setSettings({ auto: false, consent: "b2" });
    assertEqual(await b2.install(), null, "install with consent failed");
    assertEqual(downloaded.sort().join(" "), "b.wasm c.css", "update");
    assertEqual(files("b2"), "a.js b.wasm c.css", "the update's cache");
    assertEqual(told[0]?.type, "update-downloading", "told downloading");
    assertEqual(b2.state.skipped, 0, "took over without being asked");
    // The mix of #444: the running build still serves its own files.
    assertEqual([await b1.serve("a.js"), await b1.serve("b.wasm")].join(" "), "a1 b1", "the running build got the update's files");
    assertEqual(files("b1"), "a.js b.wasm", "the running build's cache changed");
    await b2.run("message", { type: "something else" });
    assertEqual(b2.state.skipped, 0, "took over on another message");
    await b2.run("message", { type: "take-over" });
    assertEqual(b2.state.skipped, 1, "did not take over when asked");
    // Consent was for b2 only.
    assert(await (await worker("b3", { ...v2, "d.js": "d3" })).install(), "b3 installed on b2's consent");
    // A download that isn't the build's (stale, half deployed) fails the
    // install; the next try downloads only what is still missing.
    await setSettings({ auto: true });
    const v4 = { "a.js": "a1", "b.wasm": "b4-wasm", "d.js": "d4" };
    network = { ...v4, "b.wasm": "b2-wasm" };
    const b4 = await worker("b4", v4);
    const mismatch = await b4.install();
    assert(String(mismatch).includes("b.wasm"), `a stale download installed: ${mismatch}`);
    await sleep(20);
    assertEqual(await b1.serve("b.wasm"), "b1", "the running build changed by a failed install");
    network = { ...v4 };
    assertEqual(await b4.install(), null, "retry failed");
    assertEqual(downloaded.join(" "), "b.wasm", "retry");
    // Nothing missing, nothing downloads; an evicted copy comes from another
    // build's cache, else the network.
    await b4.install();
    assertEqual(downloaded.join(" "), "", "no changes");
    storage.get("volya-engine-build-b4").delete(`${scope}a.js`);
    await b4.install();
    assertEqual(downloaded.join(" "), "", "evicted copy downloaded though kept elsewhere");
    for (const name of ["b1", "b2", "b3", "b4"]) storage.get(`volya-engine-build-${name}`).delete(`${scope}a.js`);
    await b4.install();
    assertEqual(downloaded.join(" "), "a.js", "evicted copy");
    // Activation deletes the other builds' caches and the legacy one, whose
    // settings move unless there are newer.
    storage.delete(SETTINGS);
    const legacy = await caches.open("volya-engine-app");
    await legacy.put(`${scope}__settings.json`, new Response('{"auto":false}'));
    await legacy.put(`${scope}a.js`, new Response("a0"));
    await b4.activate();
    assertEqual([...storage.keys()].sort().join(" "), `volya-engine-build-b4 ${SETTINGS}`, "caches after activation");
    assertEqual(await (await (await caches.open(SETTINGS)).match(`${scope}__settings.json`)).text(), '{"auto":false}', "settings moved");
    assertEqual(b4.state.claimed, 1, "claims once active");
    assertEqual(await b4.serve("d.js"), "d4", "served after activation");
  },

  "a game the browser deleted is said so, on the import screen": async () => {
    // Without graphics the app says it can't run the game instead (no import screen).
    if ((await volya.status()).gpu.state !== "ready") skip("no graphics here");
    localStorage.setItem("volya-engine.imported", "1");
    try {
      await volya.refresh();
      const notice = document.querySelector("#game .callout.problem strong");
      assertEqual(notice?.textContent, "The browser deleted the imported game", "notice");
    } finally {
      localStorage.removeItem("volya-engine.imported");
      await volya.refresh();
    }
    assert(!document.querySelector("#game .callout.problem"), "notice without the mark");
  },

  "a damaged import record is said so, on the import screen": async () => {
    if ((await volya.status()).gpu.state !== "ready") skip("no graphics here");
    const status = await volya.call("game_status");
    if (status.imported || status.written > 0) skip("a real import is present");
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("volya-engine", { create: true });
    const file = await dir.getFileHandle("import.json", { create: true });
    const writable = await file.createWritable();
    await writable.write("not json");
    await writable.close();
    try {
      await volya.refresh();
      const notice = document.querySelector("#game .callout.problem");
      assertEqual(notice?.querySelector("strong")?.textContent, "The imported game is damaged", "notice");
      assert(notice.textContent.includes("import.json:"), `engine's words missing: ${notice.textContent}`);
    } finally {
      await dir.removeEntry("import.json");
      await volya.refresh();
    }
  },

  "settings: updates and a danger zone, reachable from the bottom": async () => {
    const settings = [...document.querySelectorAll("#game .bottom button")].find((b) => b.textContent === "Settings");
    assert(settings, "no Settings button at the bottom");
    // Scrolled down to reach it, as on a short screen: Settings opens at its top.
    document.body.style.minHeight = "300vh";
    scrollTo(0, document.documentElement.scrollHeight);
    // Settled before the tap, as a player's scroll is: iOS Safari scrolls
    // asynchronously, and a scroll still on its way may land after the
    // app's scroll to the top (measured: the iOS Simulator kept the bottom
    // in 4 runs of 5 before this wait, in none of 3 since).
    const frame = () => new Promise((r) => requestAnimationFrame(r));
    for (let i = 0, last = -1; i < 50 && scrollY !== last; i++) {
      last = scrollY;
      await frame();
      await frame();
    }
    settings.click();
    for (let i = 0; i < 50 && !document.querySelector(".screen-head"); i++) await new Promise((r) => setTimeout(r, 20));
    const labels = [...document.querySelectorAll("#game .label")].map((l) => l.textContent);
    assert(labels.includes("Updates") && !labels.includes("Storage"), `labels ${labels}`);
    for (let i = 0; i < 50 && scrollY !== 0; i++) await new Promise((r) => setTimeout(r, 20));
    assertEqual(scrollY, 0, "Settings opened scrolled");
    document.body.style.minHeight = "";
    const toggle = document.querySelector('#game .switch input[type="checkbox"]');
    assert(toggle && toggle.checked, "automatic updates not on by default");
    assert(
      [...document.querySelectorAll("#game .panel button")].some((b) => b.textContent === "Check for updates"),
      "no Check for updates button",
    );
    assertEqual(document.activeElement?.textContent, "‹ Back", "focus starts on Back (keyboard)");
    // The logo leads Home too.
    document.querySelector(".brand").click();
    for (let i = 0; i < 50 && document.querySelector(".screen-head"); i++) await new Promise((r) => setTimeout(r, 20));
    assert(!document.querySelector(".screen-head"), "the logo did not lead Home");
    [...document.querySelectorAll("#game .bottom button")].find((b) => b.textContent === "Settings").click();
    for (let i = 0; i < 50 && !document.querySelector(".screen-head"); i++) await new Promise((r) => setTimeout(r, 20));
    document.activeElement.click();
    for (let i = 0; i < 50 && document.querySelector(".screen-head"); i++) await new Promise((r) => setTimeout(r, 20));
    assert(!document.querySelector(".screen-head"), "Back did not leave Settings");
  },

  "Check for updates: up to date, can't reach the server, found leads Home": async () => {
    const registration = await navigator.serviceWorker.ready;
    const wait = async (ok, what) => {
      for (let i = 0; i < 250 && !ok(); i++) await new Promise((r) => setTimeout(r, 20));
      assert(ok(), what);
    };
    const TEXTS = ["Check for updates", "Checking…", "Up to date", "Can't check while offline", "Couldn't check for updates"];
    const button = () => [...document.querySelectorAll("#game .panel button")].find((b) => TEXTS.includes(b.textContent));
    [...document.querySelectorAll("#game .bottom button")].find((b) => b.textContent === "Settings").click();
    await wait(() => button()?.textContent === "Check for updates", "no Check for updates in Settings");
    // This build is the one served: the real check finds nothing.
    button().click();
    assertEqual(button().textContent, "Checking…", "while checking");
    assert(button().disabled, "enabled while checking");
    await wait(() => button().textContent === "Up to date", `after the check: ${button().textContent}`);
    await wait(() => button().textContent === "Check for updates" && !button().disabled, "the button didn't return");
    // The server unreachable while the browser says it's online: the cause
    // isn't known. What update() rejects with, as measured.
    const offline = (on) => fetch(`${base}__test/offline/${on ? "on" : "off"}`, { method: "POST" });
    let rejection;
    await offline(true);
    try {
      rejection = await registration.update().then(
        () => "resolved",
        (e) => `${e.name}: ${e.message}`,
      );
      button().click();
      await wait(() => button().textContent === "Couldn't check for updates", `unreachable: ${button().textContent}`);
    } finally {
      await offline(false);
    }
    // An update found (a new worker installing): Home, where its row is.
    const update = registration.update;
    registration.update = async () => registration.dispatchEvent(new Event("updatefound"));
    try {
      await wait(() => button()?.textContent === "Check for updates", "the button didn't return after failing");
      button().click();
      await wait(() => !document.querySelector(".screen-head"), "found didn't lead Home");
      assert([...document.querySelectorAll("#game .bottom button")].some((b) => b.textContent === "Settings"), "not Home");
    } finally {
      registration.update = update;
    }
    return `update() unreachable: ${rejection}; navigator.onLine ${navigator.onLine}`;
  },

  "Delete settings and saved games: asks first, deletes only those, and the next game starts from the defaults": async () => {
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("volya-engine", { create: true });
    const put = async (at, name, text) => {
      const writable = await (await at.getFileHandle(name, { create: true })).createWritable();
      await writable.write(text);
      await writable.close();
    };
    const names = async () => {
      const out = [];
      for await (const name of dir.keys()) out.push(name);
      return out.sort();
    };
    const button = (label) => [...document.querySelectorAll("#game button")].find((b) => b.textContent === label);
    await put(dir, "user.ltx", "bind forward kT\r\n");
    await put(await dir.getDirectoryHandle("savedgames", { create: true }), "test.sav", "test");
    const storage = JSON.stringify(Object.entries(localStorage).sort());
    const cached = (await caches.keys()).sort().join();
    const others = (await names()).filter((n) => n !== "user.ltx" && n !== "savedgames");
    await volya.showSettings();
    const remove = button("Delete settings and saved games");
    assert(remove?.closest(".danger-zone"), "no Delete settings and saved games in the danger zone");
    // Asked first; Escape (the keyboard) cancels.
    remove.click();
    const ask = document.querySelector("#game .confirm");
    assertEqual(ask?.querySelector("strong").textContent, "Delete the game's settings and saved games?", "the question");
    assertEqual(document.activeElement?.textContent, "Cancel", "focus on Cancel");
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await until(() => !document.querySelector("#game .confirm") && button("Delete settings and saved games"), "Escape didn't cancel");
    assertEqual(document.activeElement?.textContent, "Delete settings and saved games", "focus back on the button");
    assert((await names()).includes("user.ltx"), "deleted though cancelled");
    button("Delete settings and saved games").click();
    button("Delete").click();
    await until(() => document.querySelector('#game [role="status"]')?.textContent === "Settings and saved games deleted", "not deleted");
    assertEqual(await names(), others, "what's left in storage");
    assertEqual(await volya.call("user_config"), null, "a user configuration is still read");
    assertEqual(JSON.stringify(Object.entries(localStorage).sort()), storage, "the shell's preferences changed");
    assertEqual((await caches.keys()).sort().join(), cached, "the offline copy changed");
    // Nothing left to delete: not offered.
    await volya.showSettings();
    assert(!button("Delete settings and saved games"), "offered with nothing stored");
  },

  "Remove imported game: asks in the page, Escape cancels, a failure is told": async () => {
    // The smoke page has no game imported: the UX audit frame's fake engine has one.
    const frame = Object.assign(document.createElement("iframe"), { width: 1280, height: 800 });
    frame.style.cssText = "position: fixed; left: -5000px; top: 0";
    frame.src = `${base}ux-audit/frame.html#desktop/settings-remove-game-failed`;
    document.body.append(frame);
    try {
      const doc = () => frame.contentDocument;
      const button = (label) => [...(doc()?.querySelectorAll("#game button") ?? [])].find((b) => b.textContent === label);
      await until(() => doc()?.documentElement?.dataset.ready === "true" && button("Remove imported game"), "the frame didn't render");
      const note = button("Remove imported game").closest(".danger-item").querySelector(".note");
      assertEqual(note.classList.contains("error") && note.textContent, "Couldn't remove the imported game.", "the failure");
      button("Remove imported game").click();
      const ask = doc().querySelector("#game .confirm");
      assertEqual(ask?.querySelector("strong").textContent, "Remove the imported game?", "the question");
      assertEqual(ask.querySelector("span")?.textContent, "The game's settings and saved games stay.", "the detail");
      assert(ask.closest(".danger-zone"), "not in the danger zone");
      assertEqual(doc().activeElement?.textContent, "Cancel", "focus on Cancel");
      doc().activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      await until(() => !doc().querySelector("#game .confirm") && button("Remove imported game"), "Escape didn't cancel");
      assertEqual(doc().activeElement?.textContent, "Remove imported game", "focus back on the button");
    } finally {
      frame.remove();
    }
  },

  "the UX audit page renders every state on every device": async () => {
    const { DEVICES, STATES } = await import(`${base}ux-audit/states.js`);
    const failed = [];
    for (const device of DEVICES) {
      for (const state of STATES) {
        const frame = Object.assign(document.createElement("iframe"), { width: device.width, height: device.height });
        frame.style.cssText = "position: fixed; left: -5000px; top: 0";
        frame.src = `${base}ux-audit/frame.html#${device.id}/${state.id}`;
        document.body.append(frame);
        let ok = false;
        for (let i = 0; i < 100 && !ok; i++) {
          await new Promise((r) => setTimeout(r, 20));
          // The frame's document exists before its parser has added the
          // root element (HTML, creating a document; #597): no root yet is
          // not ready yet.
          const doc = frame.contentDocument;
          // The level's states show the touch controls (or the full screen
          // offer) instead of a screen.
          const overlay = state.level?.offer ? "fullscreen-offer" : "touch-controls";
          const shown = state.level ? doc?.getElementById(overlay)?.hidden === false : doc?.getElementById("game")?.children.length > 0;
          ok = doc?.documentElement?.dataset.ready === "true" && (shown || state.startError);
        }
        if (!ok) failed.push(`${device.id}/${state.id}`);
        // The sprint lock target, where shown, is on the screen and clear
        // of its safe area's top edge (a thumb landing high lowers it).
        const target = state.level && frame.contentDocument?.querySelector(".touch-lock");
        if (target && frame.contentWindow.getComputedStyle(target).visibility === "visible") {
          const r = target.getBoundingClientRect();
          const inset = device.insets;
          const inside =
            r.left >= inset.left && r.top >= inset.top && r.right <= device.width - inset.right && r.bottom <= device.height - inset.bottom;
          if (!inside) failed.push(`${device.id}/${state.id}: lock target at ${JSON.stringify(r)}`);
        }
        frame.remove();
      }
    }
    assertEqual(failed.join(" "), "", "frames that didn't render");
  },

  "the UX audit board lays out every state and keeps its view": async () => {
    const frame = Object.assign(document.createElement("iframe"), { width: 1600, height: 900 });
    frame.style.cssText = "position: fixed; left: -5000px; top: 0";
    frame.src = `${base}ux-audit/`;
    // The board's defaults, whatever an earlier run left.
    const VIEW = "volya-engine.ux-audit.view";
    localStorage.removeItem(VIEW);
    document.body.append(frame);
    try {
      const { STATES } = await import(`${base}ux-audit/states.js`);
      let doc = null;
      for (let i = 0; i < 100 && !doc?.querySelector(".state"); i++) {
        await new Promise((r) => setTimeout(r, 20));
        doc = frame.contentDocument;
      }
      assertEqual(doc.querySelectorAll(".state").length, STATES.length, "states on the board");
      // Toggling a device re-renders once (no listeners piling up).
      const box = doc.querySelector(".devices input");
      box.click();
      box.click();
      assertEqual(doc.querySelectorAll(".state").length, STATES.length, "states after toggling a device");
      // The view is kept across reloads.
      const zoom = doc.querySelector(".zoom input");
      zoom.value = 55;
      zoom.dispatchEvent(new Event("change"));
      box.click();
      // The view seen saved before the reload, and the reloaded board read,
      // known by its document (#537): a load listener added after reload()
      // may hear the old document's own load, still due while its frames
      // load. A failure says what was saved.
      const view = () => JSON.parse(localStorage.getItem(VIEW) ?? "null");
      await until(() => view()?.zoom === "55" && !view().shown.includes("desktop"), `the view wasn't saved: ${localStorage.getItem(VIEW)}`);
      frame.contentWindow.location.reload();
      await until(() => frame.contentDocument !== doc && frame.contentDocument?.querySelector(".state"), "the board didn't reload", 10000);
      const again = frame.contentDocument;
      const seen = `the view saved: ${localStorage.getItem(VIEW)}`;
      assertEqual(again.querySelector(".zoom input").value, "55", `zoom after reload (${seen})`);
      assert(!again.querySelector(".devices input").checked, `device tick after reload (${seen})`);
    } finally {
      localStorage.removeItem(VIEW);
      frame.remove();
    }
  },

  "two copies can open the game's files at once where read-only handles exist": async () => {
    // As the engine opens them (static/../src/opfs.rs: mode "read-only"), twice.
    const code = `onmessage = async () => {
      const root = await navigator.storage.getDirectory();
      const dir = await root.getDirectoryHandle("volya-engine-test-shared", { create: true });
      const file = await dir.getFileHandle("probe.bin", { create: true });
      const open = () => file.createSyncAccessHandle({ mode: "read-only" });
      const first = await open();
      let second = null, error = null;
      try { second = await open(); } catch (e) { error = e.name + ": " + e.message; }
      first.close(); second?.close();
      await root.removeEntry("volya-engine-test-shared", { recursive: true });
      postMessage({ shared: Boolean(second), error });
    };`;
    const worker = new Worker(URL.createObjectURL(new Blob([code], { type: "text/javascript" })));
    const result = await new Promise((resolve, reject) => {
      worker.onmessage = ({ data }) => resolve(data);
      worker.onerror = (e) => reject(new Error(e.message));
      worker.postMessage(null);
    });
    worker.terminate();
    if (navigator.userAgentData) assert(result.shared, `Chromium refused a second read-only handle: ${result.error}`);
    return result.shared ? "shared" : `exclusive: ${result.error}`;
  },

  "zoom: the viewport forbids scaling, every element pans without zooming, the guard cancels pinches, the page is at scale 1": async () => {
    // Safari ignores the viewport's limits in tabs (documented); the rest is
    // what blocks zoom there (zoom.js). Real gestures: docs/testing.md.
    const index = new DOMParser().parseFromString(await (await fetch(`${base}index.html`)).text(), "text/html");
    const viewport = index.querySelector('meta[name="viewport"]').content;
    assertEqual(viewport, "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover", "index.html's viewport");
    assertEqual(document.querySelector('meta[name="viewport"]').content, viewport, "the test page's viewport");
    const touchAction = (el) => getComputedStyle(el).touchAction;
    const root = document.documentElement;
    for (const el of [root, document.body, document.querySelector("main"), document.getElementById("status")]) {
      assertEqual(touchAction(el), "pan-x pan-y", `touch-action of ${el.tagName.toLowerCase()}`);
    }
    // The engine's layers take every touch.
    const controls = document.getElementById("touch-controls");
    assert(controls, "the touch controls' layer");
    assert(!root.hasAttribute("data-engine-input"), "the engine owns the screen");
    root.toggleAttribute("data-engine-input", true);
    try {
      for (const el of [root, document.body, document.getElementById("view"), controls]) {
        assertEqual(touchAction(el), "none", `touch-action of ${el.id || el.tagName.toLowerCase()} while the engine takes input`);
      }
    } finally {
      root.toggleAttribute("data-engine-input", false);
    }
    // WebKit's gesture events and two-finger moves are cancelled; one-finger
    // moves (scrolling) are not.
    const cancelled = (type, touches) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      if (touches !== undefined) Object.defineProperty(event, "touches", { value: { length: touches } });
      controls.dispatchEvent(event);
      return event.defaultPrevented;
    };
    for (const type of ["gesturestart", "gesturechange", "gestureend"]) assert(cancelled(type), `${type} not cancelled`);
    assert(cancelled("touchmove", 2), "a two-finger touchmove not cancelled");
    assert(!cancelled("touchmove", 1), "a one-finger touchmove cancelled");
    assertEqual(visualViewport.scale, 1, "visualViewport.scale");
    assert(!root.hasAttribute("data-zoomed"), "marked zoomed at scale 1");
  },

  "diagnostic log: off by default, and nothing runs for it per frame": async () => {
    localStorage.removeItem("volya-engine.diagnostic-log");
    await volya.diagnostics.setEnabled(false, volya.call);
    assert(!volya.diagnostics.enabled(), "on by default");
    const toggle = await openSettingsLog();
    assert(!toggle.checked, "the check box is ticked by default");
    const label = toggle.closest("label").querySelector("strong").textContent;
    assertEqual(label, "Keep a diagnostic log", "label");
    const off = await volya.call("debug_diagnostics_bench", { frames: 20000 });
    assert(!off.on, "on");
    // The size of the kept log doesn't change while off.
    const before = (await volya.call("diagnostic_log_status")).bytes;
    await sleep(300);
    assertEqual((await volya.call("diagnostic_log_status")).bytes, before, "log grew while off");
    return `recording a frame's spans while off: ${off.us_per_frame.toFixed(3)} µs`;
  },

  "diagnostic log: on, it records; the download is one trace file with its sections": async () => {
    const toggle = await openSettingsLog();
    toggle.click();
    await until(() => !logDownload()?.hidden, "no Download diagnostic log once on");
    assert(volya.diagnostics.enabled(), "the setting isn't on");
    assertEqual(localStorage.getItem("volya-engine.diagnostic-log"), "on", "setting not kept");
    const on = await volya.call("debug_diagnostics_bench", { frames: 20000 });
    // A few long frames, and a page event.
    await volya.call("debug_frame_cost", { ms: 70 });
    await sleep(400);
    await volya.call("debug_frame_cost", { ms: 0 });
    volya.diagnostics.event("test marker", { run: "smoke" });
    volya.diagnostics.metric("test.page_ms", 2);
    volya.diagnostics.metric("test.page_ms", 4);
    await sleep(1300);
    const { name, trace } = await downloadLog();
    assert(/^volya-engine-log-\d{4}-\d\d-\d\d-version-[0-9A-Za-z+-]+\.json$/.test(name), `file name ${name}`);
    const events = trace.traceEvents;
    const pid = events.findLast((e) => e.name === "session").pid;
    const mine = events.filter((e) => e.pid === pid);
    const find = (ph, name) => mine.filter((e) => e.ph === ph && e.name === name);
    const meta = find("M", "process_name")[0];
    assert(meta?.args.name.startsWith("Volya Engine "), "no process name");
    const session = find("i", "session")[0].args;
    for (const key of ["version", "user_agent", "screen", "dpr", "cores", "navigation", "restored_reload", "display_mode", "gpu_state", "game"]) {
      assert(key in session, `session lacks ${key}`);
    }
    assert(session.version, "no version");
    assert(["navigate", "reload", "back_forward"].includes(session.navigation), `navigation ${session.navigation}`);
    assertEqual([session.display_mode, session.restored_reload], ["browser", false], "display mode, restored reload");
    assert(/^volya,v:\S+$/.test(find("i", "session")[0].cat), "no commit");
    if (session.game === "imported") assert(/^ver\. \S+$/.test(session.game_version) && "game_language" in session, "no game version");
    else assertEqual(session.game, "not imported", "game");
    if (session.gpu_state === "ready") {
      assert(session.gpu_textures, "no texture path");
      assert(session.gpu_renderer, "no WebGL 2 renderer");
    }
    assert(find("i", "test marker")[0]?.tid === 2, "no page event");
    // A period's series: the spans' times, the metrics and counts.
    const series = (key) => mine.filter((e) => e.ph === "C" && key in e.args).length;
    const page = mine.find((e) => e.ph === "C" && "test.page_ms mean" in e.args)?.args;
    assertEqual([page?.["test.page_ms min"], page?.["test.page_ms mean"], page?.["test.page_ms max"]].join(), "2,3,4", "page metric");
    for (const key of ["frame mean", "frame max", "frame n", "engine max", "render max", "sound max", "frame.interval_ms max", "frame.late", "MiB"]) {
      assert(series(key) > 0, `no ${key} series`);
    }
    const long = find("X", "frame").filter((e) => e.dur >= 50000);
    assert(long.length > 0, "no long frame");
    assert(
      mine.some((e) => e.ph === "X" && e.name === "engine" && e.ts >= long[0].ts && e.ts < long[0].ts + long[0].dur),
      "a long frame without what ran in it",
    );
    if (volya.audioState() === "running") {
      for (const key of ["audio.fill_ms min", "audio.underrun_frames", "audio.mix_ms max", "audio.service_gap_ms max"]) {
        assert(series(key) > 0, `no ${key} series`);
      }
    }
    assertEqual(trace.otherData.cap_bytes, 2 * 2 ** 20, "cap");
    return `recording a frame's spans while on: ${on.us_per_frame.toFixed(2)} µs; ${events.length} events; audio ${volya.audioState()}`;
  },

  "diagnostic log: the session records the imported game's version and installer": async () => {
    const status = await volya.call("game_status");
    if (status.imported || status.written > 0) skip("a real import is present");
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("volya-engine", { create: true });
    const write = async (manifest) => {
      const writable = await (await dir.getFileHandle("import.json", { create: true })).createWritable();
      await writable.write(JSON.stringify(manifest));
      await writable.close();
    };
    /** The session event as a new session would make it now. */
    const session = async () => {
      await volya.call("debug_session_event");
      const { text } = await volya.call("diagnostic_log");
      return JSON.parse(text).traceEvents.findLast((e) => e.name === "session").args;
    };
    const installer = { exe: "setup_test.exe", bin: "setup_test-1.bin", app_name: "Test Game", app_version: "2.0 test" };
    try {
      await write({ language: "ru-RU", complete: true, files: [], installer });
      let s = await session();
      assert(s.version, "no Volya Engine version");
      assertEqual(
        [s.game, s.game_version, s.game_language, s.installer_exe, s.installer_bin, s.installer_name, s.installer_version].join("|"),
        "imported|ver. 1.0006|ru-RU|setup_test.exe|setup_test-1.bin|Test Game|2.0 test",
        "imported game",
      );
      await write({ language: "", complete: false, files: [], installer });
      s = await session();
      assertEqual([s.game, s.game_version, s.game_language].join("|"), "partly imported|ver. 1.0006|", "partial import");
      // Recorded before the installer was.
      await write({ language: "en-US", complete: true, files: [] });
      s = await session();
      assertEqual([s.game, s.game_version, "installer_exe" in s].join("|"), "imported|ver. 1.0006|false", "older record");
      await dir.removeEntry("import.json");
      s = await session();
      assertEqual([s.game, "game_version" in s].join("|"), "not imported|false", "no game");
    } finally {
      await dir.removeEntry("import.json").catch(() => {});
    }
  },

  "diagnostic log: the 2 MiB cap holds, the oldest go first": async () => {
    const { bytes } = await volya.call("debug_diagnostics_fill", { events: 25000 });
    assert(bytes <= 2 * 2 ** 20 && bytes > 1.5 * 2 ** 20, `log of ${bytes} bytes`);
    const { trace } = await downloadLog();
    const fill = trace.traceEvents.filter((e) => e.name?.startsWith("fill "));
    const first = Number(fill[0].name.slice(5));
    assert(first > 0, "the oldest weren't dropped");
    assertEqual(fill.at(-1).name, "fill 0024999", "the newest");
    assertEqual(fill.length, 25000 - first, "a gap");
    return `kept ${bytes} bytes: fill ${first} on`;
  },

  "diagnostic log: turning it off asks first, offers the download, then deletes the log and the kept end": async () => {
    const END_KEY = "volya-engine.diagnostic-log-end";
    volya.diagnostics.event("test kept", { run: "smoke" });
    localStorage.setItem(END_KEY, JSON.stringify({ pid: 1, cat: "volya,v:test", ts: 0, reason: "page_hidden", message: "test" }));
    const button = (label) => [...document.querySelectorAll("#game button")].find((b) => b.textContent === label && !b.hidden);
    let toggle = await openSettingsLog();
    assert(toggle.checked, "the log isn't on");
    // Asked first; Escape (the keyboard) cancels and it stays on.
    toggle.click();
    let ask = document.querySelector("#game .confirm");
    assertEqual(ask?.querySelector("strong").textContent, "Turn off the diagnostic log and delete it?", "the question");
    assert(toggle.checked && volya.diagnostics.enabled(), "off before confirming");
    assertEqual(document.activeElement?.textContent, "Cancel", "focus on Cancel");
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await until(() => !document.querySelector("#game .confirm") && !toggle.disabled, "Escape didn't cancel");
    assert(toggle.checked && (await volya.call("diagnostic_log_status")).on, "off though cancelled");
    assertEqual(document.activeElement, toggle, "focus back on the switch");
    // Asked again: the download is offered meanwhile.
    toggle.click();
    ask = document.querySelector("#game .confirm");
    assert(ask && !logDownload().hidden, "no download offered with the question");
    const kept = await downloadLog();
    assert(kept.trace.traceEvents.some((e) => e.name === "test kept"), "the download lacks the log");
    button("Turn off and delete").click();
    await until(() => !document.querySelector("#game .confirm") && !toggle.disabled, "never turned off");
    assert(!toggle.checked && !volya.diagnostics.enabled(), "still on");
    assertEqual(localStorage.getItem("volya-engine.diagnostic-log"), null, "setting not cleared");
    assertEqual(localStorage.getItem(END_KEY), null, "the kept end is left");
    assert(logDownload().hidden && !button("Delete diagnostic log"), "a log is still offered");
    const status = await volya.call("diagnostic_log_status");
    assertEqual([status.on, status.entries, status.bytes], [false, false, 0], "the log's status");
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("volya-engine");
    const names = [];
    for await (const name of dir.keys()) names.push(name);
    assert(!names.includes("diagnostic-log"), "its files are left");
    // Settings again: nothing offered.
    toggle = await openSettingsLog();
    assert(!toggle.checked && logDownload().hidden, "a deleted log offered");
  },

  "diagnostic log: it survives a reload, in a new session; a log left while off is offered and deleted": async () => {
    // A copy of the app, logging on from its start (this page's log is off).
    localStorage.setItem("volya-engine.diagnostic-log", "on");
    const frame = Object.assign(document.createElement("iframe"), { width: 400, height: 800 });
    frame.style.cssText = "position: fixed; left: -5000px; top: 0";
    frame.src = base;
    document.body.append(frame);
    try {
      /** The copy's log status once on (not session `not`), and its call. */
      const started = async (not) => {
        let found = null;
        await until(
          async () => {
            const call = frame.contentWindow.volya?.call;
            // Calls fail until its engine has started.
            const status = await call?.("diagnostic_log_status").catch(() => null);
            if (status?.on && status.session && status.session.pid !== not) found = { call, status };
            return found;
          },
          "the copy's log isn't on",
          20000,
        );
        return found;
      };
      let { call, status: on } = await started();
      frame.contentWindow.volya.diagnostics.event("test kept", { run: "smoke" });
      // Stored now (a read stores the period so far).
      await call("diagnostic_log");
      const first = on.session.pid;
      await sleep(1100);
      frame.contentWindow.location.reload();
      ({ call, status: on } = await started(first));
      const read = async () => JSON.parse((await call("diagnostic_log")).text);
      // On once its store is open (#428): read at once, from storage.
      let trace = await read();
      assert(trace.otherData.stored, "the reloaded app's log isn't stored");
      assert(trace.traceEvents.some((e) => e.name === "test kept"), "the log didn't survive the reload");
      // Its own session, after the earlier one's events: its event comes
      // once its graphics are set up.
      const mine = () => trace.traceEvents.findLast((e) => e.name === "session" && e.pid === on.session.pid);
      await until(async () => mine() ?? ((trace = await read()), mine()), "no session event of its own", 20000);
      const before = trace.traceEvents.find((e) => e.name === "test kept");
      assert(on.session.pid !== before.pid && mine().ts > before.ts, `sessions ${on.session.pid} ${before.pid}`);
    } finally {
      localStorage.removeItem("volya-engine.diagnostic-log");
      // Gone with its log on: its log is left.
      frame.remove();
    }
    // Here, off, once the copy's worker let go of the files.
    await until(async () => (await volya.call("diagnostic_log_status")).entries, "the copy's log isn't left", 20000);
    const toggle = await openSettingsLog();
    assert(!toggle.checked, "on");
    const remove = [...document.querySelectorAll("#game button")].find((b) => b.textContent === "Delete diagnostic log");
    assert(remove && !remove.hidden && !logDownload().hidden, "the left log isn't offered");
    remove.click();
    assertEqual(document.querySelector("#game .confirm strong")?.textContent, "Delete the diagnostic log?", "the question");
    [...document.querySelectorAll("#game .confirm button")].find((b) => b.textContent === "Delete").click();
    await until(() => !document.querySelector("#game .confirm"), "never deleted");
    assert(remove.hidden && logDownload().hidden, "still offered");
    const status = await volya.call("diagnostic_log_status");
    assertEqual([status.on, status.entries], [false, false], "the log's status");
  },

  "diagnostic log: a lost GPU device and each reload end the session with engine.end, stored": async () => {
    const END_KEY = "volya-engine.diagnostic-log-end";
    // A copy of the app logging on from its start (this page's log is off),
    // finding the end an earlier session's page kept as it closed.
    localStorage.setItem("volya-engine.diagnostic-log", "on");
    const kept = { pid: 12345, cat: "volya,v:test", ts: Date.now() * 1000, reason: "page_closed", message: "the page was closed" };
    localStorage.setItem(END_KEY, JSON.stringify(kept));
    const frame = Object.assign(document.createElement("iframe"), { width: 400, height: 800 });
    frame.style.cssText = "position: fixed; left: -5000px; top: 0";
    frame.src = base;
    document.body.append(frame);
    /** The copy's session once its log is on (not session `not`): `{call, pid}`. */
    const started = async (not) => {
      let found = null;
      await until(
        async () => {
          const call = frame.contentWindow.volya?.call;
          const status = await call?.("diagnostic_log_status").catch(() => null);
          const pid = status?.on ? status.session?.pid : undefined;
          if (pid !== undefined && pid !== not) found = { call, pid };
          return found;
        },
        "the copy's log isn't on",
        20000,
      );
      return found;
    };
    /** Session `pid`'s events as stored (not those waiting for the period's end). */
    const stored = async (call, pid) => {
      const log = await call("diagnostic_log", { stored_only: true });
      return JSON.parse(log.text).traceEvents.filter((e) => e.pid === pid);
    };
    try {
      let session = await started();
      const added = (await stored(session.call, kept.pid)).at(-1);
      assertEqual([added?.name, added?.args.reason, added?.args.recorded_by], ["engine.end", "page_closed", "page"], "the kept end");
      assertEqual(localStorage.getItem(END_KEY), null, "the kept end is taken");
      const notes = [];
      // A lost device: recorded and stored at once.
      const gpu = (await session.call("status")).gpu;
      if (gpu.state === "ready") {
        await session.call("debug_lose_device");
        let lost;
        await until(async () => (lost = (await stored(session.call, session.pid)).findLast((e) => e.name === "engine.end")), "no stored engine.end");
        assertEqual(lost.args.reason, "device_lost", "reason");
        notes.push(`device lost: ${lost.args.message}`);
      } else notes.push(`no GPU: ${gpu.message}`);
      // Reloads: each session's last stored event is its end (sessions are
      // numbered by their start in seconds).
      const by = {};
      for (let i = 0; i < 3; i++) {
        await sleep(1100);
        frame.contentWindow.location.reload();
        const next = await started(session.pid);
        const last = (await stored(next.call, session.pid)).at(-1);
        assertEqual(last?.name, "engine.end", `reload ${i}: the session's last event`);
        assert(["page_closed", "page_hidden"].includes(last.args.reason), `reason ${last.args.reason}`);
        const key = `${last.args.reason} by the ${last.args.recorded_by ?? "engine worker"}`;
        by[key] = (by[key] ?? 0) + 1;
        session = next;
      }
      await frame.contentWindow.volya.diagnostics.setEnabled(false, session.call);
      notes.push(`reloads: ${Object.entries(by).map(([k, n]) => `${n}× ${k}`).join(", ")}`);
      return notes.join("; ");
    } finally {
      localStorage.removeItem("volya-engine.diagnostic-log");
      localStorage.removeItem(END_KEY);
      frame.remove();
    }
  },

  // The update flow with a real next build: the browser suites' runner (`xtask suite`) serves
  // dist-next on request (its engine and a shell module, game-ui.js, tell
  // it apart), both with GitHub Pages' cache lifetime (#446, #450). Near the
  // end: Finish update takes over this page's service worker.
  "Finish update reloads this page into the next build whole, also after a reload": async (resumed) => {
    /** The build this page runs: its engine's, the version shown, its shell module's. */
    const running = async () => {
      await until(() => volya.engineBuild && document.getElementById("status").dataset.state === "ready", "the engine didn't start", 30000);
      const { NEXT_BUILD } = await import(`${base}game-ui.js`);
      return { engine: volya.engineBuild, version: document.getElementById("status").textContent, shell: NEXT_BUILD ? "next" : "first" };
    };
    const now = await running();
    if (!resumed) {
      assertEqual(now.shell, "first", "the shell before the update");
      assert(!now.version.endsWith("-next"), `the version before the update: ${now.version}`);
      await fetch(`${base}__test/next/on`, { method: "POST" });
      await volya.updates.setAuto(true);
      assertEqual(await volya.updates.checkNow(), "found", "check for updates");
      await until(() => volya.updates.status === "ready", `the update didn't download (${volya.updates.status})`, 30000);
      return resumeAfterReload({ before: now.engine, step: "finished" }, () => volya.updates.apply());
    }
    const what = resumed.step === "finished" ? "after Finish update" : "after a reload";
    assert(now.engine !== resumed.before, `${what}: the engine is the first build's`);
    assert(now.version.endsWith("-next"), `${what}: the version shown is ${now.version}`);
    assertEqual(now.shell, "next", `${what}: the shell module`);
    if (resumed.step === "finished") return resumeAfterReload({ ...resumed, step: "reloaded" }, () => location.reload());
    return `${resumed.before} → ${now.engine}`;
  },

  // The same in a copy of the app, its page served by the service worker:
  // back from the next build to the first.
  "Finish update runs the other build whole in a copy of the app, also after a reload": async () => {
    const frame = Object.assign(document.createElement("iframe"), { width: 400, height: 800 });
    frame.style.cssText = "position: fixed; left: -5000px; top: 0";
    const load = () => new Promise((r) => frame.addEventListener("load", r, { once: true }));
    /** The copy's build: engine, version shown and shell module, once its engine runs. */
    const started = async (when) => {
      await until(
        () => {
          const doc = frame.contentDocument;
          const problem = doc?.querySelector("#game h2")?.textContent.includes("couldn't start");
          if (problem) throw new Error(`${when}: ${doc.getElementById("game").textContent}`);
          return frame.contentWindow.volya?.engineBuild && doc.getElementById("status").dataset.state === "ready";
        },
        `${when}: the engine didn't start`,
        30000,
      );
      // The copy's own module (its realm's module map).
      const { NEXT_BUILD } = await new frame.contentWindow.Function("url", "return import(url)")(`${location.origin}${base}game-ui.js`);
      return {
        engine: frame.contentWindow.volya.engineBuild,
        version: frame.contentDocument.getElementById("status").textContent,
        shell: NEXT_BUILD ? "next" : "first",
      };
    };
    let loaded = load();
    frame.src = base;
    document.body.append(frame);
    try {
      await loaded;
      const before = await started("before");
      await fetch(`${base}__test/next/off`, { method: "POST" });
      const updates = frame.contentWindow.volya.updates;
      await updates.setAuto(true);
      assertEqual(await updates.checkNow(), "found", "check for updates");
      await until(() => updates.status === "ready", `the update didn't download (${updates.status})`, 30000);
      loaded = load();
      updates.apply();
      await loaded;
      const after = await started("after Finish update");
      assert(after.engine !== before.engine, "the engine is still the running build's");
      assert(after.version !== before.version, `the version shown is still ${after.version}`);
      assert(after.shell !== before.shell, `the shell module is still the ${after.shell} build's`);
      loaded = load();
      frame.contentWindow.location.reload();
      await loaded;
      assertEqual(await started("after a reload"), after, "after a reload");
      return `${before.engine} → ${after.engine}`;
    } finally {
      await fetch(`${base}__test/next/off`, { method: "POST" });
      frame.remove();
    }
  },

  "layout fits the viewport": () => {
    const overflow = document.documentElement.scrollWidth - window.innerWidth;
    assert(overflow <= 0, `horizontal overflow ${overflow}px`);
  },
};
