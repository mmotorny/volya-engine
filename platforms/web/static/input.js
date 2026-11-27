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
//
// Whenever the game shows its mouse cursor, touch is a touchpad for it
// (touchpad.js, #108): swipes move the cursor, taps click at it, and a
// touch never clicks where it lands. Elsewhere touches go as contacts.
// Each touchpad touch stays one until lifted; direct touches still down
// when the cursor appears are dropped (their button released, the cursor
// left where it is).
//
// The page's text entry is focused in the handler of the touchpad's click
// (`onClick`): that the device's keyboard rises for a focus in a pointerup
// handler is believed, not measured (WebKit dispatches pointer events from
// its touch events). While the cursor shows, touchend is cancelled too: the
// Touch Events specification documents that this suppresses the
// compatibility mouse events and the click, so no synthetic mousedown blurs
// the input just focused.

import { createTouchpad } from "./touchpad.js";

// KeyboardEvent.code → engine key name (layout-independent positions): the
// W3C UI Events code values (a public standard) paired with the game's key
// names by the Windows scan code both stand for (the code tables of the
// standard; Microsoft's DirectInput documentation for the key names). See the
// key list in crates/volya-engine-core/src/input.rs.
export const KEYS = {
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
  NumpadEnter: "numpad_enter",
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

// Mouse `button` → engine button name. UI Events numbers the back and
// forward buttons 3 and 4 (documented), the game's fourth and fifth
// (`mouse4`, `mouse5`); browsers report no further buttons.
const BUTTONS = ["left", "middle", "right", "mouse4", "mouse5"];
// Mouse `button` → its bit in `buttons`.
const BUTTON_BITS = [1, 4, 2, 8, 16];

// Standard Gamepad mapping (W3C) indices → engine names.
const PAD_BUTTONS = {
  0: "south",
  1: "east",
  2: "west",
  3: "north",
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
 * Starts listening; `view()` is the canvas (replaced when the engine starts
 * anew, main.js), `send(event)` posts one message to the engine worker.
 * `onClick()` runs in the event handler of a touchpad gesture pressing the
 * left button (the page may focus its text entry there: a device's keyboard
 * rises only in a user gesture). Returns `setCapture(on)`, called when the
 * engine reports capture changes, `setLevel(on, keys)`, called when the
 * level's controls go live or stop and when the character takes the
 * keyboard's keys under a screen (the inventory screen, #110), and
 * `setCursor(shown)`, called when the game's cursor shows or hides,
 * `setConsole(open)`, called when the console opens or closes, and
 * `consoleKeys`, the keys of the console's text entry for touch screens:
 * `press(name, pressed)`, `type(text)` and `paste(text)`.
 */
export function startInput(view, send, { onClick = () => {} } = {}) {
  let capture = false;
  // The page's text entries, for the save screen's name field
  // (text-entry.js) and the console (console-touch.js): characters typed
  // into them arrive from them.
  const entry = (event) => event.target?.closest?.("#text-entry, #console-entry");
  let level = false;
  // The character takes keys: the level's controls are live, or a screen
  // over the running level passes the keys it doesn't use on (#110).
  let keys = false;
  // The page itself is leaving pointer lock (the level's controls stopped):
  // no release is swallowed then, so nothing is released and no unlock is
  // told (#110: held keys walk on under the inventory screen).
  let leaving = false;
  // The touchpad while the game shows its cursor; which way each touch
  // down goes ("pad" or "direct").
  let cursor = false;
  // The console is open: Insert reads the clipboard.
  let consoleOpen = false;
  const pad = createTouchpad();
  const routes = new Map();
  let longPress = null;
  const padSend = (messages) => {
    for (const message of messages) send(message);
    clearTimeout(longPress);
    const due = pad.deadline();
    if (due !== null) {
      longPress = setTimeout(() => padSend(pad.tick(performance.now())), Math.max(0, due - performance.now()));
    }
  };
  const touchpad = (event) => {
    const [x, y] = at(event);
    const time = event.timeStamp;
    const id = event.pointerId;
    const out = {
      pointerdown: () => pad.down(id, x, y, time),
      pointermove: () => pad.move(id, x, y, time),
      pointerup: () => pad.up(id, x, y, time),
      pointercancel: () => pad.cancel(id, time),
    }[event.type]();
    if (out.some(([type, name, pressed]) => type === "cursor_button" && name === "left" && pressed)) onClick();
    padSend(out);
  };
  // The page's root takes the lock: the canvas lets pointer events through
  // (style.css), and locked events then target the root and reach `pointer`.
  const root = document.documentElement;
  const locked = () => document.pointerLockElement === root;

  const at = (event) => {
    const canvas = view();
    const rect = canvas.getBoundingClientRect();
    // Turned 90° clockwise on upright phones (style.css): the on-screen box
    // is then portrait while the canvas's own size is landscape.
    if (rect.width < rect.height !== canvas.clientWidth < canvas.clientHeight) {
      return [event.clientY - rect.top, rect.right - event.clientX];
    }
    return [event.clientX - rect.left, event.clientY - rect.top];
  };

  const pointer = (event) => {
    if (!capture) return;
    if (event.pointerType === "touch") {
      event.preventDefault();
      if (event.type === "pointerdown") routes.set(event.pointerId, cursor ? "pad" : "direct");
      const route = routes.get(event.pointerId);
      if (event.type === "pointerup" || event.type === "pointercancel") routes.delete(event.pointerId);
      if (route === "pad") {
        touchpad(event);
        return;
      }
      if (route !== "direct") return;
      // Every contact, with its id; the primary one also drives the cursor.
      const phase = { pointerdown: "start", pointermove: "move" }[event.type] ?? "end";
      send(["touch", ...at(event), phase, event.timeStamp, event.pointerId, event.isPrimary]);
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
  // The back and forward buttons navigate on their mouseup, which Chrome
  // lets the page cancel (believed, not measured).
  addEventListener("mouseup", (event) => capture && event.button >= 3 && event.preventDefault());
  addEventListener("touchend", (event) => capture && cursor && event.cancelable && event.preventDefault(), {
    passive: false,
  });
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
    // A character into the text entry is its text, not a key.
    if (entry(event) && event.key?.length === 1 && !event.ctrlKey && !event.metaKey) return;
    if (!BROWSER_KEYS.has(event.code)) event.preventDefault();
    const pressed = event.type === "keydown";
    // Physical presses only; shortcuts with modifiers belong to the
    // browser. While the character takes keys Control is a game key
    // (crouch), so keys pressed with it are the game's there; the engine
    // decides which reach the character.
    if (pressed && (event.repeat || event.metaKey || event.altKey || (event.ctrlKey && !keys))) return;
    press(name, pressed);
  };
  // A key's press or release, from the keyboard or the console's key strip
  // for touch screens (console-touch.js).
  const press = (name, pressed) => {
    if (pressed && name === "insert" && consoleOpen) {
      paste();
      return;
    }
    keySend(["key", name, pressed]);
  };
  // Insert in the open console reads the clipboard (#639): the async
  // clipboard API is the window's, not the worker's (documented, MDN), and
  // Chrome asks permission on its first use (documented). The press goes
  // with what was read, and keys after it wait for it, keeping their order.
  let reads = 0;
  let queue = Promise.resolve();
  const keySend = (message) => {
    if (reads === 0) send(message);
    else queue = queue.then(() => send(message));
  };
  // Pasted into the console's text entry (the device keyboard's own
  // paste): what Insert does with that text.
  const pasted = (text) => {
    keySend(["key", "insert", true, ...(text === "" ? ["none"] : ["text", text])]);
    keySend(["key", "insert", false]);
  };
  const paste = () => {
    reads += 1;
    // Read within the key's handler; no clipboard API is a failed read.
    const read = new Promise((resolve) => resolve(navigator.clipboard.readText())).then(
      (text) => (text === "" ? ["none"] : ["text", text]),
      () => ["unavailable"],
    );
    queue = queue
      .then(() => read)
      .then((clipboard) => {
        reads -= 1;
        send(["key", "insert", true, ...clipboard]);
      });
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
    padSend(pad.reset(performance.now()));
    send(["blur"]);
    for (const last of pads.values()) last.axes = {};
  };
  addEventListener("blur", () => capture && releaseAll());
  // Nor do releases reach a hidden page (our design, #93).
  document.addEventListener("visibilitychange", () => capture && document.visibilityState === "hidden" && releaseAll());
  // Losing pointer lock (Escape, or the browser taking it back) may swallow
  // releases: nothing stays held. The browser takes Escape to leave pointer
  // lock (documented: the Pointer Lock specification's exit gesture; believed
  // for Chrome), so the engine is told: in a level it opens the menu, as
  // Escape would have.
  document.addEventListener("pointerlockchange", () => {
    if (locked()) return;
    if (leaving) {
      leaving = false;
      return;
    }
    if (!capture) return;
    releaseAll();
    send(["unlock"]);
  });
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

  // Touchpad touches down when the cursor hides are forgotten (a drag
  // released); direct touches down when it shows are dropped.
  const setCursor = (shown) => {
    if (shown === cursor) return;
    cursor = shown;
    if (!shown) padSend(pad.reset(performance.now()));
    const direct = [...routes].filter(([, route]) => route === "direct");
    if (shown && direct.length > 0) {
      for (const [id] of direct) routes.set(id, "dropped");
      send(["touch_drop"]);
    }
  };

  const setLevel = (on, takesKeys = on) => {
    level = on;
    keys = takesKeys;
    if (!on && locked()) {
      leaving = true;
      document.exitPointerLock();
    }
  };

  const setCapture = (on) => {
    if (on === capture) return;
    capture = on;
    padSend(pad.reset(performance.now()));
    routes.clear();
    // The engine takes touch gestures (no scrolling or zooming).
    document.documentElement.toggleAttribute("data-engine-input", on);
    pads.clear();
    if (on && !polling) {
      polling = true;
      requestAnimationFrame(poll);
    }
  };
  const setConsole = (open) => {
    consoleOpen = open;
  };
  // The console's text entry and key strip for touch screens
  // (console-touch.js), in order with the keyboard's keys.
  const consoleKeys = {
    press: (name, pressed) => capture && press(name, pressed),
    type: (text) => capture && keySend(["typed", text]),
    paste: (text) => capture && consoleOpen && pasted(text),
  };
  return { setCapture, setLevel, setCursor, setConsole, consoleKeys };
}
