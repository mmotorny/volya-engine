// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import { assert, assertEqual, skip } from "./harness.js";

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
      const updates = startUpdates(Promise.resolve(registration), { container, store });
      const waitFor = async () => {
        registration.installing = worker;
        registration.dispatchEvent(new Event("updatefound"));
        registration.installing = null;
        registration.waiting = worker;
        worker.state = "installed";
        worker.dispatchEvent(new Event("statechange"));
      };
      return { updates, registration, container, worker, store: () => saved, waitFor };
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

  // GitHub Pages cannot send COOP/COEP; the engine must not depend on them.
  "page is not cross-origin isolated": () => {
    assertEqual(self.crossOriginIsolated, false);
  },

  "service worker controls the app scope": async () => {
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 10000));
    const registration = await Promise.race([navigator.serviceWorker.ready, timeout]);
    assert(registration.scope.endsWith("/volya-engine/"), `scope ${registration.scope}`);
  },

  "the whole app works offline once installed, with only the wasm build it runs": async () => {
    const registration = await navigator.serviceWorker.ready;
    // The page must be controlled to use the kept copies.
    if (!navigator.serviceWorker.controller) {
      await new Promise((resolve) => navigator.serviceWorker.addEventListener("controllerchange", resolve, { once: true }));
    }
    assert(registration.active, "no active service worker");
    const precache = precacheOf(await (await fetch(`${base}sw.js`, { cache: "no-store" })).text());
    // Of the two wasm builds (#186), the one this browser runs, which the
    // page told the worker to keep once the engine started.
    const { build } = await volya.status();
    const files = precache.filter(([, , , b]) => !b || b === build).map(([path]) => path);
    const others = precache.filter(([, , , b]) => b && b !== build).map(([path]) => path);
    assert(files.includes("./") && files.includes("index.html") && files.some((f) => f.endsWith(".wasm")), `files ${files.join(" ")}`);
    assert(others.some((f) => f.endsWith(".wasm")), `no other wasm build: ${precache.map(([p]) => p).join(" ")}`);
    const cache = await caches.open("volya-engine-app");
    const kept = async (path) => Boolean(await cache.match(new URL(path, registration.scope).href));
    for (let i = 0; i < 100 && !(await Promise.all(files.map(kept))).every(Boolean); i++) {
      await new Promise((r) => setTimeout(r, 100));
    }
    const downloaded = (await Promise.all(others.map(kept))).some(Boolean);
    assert(!downloaded, `the other wasm build is kept too: ${others.join(" ")}`);
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
      delete: async (key) => store.delete(typeof key === "string" ? key : key.url),
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

    // Two wasm builds (#186): a browser keeps only the one it runs.
    const builds = (gpu, gl) => [["c.css", "dddd", 300], ["pkg/e.wasm", gpu, 1000, "webgpu"], ["pkg-webgl/e.wasm", gl, 2000, "webgl"]];
    await install("b5", builds("g1", "l1"));
    assertEqual(downloaded.join(" "), "", "a wasm build kept before it is known to run");
    const told5 = worker("b5", builds("g1", "l1"));
    await told5.run("message", { type: "build", build: "webgl" });
    assertEqual(downloaded.join(" "), "pkg-webgl/e.wasm", "the build the page runs");
    downloaded = [];
    await told5.run("message", { type: "build", build: "webgl" });
    assertEqual(downloaded.join(" "), "", "a kept build again");
    await told5.run("message", { type: "build", build: "none such" });
    assertEqual(downloaded.join(" "), "", "an unknown build");
    // Updates bring the kept build only, and count only it.
    const both = await install("b6", builds("g2", "l2"));
    assertEqual(downloaded.join(" "), "pkg-webgl/e.wasm", "update of the kept build");
    assertEqual(JSON.stringify(told), '[{"type":"update-downloading","build":"b6","bytes":2000,"files":1}]', "update size");
    await both.run("activate");
    assertEqual(kept(), "c.css pkg-webgl/e.wasm", "kept after activation");
    // Kept from before builds were recorded (one build, in pkg/): updated
    // until the page says which it runs, then dropped if not that one.
    store.clear();
    store.set(`${scope}__precache.json`, new Response(JSON.stringify([["c.css", "dddd", 300], ["pkg/e.wasm", "g0", 1000]])));
    store.set(`${scope}c.css`, new Response("css"));
    store.set(`${scope}pkg/e.wasm`, new Response("old wasm"));
    setSettings({ auto: true });
    await install("b7", builds("g3", "l3"));
    assertEqual(downloaded.join(" "), "pkg/e.wasm", "a build kept before builds were recorded");
    downloaded = [];
    await worker("b7", builds("g3", "l3")).run("message", { type: "build", build: "webgl" });
    assertEqual(downloaded.join(" "), "pkg-webgl/e.wasm", "the build the page runs, after an old install");
    assertEqual(kept(), "c.css pkg-webgl/e.wasm", "the build it doesn't run dropped");
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
    settings.click();
    for (let i = 0; i < 50 && !document.querySelector(".screen-head"); i++) await new Promise((r) => setTimeout(r, 20));
    const labels = [...document.querySelectorAll("#game .label")].map((l) => l.textContent);
    assert(labels.includes("Updates") && !labels.includes("Storage"), `labels ${labels}`);
    for (let i = 0; i < 50 && scrollY !== 0; i++) await new Promise((r) => setTimeout(r, 20));
    assertEqual(scrollY, 0, "Settings opened scrolled");
    document.body.style.minHeight = "";
    const toggle = document.querySelector("#game .switch input");
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
          const doc = frame.contentDocument;
          // The level's states show the touch controls (or the full screen
          // offer) instead of a screen.
          const overlay = state.level?.offer ? "fullscreen-offer" : "touch-controls";
          const shown = state.level ? doc?.getElementById(overlay)?.hidden === false : doc?.getElementById("game")?.children.length > 0;
          ok = doc?.documentElement.dataset.ready === "true" && (shown || state.startError);
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
      frame.contentWindow.location.reload();
      await new Promise((r) => frame.addEventListener("load", r, { once: true }));
      for (let i = 0; i < 100 && !frame.contentDocument.querySelector(".state"); i++) await new Promise((r) => setTimeout(r, 20));
      const again = frame.contentDocument;
      assertEqual(again.querySelector(".zoom input").value, "55", "zoom after reload");
      assert(!again.querySelector(".devices input").checked, "device tick after reload");
      localStorage.removeItem("volya-engine.ux-audit.view");
    } finally {
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
    for (const key of ["version", "user_agent", "screen", "dpr", "cores", "build", "gpu_state"]) assert(key in session, `session lacks ${key}`);
    if (session.gpu_state === "ready") assert(["webgpu", "webgl2"].includes(session.gpu_backend) && session.gpu_textures, "no backend");
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
      for (const key of ["audio.fill_ms min", "audio.reply_wait_ms max", "audio.underrun_frames", "audio.mix max"]) {
        assert(series(key) > 0, `no ${key} series`);
      }
    }
    assertEqual(trace.otherData.cap_bytes, 2 * 2 ** 20, "cap");
    return `recording a frame's spans while on: ${on.us_per_frame.toFixed(2)} µs; ${events.length} events; audio ${volya.audioState()}`;
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

  "diagnostic log: off, it is kept and offered; it survives a reload, in a new session": async () => {
    volya.diagnostics.event("test kept", { run: "smoke" });
    const toggle = await openSettingsLog();
    toggle.click();
    await until(() => !toggle.disabled, "never turned off");
    assert(!volya.diagnostics.enabled(), "still on");
    assertEqual(localStorage.getItem("volya-engine.diagnostic-log"), null, "setting not cleared");
    assert(!logDownload().hidden, "a kept log isn't offered");
    const kept = await downloadLog();
    assert(kept.trace.traceEvents.some((e) => e.name === "test kept"), "the event wasn't kept");
    // A new copy of the app, logging on from its start.
    localStorage.setItem("volya-engine.diagnostic-log", "on");
    const frame = Object.assign(document.createElement("iframe"), { width: 400, height: 800 });
    frame.style.cssText = "position: fixed; left: -5000px; top: 0";
    frame.src = base;
    document.body.append(frame);
    try {
      await until(() => frame.contentWindow.volya?.call, "the app didn't start", 15000);
      const call = frame.contentWindow.volya.call;
      // Calls fail until its engine has started.
      const status = () => call("diagnostic_log_status").catch(() => null);
      await until(async () => (await status())?.on, "the reloaded app's log isn't on", 20000);
      const { text } = await call("diagnostic_log");
      const events = JSON.parse(text).traceEvents;
      assert(events.some((e) => e.name === "test kept"), "the log didn't survive the reload");
      // Its own session, after the earlier one's events.
      const now = events.findLast((e) => e.name === "session");
      const before = events.find((e) => e.name === "test kept");
      assert(now && now.pid !== before.pid && now.ts > before.ts, `sessions ${now?.pid} ${before.pid}`);
      await frame.contentWindow.volya.diagnostics.setEnabled(false, call);
    } finally {
      localStorage.removeItem("volya-engine.diagnostic-log");
      frame.remove();
    }
  },

  "layout fits the viewport": () => {
    const overflow = document.documentElement.scrollWidth - window.innerWidth;
    assert(overflow <= 0, `horizontal overflow ${overflow}px`);
  },
};
