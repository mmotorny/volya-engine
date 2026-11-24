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
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Enter: "enter",
  NumpadEnter: "enter",
  Escape: "escape",
  KeyQ: "q",
  F12: "f12",
  Backquote: "grave",
  KeyW: "w",
  KeyA: "a",
  KeyS: "s",
  KeyD: "d",
  KeyX: "x",
  Space: "space",
  ShiftLeft: "left_shift",
  ControlLeft: "left_control",
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
  addEventListener("blur", () => capture && send(["blur"]));
  // Losing pointer lock (Escape, or the browser taking it back) may swallow
  // releases: nothing stays held.
  document.addEventListener("pointerlockchange", () => capture && !locked() && send(["blur"]));

  // Gamepads have no events for buttons and axes: poll each frame.
  const pads = new Map(); // index → { buttons, axes } last sent
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
