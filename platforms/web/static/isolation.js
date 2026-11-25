// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Cross-origin isolation (#354): audio output needs SharedArrayBuffer, which
// only an isolated page has, and only the service worker's responses carry
// the headers that isolate it (sw.js). A page it didn't serve, the first
// visit or a hard reload (which bypasses it), reloads once as soon as the
// service worker can serve it: at once when it is installed, else once it
// is (it then claims the page). This happens before anything shows, so it
// looks like a redirect. A reload that didn't make the page isolated isn't
// tried again in the page session (sessionStorage, cleared once a page is
// isolated), so a browser that ignores the headers isn't reloaded again and
// again; the player is told instead (game-ui.js).

const KEY = "volya-engine.isolation-reload";

/**
 * `"isolated"`, `"reloading"` (the page is being reloaded) or `"blocked"`
 * (not isolated, and a reload won't change it). `registered` is the
 * service worker's registration promise (undefined when registering
 * failed). The environment is given for tests.
 */
export async function ensureIsolated({
  registered,
  isolated = globalThis.crossOriginIsolated === true,
  container = navigator.serviceWorker,
  storage = globalThis.sessionStorage,
  reload = () => location.reload(),
} = {}) {
  if (isolated) {
    try {
      storage.removeItem(KEY);
    } catch {
      // Nothing kept.
    }
    return "isolated";
  }
  if (!container) return "blocked";
  let reloaded = true;
  try {
    reloaded = storage.getItem(KEY) !== null;
  } catch {
    // No session storage: never reload, rather than maybe forever.
  }
  // Served by the service worker and still not isolated: the browser
  // ignores the headers; reloading changes nothing.
  if (reloaded || container.controller) return "blocked";
  const registration = await registered;
  if (!registration) return "blocked";
  if (!registration.active) {
    const installed = await new Promise((resolve) => {
      container.addEventListener("controllerchange", () => resolve(true), { once: true });
      const worker = registration.installing ?? registration.waiting;
      worker?.addEventListener("statechange", () => worker.state === "redundant" && resolve(false));
      if (!worker) resolve(false);
    });
    if (!installed) return "blocked";
  }
  try {
    storage.setItem(KEY, "1");
  } catch {
    return "blocked";
  }
  reload();
  return "reloading";
}
