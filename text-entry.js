// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The device's own keyboard for the save screen's name field on touch
// screens: a Volya Engine addition, decided by the user (#94). The field
// takes only key presses (docs/behavior/save-load-screens.md section 4.4),
// which a touch screen has none of.
//
// An unseen text input lies where the field is while the save screen shows
// it (the engine reports where: {type: "text_field"}), on touch devices
// only. Touch drives the cursor there with the touch controls' stick and
// Click (#856), so the field is clicked with the cursor like everything
// else, and the input never takes taps itself. A device's keyboard rises
// only for an input focused within a user gesture (iOS; that a pointerup
// handler counts is believed, not measured): so the engine reports whether
// the cursor is over the field, and when Click is lifted with the cursor
// over it, touch-controls.js calls `click()` in that very event handler,
// which focuses the input; Click's press reached the engine and focused
// its field as a mouse click does. What is typed into the input is
// sent as text (["text", s]) and the input emptied at once; the engine
// keeps the field's own rules (the characters a key types, its width
// limit). Backspace, Enter and the arrows arrive as keys: from input.js
// when the keyboard reports a key code, otherwise from here. Enter (or
// Esc) ending the field's focus closes the keyboard.

import { KEYS } from "./input.js";

/** A click the engine doesn't answer by focusing its field within this
 * long, milliseconds, closes the keyboard it raised (our choice). */
const FOCUS_WAIT_MS = 500;

/** Editing keys a device keyboard may report without a key code. */
const EDITING = { Backspace: "backspace", Delete: "delete", Enter: "enter", ArrowLeft: "left", ArrowRight: "right" };

/**
 * The text entry; `send(message)` posts one input message to the engine.
 * `update({ rect, focused, over })` follows the engine's field: canvas CSS
 * pixels, or `rect: null` when it doesn't show; `over` while the cursor is
 * over it. `click(at)` runs in the handler of a gesture clicking: Click's
 * lift at the cursor, a direct click's at `at` (canvas CSS pixels, the
 * ergonomic mode, #807).
 */
export function createTextEntry(send) {
  // Laid out as the canvas is (turned on upright phones, style.css), so the
  // engine's canvas positions apply.
  const root = document.createElement("div");
  root.id = "text-entry";
  root.hidden = true;
  const input = document.createElement("input");
  input.type = "text";
  input.setAttribute("aria-label", "Name");
  input.autocomplete = "off";
  input.setAttribute("autocapitalize", "off");
  input.setAttribute("autocorrect", "off");
  input.spellcheck = false;
  input.enterKeyHint = "done";
  root.append(input);

  // An input method composing text (e.g. a Japanese keyboard) keeps it in
  // the input until done: the composed text is sent then.
  let composing = false;
  input.addEventListener("compositionstart", () => {
    composing = true;
  });
  input.addEventListener("compositionend", (event) => {
    composing = false;
    if (event.data) send(["text", event.data]);
    input.value = "";
  });
  input.addEventListener("input", (event) => {
    const type = event.inputType ?? "";
    if (composing || event.isComposing || type === "insertCompositionText" || type === "insertFromComposition") return;
    if (type.startsWith("insert") && event.data) send(["text", event.data]);
    else if (type === "deleteContentBackward") tap("backspace");
    input.value = "";
  });
  input.addEventListener("keydown", (event) => {
    const name = EDITING[event.key];
    if (KEYS[event.code] || !name || event.repeat) return;
    event.preventDefault();
    tap(name);
  });
  const tap = (name) => {
    send(["key", name, true]);
    send(["key", name, false]);
  };

  // Touch devices: a coarse pointer, or once a touch is seen.
  let touched = matchMedia("(pointer: coarse)").matches;
  addEventListener("pointerdown", (event) => {
    if (event.pointerType === "touch") touched = true;
  }, { capture: true });

  let field = { rect: null, focused: false, over: false };
  let wait = null;
  const update = ({ rect, focused, over = false }) => {
    field = { rect, focused, over };
    const shown = Boolean(rect) && touched;
    root.hidden = !shown;
    if (!shown) {
      input.blur();
      return;
    }
    const [x, y, width, height] = rect;
    Object.assign(input.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` });
    if (!focused && document.activeElement === input) input.blur();
  };
  return {
    element: root,
    update,
    click(at = null) {
      if (!field.rect) return;
      const [x, y, width, height] = field.rect;
      const over = at ? at[0] >= x && at[0] < x + width && at[1] >= y && at[1] < y + height : field.over;
      if (!over) return;
      // A touch click is a touch device's.
      touched = true;
      update(field);
      input.focus({ preventScroll: true });
      clearTimeout(wait);
      wait = setTimeout(() => {
        if (!field.focused && document.activeElement === input) input.blur();
      }, FOCUS_WAIT_MS);
    },
  };
}
