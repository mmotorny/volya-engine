// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The console on touch screens: a Volya Engine addition, decided by the
// user (#642; the proposal on #609). The original opens the console only
// with a key and types into it only by key codes (console-window.md
// section 1, console-input.md section 2), which a touch screen has none
// of. On touch devices only:
//
// - A Console button (`>_`, a disc as the level's touch controls draw
//   them): in a level right of Menu (touch-controls.js), over the main
//   menu here, at the same top-left place, while a key bound to `console`
//   would open the console there (the engine says: {type: "console",
//   menu}). It presses the engine's Console touch button, which opens the
//   console as that key does, and in the same tap focuses an unseen text
//   input, so the device's keyboard rises (as text-entry.js does for the
//   save screen's name field: iOS raises it only for a focus within a
//   user gesture, and a touch's pointerup is one, documented by HTML's
//   activation-triggering events; that iOS honours it there is believed,
//   not measured). If the console doesn't open within FOCUS_WAIT_MS the
//   keyboard is closed again.
// - What is typed goes to the engine as characters (["typed", s]), which
//   become the presses of the keys that type them on the console's table;
//   characters no key types do nothing there. The keyboard's Backspace and
//   Return are Backspace and Enter; its own paste is what Insert does with
//   the pasted text.
// - A key strip above the device's keyboard for the keys it lacks: Esc,
//   Tab, Up, Down, Page Up, Page Down and Paste (Insert). Each is held
//   from its press to its release, so the console's own key repeat
//   applies; Paste acts on its tap's release, where the clipboard may be
//   read (the same activation rule).
// - Hiding the keyboard leaves the console open; a tap on the console's
//   area (the top half while a level or the menu is below,
//   console-window.md 4.1) raises it again. The console closing closes it.

import { KEYS } from "./input.js";
import { approximateArt, gameArt, paintDisc, paintGlyph, vectorGlyph } from "./touch-art.js";

/** A tap the engine doesn't answer by opening the console within this
 * long, milliseconds, closes the keyboard it raised (our choice, as
 * text-entry.js). */
const FOCUS_WAIT_MS = 500;
/** The Console button's size, CSS pixels (as the level's, style.css). */
const SIZE = 44;

/** Keys a device keyboard may report without a key code. */
const EDITING = {
  Backspace: "backspace",
  Enter: "enter",
  Escape: "escape",
  Tab: "tab",
  ArrowUp: "up",
  ArrowDown: "down",
  PageUp: "page_up",
  PageDown: "page_down",
};

/** The key strip: engine key name, label, and its spoken name where the
 * label is a symbol or an abbreviation. */
const STRIP = [
  ["escape", "Esc", "Escape"],
  ["tab", "Tab"],
  ["up", "↑", "Up"],
  ["down", "↓", "Down"],
  ["page_up", "PgUp", "Page Up"],
  ["page_down", "PgDn", "Page Down"],
  ["insert", "Paste"],
];

/**
 * The console's touch interface; `send(message)` posts one input message
 * to the engine, `keys` is input.js's `consoleKeys` (`press(name,
 * pressed)`, `type(text)`, `paste(text)`), and `call(op, args)` runs an
 * engine op (for the game's textures). `update({ open, menu })` follows
 * the engine's console, `setCapture(on)` whether the engine owns the
 * screen; `raise()` runs in the handler of a tap on a Console button.
 * `touch` says the device has touch without waiting for one (UX audit
 * page).
 */
