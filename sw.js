// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Service worker: keeps the whole build for offline use and serves it, so a
// page always runs one consistent version. The established precache pattern
// (as in Workbox's injectManifest):
//
// - the site's build (`tools/site`) injects the build's files, each with a hash of its
//   content and its size, into PRECACHE, and a cache name made from them, so
//   this script changes exactly when the build does, and browsers install it
//   anew then (they compare the script's bytes when the app opens and on
//   `registration.update()`).
// - Each build has a cache of its own (CACHE). Install fills it: files an
//   earlier build's cache holds with the same hash are copied, the rest
//   downloaded and checked against their hash; a mismatch (a stale or
//   half-deployed response) fails the install, and the browser tries again at
//   its next check. Nothing it writes is served before it activates.
// - A new worker waits until the page asks it to take over (update.js), so a
//   running page never gets files of another version. Activation deletes the
//   other builds' caches.
// - Files of the build are served from its cache; anything else goes to the
//   network.
//
// Downloading an update needs consent: automatic updates (the default), or
// the player's tap for this version when they turned them off (metered
// connections). Without it, install downloads nothing: it tells the pages
// what the update would download and fails, so the browser drops this worker
// and offers it again at its next check. The first install always downloads:
// the page itself was just loaded from the network.
//
// Cache names are namespaced: the github.io origin is shared
// (docs/architecture/web-platform.md).
//
// Every response it serves carries COOP and COEP (ISOLATION), which GitHub
// Pages can't send: they make the page cross-origin isolated, so it has
// SharedArrayBuffer, which audio output needs (#354). Browsers apply a
// navigation's headers as the service worker's response gives them
// (documented, the Fetch and HTML standards; the method of the common
// coi-serviceworker). A page the service worker didn't serve (the first
// visit, a hard reload) reloads once to be (isolation.js).

/** The prefix of every build's cache. */
const BUILDS = "volya-engine-build-";
/** This build's cache (injected: named by a hash of PRECACHE). */
const CACHE = "volya-engine-build-74688da910dd00fa";
/** This build's commit (injected). */
const BUILD = "6f914a05";
/** `[path, content hash, size]` of every file of the build; `./` is the page. */
const PRECACHE = [["audio-ring.js","84370365160e5bae",1204],["audio-worker.js","f446f1063d14fc14",3545],["audio-worklet.js","4b794e2464e37b82",2269],["console-touch.js","a646006b79476bd1",16070],["diagnostics.js","154722144c9a327b",7033],["direct-touch.js","2b3fa693f2b41d15",7370],["engine-worker.js","a8d85a4bcd45894a",12964],["fullscreen.js","c4759f719df2070c",8495],["game-ui.js","b86ca50c08ec9781",37736],["icon-180.png","e6874c5c48457d36",43246],["icon-192.png","ec21016448fe618a",48598],["icon-512.png","da291d5f56851f87",233991],["icon.svg","4079e3f4cce080d5",5034],["import-worker.js","b29fd451cd873711",2561],["index.html","21a7ee56f195aa48",2166],["./","21a7ee56f195aa48",2166],["input.js","e64ef19b9fffe8df",21689],["install.js","e5824624be13c5cf",2572],["isolation.js","6347d25b906fd8b5",2500],["logo-drawing.svg","1f101c8cf8d61765",4553],["logo-wordmark.svg","904c0aa8b2ef95ac",1986],["logo.svg","2446e97e7516260d",6277],["main.js","fe0a604f1b553c86",15664],["manifest.webmanifest","484bf06580fc2cb6",460],["pkg/volya_engine_web.js","8bbb30b911a0d654",77667],["pkg/volya_engine_web_bg.wasm","9f0216bd2c4e7f92",9910496],["style.css","086a79a865fb56f8",34709],["text-entry.js","69a5151af749ba50",5283],["third-party-licenses.txt","e8f99c7d0193c23d",128645],["touch-art.js","db51f0bbdac51b62",17680],["touch-controls.js","14bb2b8d9f4bfd6e",37009],["touch-wheel.js","cbda651685be7761",3878],["update.js","877958441745872e",9225],["zoom.js","582ad40ab52e4aa4",2046]];
/** The header a kept copy carries its content hash in. */
const HASH = "X-Volya-Engine-Hash";
/** Settings shared with the page (update.js): `{ auto, consent }`. */
const SETTINGS = "__settings.json";
/** The cache the settings live in (update.js). */
const SETTINGS_CACHE = "volya-engine-settings";
/** The one cache of the versions before #444; its settings move on activation. */
const LEGACY = "volya-engine-app";

