// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Input forwarding (#31): pointer (mouse and touch) and keyboard events
// synthesized in the page reach the engine's input state. Gamepads cannot be
// synthesized; their mapping is unit-tested in volya-engine-core.

import { assert, assertEqual } from "./harness.js";

const state = () => volya.call("input_state");
const view = () => document.getElementById("view");
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Actions recorded after `before` (an `input_state` result). */
async function since(before) {
  const after = await state();
  return { after, actions: after.recent.filter((a) => a.seq > before.count) };
}

/** The page position of canvas position (x, y), CSS pixels. */
function client(x, y) {
  const rect = view().getBoundingClientRect();
  // Turned 90° clockwise on upright phones (style.css).
  if (rotated()) return [rect.right - y, rect.top + x];
  return [rect.left + x, rect.top + y];
}

function rotated() {
  const rect = view().getBoundingClientRect();
  return rect.width < rect.height !== view().clientWidth < view().clientHeight;
}

/** Dispatches a pointer event at canvas position (x, y); false if its default was prevented. */
function pointer(type, x, y, init = {}) {
  const [clientX, clientY] = client(x, y);
  const event = new PointerEvent(type, {
    clientX,
    clientY,
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

  "the game is drawn in landscape; upright phones turn it": async () => {
    assert(view().clientWidth >= view().clientHeight, `canvas ${view().clientWidth}×${view().clientHeight}`);
    const portrait = innerHeight > innerWidth;
    const coarse = matchMedia("(pointer: coarse)").matches;
    assertEqual(rotated(), portrait && coarse, "turned");
    // Keyboards (and gamepads, which share the path) reach the game in any
    // orientation; touches are covered by the tests below, which run turned
    // on the upright simulator.
    const before = await state();
    key("keydown", "ArrowDown");
    key("keyup", "ArrowDown");
    assertEqual(summary((await since(before)).actions), ["press down", "release down"], "keys");
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
    await sleep(300);
    const { clientWidth: w, clientHeight: h } = view();
    const before = await state();
    const touch = { pointerType: "touch", button: 0, buttons: 1 };
    const prevented = !pointer("pointerdown", (3 * w) / 4, h / 2, touch);
    const held = (await state()).held;
    pointer("pointermove", (3 * w) / 4, (3 * h) / 4, { ...touch, button: -1 });
    // Further fingers do not move the cursor (they reach the free camera).
    pointer("pointerdown", 0, 0, { ...touch, pointerId: 2, isPrimary: false });
    pointer("pointerup", (3 * w) / 4, (3 * h) / 4, { ...touch, buttons: 0 });
    const { after, actions } = await since(before);
    assert(prevented, "default prevented");
    assertEqual(held, ["mouse_left"], "held while touching");
    assertEqual(summary(actions), ["cursor", "press mouse_left", "cursor", "release mouse_left"]);
    assertNear(after.cursor, [0.5, 0.5], "cursor");
  },

  "a cancelled touch releases": async () => {
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
    key("keydown", "KeyZ");
    key("keyup", "KeyZ");
    key("keydown", "KeyQ", { metaKey: true });
    const { actions } = await since(before);
    assertEqual(actions.length, 0, "actions");
  },

  "Control with a key is the browser's outside a level": async () => {
    const before = await state();
    key("keydown", "KeyQ", { ctrlKey: true });
    key("keyup", "KeyQ", { ctrlKey: true });
    const { actions } = await since(before);
    // The release arrives, but nothing was pressed.
    assertEqual(summary(actions.filter((a) => a.action !== "cursor")), [], "actions");
  },

  "blur and pointer-lock loss release everything held": async () => {
    for (const event of [() => dispatchEvent(new Event("blur")), () => document.dispatchEvent(new Event("pointerlockchange"))]) {
      key("keydown", "ArrowUp");
      pointer("pointerdown", 5, 5, { button: 0, buttons: 1 });
      const held = (await state()).held;
      assert(held.includes("up") && held.includes("mouse_left"), `held ${held}`);
      event();
      assertEqual((await state()).held, [], "released");
    }
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
