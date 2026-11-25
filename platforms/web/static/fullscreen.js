// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Full screen with Keyboard Lock while playing: a Volya Engine addition,
// decided by the user (#85). The game crouches on Control, and outside
// full screen with Keyboard Lock the browser keeps shortcuts such as
// Ctrl+W (close the tab), Ctrl+T and Ctrl+N from the page (documented:
// the Keyboard Lock specification, Chrome's Keyboard Lock article).
//
// Offered, never imposed: a small offer in the level's corner, shown where
// the browser has `navigator.keyboard.lock` (desktop Chrome; Safari lacks
// it) while the level's controls are live and the mouse is free (before
// the click that takes pointer lock; it can't be clicked under the lock).
// Accepting, in the click (full screen needs a user gesture):
// `navigator.keyboard.lock(codes)` with the game's bound keys (never Tab,
// F4 or the OS keys: Alt+Tab, Alt+F4 and the OS key stay the system's) and
// W, T, N and Escape, then `requestFullscreen()`, in the order the specification
// recommends (lock first, so the browser shows one exit notice). Under the
// lock a short Escape reaches the game (the menu opens as usual) and
// holding it two seconds leaves full screen (documented: the
// specification's long press); the offer says so. Pointer lock works
// inside it as before (both together: documented, the specification's
// Pointer Lock section). When the player leaves the game the page leaves
// full screen (it was for playing), then unlocks the keyboard.

import { KEYS } from "./input.js";

/** Keys locked whatever the bindings: the browser's tab and window
 * shortcuts with Control, and Escape (the menu). */
const ALWAYS = ["KeyW", "KeyT", "KeyN", "Escape"];

/** Keys never locked, even when bound (our choice): the system's own ways
 * out stay its own (Alt+Tab, Alt+F4, the OS key). */
const NEVER = new Set(["Tab", "F4", "MetaLeft", "MetaRight"]);

/** Engine key name → KeyboardEvent.code. */
const CODES = Object.fromEntries(Object.entries(KEYS).map(([code, name]) => [name, code]));

/** Whether this runs on macOS, from the data the browser gives
 * (`navigator.userAgentData.platform`, else `navigator.platform`). */
const mac = () => /^mac/i.test(navigator.userAgentData?.platform || navigator.platform || "");

/** Whether this is a desktop browser that can lock the keyboard in full
 * screen, not on macOS: there the browser's shortcuts use Command, which
 * is never locked, so the offer's benefit doesn't apply (our choice). */
export const supported = () =>
  Boolean(navigator.keyboard?.lock && document.documentElement.requestFullscreen && document.fullscreenEnabled !== false) &&
  !matchMedia("(pointer: coarse)").matches &&
  !mac();

/**
 * The offer. `boundKeys()` resolves to the engine's names of the keys the
 * game binds. `update({ level })` follows the level's controls; `leave()`
 * ends full screen when the game ends. `show: true` shows it without a
 * browser that supports it (UX audit page).
 *
 * Shown at most once per game, from Play until the player leaves (our
 * choice): it goes once anything hides it (the click that takes pointer
 * lock, the menu, accepting it, or its close button) and doesn't come
 * back until the next game.
 */
export function createFullscreenOffer(boundKeys, { show = false } = {}) {
  const root = document.createElement("div");
  root.id = "fullscreen-offer";
  root.className = "fullscreen-offer";
  root.hidden = true;
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = "Full screen";
  const note = document.createElement("p");
  note.textContent = "Keeps shortcuts such as Ctrl+W in the game. Hold Esc to leave full screen.";
  const close = document.createElement("button");
  close.type = "button";
  close.className = "close";
  close.setAttribute("aria-label", "Close");
  close.textContent = "×";
  root.append(close, button, note);

  // The mouse here is the page's, not the engine's (input.js leaves it).
  for (const type of ["pointerdown", "pointerup", "pointermove", "click", "contextmenu"]) {
    root.addEventListener(type, (event) => event.stopPropagation());
  }

  let codes = [...ALWAYS];
  let live = false;
  let entered = false;
  // Shown and then hidden in this game: not shown again.
  let spent = false;
  const fullscreen = () => Boolean(document.fullscreenElement);

  // Shown while the controls are live, the mouse is free and the page is
  // not in full screen, once.
  const render = () => {
    const shown = (show || supported()) && !spent && live && !document.pointerLockElement && !fullscreen();
    if (!shown && !root.hidden) spent = true;
    root.hidden = !shown;
    // The system pointer shows while the offer does: the engine draws no
    // cursor in the level.
    document.documentElement.toggleAttribute("data-offer", shown);
  };

  close.addEventListener("click", () => {
    spent = true;
    render();
  });
  button.addEventListener("click", () => {
    if (!supported()) return;
    spent = true;
    // Both in the click: the lock first (the specification's order), then
    // full screen, which needs the gesture.
    navigator.keyboard.lock(codes).catch((error) => console.warn(`keyboard lock: ${error.message}`));
    entered = true;
    document.documentElement
      .requestFullscreen({ navigationUI: "hide" })
      .catch((error) => {
        entered = false;
        navigator.keyboard.unlock?.();
        console.warn(`full screen: ${error.message}`);
      })
      .finally(() => render());
  });
  document.addEventListener("fullscreenchange", () => {
    // Left by the player (the long Escape) or the browser: the lock ends
    // with it.
    if (!fullscreen()) {
      entered = false;
      navigator.keyboard?.unlock?.();
    }
    render();
  });
  document.addEventListener("pointerlockchange", () => render());

  return {
    element: root,
    update({ level }) {
      const started = Boolean(level) && !live;
      live = Boolean(level);
      // The bindings as the level's controls go live (the options screen
      // may have changed them), ready for the click.
      if (started) {
        boundKeys()
          .then((names) => {
            const bound = names.map((name) => CODES[name]).filter((code) => code && !NEVER.has(code));
            const next = [...new Set([...ALWAYS, ...bound])];
            const changed = next.join() !== codes.join();
            codes = next;
            // In full screen already: the new bindings are locked now.
            if (changed && entered && fullscreen()) {
              navigator.keyboard?.lock(codes).catch((error) => console.warn(`keyboard lock: ${error.message}`));
            }
          })
          .catch(() => {});
      }
      render();
    },
    /** The game ended: full screen entered here ends too. */
    leave() {
      if (entered && fullscreen()) {
        document
          .exitFullscreen()
          .catch(() => {})
          .finally(() => navigator.keyboard?.unlock?.());
      }
      entered = false;
      spent = false;
    },
    /** For tests: the codes the next lock takes. */
    codes: () => codes,
  };
}
