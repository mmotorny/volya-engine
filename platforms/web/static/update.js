// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Updates. The service worker (sw.js) serves the version this page runs; a
// newer deploy installs as a waiting worker. Browsers look for one when the
// app opens (fetching sw.js, a few KB; sites can't turn that off); an
// installed app is usually resumed rather than reopened, so with automatic
// updates also ask (`registration.update()`) when it returns to the
// foreground, at most hourly. Settings' Check for updates asks at once.
//
// Downloads are automatic by default: the update downloads in the
// background. Turned off, nothing downloads by itself (metered connections):
// the interface offers the update with its size. Either way the app never
// restarts by itself: once downloaded, the interface offers Finish update
// (the new worker takes over, the page reloads), as Chrome does; closing and
// reopening the app finishes it too (browsers activate a waiting worker once
// no page uses the old one).
//
// Finishing an update in one copy of the app (a tab, a window, the installed
// app) hands every other open copy the new worker too (Service Workers spec,
// Activate; measured in Chrome, #445), leaving them old page code served by
// the new build: they reload themselves at once, whatever they show (the
// user's decision; docs/architecture/web-platform.md).
//
// The setting lives in a cache, where sw.js reads it, with the app's other
// settings (the touch controls', #802).

/** This build's commit and its time (ISO 8601), set by the site's build (`tools/site`). */
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

const CACHE = "volya-engine-settings";
/** Where versions before #444 kept them; sw.js moves them on activation. */
const LEGACY = "volya-engine-app";
const SETTINGS = "__settings.json";
const MIN_INTERVAL_MS = 60 * 60 * 1000;

/**
 * Stores `patch` over the settings in `store` now, keeping the others
 * (each part of the app writes only its own).
 */
export async function saveSettings(store, patch) {
  const stored = await store.read().catch(() => ({}));
  await store.write({ ...stored, ...patch });
}

/** The settings shared with sw.js, kept in a cache of their own. */
export const settingsStore = (scope, cacheStorage = caches) => {
  const url = new URL(SETTINGS, scope).href;
  return {
    async read() {
      for (const name of [CACHE, LEGACY]) {
        if (!(await cacheStorage.has(name))) continue;
        const response = await (await cacheStorage.open(name)).match(url);
        if (response) return response.json();
      }
      return {};
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
 * - `apply()` (finish: take over and reload; reloads anyway when the new
 *   worker doesn't take over within `takeOverMs`: the player asked; an
 *   update another copy finished reloads this page without asking),
 *   `check()`, `onChange(callback)`;
 * - `checkNow()`: the player's check, resolving with "found", "none",
 *   "offline" or "failed"; once found, `requested` is set (the player waits
 *   for this update, so its automatic download is shown too).
 */
export function startUpdates(
  registration,
  { container = navigator.serviceWorker, store = null, reload = () => location.reload(), takeOverMs = 3000 } = {},
) {
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
      if (store) await saveSettings(store, { auto: on });
      changed();
      if (on && state.status === "available") await state.check();
    },
    async download() {
      if (state.status !== "available") return;
      settings = { ...settings, consent: state.build };
      if (store) await saveSettings(store, { consent: state.build });
      state.status = "downloading";
      changed();
      await state.check();
    },
    apply() {
      if (!reg?.waiting || applying) return;
      applying = true;
      let reloaded = false;
      const once = () => {
        if (!reloaded) reload();
        reloaded = true;
      };
      container.addEventListener("controllerchange", once, { once: true });
      setTimeout(once, takeOverMs);
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
    async checkNow() {
      if (!reg) return "failed";
      checked = Date.now();
      // Found: a new worker installs (automatic downloads, or with consent),
      // or the worker tells its size and refuses (opted out; it may say so
      // after update() settles, but installs first).
      let found = false;
      const seen = () => (found = true);
      const offered = ({ data }) => data?.type === "update-available" && seen();
      reg.addEventListener("updatefound", seen);
      container.addEventListener("message", offered);
      let failed = false;
      try {
        await reg.update();
      } catch {
        // A TypeError whether the network is down or sw.js can't be had
        // (Service Workers spec, Update; see docs/architecture/web-platform.md).
        failed = true;
      } finally {
        reg.removeEventListener("updatefound", seen);
        container.removeEventListener("message", offered);
      }
      const update = container.controller && (found || reg.installing || reg.waiting || state.status !== "current");
      if (update) state.requested = true;
      changed();
      if (update) return "found";
      // Offline only when the browser knows it is (onLine true proves nothing).
      if (failed) return navigator.onLine === false ? "offline" : "failed";
      return "none";
    },
    requested: false,
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

  // A new controller this page didn't ask for (not its own apply()) while it
  // had one: another copy finished an update. Without one before, it is the
  // first install taking over.
  let controlled = Boolean(container?.controller);
  container?.addEventListener?.("controllerchange", () => {
    if (controlled && !applying) {
      applying = true;
      reload();
    }
    controlled = Boolean(container.controller);
  });

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
