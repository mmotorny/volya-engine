// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Service worker: network-first for the app's own files, so online use always
// gets the latest deploy and offline use gets the last copy seen (the whole
// build is copied on install). GitHub Pages
// controls HTTP caching, so requests revalidate (`cache: "no-cache"`).
// The cache name is namespaced: the github.io origin is shared
// (docs/architecture.md).

const CACHE = "volya-engine-app";

// Install keeps the whole build for offline use, even files the app hasn't
// fetched yet. `files.txt` (written by `cargo xtask web`) lists each file after
// a hash of its content; only files whose hash differs from the copy kept (or
// that aren't kept) are downloaded. xtask stamps the build into this file so
// every deploy installs anew.
const MANIFEST = "files.txt";

const parse = (text) =>
  new Map(
    text
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const space = line.indexOf(" ");
        return [line.slice(space + 1), line.slice(0, space)];
      }),
  );

self.addEventListener("install", (event) => {
  self.skipWaiting();
  event.waitUntil(
    (async () => {
      const response = await fetch(MANIFEST, { cache: "no-cache" });
      if (!response.ok) return;
      const text = await response.clone().text();
      const cache = await caches.open(CACHE);
      const kept = await cache.match(MANIFEST);
      const old = kept ? parse(await kept.text()) : new Map();
      const changed = [];
      for (const [file, hash] of parse(text)) {
        if (old.get(file) !== hash || !(await cache.match(file))) changed.push(file);
      }
      await cache.addAll(changed.map((file) => new Request(file, { cache: "no-cache" })));
      // Recorded last: an interrupted install downloads the rest next time.
      await cache.put(MANIFEST, response);
    })().catch((error) => console.warn(`offline copy: ${error.message}`)),
  );
});

self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  const scope = new URL(self.registration.scope);
  if (
    request.method !== "GET" ||
    url.origin !== scope.origin ||
    !url.pathname.startsWith(scope.pathname) ||
    url.pathname.startsWith(`${scope.pathname}__test/`)
  ) {
    return;
  }
  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);
      try {
        const response = await fetch(request, { cache: "no-cache" });
        if (response.ok) await cache.put(request, response.clone());
        return response;
      } catch (error) {
        const cached = await cache.match(request, { ignoreSearch: true });
        if (cached) return cached;
        throw error;
      }
    })(),
  );
});
