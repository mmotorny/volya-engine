// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Input forwarding (#31): pointer (mouse and touch) and keyboard events
// synthesized in the page reach the engine's input state. Gamepads cannot be
// synthesized; their mapping is unit-tested in volya-engine-core.

import { assert, assertEqual, skip } from "./harness.js";

const upright = () => matchMedia("(orientation: portrait) and (pointer: coarse)").matches;

const state = () => volya.call("input_state");
const view = () => document.getElementById("view");
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Actions recorded after `before` (an `input_state` result). */
async function since(before) {
  const after = await state();
  return { after, actions: after.recent.filter((a) => a.seq > before.count) };
}

/** Dispatches a pointer event; false if its default was prevented. */
function pointer(type, x, y, init = {}) {
  const event = new PointerEvent(type, {
    clientX: x,
    clientY: y,
    pointerId: 1,
    pointerType: "mouse",
    isPrimary: true,
    button: -1,
    buttons: 0,
    bubbles: true,
    cancelable: true,
    ...init,
  });
  return document.body.dispatchEvent(event);
}

/** Dispatches a key event; false if its default was prevented. */
function key(type, code, init = {}) {
  return document.body.dispatchEvent(new KeyboardEvent(type, { code, bubbles: true, cancelable: true, ...init }));
}

