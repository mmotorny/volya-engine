// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Page input for the engine: pointer events (mouse and touch), keyboard and
// gamepads (polled), forwarded to the engine worker as compact messages
// (format in src/input.rs) while the engine owns the screen. Otherwise the
// page's DOM UI keeps all input. Positions are CSS pixels relative to the
// canvas; the engine knows the canvas's CSS size from "resize".
//
// While the player's controls are live in a level, a click takes pointer
// lock (mouse look needs relative motion), and leaving the level releases
// it. The on-screen touch controls (touch-controls.js) send their own
// messages; touches on them don't arrive here.

// KeyboardEvent.code → engine key name (layout-independent positions).
const KEYS = {
  Escape: "escape",
  Digit1: "1",
  Digit2: "2",
  Digit3: "3",
  Digit4: "4",
  Digit5: "5",
  Digit6: "6",
  Digit7: "7",
  Digit8: "8",
  Digit9: "9",
  Digit0: "0",
  Minus: "minus",
  Equal: "equals",
  Backspace: "backspace",
  Tab: "tab",
  KeyQ: "q",
  KeyW: "w",
  KeyE: "e",
  KeyR: "r",
  KeyT: "t",
  KeyY: "y",
  KeyU: "u",
  KeyI: "i",
  KeyO: "o",
  KeyP: "p",
  BracketLeft: "left_bracket",
  BracketRight: "right_bracket",
  Enter: "enter",
  NumpadEnter: "enter",
  ControlLeft: "left_control",
  KeyA: "a",
  KeyS: "s",
  KeyD: "d",
  KeyF: "f",
  KeyG: "g",
  KeyH: "h",
  KeyJ: "j",
  KeyK: "k",
  KeyL: "l",
  Semicolon: "semicolon",
  Quote: "apostrophe",
  Backquote: "grave",
  ShiftLeft: "left_shift",
  Backslash: "backslash",
  KeyZ: "z",
  KeyX: "x",
  KeyC: "c",
  KeyV: "v",
  KeyB: "b",
  KeyN: "n",
  KeyM: "m",
  Comma: "comma",
  Period: "period",
  Slash: "slash",
  ShiftRight: "right_shift",
  NumpadMultiply: "numpad_multiply",
  AltLeft: "left_alt",
  Space: "space",
  CapsLock: "caps_lock",
  F1: "f1",
  F2: "f2",
  F3: "f3",
  F4: "f4",
  F5: "f5",
  F6: "f6",
  F7: "f7",
  F8: "f8",
  F9: "f9",
  F10: "f10",
  NumLock: "num_lock",
  ScrollLock: "scroll_lock",
  Numpad7: "numpad7",
  Numpad8: "numpad8",
  Numpad9: "numpad9",
  NumpadSubtract: "numpad_subtract",
  Numpad4: "numpad4",
  Numpad5: "numpad5",
  Numpad6: "numpad6",
  NumpadAdd: "numpad_add",
  Numpad1: "numpad1",
  Numpad2: "numpad2",
  Numpad3: "numpad3",
  Numpad0: "numpad0",
  NumpadDecimal: "numpad_decimal",
  F11: "f11",
  F12: "f12",
  F13: "f13",
  F14: "f14",
  F15: "f15",
  KanaMode: "kana",
  Convert: "convert",
  NonConvert: "no_convert",
  IntlYen: "yen",
  NumpadEqual: "numpad_equals",
  ControlRight: "right_control",
  NumpadComma: "numpad_comma",
  NumpadDivide: "numpad_divide",
  PrintScreen: "print_screen",
  AltRight: "right_alt",
  Pause: "pause",
  Home: "home",
  ArrowUp: "up",
  PageUp: "page_up",
  ArrowLeft: "left",
  ArrowRight: "right",
  End: "end",
  ArrowDown: "down",
  PageDown: "page_down",
  Insert: "insert",
  Delete: "delete",
  MetaLeft: "left_win",
  MetaRight: "right_win",
  ContextMenu: "apps",
};
// Keys whose browser action (developer tools) is left alone.
const BROWSER_KEYS = new Set(["F12"]);

// Mouse `button` → engine button name.
const BUTTONS = ["left", "middle", "right"];
// Mouse `button` → its bit in `buttons`.
const BUTTON_BITS = [1, 4, 2];

// Standard Gamepad mapping (W3C) indices → engine names.
const PAD_BUTTONS = {
  0: "south",
  1: "east",
  2: "west",
  4: "left_shoulder",
  5: "right_shoulder",
  10: "left_stick",
  12: "dpad_up",
  13: "dpad_down",
  14: "dpad_left",
  15: "dpad_right",
};
const PAD_AXES = { 0: "left_x", 1: "left_y", 2: "right_x", 3: "right_y" };
// Analog triggers are buttons with a value in the standard mapping.
const PAD_TRIGGERS = { 6: "left_trigger", 7: "right_trigger" };
// CSS pixels of wheel scrolling per wheel step.
const WHEEL_PIXELS = 50;
// Axis changes smaller than this are not sent.
const AXIS_EPSILON = 0.02;

/**
 * Starts listening; `send(event)` posts one message to the engine worker.
 * Returns `setCapture(on)`, called when the engine reports capture changes,
 * and `setLevel(on)`, called when the level's controls go live or stop.
 */
