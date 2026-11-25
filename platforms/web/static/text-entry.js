// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The device's own keyboard for the save screen's name field on touch
// screens: a Volya Engine addition, decided by the user (#94). The field
// takes only key presses (docs/behavior/save-load-screens.md section 4.4),
// which a touch screen has none of.
//
// A transparent text input lies over the field while the save screen shows
// it (the engine reports where: {type: "text_field"}), on touch devices
// only. A tap on the field lands on it: the browser focuses it and raises
// its keyboard (which needs the tap itself; a page can't raise it later),
// and the same tap reaches the engine, focusing its field as a click would.
// What is typed into the input is sent as text (["text", s]) and the input
// emptied at once; the engine keeps the field's own rules (the characters
// a key types, its width limit). Backspace, Enter and the arrows arrive as
// keys: from input.js when the keyboard reports a key code, otherwise from
// here. Enter (or Esc) ending the field's focus closes the keyboard.

import { KEYS } from "./input.js";

/** Editing keys a device keyboard may report without a key code. */
const EDITING = { Backspace: "backspace", Delete: "delete", Enter: "enter", ArrowLeft: "left", ArrowRight: "right" };

/**
 * The text entry; `send(message)` posts one input message to the engine.
 * `update({ rect, focused })` follows the engine's field: canvas CSS
 * pixels, or `rect: null` when it doesn't show.
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

  return {
    element: root,
    update({ rect, focused }) {
      const shown = Boolean(rect) && touched;
      root.hidden = !shown;
      if (!shown) {
        input.blur();
        return;
      }
      const [x, y, width, height] = rect;
      Object.assign(input.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` });
      if (!focused && document.activeElement === input) input.blur();
    },
  };
}
