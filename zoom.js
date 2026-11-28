// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// No browser zoom anywhere in the app (decided by the user, #345): once
// zoomed, the game takes every touch and the page can't be zoomed back out.
// Three layers, each enough alone against a pinch in the iOS Simulator's
// Safari (docs/architecture/web-platform.md, "Browser zoom"):
// - the viewport meta's `maximum-scale=1, user-scalable=no` (index.html),
//   which Safari ignores in tabs since iOS 10 (documented; measured);
// - `touch-action: pan-x pan-y` on every element, `none` where the engine
//   takes input (style.css);
// - here, WebKit's gesture events and two-finger moves cancelled (pointer
//   events, which the game reads, still arrive: measured).
// A page found zoomed anyway lets zooming through until it's back at 1:
// no API resets the zoom (changing the viewport meta doesn't: measured).

/** Whether the page is zoomed (scale 1 within rounding). */
export const zoomed = () => Math.abs((globalThis.visualViewport?.scale ?? 1) - 1) > 0.01;

let installed = false;

/** Installs the guard on the document (once). */
export function blockZoom() {
  if (installed) return;
  installed = true;
  const root = document.documentElement;
  const cancel = (event) => {
    if (event.cancelable && !zoomed()) event.preventDefault();
  };
  // WebKit's pinch: gesturestart, gesturechange, gestureend (Safari only).
  for (const type of ["gesturestart", "gesturechange", "gestureend"]) {
    document.addEventListener(type, cancel, { passive: false, capture: true });
  }
  // Two fingers moving: a pinch in any browser.
  document.addEventListener("touchmove", (event) => event.touches.length > 1 && cancel(event), {
    passive: false,
    capture: true,
  });
  // Zoomed anyway (e.g. a tab from before the guard): style.css lets every
  // touch pan and zoom until the scale is back at 1.
  const mark = () => root.toggleAttribute("data-zoomed", zoomed());
  mark();
  globalThis.visualViewport?.addEventListener("resize", mark);
}