function assertNear(actual, expected, message) {
  const close = actual.length === expected.length && actual.every((v, i) => Math.abs(v - expected[i]) < 1e-3);
  assert(close, `${message}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}

const summary = (actions) =>
  actions.map((a) => (a.action === "cursor" ? "cursor" : `${a.action} ${a.control}${a.double ? " double" : ""}`));

export const tests = {
  "input goes to the page by default": async () => {
    const before = await state();
    assertEqual(before.capture, false, "capture");
    pointer("pointermove", 10, 10);
    key("keydown", "ArrowDown");
    key("keyup", "ArrowDown");
    const { actions } = await since(before);
    assertEqual(actions.length, 0, "actions");
    assert(!document.documentElement.hasAttribute("data-engine-input"), "page gestures are the page's");
  },

  "input_capture hands input to the engine": async () => {
    await volya.call("input_capture", { on: true });
    assertEqual((await state()).capture, true, "capture");
    assert(document.documentElement.hasAttribute("data-engine-input"), "data-engine-input");
    assertEqual(getComputedStyle(document.documentElement).touchAction, "none", "touch-action");
  },

  "screen size is the canvas's CSS size": async () => {
    assertEqual((await state()).screen, [view().clientWidth, view().clientHeight]);
  },

  "upright phones are asked to turn to landscape": async () => {
    const overlay = getComputedStyle(document.documentElement, "::after").content;
    assertEqual(overlay.includes("landscape"), upright(), `overlay ${overlay}`);
  },

  "upright phones get no touches until turned": async () => {
    if (!upright()) skip("not upright");
    await sleep(300);
    const before = await state();
    const touch = { pointerType: "touch", button: 0, buttons: 1 };
    const prevented = !pointer("pointerdown", 10, 10, touch);
    pointer("pointermove", 20, 20, { ...touch, button: -1 });
    const held = (await state()).held;
    pointer("pointerup", 20, 20, { ...touch, buttons: 0 });
    const { actions } = await since(before);
    assert(prevented, "default prevented");
    assertEqual(held, [], "held");
    assertEqual(summary(actions), [], "actions");
    // Keyboards (and gamepads, which share the path) reach the game in any
    // orientation.
    const keyed = await state();
    key("keydown", "ArrowDown");
    key("keyup", "ArrowDown");
    assertEqual(summary((await since(keyed)).actions), ["press down", "release down"], "keys while upright");
  },

  "mouse moves the cursor": async () => {
    const { clientWidth: w, clientHeight: h } = view();
    pointer("pointermove", w / 4, h / 4);
    const s = await state();
    assertNear(s.cursor, [-0.5, -0.5], "normalized");
    assertNear(s.ui, [256, 192], "ui");
    pointer("pointermove", w + 50, -50);
    assertNear((await state()).cursor, [1, -1], "clamped to the corner");
  },

  "mouse click presses the left button; a quick second click is a double click": async () => {
    const { clientWidth: w, clientHeight: h } = view();
    const before = await state();
    let prevented = false;
    for (let i = 0; i < 2; i++) {
      prevented = !pointer("pointerdown", w / 2, h / 2, { button: 0, buttons: 1 }) || prevented;
      pointer("pointerup", w / 2, h / 2, { button: 0, buttons: 0 });
    }
    const { after, actions } = await since(before);
    assert(prevented, "default prevented");
    assertEqual(summary(actions), [
      "cursor",
      "press mouse_left",
      "release mouse_left",
      "press mouse_left double",
      "release mouse_left",
    ]);
    assertNear(after.cursor, [0, 0], "cursor");
  },

  "a right button pressed while the left is held": async () => {
    await sleep(300); // Not a double click with the clicks above.
    const before = await state();
    pointer("pointerdown", 0, 0, { button: 0, buttons: 1 });
    pointer("pointermove", 0, 0, { button: 2, buttons: 3 });
    const held = (await state()).held;
    pointer("pointermove", 0, 0, { button: 2, buttons: 1 });
    pointer("pointerup", 0, 0, { button: 0, buttons: 0 });
    const { actions } = await since(before);
    assertEqual(held, ["mouse_left", "mouse_right"], "held");
    assertEqual(summary(actions), [
      "cursor",
      "press mouse_left",
      "press mouse_right",
      "release mouse_right",
      "release mouse_left",
    ]);
  },

  "touch moves the cursor there and presses": async () => {
    if (upright()) skip("upright: touches are blocked until turned (covered on desktop)");
    await sleep(300);
    const { clientWidth: w, clientHeight: h } = view();
    const before = await state();
    const touch = { pointerType: "touch", button: 0, buttons: 1 };
    const prevented = !pointer("pointerdown", (3 * w) / 4, h / 2, touch);
    const held = (await state()).held;
    pointer("pointermove", (3 * w) / 4, (3 * h) / 4, { ...touch, button: -1 });
    // Further fingers are ignored.
    pointer("pointerdown", 0, 0, { ...touch, pointerId: 2, isPrimary: false });
    pointer("pointerup", (3 * w) / 4, (3 * h) / 4, { ...touch, buttons: 0 });
    const { after, actions } = await since(before);
    assert(prevented, "default prevented");
    assertEqual(held, ["mouse_left"], "held while touching");
    assertEqual(summary(actions), ["cursor", "press mouse_left", "cursor", "release mouse_left"]);
    assertNear(after.cursor, [0.5, 0.5], "cursor");
  },

  "a cancelled touch releases": async () => {
    if (upright()) skip("upright: touches are blocked until turned (covered on desktop)");
    await sleep(300);
    const before = await state();
    const touch = { pointerType: "touch", button: 0, buttons: 1 };
    pointer("pointerdown", 0, 0, touch);
    pointer("pointercancel", 0, 0, { ...touch, buttons: 0 });
    const { after, actions } = await since(before);
    assertEqual(summary(actions), ["cursor", "press mouse_left", "release mouse_left"]);
    assertEqual(after.held, [], "held");
  },

  "keys press and release; auto-repeat is not a press": async () => {
    const before = await state();
    const prevented = !key("keydown", "ArrowDown");
    key("keydown", "ArrowDown", { repeat: true });
    const held = (await state()).held;
    key("keyup", "ArrowDown");
    for (const code of ["ArrowUp", "Enter", "Escape", "KeyQ"]) {
      key("keydown", code);
      key("keyup", code);
    }
    const { actions } = await since(before);
    assert(prevented, "arrow keys do not scroll the page");
    assertEqual(held, ["down"], "held");
    assertEqual(summary(actions), [
      "press down",
      "release down",
      ...["up", "enter", "escape", "q"].flatMap((k) => [`press ${k}`, `release ${k}`]),
    ]);
  },

  "other keys and browser shortcuts are not forwarded": async () => {
    const before = await state();
    key("keydown", "KeyA");
    key("keyup", "KeyA");
    key("keydown", "KeyQ", { metaKey: true });
    const { actions } = await since(before);
    assertEqual(actions.length, 0, "actions");
  },

  "losing capture releases held input and returns input to the page": async () => {
    key("keydown", "ArrowUp");
    assertEqual((await state()).held, ["up"], "held");
    await volya.call("input_capture", { on: false });
    const before = await state();
    assertEqual(before.held, [], "released");
    assertEqual(summary(before.recent.slice(-1)), ["release up"], "release recorded");
    assert(!document.documentElement.hasAttribute("data-engine-input"), "data-engine-input");
    pointer("pointermove", 5, 5);
    key("keydown", "ArrowDown");
    const { actions } = await since(before);
    assertEqual(actions.length, 0, "actions");
  },
};