export function startInput(view, send) {
  let capture = false;
  let level = false;
  // The page's root takes the lock: the canvas lets pointer events through
  // (style.css), and locked events then target the root and reach `pointer`.
  const root = document.documentElement;
  const locked = () => document.pointerLockElement === root;

  const at = (event) => {
    const rect = view.getBoundingClientRect();
    // Turned 90° clockwise on upright phones (style.css): the on-screen box
    // is then portrait while the canvas's own size is landscape.
    if (rect.width < rect.height !== view.clientWidth < view.clientHeight) {
      return [event.clientY - rect.top, rect.right - event.clientX];
    }
    return [event.clientX - rect.left, event.clientY - rect.top];
  };

  const pointer = (event) => {
    if (!capture) return;
    if (event.pointerType === "touch") {
      // Every contact, with its id; the primary one also drives the cursor.
      const phase = { pointerdown: "start", pointermove: "move" }[event.type] ?? "end";
      send(["touch", ...at(event), phase, event.timeStamp, event.pointerId, event.isPrimary]);
      event.preventDefault();
      return;
    }
    // Mouse and pen. A button pressed or released while another is held
    // arrives as pointermove with `button` set.
    if (event.type === "pointercancel") return;
    // Mouse look needs pointer lock, which browsers grant on a click.
    if (level && event.type === "pointerdown" && event.pointerType === "mouse" && !locked()) {
      root.requestPointerLock?.()?.catch?.(() => {});
    }
    if (event.button >= 0 && BUTTONS[event.button]) {
      const pressed = event.type === "pointermove" ? (event.buttons & BUTTON_BITS[event.button]) !== 0 : event.type === "pointerdown";
      send(["button", ...at(event), BUTTONS[event.button], pressed, event.timeStamp]);
      if (event.type !== "pointermove") event.preventDefault();
    } else if (locked()) {
      send(["motion", event.movementX, event.movementY]);
    } else {
      send(["move", ...at(event)]);
    }
  };
  for (const type of ["pointermove", "pointerdown", "pointerup", "pointercancel"]) {
    addEventListener(type, pointer, { passive: false });
  }
  addEventListener("contextmenu", (event) => capture && event.preventDefault());
  // Wheel steps (our choice): one per line or page, one per WHEEL_PIXELS of
  // pixel scrolling (trackpads send many small deltas); negative is up.
  let wheel = 0;
  addEventListener(
    "wheel",
    (event) => {
      if (!capture) return;
      event.preventDefault();
      if (event.deltaMode !== 0) {
        send(["wheel", Math.sign(event.deltaY)]);
        return;
      }
      wheel += event.deltaY;
      const steps = Math.trunc(wheel / WHEEL_PIXELS);
      if (steps !== 0) {
        wheel -= steps * WHEEL_PIXELS;
        send(["wheel", steps]);
      }
    },
    { passive: false },
  );

  const key = (event) => {
    const name = KEYS[event.code];
    if (!capture || !name) return;
    if (!BROWSER_KEYS.has(event.code)) event.preventDefault();
    const pressed = event.type === "keydown";
    // Physical presses only; shortcuts with modifiers belong to the
    // browser. While the level's controls are live Control is a game key
    // (crouch), so keys pressed with it are the game's there.
    if (pressed && (event.repeat || event.metaKey || event.altKey || (event.ctrlKey && !level))) return;
    send(["key", name, pressed]);
  };
  addEventListener("keydown", key);
  addEventListener("keyup", key);
  // Gamepads have no events for buttons and axes: poll each frame.
  const pads = new Map(); // index → { buttons, axes } last sent
  // The engine releases everything. Our choice: a button still held counts
  // as released until pressed again, like a key (sending it again would be
  // a fresh press: a toggle, a jump, an Escape); stick and trigger values are
  // sent again by the next poll.
  const releaseAll = () => {
    send(["blur"]);
    for (const last of pads.values()) last.axes = {};
  };
  addEventListener("blur", () => capture && releaseAll());
  // Losing pointer lock (Escape, or the browser taking it back) may swallow
  // releases: nothing stays held.
  document.addEventListener("pointerlockchange", () => capture && !locked() && releaseAll());
  let polling = false;
  const poll = () => {
    if (!capture) {
      polling = false;
      return;
    }
    for (const pad of navigator.getGamepads?.() ?? []) {
      if (!pad || pad.mapping !== "standard") continue;
      const last = pads.get(pad.index) ?? { buttons: {}, axes: {} };
      pads.set(pad.index, last);
      for (const [index, name] of Object.entries(PAD_BUTTONS)) {
        const pressed = pad.buttons[index]?.pressed ?? false;
        if (pressed !== (last.buttons[index] ?? false)) {
          last.buttons[index] = pressed;
          send(["pad", name, pressed]);
        }
      }
      const values = [
        ...Object.entries(PAD_AXES).map(([index, name]) => [`a${index}`, name, pad.axes[index] ?? 0]),
        ...Object.entries(PAD_TRIGGERS).map(([index, name]) => [`t${index}`, name, pad.buttons[index]?.value ?? 0]),
      ];
      for (const [index, name, value] of values) {
        if (Math.abs(value - (last.axes[index] ?? 0)) >= AXIS_EPSILON) {
          last.axes[index] = value;
          send(["axis", name, value]);
        }
      }
    }
    requestAnimationFrame(poll);
  };

  const setLevel = (on) => {
    level = on;
    if (!on && locked()) document.exitPointerLock();
  };

  const setCapture = (on) => {
    if (on === capture) return;
    capture = on;
    // The engine takes touch gestures (no scrolling or zooming).
    document.documentElement.toggleAttribute("data-engine-input", on);
    pads.clear();
    if (on && !polling) {
      polling = true;
      requestAnimationFrame(poll);
    }
  };
  return { setCapture, setLevel };
}