export function createConsoleTouch(send, keys, { call = null, touch = false } = {}) {
  // In the game's landscape frame, as the canvas (turned on upright
  // phones, style.css): the menu's Console button, the console's area and
  // the unseen input (never hidden itself: a hidden input can't take
  // focus; it takes no taps).
  const root = document.createElement("div");
  root.id = "console-touch";
  root.className = "touch-controls";
  const button = document.createElement("button");
  button.type = "button";
  button.className = "touch-button console";
  button.setAttribute("aria-label", "Console");
  const disc = document.createElement("canvas");
  disc.className = "touch-disc";
  const glyph = document.createElement("canvas");
  glyph.className = "touch-glyph";
  for (const c of [disc, glyph]) c.setAttribute("aria-hidden", "true");
  button.append(disc, glyph);
  const area = document.createElement("div");
  area.className = "console-area";
  area.setAttribute("aria-label", "Show keyboard");
  area.setAttribute("role", "button");
  const input = document.createElement("input");
  input.id = "console-entry";
  input.type = "text";
  input.setAttribute("aria-label", "Console input");
  input.autocomplete = "off";
  input.setAttribute("autocapitalize", "off");
  input.setAttribute("autocorrect", "off");
  input.spellcheck = false;
  input.enterKeyHint = "go";
  root.append(area, button, input);

  // Above the device's keyboard, upright with it (not turned).
  const strip = document.createElement("div");
  strip.id = "console-keys";
  strip.hidden = true;
  strip.setAttribute("role", "toolbar");
  strip.setAttribute("aria-label", "Console keys");
  for (const [name, label, spoken] of STRIP) {
    const key = document.createElement("button");
    key.type = "button";
    key.dataset.key = name;
    key.textContent = label;
    key.tabIndex = -1;
    if (spoken) key.setAttribute("aria-label", spoken);
    strip.append(key);
  }

  let touched = touch || matchMedia("(pointer: coarse)").matches;
  addEventListener(
    "pointerdown",
    (event) => {
      if (event.pointerType !== "touch" || touched) return;
      touched = true;
      render();
    },
    { capture: true },
  );
  let capture = false;
  let open = false;
  let menu = false;
  let focused = false;
  let wait = null;
  /** The keyboard's height (CSS pixels) on the UX audit page, which has
   * none; null to follow the visual viewport. */
  let keyboardHeight = null;

  const raise = () => {
    touched = true;
    input.focus({ preventScroll: true });
    clearTimeout(wait);
    wait = setTimeout(() => {
      if (!open && document.activeElement === input) input.blur();
    }, FOCUS_WAIT_MS);
  };
  // Held strip keys, released when the strip goes.
  const held = new Map(); // key element → its name
  const releaseAll = () => {
    for (const [key, name] of held) {
      key.classList.remove("down");
      keys.press(name, false);
    }
    held.clear();
  };
  const render = () => {
    const shown = touched && capture;
    button.hidden = !(shown && menu && !open);
    area.hidden = !(shown && open && !focused);
    const keyStrip = shown && open && focused;
    if (!keyStrip) releaseAll();
    strip.hidden = !keyStrip;
    if (keyStrip) place();
  };
  // At the visual viewport's bottom, which the device's keyboard raises
  // (documented: the Visual Viewport API; that iOS shrinks it for the
  // keyboard is believed, not measured).
  const place = () => {
    if (strip.hidden) return;
    if (keyboardHeight !== null) {
      Object.assign(strip.style, { top: "auto", bottom: `${keyboardHeight}px`, width: "100%" });
      return;
    }
    const v = globalThis.visualViewport;
    const bottom = v ? v.offsetTop + v.height : innerHeight;
    Object.assign(strip.style, {
      top: `${bottom - strip.offsetHeight}px`,
      left: `${v ? v.offsetLeft : 0}px`,
      width: `${v ? v.width : innerWidth}px`,
    });
  };
  globalThis.visualViewport?.addEventListener("resize", place);
  globalThis.visualViewport?.addEventListener("scroll", place);
  input.addEventListener("focus", () => {
    focused = true;
    render();
  });
  input.addEventListener("blur", () => {
    focused = false;
    render();
  });

  // Typing, as text-entry.js: composed text once composing ends, the input
  // emptied at once; keys with a key code are input.js's.
  let composing = false;
  input.addEventListener("compositionstart", () => {
    composing = true;
  });
  input.addEventListener("compositionend", (event) => {
    composing = false;
    if (event.data) keys.type(event.data);
    input.value = "";
  });
  const tap = (name) => {
    keys.press(name, true);
    keys.press(name, false);
  };
  input.addEventListener("input", (event) => {
    const type = event.inputType ?? "";
    if (composing || event.isComposing || type === "insertCompositionText" || type === "insertFromComposition") return;
    if (type === "insertFromPaste" || type === "insertFromDrop") keys.paste(event.data ?? input.value);
    else if (type === "insertLineBreak" || type === "insertParagraph") tap("enter");
    else if (type.startsWith("insert") && event.data) keys.type(event.data);
    else if (type === "deleteContentBackward") tap("backspace");
    input.value = "";
  });
  input.addEventListener("keydown", (event) => {
    const name = EDITING[event.key];
    if (KEYS[event.code] || !name || event.repeat) return;
    event.preventDefault();
    tap(name);
  });

  // Touches on these stay with them (not input.js's), and take no focus
  // from the input: no compatibility mouse events, no click (documented:
  // Pointer Events and Touch Events).
  const own = (target, handlers) => {
    for (const [type, handler] of Object.entries(handlers)) {
      target.addEventListener(
        type,
        (event) => {
          event.stopPropagation();
          event.preventDefault();
          handler(event);
        },
        { passive: false },
      );
    }
    for (const type of ["touchstart", "touchend", "mousedown"]) {
      target.addEventListener(type, (event) => event.cancelable && event.preventDefault(), { passive: false });
    }
    target.addEventListener("contextmenu", (event) => event.preventDefault());
  };
  const captured = (event) => {
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Not an active pointer.
    }
  };
  // The menu's Console button: its tap's release, as the level's.
  own(button, {
    pointerdown: (event) => {
      captured(event);
      button.classList.add("down");
    },
    pointerup: () => {
      if (!button.classList.contains("down")) return;
      button.classList.remove("down");
      send(["tbutton", "console", true]);
      send(["tbutton", "console", false]);
      raise();
    },
    pointercancel: () => button.classList.remove("down"),
  });
  own(area, {
    pointerdown: captured,
    pointerup: () => open && raise(),
  });
  for (const key of strip.children) {
    const name = key.dataset.key;
    const paste = name === "insert";
    own(key, {
      pointerdown: (event) => {
        captured(event);
        key.classList.add("down");
        if (paste || held.has(key)) return;
        held.set(key, name);
        keys.press(name, true);
      },
      pointerup: () => {
        if (!key.classList.contains("down")) return;
        key.classList.remove("down");
        // The clipboard is read in this handler (input.js).
        if (paste) tap(name);
        else if (held.delete(key)) keys.press(name, false);
        // A tap that took the input's focus after all gives it back.
        if (open && document.activeElement !== input) raise();
      },
      pointercancel: () => {
        key.classList.remove("down");
        if (held.delete(key)) keys.press(name, false);
      },
    });
  }

  // The art, as the level's touch controls draw theirs.
  let art = approximateArt();
  const paint = () => {
    paintDisc(disc, art, SIZE);
    const size = SIZE * 0.56;
    paintGlyph(glyph, vectorGlyph("console", Math.round(size * (devicePixelRatio || 1))), size, size, art.grey);
    for (const [name, color] of Object.entries({ teal: art.teal, "teal-hi": art.tealHi, grey: art.grey })) {
      root.style.setProperty(`--hud-${name}`, color.join(", "));
    }
  };
  paint();
  let asked = false;
  const askArt = () => {
    if (asked || !call) return;
    asked = true;
    gameArt(call).then(
      (game) => {
        art = game;
        paint();
      },
      () => {
        asked = false;
      },
    );
  };

  render();
  return {
    elements: [root, strip],
    raise,
    update({ open: isOpen, menu: overMenu }) {
      open = Boolean(isOpen);
      menu = Boolean(overMenu);
      if (menu) askArt();
      if (!open && document.activeElement === input) input.blur();
      render();
    },
    setCapture(on) {
      capture = on;
      if (!on && document.activeElement === input) input.blur();
      render();
    },
    /** A keyboard `height` CSS pixels high and the strip on it, `down` a
     * strip key held (UX audit page). */
    showKeys({ height, down } = {}) {
      keyboardHeight = height;
      focused = true;
      render();
      if (down) strip.querySelector(`[data-key="${down}"]`).classList.add("down");
    },
  };
}
