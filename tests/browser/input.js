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

/** The app's touchpad recognizer (touchpad.js, #108), unit-tested here. */
async function touchpad() {
  const path = location.pathname;
  return import(`${path.slice(0, path.indexOf("/__test/") + 1)}touchpad.js`);
}

/** The messages' kinds and values, speeds left out. */
const plain = (messages) =>
  messages.map((m) => (m[0] === "cursor_motion" ? `motion ${m[1]},${m[2]}` : m[0] === "wheel" ? `wheel ${m[1]}` : `${m[1]} ${m[2] ? "down" : "up"} @${m[3]}`));

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

  "touchpad: a tap clicks at once; small jitter stays a tap; a long touch or a move is no tap": async () => {
    const { createTouchpad, TAP_SLOP, LONG_PRESS_MS } = await touchpad();
    const pad = createTouchpad();
    assertEqual(pad.down(1, 100, 100, 0), [], "down");
    assertEqual(pad.move(1, 100 + TAP_SLOP, 100, 50), [], "within the slop");
    assertEqual(plain(pad.up(1, 100 + TAP_SLOP, 100, 100)), ["left down @100", "left up @100"], "tap");
    // Lifted at the long press's time: no tap (the long press had fired).
    const late = createTouchpad();
    late.down(1, 0, 0, 1000);
    assertEqual(late.deadline(), 1000 + LONG_PRESS_MS, "deadline");
    assertEqual(late.tick(1000 + LONG_PRESS_MS - 1), [], "not yet");
    assertEqual(plain(late.up(1, 0, 0, 1000 + LONG_PRESS_MS)), [`right down @${1000 + LONG_PRESS_MS}`, `right up @${1000 + LONG_PRESS_MS}`], "a long press, no tap");
    // Moved past the slop: a move, from where it began, then no tap.
    const moved = createTouchpad();
    moved.down(1, 0, 0, 0);
    assertEqual(plain(moved.move(1, TAP_SLOP + 1, 0, 20)), [`motion ${TAP_SLOP + 1},0`], "past the slop");
    assertEqual(plain(moved.move(1, TAP_SLOP + 1, 7, 40)), ["motion 0,7"], "then by its motion");
    assertEqual(plain(moved.move(1, 0, 0, 60)), [`motion ${-TAP_SLOP - 1},-7`], "back");
    assertEqual(plain(moved.up(1, 0, 0, 80)), [], "no tap after a move");
    assertEqual(moved.deadline(), null, "no long press");
  },

  "touchpad: swipe speed is smoothed from the events' times": async () => {
    const { createTouchpad, SPEED_SMOOTH_MS } = await touchpad();
    const pad = createTouchpad();
    pad.down(1, 0, 0, 0);
    let last;
    // 10 px every 10 ms: 1000 px/s, approached.
    for (let i = 2; i <= 40; i++) last = pad.move(1, i * 10, 0, i * 10)[0];
    assert(Math.abs(last[3] - 1000) < 10, `speed ${last[3]}`);
    // Slowing to 100 px/s: the speed follows within a few smoothing times.
    let t = 400;
    let x = 400;
    for (let i = 0; i < 8; i++) last = pad.move(1, (x += 10), 0, (t += SPEED_SMOOTH_MS));
    assert(Math.abs(last[0][3] - 133) < 10, `slow speed ${last[0][3]}`);
  },

  "touchpad: a long press is a right click; double-tap and hold drags with a single press; a quick second tap is a double click": async () => {
    const { createTouchpad, LONG_PRESS_MS, DOUBLE_TAP_MS, TAP_SLOP } = await touchpad();
    const pad = createTouchpad();
    pad.down(1, 50, 50, 0);
    assertEqual(plain(pad.tick(LONG_PRESS_MS)), [`right down @${LONG_PRESS_MS}`, `right up @${LONG_PRESS_MS}`], "long press");
    assertEqual(pad.tick(LONG_PRESS_MS + 100), [], "once");
    assertEqual(plain(pad.up(1, 50, 50, 900)), [], "no tap after it");
    // Lifted past the long press before its timer ran: the right click then.
    const late = createTouchpad();
    late.down(1, 0, 0, 0);
    assertEqual(plain(late.up(1, 0, 0, LONG_PRESS_MS)), [`right down @${LONG_PRESS_MS}`, `right up @${LONG_PRESS_MS}`], "late lift");
    // A tap, then a touch soon after: moved past the slop, it presses (a
    // single press) and drags.
    pad.down(2, 0, 0, 1000);
    pad.up(2, 0, 0, 1050);
    const t2 = 1050 + DOUBLE_TAP_MS - 1;
    assertEqual(pad.down(3, 300, 300, t2), [], "nothing yet");
    assertEqual(pad.move(3, 300 + TAP_SLOP, 300, t2 + 10), [], "within the slop");
    const pressed = pad.move(3, 301 + TAP_SLOP, 300, t2 + 20);
    assertEqual(plain(pressed), [`left down @${t2 + 20}`, `motion ${TAP_SLOP + 1},0`], "drag pressed");
    assertEqual(pressed[0][4], false, "a single press");
    assertEqual(pad.deadline(), null, "no long press while dragging");
    assertEqual(plain(pad.move(3, 303 + TAP_SLOP, 300, 1400)), ["motion 2,0"], "dragging");
    assertEqual(pad.tick(5000), [], "held, not a right click");
    // A second finger while dragging does nothing; it lifting first leaves
    // the drag on.
    assertEqual(pad.down(4, 0, 0, 5000), [], "second finger");
    assertEqual(pad.move(4, 0, 300, 5010), [], "no scroll while dragging");
    assertEqual(pad.up(4, 0, 300, 5020), [], "the second finger lifted");
    assertEqual(plain(pad.move(3, 305 + TAP_SLOP, 300, 5030)), ["motion 2,0"], "still dragging");
    assertEqual(plain(pad.up(3, 305 + TAP_SLOP, 300, 5100)), ["left up @5100"], "released");
    // Held still after a tap: the press once the long press's time comes.
    pad.down(5, 0, 0, 6000);
    pad.up(5, 0, 0, 6010);
    pad.down(6, 0, 0, 6020);
    assertEqual(pad.deadline(), 6020 + LONG_PRESS_MS, "deadline");
    assertEqual(plain(pad.tick(6020 + LONG_PRESS_MS)), [`left down @${6020 + LONG_PRESS_MS}`], "held press");
    assertEqual(plain(pad.up(6, 0, 0, 7000)), ["left up @7000"], "held release");
    // Too late for a second touch: a fresh one.
    pad.down(7, 0, 0, 8000);
    pad.up(7, 0, 0, 8010);
    pad.down(8, 0, 0, 8010 + DOUBLE_TAP_MS);
    assertEqual(plain(pad.up(8, 0, 0, 8300)), ["left down @8300", "left up @8300"], "a plain tap");
    // Two quick taps: a click, then a double click.
    const twice = createTouchpad();
    twice.down(1, 0, 0, 0);
    const first = twice.up(1, 0, 0, 40);
    assertEqual(twice.down(1, TAP_SLOP * 5, 0, 120), [], "the second touch waits");
    const second = twice.up(1, TAP_SLOP * 5, 0, 160);
    assertEqual(plain([...first, ...second]), ["left down @40", "left up @40", "left down @160", "left up @160"], "double tap");
    assertEqual([first[0][4], second[0][4]], [false, true], "the second is a double click");
    // A drag is released by a cancel of any finger, and by a reset.
    const drag = (p, t) => {
      p.down(1, 0, 0, t);
      p.up(1, 0, 0, t + 10);
      p.down(1, 0, 0, t + 20);
      return p.move(1, 50, 0, t + 30);
    };
    const cancelled = createTouchpad();
    drag(cancelled, 0);
    cancelled.down(2, 0, 0, 40);
    assertEqual(plain(cancelled.cancel(2, 50)), ["left up @50"], "cancel of the other finger");
    assertEqual(cancelled.move(1, 90, 0, 60), [], "then nothing");
    assertEqual(cancelled.up(1, 90, 0, 70), [], "no second release");
    drag(cancelled, 1000);
    assertEqual(plain(cancelled.cancel(1, 1040)), ["left up @1040"], "cancel of the lead");
    drag(cancelled, 2000);
    assertEqual(plain(cancelled.reset(2040)), ["left up @2040"], "reset");
    assertEqual(cancelled.move(1, 90, 50, 2050), [], "forgotten");
    assertEqual(cancelled.reset(2060), [], "nothing held");
  },

  "touchpad: two fingers swiped up or down scroll; no tap or move after": async () => {
    const { createTouchpad, SCROLL_STEP } = await touchpad();
    const pad = createTouchpad();
    pad.down(1, 100, 300, 0);
    pad.down(2, 200, 300, 10);
    assertEqual(pad.deadline(), null, "no long press");
    // Both up by a step: the content follows, wheel down.
    assertEqual(pad.move(1, 100, 300 - SCROLL_STEP, 20), [], "half a step");
    assertEqual(pad.move(2, 200, 300 - SCROLL_STEP, 30), [["wheel", 1]], "a step down");
    assertEqual(pad.move(1, 100, 300 + SCROLL_STEP * 3, 40), [["wheel", -2]], "back up");
    assertEqual(pad.up(1, 100, 300 + SCROLL_STEP * 3, 50), [], "lifted");
    assertEqual(pad.move(2, 200, 0, 60), [], "one finger left does nothing");
    assertEqual(pad.up(2, 200, 0, 70), [], "no tap");
    // A moving finger joined by a second scrolls too.
    pad.down(3, 0, 0, 100);
    pad.move(3, 50, 0, 110);
    pad.down(4, 0, 100, 120);
    assertEqual(pad.move(4, 0, 100 + SCROLL_STEP * 2, 130), [["wheel", -1]], "scroll after a move");
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

  "keys outside the game's key list and browser shortcuts are not forwarded": async () => {
    const before = await state();
    // IntlBackslash (the key left of Z on ISO keyboards) has no name in the
    // game's key list (options-screens.md section 11.2).
    key("keydown", "IntlBackslash");
    key("keyup", "IntlBackslash");
    key("keydown", "KeyQ", { metaKey: true });
    const { actions } = await since(before);
    assertEqual(actions.length, 0, "actions");
    // A key of the list is forwarded, so it can be bound.
    key("keydown", "KeyZ");
    key("keyup", "KeyZ");
    const after = await since(before);
    assertEqual(summary(after.actions.filter((a) => a.action !== "cursor")), ["press z", "release z"], "KeyZ");
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

  "a gamepad button held through blur or pointer-lock loss is not pressed again": async () => {
    const pad = { index: 0, mapping: "standard", axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) };
    const getGamepads = navigator.getGamepads;
    navigator.getGamepads = () => [pad];
    try {
      pad.buttons[0].pressed = true;
      for (let i = 0; i < 50 && !(await state()).held.includes("enter"); i++) await new Promise((r) => setTimeout(r, 20));
      assert((await state()).held.includes("enter"), "south held as Enter");
      for (const event of [() => dispatchEvent(new Event("blur")), () => document.dispatchEvent(new Event("pointerlockchange"))]) {
        event();
        const before = await state();
        assertEqual(before.held, [], "released");
        // Several polls with the button still down.
        await new Promise((r) => setTimeout(r, 200));
        const { after, actions } = await since(before);
        assertEqual(summary(actions), [], "no new press");
        assertEqual(after.held, [], "still released");
      }
    } finally {
      pad.buttons[0].pressed = false;
      await new Promise((r) => setTimeout(r, 100));
      navigator.getGamepads = getGamepads;
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
