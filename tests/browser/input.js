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

/** The app's two-finger wheel recognizer (touch-wheel.js, #856), unit-tested here. */
async function touchWheel() {
  const path = location.pathname;
  return import(`${path.slice(0, path.indexOf("/__test/") + 1)}touch-wheel.js`);
}

/** The app's touch controls (touch-controls.js), unit-tested here. */
async function touchControls() {
  const path = location.pathname;
  return import(`${path.slice(0, path.indexOf("/__test/") + 1)}touch-controls.js`);
}

/** The app's direct touch recognizer (direct-touch.js, #807), unit-tested here. */
async function directTouch() {
  const path = location.pathname;
  return import(`${path.slice(0, path.indexOf("/__test/") + 1)}direct-touch.js`);
}

/** Direct touch's messages: the button's, or the zoom's rounded. */
const clicks = (messages) =>
  messages.map(([type, ...rest]) =>
    type === "zoom" ? `zoom ${rest[0].scale.toFixed(3)} ${rest[0].x.toFixed(1)},${rest[0].y.toFixed(1)}` : `${type} ${rest[1]},${rest[2]} @${rest[3]}`,
  );


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

  "the back and forward buttons press mouse4 and mouse5": async () => {
    const before = await state();
    pointer("pointerdown", 0, 0, { button: 3, buttons: 8 });
    pointer("pointermove", 0, 0, { button: 4, buttons: 24 });
    const held = (await state()).held;
    pointer("pointermove", 0, 0, { button: 4, buttons: 8 });
    pointer("pointerup", 0, 0, { button: 3, buttons: 0 });
    const { actions } = await since(before);
    assertEqual(held, ["mouse4", "mouse5"], "held");
    assertEqual(summary(actions), ["press mouse4", "press mouse5", "release mouse5", "release mouse4"]);
  },

  "touch wheel: one finger does nothing; two fingers swiped up or down scroll; none after one lifts": async () => {
    const { createTouchWheel, SCROLL_STEP, TAP_SLOP } = await touchWheel();
    const pad = createTouchWheel();
    assertEqual(pad.down(1, 100, 300), [], "one finger");
    assertEqual(pad.move(1, 100, 100), [], "moved: nothing");
    assertEqual(pad.up(1, 100, 100), [], "lifted: no click");
    pad.down(1, 100, 300);
    pad.down(2, 200, 300);
    // Both up by a step: the content follows, wheel down.
    assertEqual(pad.move(1, 100, 300 - SCROLL_STEP), [], "half a step");
    assertEqual(pad.move(2, 200, 300 - SCROLL_STEP), [["wheel", 1]], "a step down");
    assertEqual(pad.move(1, 100, 300 + SCROLL_STEP * 3), [["wheel", -2]], "back up");
    assertEqual(pad.up(1, 100, 300 + SCROLL_STEP * 3), [], "lifted");
    assertEqual(pad.move(2, 200, 0), [], "one finger left does nothing");
    assertEqual(pad.up(2, 200, 0), [], "lifted");
    // Within the slop, nothing; a third finger ends it until all lift.
    pad.down(3, 0, 0);
    pad.down(4, 0, 100);
    assertEqual(pad.move(4, 0, 100 + TAP_SLOP), [], "within the slop");
    pad.down(5, 50, 50);
    assertEqual(pad.move(4, 0, 100 + SCROLL_STEP * 3), [], "three fingers: nothing");
    for (const id of [3, 4, 5]) pad.up(id, 0, 0);
    // A cancel, and a reset, forget the fingers.
    pad.down(6, 0, 0);
    pad.down(7, 0, 100);
    assertEqual(pad.cancel(7), [], "cancelled");
    assertEqual(pad.move(6, 0, SCROLL_STEP * 3), [], "nothing after");
    pad.reset();
    pad.down(8, 0, 0);
    pad.down(9, 0, 100);
    assertEqual(pad.move(9, 0, 100 + SCROLL_STEP * 2), [["wheel", -1]], "afresh after a reset");
  },

  "touch wheel: two fingers pinched apart are wheel down, together wheel up": async () => {
    const { createTouchWheel, PINCH_STEP } = await touchWheel();
    const pad = createTouchWheel();
    pad.down(1, 100, 300);
    pad.down(2, 200, 300);
    // Apart by a step, half from each finger: wheel down.
    assertEqual(pad.move(1, 100 - PINCH_STEP / 2, 300), [], "half a step");
    assertEqual(pad.move(2, 200 + PINCH_STEP / 2, 300), [["wheel", 1]], "a step apart");
    assertEqual(pad.move(1, 100 + PINCH_STEP, 300), [["wheel", -1]], "together");
    // Still a pinch after a vertical move: no scroll.
    assertEqual(pad.move(2, 200 + PINCH_STEP / 2, 270), [], "no scroll");
    pad.up(1, 100 + PINCH_STEP, 300);
    assertEqual(pad.up(2, 200, 300), [], "no click");
  },

  "touch controls (#856): with the game's cursor the stick and Click, Right-click and Menu are the cursor's; in the level and the ergonomic mode they are not": async () => {
    const { createTouchControls } = await touchControls();
    // The app's style sheet: what shows is its doing.
    const path = location.pathname;
    const sheet = document.createElement("link");
    sheet.rel = "stylesheet";
    sheet.href = `${path.slice(0, path.indexOf("/__test/") + 1)}style.css`;
    await new Promise((resolve, reject) => {
      sheet.onload = resolve;
      sheet.onerror = () => reject(new Error("style.css did not load"));
      document.head.append(sheet);
    });
    const sent = [];
    let clicked = 0;
    const touch = createTouchControls((m) => sent.push(m), { touch: true, fade: false, onClick: () => clicked++ });
    document.body.append(touch.element);
    const root = touch.element;
    try {
      const shows = (selector) => root.querySelector(selector).getClientRects().length > 0;
      const tap = (el, type, at, id = 7) => {
        const r = el.getBoundingClientRect();
        const [x, y] = at ?? [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
        el.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, pointerId: id, pointerType: "touch", isPrimary: true, button: 0, buttons: type === "pointerup" ? 0 : 1, bubbles: true, cancelable: true }));
      };
      touch.update({ level: false });
      assert(root.hidden, "hidden without the cursor");
      touch.setCursor({ shown: true, capture: true });
      assertEqual(root.dataset.view, "cursor", "the cursor's controls");
      for (const s of [".touch-move", ".touch-button.menu", ".touch-button.click", ".touch-button.right-click"]) assert(shows(s), `${s} shown`);
      for (const s of [".touch-button.jump", ".touch-button.crouch", ".touch-button.inventory", ".touch-button.pda", ".touch-button.console", ".touch-look"]) assert(!shows(s), `${s} hidden`);
      // The stick: its reach, to the rim at most, and zero when let go.
      // In the controls' own frame, turned with the canvas on upright
      // phones (style.css).
      const page = ([x, y]) => {
        const b = root.getBoundingClientRect();
        const turned = b.width < b.height !== root.clientWidth < root.clientHeight;
        return turned ? [b.right - y, b.top + x] : [b.left + x, b.top + y];
      };
      const move = root.querySelector(".touch-move");
      const origin = [100, root.clientHeight - 100];
      const at = page(origin);
      tap(move, "pointerdown", at);
      tap(move, "pointermove", page([origin[0] + 28, origin[1]]));
      tap(move, "pointermove", page([origin[0] + 400, origin[1]]));
      tap(move, "pointerup", page([origin[0] + 400, origin[1]]));
      assertEqual(sent.splice(0), [["cursor_stick", 0, 0], ["cursor_stick", 0.5, 0], ["cursor_stick", 1, 0], ["cursor_stick", 0, 0]], "the stick");
      // Click: pressed and released, the page's click in its lift; Right-click; Menu is back.
      tap(root.querySelector(".touch-button.click"), "pointerdown");
      assertEqual(clicked, 0, "no click at the press");
      tap(root.querySelector(".touch-button.click"), "pointerup");
      assertEqual(clicked, 1, "the click in the lift");
      tap(root.querySelector(".touch-button.right-click"), "pointerdown");
      tap(root.querySelector(".touch-button.right-click"), "pointerup");
      tap(root.querySelector(".touch-button.menu"), "pointerdown");
      tap(root.querySelector(".touch-button.menu"), "pointerup");
      assertEqual(
        sent.splice(0),
        [
          ["cursor_button", "click", true],
          ["cursor_button", "click", false],
          ["cursor_button", "right_click", true],
          ["cursor_button", "right_click", false],
          ["cursor_button", "back", true],
          ["cursor_button", "back", false],
        ],
        "the buttons",
      );
      // Held as the screen goes: released as it was pressed.
      tap(root.querySelector(".touch-button.click"), "pointerdown");
      tap(move, "pointerdown", at, 8);
      touch.update({ level: true });
      assertEqual(root.dataset.view, "level", "the level's controls");
      assertEqual(sent.splice(0), [["cursor_button", "click", true], ["cursor_stick", 0, 0], ["cursor_stick", 0, 0], ["cursor_button", "click", false]], "let go");
      // The PDA open over the level: the cursor's controls and PDA.
      touch.update({ level: false, pda: true });
      assertEqual(root.dataset.view, "cursor", "over the PDA");
      assert(shows(".touch-button.pda") && shows(".touch-button.click"), "PDA and Click");
      // The console open, or the ergonomic mode: none of them.
      touch.update({ level: false });
      touch.setCursor({ console: true });
      assert(root.hidden, "hidden under the console");
      touch.setCursor({ console: false });
      touch.setMode("experimental");
      assert(root.hidden, "hidden in the ergonomic mode");
      touch.setMode("standard");
      assertEqual(root.dataset.view, "cursor", "back");
      touch.setCursor({ capture: false });
      assert(root.hidden, "hidden while the page's interface shows");
    } finally {
      root.remove();
      sheet.remove();
    }
  },
  "direct touch (experimental, #807): a tap presses and releases where it lands; held still it presses after the delay; moved past the slop it presses where it began and drags": async () => {
    const { createDirectTouch, PRESS_DELAY_MS } = await directTouch();
    const { TAP_SLOP } = await touchWheel();
    const d = createDirectTouch(() => [800, 400]);
    assertEqual(d.down(1, 100, 100, 0), [], "nothing at once: a second finger may follow");
    assertEqual(d.deadline(), PRESS_DELAY_MS, "the press's deadline");
    assertEqual(clicks(d.up(1, 102, 101, 60)), ["press 100,100 @0", "release 102,101 @60"], "a tap");
    // Held: pressed at the deadline, then each move drags.
    d.down(2, 300, 200, 1000);
    assertEqual(d.tick(1000 + PRESS_DELAY_MS - 1), [], "not yet");
    assertEqual(clicks(d.tick(1000 + PRESS_DELAY_MS)), ["press 300,200 @1000"], "held still");
    assertEqual(d.deadline(), null, "pressed");
    assertEqual(clicks(d.move(2, 303, 200, 1150)), ["drag 303,200 @1150"], "a drag within the slop too");
    assertEqual(clicks(d.up(2, 303, 200, 1200)), ["release 303,200 @1200"], "lifted");
    // Moved past the slop before the delay: pressed where it began.
    d.down(3, 0, 0, 2000);
    assertEqual(d.move(3, TAP_SLOP, 0, 2010), [], "within the slop");
    assertEqual(clicks(d.move(3, TAP_SLOP + 5, 0, 2020)), ["press 0,0 @2000", `drag ${TAP_SLOP + 5},0 @2020`], "past it");
    // A second finger after the press does nothing; the press holds.
    assertEqual(d.down(4, 500, 300, 2030), [], "a second finger");
    assertEqual(d.move(4, 600, 300, 2040), [], "it moves nothing");
    assertEqual(d.up(4, 600, 300, 2050), [], "nor lifts anything");
    assertEqual(clicks(d.cancel(3, 2060)), [`release ${TAP_SLOP + 5},0 @2060`], "cancelled: released");
    // A reset releases a press too.
    d.down(5, 10, 10, 3000);
    d.tick(3000 + PRESS_DELAY_MS);
    assertEqual(clicks(d.reset(3200)), ["release 10,10 @3200"], "reset");
    assertEqual(d.up(5, 10, 10, 3300), [], "forgotten");
  },

  "direct touch: two fingers pinch the picture about their midpoint, 1 to MAX_ZOOM times, covering the screen, and drag it while magnified; they click nothing": async () => {
    const { createDirectTouch, MAX_ZOOM, zoomed, zoomTransform } = await directTouch();
    const d = createDirectTouch(() => [800, 400]);
    d.down(1, 300, 200, 0);
    d.down(2, 500, 200, 30);
    assertEqual(d.deadline(), null, "no press to wait for");
    // Apart twice as far about (400, 200): that point stays.
    assertEqual(clicks(d.move(1, 200, 200, 40)), ["zoom 1.500 -250.0,-100.0"], "half apart");
    assertEqual(clicks(d.move(2, 600, 200, 50)), ["zoom 2.000 -400.0,-200.0"], "twice as far");
    assertEqual(zoomed(d.zoom, [400, 200]), [400, 200], "the midpoint's point under it");
    assertEqual(zoomTransform(d.zoom), "translate(-400px, -200px) scale(2)", "the transform");
    // Both dragged right by 100: the picture follows them.
    d.move(1, 300, 200, 60);
    assertEqual(clicks(d.move(2, 700, 200, 70)), ["zoom 2.000 -300.0,-200.0"], "dragged");
    // Dragged far: the picture always covers the screen.
    const covers = () => {
      const { scale, x, y } = d.zoom;
      assert(x <= 0 && y <= 0 && x >= 800 * (1 - scale) - 1e-9 && y >= 400 * (1 - scale) - 1e-9, JSON.stringify(d.zoom));
    };
    for (let k = 1; k <= 5; k++) {
      d.move(1, 300 + 100 * k, 200, 70 + k);
      covers();
      d.move(2, 700 + 100 * k, 200, 70 + k);
      covers();
    }
    assertEqual(d.zoom.x, 0, "at the left edge");
    // Far apart: no more than MAX_ZOOM.
    d.move(1, -2000, 200, 100);
    assertEqual(d.zoom.scale, MAX_ZOOM, "at most");
    covers();
    // Together: back to the picture's own size, no less.
    d.move(1, 1190, 200, 110);
    assertEqual(d.zoom, { scale: 1, x: 0, y: 0 }, "at least its own size");
    assertEqual(d.up(1, 1190, 200, 120), [], "no click");
    assertEqual(d.move(2, 0, 0, 130), [], "the other finger does nothing");
    assertEqual(d.up(2, 0, 0, 140), [], "nor its lift");
    // Unzoomed on request; once.
    d.down(3, 300, 200, 200);
    d.down(4, 500, 200, 210);
    d.move(4, 700, 200, 220);
    assert(d.zoom.scale > 1, "zoomed");
    assertEqual(clicks(d.unzoom()), ["zoom 1.000 0.0,0.0"], "unzoomed");
    assertEqual(d.unzoom(), [], "already");
  },

  "touch positions are the canvas's own while the screen's picture is magnified": async () => {
    await sleep(300);
    const screen = document.getElementById("screen");
    const { clientWidth: w, clientHeight: h } = view();
    // Magnified twice about the canvas's centre on the screen.
    const box = view().getBoundingClientRect();
    const [cx, cy] = [box.left + box.width / 2, box.top + box.height / 2];
    // The canvas's point a quarter in from its top left, unmagnified.
    const [qx, qy] = client(w / 4, h / 4);
    screen.style.transform = `translate(${-cx}px, ${-cy}px) scale(2)`;
    try {
      // It now shows at the screen's point twice as far from the
      // transform's origin.
      const shown = [2 * qx - cx, 2 * qy - cy];
      const touch = { pointerType: "touch", button: 0, buttons: 1, pointerId: 7 };
      const at = { clientX: shown[0], clientY: shown[1], isPrimary: true, bubbles: true, cancelable: true };
      document.body.dispatchEvent(new PointerEvent("pointerdown", { ...at, ...touch }));
      document.body.dispatchEvent(new PointerEvent("pointerup", { ...at, ...touch, buttons: 0 }));
      assertNear((await state()).cursor, [-0.5, -0.5], "the canvas's quarter point");
    } finally {
      screen.style.transform = "";
    }
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
