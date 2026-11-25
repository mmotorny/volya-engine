// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Service worker: keeps the whole build for offline use and serves it, so a
// page always runs one consistent version. The established precache pattern
// (as in Workbox's injectManifest):
//
// - `cargo xtask web` injects the build's files, each with a hash of its
//   content and its size, into PRECACHE, so this script changes exactly when
//   the build does, and browsers install it anew then (they compare the
//   script's bytes when the app opens and on `registration.update()`).
// - Install downloads only files whose hash changed or that aren't kept.
// - A new worker waits until the page asks it to take over (update.js), so a
//   running page never gets files of another version. Activation deletes
//   copies of files that left the build.
// - Kept files are served from the cache; anything else goes to the network.
//
// The engine comes in two wasm builds (wasm-build.js, #186), and a browser
// runs one: their files are marked with the build's name in PRECACHE, and
// only the builds this browser ran are kept (BUILDS, recorded when the page
// says which it runs, or when one of their files is fetched). The first
// install keeps none: the page then tells it. A build's copies kept from
// before builds were recorded (an install older than #186 kept `pkg/`) are
// updated like any kept file until the page says which build it runs; then
// those of builds it doesn't run go.
//
// Downloading an update needs consent: automatic updates (the default), or
// the player's tap for this version when they turned them off (metered
// connections). Without it, install downloads nothing: it tells the pages
// what the update would download and fails, so the browser drops this worker
// and offers it again at its next check. The first install always downloads:
// the page itself was just loaded from the network.
//
// The cache name is namespaced: the github.io origin is shared
// (docs/architecture.md).

const CACHE = "volya-engine-app";
/** This build's commit (injected). */
const BUILD = "dev";
/**
 * `[path, content hash, size, wasm build?]` of every file of the build; `./`
 * is the page. A wasm build's files carry its name.
 */
const PRECACHE = [/* injected by cargo xtask web */];
/** The PRECACHE of the copies kept, recorded when an install finishes. */
const KEPT = "__precache.json";
/** Settings shared with the page (update.js): `{ auto, consent }`. */
const SETTINGS = "__settings.json";
/** The wasm builds this browser runs, kept (wasm-build.js). */
const BUILDS = "__builds.json";

const scope = () => self.registration.scope;
const absolute = (path) => new URL(path, scope()).href;
const readJson = async (cache, path) => {
  const response = await cache.match(absolute(path));
  return response ? response.json() : null;
};

const tell = async (message) => {
  for (const client of await self.clients.matchAll({ includeUncontrolled: true })) client.postMessage(message);
};

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      const kept = await readJson(cache, KEPT);
      const hashes = new Map((kept ?? []).map(([path, hash]) => [path, hash]));
      const builds = (await readJson(cache, BUILDS)) ?? [];
      const changed = [];
      for (const [path, hash, size, build] of PRECACHE) {
        if (build && !builds.includes(build) && !(await cache.match(absolute(path)))) continue;
        if (hashes.get(path) !== hash || !(await cache.match(absolute(path)))) changed.push([path, size]);
      }
      const settings = (await readJson(cache, SETTINGS)) ?? {};
      const consented = kept === null || settings.auto !== false || settings.consent === BUILD;
      const bytes = changed.reduce((sum, [, size]) => sum + size, 0);
      if (!consented) {
        await tell({ type: "update-available", build: BUILD, bytes, files: changed.length });
        throw new Error(`update ${BUILD} needs consent`);
      }
      // A first install is no update: nothing to say.
      if (kept !== null) await tell({ type: "update-downloading", build: BUILD, bytes, files: changed.length });
      await cache.addAll(changed.map(([path]) => new Request(absolute(path), { cache: "no-cache" })));
      // Recorded last: an interrupted install downloads the rest next time.
      await cache.put(absolute(KEPT), new Response(JSON.stringify(PRECACHE)));
    })(),
  );
});

/** Records that this browser runs wasm build `build`: later installs keep it. */
const recordBuild = async (cache, build) => {
  const builds = (await readJson(cache, BUILDS)) ?? [];
  if (!builds.includes(build)) await cache.put(absolute(BUILDS), new Response(JSON.stringify([...builds, build])));
};

/**
 * Keeps wasm build `build` from now on: recorded, its files fetched if
 * missing; copies of builds never recorded (kept before builds were) go.
 */
const keepBuild = async (build) => {
  if (!PRECACHE.some((entry) => entry[3] === build)) return;
  const cache = await caches.open(CACHE);
  await recordBuild(cache, build);
  const builds = (await readJson(cache, BUILDS)) ?? [];
  const missing = [];
  for (const [path, , , b] of PRECACHE) {
    if (b && !builds.includes(b)) await cache.delete(absolute(path));
    else if (b === build && !(await cache.match(absolute(path)))) missing.push(new Request(absolute(path), { cache: "no-cache" }));
  }
  await cache.addAll(missing);
};

self.addEventListener("message", (event) => {
  if (event.data?.type === "take-over") self.skipWaiting();
  else if (event.data?.type === "build") event.waitUntil(keepBuild(event.data.build));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      const wanted = new Set([...PRECACHE.map(([path]) => absolute(path)), absolute(KEPT), absolute(SETTINGS), absolute(BUILDS)]);
      for (const request of await cache.keys()) {
        if (!wanted.has(request.url)) await cache.delete(request);
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
  const entry = PRECACHE.find(([path]) => absolute(path) === url.href);
  if (!entry) return;
  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);
      const kept = await cache.match(url.href);
      if (kept) return kept;
      const response = await fetch(request);
      // A wasm build's file not kept yet: this browser runs that build.
      if (entry[3] && response.ok) {
        const copy = response.clone();
        event.waitUntil(recordBuild(cache, entry[3]).then(() => cache.put(url.href, copy)));
      }
      return response;
    })(),
  );
});
