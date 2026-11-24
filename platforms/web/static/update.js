// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Updates: the service worker (sw.js) serves the version this page runs; a
// newer deploy installs as a waiting worker. Browsers look for one when the
// app is opened; an installed app is usually resumed rather than reopened, so
// also ask (`registration.update()`, one small request) when it comes back to
// the foreground, at most once an hour. When a worker is waiting, offer a
// reload; on the tap the new worker takes over and the page reloads. Never
// by itself: that would interrupt a game. The offer sits in the page's own
// interface, which is hidden while the engine owns the screen.

/** This build's commit, set by `cargo xtask web`. */
export const BUILD = "dev";

const MIN_INTERVAL_MS = 60 * 60 * 1000;

/**
 * Offers updates found by `registration` (a ServiceWorkerRegistration, or a
 * stand-in in tests) with a button prepended to `footer`. Returns
 * `{ button, check }`.
 */
export function startUpdateCheck(footer, registration, container = navigator.serviceWorker) {
  const button = document.createElement("button");
  button.className = "primary wide";
  button.hidden = true;
  button.textContent = "Update available · Reload";
  footer.prepend(button);

  // A waiting worker only matters if one already serves this page
  // (otherwise it is the first install, which takes over by itself).
  const offer = () => {
    if (registration.waiting && container.controller) button.hidden = false;
  };
  const watch = (worker) => worker?.addEventListener("statechange", () => worker.state === "installed" && offer());
  registration.addEventListener("updatefound", () => watch(registration.installing));
  watch(registration.installing);
  offer();

  button.addEventListener("click", () => {
    container.addEventListener("controllerchange", () => location.reload(), { once: true });
    registration.waiting?.postMessage({ type: "take-over" });
  });

  let checked = Date.now();
  const check = async () => {
    checked = Date.now();
    try {
      await registration.update();
    } catch {
      // Offline: try again later.
    }
    offer();
  };
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && Date.now() - checked >= MIN_INTERVAL_MS) check();
  });
  return { button, check };
}
