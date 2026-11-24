// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Service worker: network-first for the app's own files, so online use always
// gets the latest deploy and offline use gets the last copy seen (the whole
// build is copied on install). GitHub Pages
// controls HTTP caching, so requests revalidate (`cache: "no-cache"`).
// The cache name is namespaced: the github.io origin is shared
// (docs/architecture.md).

const CACHE = "volya-engine-app";

// Install precaches the whole build (`files.txt`, written by `cargo xtask
// web`, which also stamps the build into this file so every deploy installs
// anew), so offline use works even for files the app hasn't fetched yet.
self.addEventListener("install", (event) => {
  self.skipWaiting();
  event.waitUntil(
    (async () => {
      const response = await fetch("files.txt", { cache: "no-cache" });
      if (!response.ok) return;
      const files = (await response.text()).split("\n").filter(Boolean);
      const cache = await caches.open(CACHE);
      await cache.addAll(files.map((file) => new Request(file, { cache: "no-cache" })));
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
