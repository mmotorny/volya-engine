// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Updates. The service worker (sw.js) serves the version this page runs; a
// newer deploy installs as a waiting worker. Browsers look for one when the
// app opens (fetching sw.js, a few KB; sites can't turn that off); an
// installed app is usually resumed rather than reopened, so with automatic
// updates also ask (`registration.update()`) when it returns to the
// foreground, at most hourly.
//
// Downloads are automatic by default: the update downloads in the
// background. Turned off, nothing downloads by itself (metered connections):
// the interface offers the update with its size. Either way the app never
// restarts by itself: once downloaded, the interface offers Finish update
// (the new worker takes over, the page reloads), as Chrome does; closing and
// reopening the app finishes it too (browsers activate a waiting worker once
// no page uses the old one).
//
// The setting lives in the service worker's cache, where sw.js reads it.

/** This build's commit and its time (ISO 8601), set by `cargo xtask web`. */
export const BUILD = "dev";
export const BUILT = "";

/**
 * The version shown: when the build was made (local time, sortable) and its
 * commit, e.g. "2026-11-24 00:58 · b0fa5b8". One version, no SemVer.
 */
export function versionText(build = BUILD, built = BUILT) {
  const time = built ? new Date(built) : null;
  if (!time || Number.isNaN(time.getTime())) return build;
  const two = (n) => String(n).padStart(2, "0");
  const date = `${time.getFullYear()}-${two(time.getMonth() + 1)}-${two(time.getDate())}`;
  return `${date} ${two(time.getHours())}:${two(time.getMinutes())} · ${build}`;
}

const CACHE = "volya-engine-app";
const SETTINGS = "__settings.json";
const MIN_INTERVAL_MS = 60 * 60 * 1000;

/** The settings shared with sw.js, kept in its cache. */
export const settingsStore = (scope, cacheStorage = caches) => {
  const url = new URL(SETTINGS, scope).href;
  return {
    async read() {
      const response = await (await cacheStorage.open(CACHE)).match(url);
      return response ? response.json() : {};
    },
    async write(settings) {
      await (await cacheStorage.open(CACHE)).put(url, new Response(JSON.stringify(settings)));
    },
  };
};

/**
 * Watches `registration` (a promise of a ServiceWorkerRegistration, or a
 * stand-in in tests). Returns the updates' state for the interface:
 * - `status`: "current", "available" (opted out; `bytes`, `build`),
 *   "downloading", or "ready" (waiting to take over);
 * - `auto` and `setAuto(on)`;
 * - `download()`: consent to and download the available update;
 * - `apply()` (finish: take over and reload), `check()`, `onChange(callback)`.
 */
export function startUpdates(registration, { container = navigator.serviceWorker, store = null } = {}) {
  const listeners = new Set();
  let reg = null;
  let settings = { auto: true };
  let applying = false;

  const state = {
    status: "current",
    bytes: 0,
    build: null,
    get auto() {
      return settings.auto !== false;
    },
    async setAuto(on) {
      settings = { ...settings, auto: on };
      await store?.write(settings);
      changed();
      if (on && state.status === "available") await state.check();
    },
    async download() {
      if (state.status !== "available") return;
      settings = { ...settings, consent: state.build };
      await store?.write(settings);
      state.status = "downloading";
      changed();
      await state.check();
    },
    apply() {
      if (!reg?.waiting || applying) return;
      applying = true;
      container.addEventListener("controllerchange", () => location.reload(), { once: true });
      reg.waiting.postMessage({ type: "take-over" });
    },
    async check() {
      if (!reg) return;
      checked = Date.now();
      try {
        await reg.update();
      } catch {
        // Offline, or an update waiting for consent: nothing to do now.
      }
      changed();
    },
    onChange(callback) {
      listeners.add(callback);
      return () => listeners.delete(callback);
    },
  };

  // A waiting worker only matters if one already serves this page
  // (otherwise it is the first install, which takes over by itself).
  const changed = () => {
    if (reg?.waiting && container.controller) state.status = "ready";
    else if (state.status === "ready") state.status = "current";
    for (const callback of listeners) callback(state);
  };

  container?.startMessages?.();
  container?.addEventListener?.("message", ({ data }) => {
    if (!container.controller) return;
    if (data?.type === "update-available") {
      Object.assign(state, { status: "available", bytes: data.bytes, build: data.build });
      changed();
    } else if (data?.type === "update-downloading") {
      Object.assign(state, { status: "downloading", bytes: data.bytes, build: data.build });
      changed();
    }
  });

  let checked = Date.now();
  Promise.resolve(registration).then(async (r) => {
    if (!r) return;
    reg = r;
    store ??= settingsStore(r.scope);
    settings = { auto: true, ...(await store.read().catch(() => ({}))) };
    const watch = (worker) =>
      worker?.addEventListener("statechange", () => {
        // A refused install (no consent) or a finished one.
        if (worker.state === "redundant" && state.status === "downloading") state.status = "current";
        changed();
      });
    r.addEventListener("updatefound", () => watch(r.installing));
    watch(r.installing);
    changed();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && state.auto && Date.now() - checked >= MIN_INTERVAL_MS) state.check();
  });
  return state;
}
