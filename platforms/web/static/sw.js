// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Service worker: keeps the whole build for offline use and serves it, so a
// page always runs one consistent version. The established precache pattern
// (as in Workbox's injectManifest):
//
// - `cargo xtask web` injects the build's files with a hash of each one's
//   content into PRECACHE, so this script changes exactly when the build
//   does, and browsers install it anew then (they compare the script's bytes
//   on navigation and on `registration.update()`).
// - Install downloads only files whose hash changed or that aren't kept.
// - A new worker waits until the page asks it to take over (the user taps
//   Reload, update.js), so a running page never gets files of another
//   version. Activation deletes copies of files that left the build.
// - Kept files are served from the cache; anything else goes to the network.
//
// The cache name is namespaced: the github.io origin is shared
// (docs/architecture.md).

const CACHE = "volya-engine-app";
/** `[path, content hash]` of every file of the build; `./` is the page. */
const PRECACHE = [/* injected by cargo xtask web */];
/** The PRECACHE of the copies kept, recorded when an install finishes. */
const KEPT = "__precache.json";

const scope = () => self.registration.scope;
const absolute = (path) => new URL(path, scope()).href;

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      const record = await cache.match(absolute(KEPT));
      const kept = new Map(record ? await record.json() : []);
      const changed = [];
      for (const [path, hash] of PRECACHE) {
        if (kept.get(path) !== hash || !(await cache.match(absolute(path)))) changed.push(path);
      }
      await cache.addAll(changed.map((path) => new Request(absolute(path), { cache: "no-cache" })));
      // Recorded last: an interrupted install downloads the rest next time.
      await cache.put(absolute(KEPT), new Response(JSON.stringify(PRECACHE)));
    })(),
  );
});

self.addEventListener("message", (event) => {
  if (event.data?.type === "take-over") self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      const wanted = new Set([...PRECACHE.map(([path]) => absolute(path)), absolute(KEPT)]);
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
  if (!PRECACHE.some(([path]) => absolute(path) === url.href)) return;
  event.respondWith(
    (async () => (await (await caches.open(CACHE)).match(url.href)) ?? fetch(request))(),
  );
});