/** The headers that make a page cross-origin isolated. */
const ISOLATION = { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" };

/**
 * `response` with ISOLATION's headers, and `no-cache`: kept with GitHub
 * Pages' `max-age=600`, Safari reused an earlier build's copy for a worker's
 * module import without asking this worker, after an update took over
 * (measured in the iOS Simulator, #446); revalidating, it asks.
 */
const isolated = (response) => {
  if (response.status === 0) return response;
  const headers = new Headers(response.headers);
  for (const [name, value] of Object.entries(ISOLATION)) headers.set(name, value);
  headers.set("Cache-Control", "no-cache");
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
};

const scope = () => self.registration.scope;
const absolute = (path) => new URL(path, scope()).href;

/** The hash PRECACHE gives `bytes`: the first 8 bytes of its SHA-256, in hex. */
const hashOf = async (bytes) =>
  Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)).slice(0, 8), (b) => b.toString(16).padStart(2, "0")).join("");

/** The stored settings: in their cache, else in the legacy one. */
const readSettings = async () => {
  for (const name of [SETTINGS_CACHE, LEGACY]) {
    if (!(await caches.has(name))) continue;
    const response = await (await caches.open(name)).match(absolute(SETTINGS));
    if (response) return response.json();
  }
  return {};
};

const tell = async (message) => {
  for (const client of await self.clients.matchAll({ includeUncontrolled: true })) client.postMessage(message);
};

/** Downloads `path` into `cache`, failing unless its content has `hash`. */
const download = async (cache, path, hash) => {
  const response = await fetch(absolute(path), { cache: "no-cache" });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  const bytes = await response.arrayBuffer();
  const got = await hashOf(bytes);
  if (got !== hash) throw new Error(`${path}: content ${got}, expected ${hash}`);
  const headers = new Headers(response.headers);
  headers.set(HASH, hash);
  await cache.put(absolute(path), new Response(bytes, { status: response.status, statusText: response.statusText, headers }));
};

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      // This build's own cache first: an interrupted install keeps its files.
      const others = (await caches.keys()).filter((name) => name.startsWith(BUILDS) && name !== CACHE);
      const copies = [];
      const changed = [];
      for (const [path, hash, size] of PRECACHE) {
        if ((await cache.match(absolute(path)))?.headers.get(HASH) === hash) continue;
        let copy = null;
        for (const name of others) {
          const response = await (await caches.open(name)).match(absolute(path));
          if (response?.headers.get(HASH) === hash) {
            copy = response;
            break;
          }
        }
        if (copy) copies.push([path, copy]);
        else changed.push([path, hash, size]);
      }
      const update = Boolean(self.registration.active);
      const settings = await readSettings();
      const consented = !update || settings.auto !== false || settings.consent === BUILD;
      const bytes = changed.reduce((sum, [, , size]) => sum + size, 0);
      if (!consented) {
        await tell({ type: "update-available", build: BUILD, bytes, files: changed.length });
        throw new Error(`update ${BUILD} needs consent`);
      }
      // A first install is no update: nothing to say.
      if (update) await tell({ type: "update-downloading", build: BUILD, bytes, files: changed.length });
      for (const [path, response] of copies) await cache.put(absolute(path), response);
      await Promise.all(changed.map(([path, hash]) => download(cache, path, hash)));
    })(),
  );
});

self.addEventListener("message", (event) => {
  if (event.data?.type === "take-over") self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      // Settings kept by a version before #444 move to their own cache.
      if (await caches.has(LEGACY)) {
        const legacy = await (await caches.open(LEGACY)).match(absolute(SETTINGS));
        const settings = await caches.open(SETTINGS_CACHE);
        if (legacy && !(await settings.match(absolute(SETTINGS)))) await settings.put(absolute(SETTINGS), legacy);
      }
      for (const name of await caches.keys()) {
        if (name === LEGACY || (name.startsWith(BUILDS) && name !== CACHE)) await caches.delete(name);
      }
      // A first install controls the open page at once (it has no version to
      // conflict with); an update only activates when the page asked for it.
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  url.search = "";
  url.hash = "";
  if (!PRECACHE.some(([path]) => absolute(path) === url.href)) return;
  event.respondWith(
    (async () => {
      const kept = await (await caches.open(CACHE)).match(url.href);
      return isolated(kept ?? (await fetch(request)));
    })(),
  );
});
