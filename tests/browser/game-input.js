// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Playing a level from the kept import, with every input path: the desktop's
// full screen offer and Keyboard Lock, the keyboard (walk, jump, torch, the
// console and its clipboard), the touch controls and zoom guard, the
// inventory screen (pointer lock, mouse, touch), the PDA window (its keys,
// mouse, the touch PDA button, the touch controls' stick and Click), touch's standard
// controls (the stick and its sprint lock, the look curve) and
// experimental ones (tap-to-move, direct manipulation look), Jump, Crouch
// and Menu, and a gamepad.

import { assert, assertEqual, skip } from "./harness.js";
import {
  button,
  centreOf,
  centreOfRow,
  clickAt,
  client,
  coarse,
  enterLevel,
  inGame,
  keptSetup,
  keptTeardown,
  key,
  localRect,
  needsGame,
  newGame,
  nextTouch,
  pointer,
  quitToTitle,
  render,
  setTouchMode,
  sleep,
  state,
  stickTo,
  tapClick,
  cursorButton,
  cursorControls,
  toClient,
  turned,
  until,
  upright,
  view,
} from "./game.js";

export const tests = {
  ...keptSetup({ level: true }),

  "desktop: full screen with Keyboard Lock is offered in the level; accepting locks the keys, then asks for full screen": async () => {
    needsGame();
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    const offer = document.getElementById("fullscreen-offer");
    if (coarse()) {
      assert(offer.hidden, "no offer on a touch device");
      skip("desktop only: a touch device gets no offer");
    }
    const request = Element.prototype.requestFullscreen;
    const native = Boolean(navigator.keyboard?.lock);
    // Headless Chrome can't enter full screen without a real gesture: the
    // APIs are stubbed, recording their calls.
    const calls = [];
    const keyboard = { lock: async (codes) => calls.push(["lock", codes]), unlock: () => calls.push(["unlock"]) };
    Object.defineProperty(navigator, "keyboard", { value: keyboard, configurable: true });
    Element.prototype.requestFullscreen = async function () {
      calls.push(["fullscreen", this === document.documentElement]);
      Object.defineProperty(document, "fullscreenElement", { value: document.documentElement, configurable: true });
      document.dispatchEvent(new Event("fullscreenchange"));
    };
    // Not offered on macOS (our choice): the platform reads as Windows here.
    Object.defineProperty(navigator, "userAgentData", { value: { platform: "Windows" }, configurable: true });
    const exit = document.exitFullscreen;
    document.exitFullscreen = async () => {
      calls.push(["exit"]);
      delete document.fullscreenElement;
      document.dispatchEvent(new Event("fullscreenchange"));
    };
    const render = () => document.dispatchEvent(new Event("fullscreenchange"));
    // The engine stops owning the screen (as when the player quits), then
    // owns it again: to the offer, the game ended and a new one began.
    const newGame = async () => {
      await volya.call("input_capture", { on: false });
      await volya.call("input_capture", { on: true });
    };
    const shown = async (want) => {
      for (let i = 0; i < 50 && offer.hidden === want; i++) await sleep(20);
      return !offer.hidden;
    };
    try {
      // Shown now that the APIs are there (it shows again on any change).
      render();
      assert(await shown(true), "the offer shows");
      const button = offer.querySelector("button:not(.close)");
      const close = offer.querySelector("button.close");
      assertEqual([button.textContent, offer.querySelector("p").textContent], ["Full screen", "Keeps shortcuts such as Ctrl+W in the game. Hold Esc to leave full screen."], "text");
      assertEqual(close.getAttribute("aria-label"), "Close", "the close control");
      // Closed, it stays away in this game, through the menu too.
      close.click();
      assert(offer.hidden, "closed");
      render();
      key("Escape");
      await until((s) => inGame(s), "the in-game menu");
      key("Escape");
      await until((s) => s.phase === "level" && !s.menu, "back in the game");
      assert(!(await shown(true)), "not shown again after the menu");
      // The next game offers it again.
      await newGame();
      render();
      assert(await shown(true), "offered in the next game");
      calls.length = 0;
      // A press on it is the page's, not the game's.
      const before = await volya.call("input_state");
      const at = button.getBoundingClientRect();
      button.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: at.x + 5, clientY: at.y + 5, pointerType: "mouse", button: 0, buttons: 1 }));
      const after = await volya.call("input_state");
      assert(!after.recent.some((a) => a.seq > before.count && a.action === "press"), "no press reached the game");
      button.click();
      await until(() => calls.some(([c]) => c === "fullscreen"), "full screen asked for");
      assertEqual(calls[0][0], "lock", "the keyboard locked first");
      for (const code of ["KeyW", "KeyT", "KeyN", "Escape", "ControlLeft", "KeyA", "Space"]) {
        assert(calls[0][1].includes(code), `${code} locked: ${calls[0][1]}`);
      }
      // The system's ways out stay its own, bound or not.
      for (const code of ["Tab", "F4", "MetaLeft", "MetaRight"]) {
        assert(!calls[0][1].includes(code), `${code} not locked`);
      }
      assertEqual(calls[1], ["fullscreen", true], "the page's root in full screen");
      assert(offer.hidden, "no offer in full screen");
      // Escape (a short press under the lock) still opens the menu.
      key("Escape");
      await until((s) => inGame(s), "the in-game menu");
      key("Escape");
      await until((s) => s.phase === "level" && !s.menu, "back in the game");
      // The game ends: the page leaves full screen, then the lock ends.
      await volya.call("input_capture", { on: false });
      await until(() => calls.some(([c]) => c === "exit"), "full screen left on quitting");
      await until(() => calls.some(([c]) => c === "unlock"), "unlocked");
      await volya.call("input_capture", { on: true });
      return `navigator.keyboard.lock ${native ? "present" : "absent"} in headless Chrome; locked ${calls[0][1].length} keys`;
    } finally {
      delete navigator.keyboard;
      delete navigator.userAgentData;
      Element.prototype.requestFullscreen = request;
      document.exitFullscreen = exit;
      delete document.fullscreenElement;
      document.dispatchEvent(new Event("fullscreenchange"));
    }
  },

  "keyboard: W walks forward and Space jumps, leaving and regaining the ground": async () => {
    needsGame();
    const player = async () => (await state()).level.player;
    const down = (code) => document.body.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true }));
    const up = (code) => document.body.dispatchEvent(new KeyboardEvent("keyup", { code, bubbles: true, cancelable: true }));
    // Collision stops a run along the start view after about 0.8 m
    // (measured natively): turn half round with the arrow keys (π rad/s)
    // first, where the way is clear for more than a metre.
    let s = await state();
    const yaw = s.level.yaw;
    down("ArrowRight");
    await until((s) => s.level.yaw > yaw + 2.9, "the right arrow to turn round", 3000);
    up("ArrowRight");
    // Space held a moment where the player stands (a ceiling measured
    // 3.2 m above the start): up (0.92 m at most) and back down.
    const ground = await player();
    assert(!ground.airborne, "on the ground at the start");
    down("Space");
    s = await until((s) => s.level.player.airborne, "Space to jump", 2000);
    await sleep(80);
    up("Space");
    let top = s.level.player.position[1];
    s = await until((s) => {
      top = Math.max(top, s.level.player.position[1]);
      return !s.level.player.airborne;
    }, "the jump to land", 3000);
    const height = top - ground.position[1];
    assert(height > 0.5 && height < 1.0, `jump height ${height.toFixed(2)} m`);
    await sleep(400);
    const start = await player();
    down("KeyW");
    s = await until((s) => s.level.player.running && s.level.player.moving, "W to run");
    await sleep(250);
    up("KeyW");
    s = await until((s) => !s.level.player.moving, "the player to stop");
    const p = s.level.player;
    const moved = Math.hypot(p.position[0] - start.position[0], p.position[2] - start.position[2]);
    assert(moved > 0.5, `moved ${moved.toFixed(2)} m`);
    assert(Math.abs(p.position[1] - start.position[1]) < 0.05, `height ${p.position[1] - start.position[1]}`);
    assert(!p.airborne, "still on the ground");
    // The level's controls are live (the engine's message arrived), and with
    // no touch in the level yet desktop Chrome shows no touch controls.
    const touchControls = document.getElementById("touch-controls");
    for (let i = 0; i < 100 && touchControls.dataset.live !== "true"; i++) await sleep(20);
    assertEqual(touchControls.dataset.live, "true", "the level's controls are live");
    // A touch device shows them from the start.
    assertEqual(touchControls.hidden, !coarse(), "touch controls hidden until a touch, on desktop");
    // The torch (docs/behavior/torch.md 12): off at the start; L switches
    // it on, its main light and glow with it, and off again.
    assert(s.level.torch && !s.level.torch.on && s.level.torch.attached, "the torch attached and off");
    const active = s.level.lights.active;
    key("KeyL");
    s = await until((s) => s.level.torch.on, "L to switch the torch on");
    assert(s.level.torch.main_active && s.level.torch.glow_on && !s.level.torch.second_active, "its main light and glow");
    assertEqual(s.level.lights.active, active + 1, "one more active light record");
    key("KeyL");
    await until((s) => !s.level.torch.on && s.level.lights.active === active, "L to switch it off");
    return `ran ${moved.toFixed(2)} m, jumped ${height.toFixed(2)} m`;
  },

  "keyboard: the console key opens the console over the level, keys type into it, Insert pastes the clipboard in order, Escape closes it": async () => {
    needsGame();
    if (coarse()) skip("desktop keyboard only: the touch screen's way has its own test");
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    const touchControls = document.getElementById("touch-controls");
    const live = async (want) => {
      for (let i = 0; i < 100 && (touchControls.dataset.live === "true") !== want; i++) await sleep(20);
      assertEqual(touchControls.dataset.live === "true", want, `the level's controls live: ${want}`);
    };
    key("Backquote");
    await until((s) => s.console.open, "the console key to open the console");
    await live(false);
    // The clipboard API is stubbed (headless Chrome asks no one): text,
    // then a refused read.
    const clipboard = Object.getOwnPropertyDescriptor(Navigator.prototype, "clipboard");
    let read = async () => "Cfg_Load\tA";
    Object.defineProperty(navigator, "clipboard", { value: { readText: () => read() }, configurable: true });
    try {
      key("KeyQ");
      // Insert, then a key at once: it waits for the read.
      key("Insert");
      key("KeyW");
      await until((s) => s.console.line === "cfg_load aw", "the pasted text, then w");
      read = async () => {
        throw new DOMException("refused", "NotAllowedError");
      };
      key("Insert");
      key("KeyX");
      await until((s) => s.console.line === "cfg_load awx", "a refused read to leave the line");
    } finally {
      delete navigator.clipboard;
      if (clipboard) Object.defineProperty(Navigator.prototype, "clipboard", clipboard);
    }
    key("Escape");
    const s = await until((s) => !s.console.open, "Escape to close the console");
    assert(!s.menu, "Escape closed the console only");
    await live(true);
    return "opened, typed, pasted, closed";
  },

  "touch: Console opens the console and focuses the device keyboard's input; typed characters, Backspace, Return, the key strip (held Up, Paste) and the keyboard's paste type; hidden, a tap on the console raises it; Esc closes; over the menu too": async () => {
    needsGame();
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    const controls = document.getElementById("touch-controls");
    const entry = document.getElementById("console-entry");
    const strip = document.getElementById("console-keys");
    const layer = document.getElementById("console-touch");
    // A touch anywhere shows the controls (a touch device).
    const t = { pointerType: "touch", isPrimary: true, button: 0, bubbles: true, cancelable: true };
    pointer("pointerdown", client(512, 100), { ...t, pointerId: 70, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...t, pointerId: 70, buttons: 0 });
    for (let i = 0; i < 100 && (controls.hidden || controls.dataset.live !== "true"); i++) await sleep(20);
    // A touch's tap: its pointer events, then its touchend (as WebKit
    // orders them, believed).
    const release = (el) => el.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));
    const tap = (el, id, { end = true } = {}) => {
      const r = el.getBoundingClientRect();
      const at = { clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 };
      el.dispatchEvent(new PointerEvent("pointerdown", { ...t, ...at, pointerId: id, buttons: 1 }));
      el.dispatchEvent(new PointerEvent("pointerup", { ...t, ...at, pointerId: id, buttons: 0 }));
      if (end) release(el);
    };
    const shown = async (el, want, what) => {
      for (let i = 0; i < 100 && el.hidden === want; i++) await sleep(20);
      assertEqual(!el.hidden, want, what);
    };
    // Console, right of Menu in the top row.
    const button = controls.querySelector(".touch-button.console");
    const [menu, own] = [localRect(controls.querySelector(".touch-button.menu"), controls), localRect(button, controls)];
    assert(own.left > menu.right && Math.abs(own.top - menu.top) < 1, `Console right of Menu: ${JSON.stringify([menu, own])}`);
    assertEqual(button.getAttribute("aria-label"), "Console", "its name");
    // The input is focused in the tap's touchend, where iOS raises the
    // keyboard for a focus.
    tap(button, 71, { end: false });
    assert(document.activeElement !== entry, "no focus before the touchend");
    release(button);
    assertEqual(document.activeElement, entry, "the input focused in the tap's touchend");
    await until((s) => s.console.open, "Console to open the console");
    await shown(strip, true, "the key strip");
    assertEqual([...strip.children].map((k) => k.textContent), ["Esc", "Tab", "↑", "↓", "PgUp", "PgDn", "Paste"], "the strip's keys");
    // Typed characters as the keys that type them (Cyrillic types nothing).
    const input = (inputType, data) => {
      entry.value = data ?? "";
      entry.dispatchEvent(new InputEvent("input", { inputType, data, bubbles: true }));
    };
    input("insertText", "Zz_Q ~1!ж");
    await until((s) => s.console.line === "zz_q ~1!", "the typed line");
    assertEqual(entry.value, "", "the input emptied");
    const press = (key) => entry.dispatchEvent(new KeyboardEvent("keydown", { key, code: "", bubbles: true, cancelable: true }));
    press("Backspace");
    input("deleteContentBackward");
    await until((s) => s.console.line === "zz_q ~", "Backspace twice");
    press("Enter");
    let s = await until((s) => s.console.line === "" && s.console.log?.includes("zz_q"), "Return to execute the line");
    // Up held on the strip: the history's newest entry.
    const up = strip.querySelector('[data-key="up"]');
    const r = up.getBoundingClientRect();
    const at = { clientX: r.left + 5, clientY: r.top + 5 };
    up.dispatchEvent(new PointerEvent("pointerdown", { ...t, ...at, pointerId: 72, buttons: 1 }));
    assert(up.classList.contains("down"), "Up shown held");
    await until((s) => s.console.line === "zz_q ~", "Up to recall the line");
    up.dispatchEvent(new PointerEvent("pointerup", { ...t, ...at, pointerId: 72, buttons: 0 }));
    // Paste reads the clipboard in its tap (stubbed: headless Chrome asks
    // no one) and replaces the line, as Insert; so does the keyboard's own
    // paste.
    const clipboard = Object.getOwnPropertyDescriptor(Navigator.prototype, "clipboard");
    Object.defineProperty(navigator, "clipboard", { value: { readText: async () => "Hud_Weapon" }, configurable: true });
    try {
      tap(strip.querySelector('[data-key="insert"]'), 73);
      await until((s) => s.console.line === "hud_weapon", "Paste");
    } finally {
      delete navigator.clipboard;
      if (clipboard) Object.defineProperty(Navigator.prototype, "clipboard", clipboard);
    }
    input("insertFromPaste", "Stat_Memory");
    await until((s) => s.console.line === "stat_memory", "the keyboard's paste");
    assertEqual(document.activeElement, entry, "strip taps keep the input focused");
    // The keyboard hidden: the console stays open, the strip goes, a tap
    // on the console's area raises the keyboard again.
    entry.blur();
    await shown(strip, false, "the strip with the keyboard hidden");
    const area = layer.querySelector(".console-area");
    await shown(area, true, "the console's area");
    assert((await state()).console.open, "the console open with the keyboard hidden");
    tap(area, 74);
    assertEqual(document.activeElement, entry, "a tap on the console raises the keyboard");
    await shown(strip, true, "the strip again");
    // Esc closes the console and the keyboard; the controls come back.
    tap(strip.querySelector('[data-key="escape"]'), 75);
    s = await until((s) => !s.console.open, "Esc to close the console");
    assert(!s.menu, "Esc closed the console only");
    for (let i = 0; i < 50 && document.activeElement === entry; i++) await sleep(20);
    assert(document.activeElement !== entry, "the keyboard closed");
    await shown(strip, false, "the strip closed");
    for (let i = 0; i < 100 && controls.dataset.live !== "true"; i++) await sleep(20);
    // Over the main menu: Console at the top left, opening the console
    // there; Esc closes it, the menu stays.
    key("Escape");
    await until((s) => inGame(s), "the in-game menu");
    const overMenu = layer.querySelector(".touch-button.console");
    await shown(overMenu, true, "Console over the menu");
    const place = localRect(overMenu, layer);
    assert(place.left < 100 && place.top < 100, `at the top left: ${JSON.stringify(place)}`);
    tap(overMenu, 76);
    await until((s) => s.console.open, "Console to open the console over the menu");
    await shown(overMenu, false, "Console hidden while the console is open");
    tap(strip.querySelector('[data-key="escape"]'), 77);
    s = await until((s) => !s.console.open, "Esc to close the console over the menu");
    assert(inGame(s), "the menu stays");
    key("Escape");
    await until((s) => s.phase === "level" && !s.menu, "back in the level");
    await shown(overMenu, false, "no Console over the level");
    return "opened, typed, recalled, pasted, closed, over the menu too";
  },

  "touch: the controls are drawn from the game's textures": async () => {
    needsGame();
    // Asked for once the level's controls were live (the `ui_images` op),
    // drawn even while hidden.
    const controls = document.getElementById("touch-controls");
    for (let i = 0; i < 250 && controls.dataset.art !== "game"; i++) await sleep(20);
    assertEqual(controls.dataset.art, "game", "the controls' art");
    // The page's own art, cached for the session (the same module as the
    // app's: the harness imports the app from the same base).
    const base = location.pathname.slice(0, location.pathname.indexOf("/__test/") + 1);
    const { gameArt } = await import(`${base}touch-art.js`);
    const art = await gameArt(() => Promise.reject(new Error("the textures asked for twice")));
    assertEqual(art.missing, [], "pictures the engine couldn't give");
    // Every canvas holds a picture, in device pixels.
    let bytes = 0;
    for (const canvas of controls.querySelectorAll("canvas")) {
      const g = canvas.getContext("2d");
      const pixels = g.getImageData(0, 0, canvas.width, canvas.height).data;
      let alpha = 0;
      for (let i = 3; i < pixels.length; i += 4) alpha += pixels[i];
      assert(alpha > 0, `an empty canvas: ${canvas.className} in ${canvas.parentElement.className}`);
      bytes += canvas.width * canvas.height * 4;
    }
    // The op itself: a posture icon's silhouette is teal and closed, the
    // crosshair's colour is read.
    const answer = await volya.call("ui_images", { images: [{ id: "ui_hud_soldier_crouch", silhouette: true }, { id: "no such id" }] });
    const [icon, missing] = answer.images;
    assertEqual([icon.width, icon.height], [65, 75], "the crouch icon's size");
    const [r, g, b] = icon.mean;
    assert(g > r + 60 && b > r + 60, `the crouch icon is teal: ${icon.mean}`);
    assert(missing.error, "an unknown id answers an error");
    assert(answer.cross_color?.length === 4, `the crosshair's colour: ${answer.cross_color}`);
    const sources = ["frame", "crouch", "creep", "sprint", "segments", "spot", "torchIcon", "pda"].reduce((n, k) => n + art[k].width * art[k].height * 4, 0);
    return `decoded in ${art.ms.toFixed(1)} ms (engine), ${art.totalMs.toFixed(1)} ms with the page's part; sources ${(sources / 1024).toFixed(0)} KiB, canvases ${(bytes / 1024).toFixed(0)} KiB at ${devicePixelRatio}x`;
  },

  "zoom: in the level the engine's layers take every touch, pinches are cancelled on the controls, the page is at scale 1": async () => {
    needsGame();
    const root = document.documentElement;
    const controls = document.getElementById("touch-controls");
    assert(root.hasAttribute("data-engine-input"), "the engine doesn't own the screen");
    for (const el of [root, document.getElementById("view"), controls]) {
      assertEqual(getComputedStyle(el).touchAction, "none", `touch-action of ${el.id || el.tagName.toLowerCase()}`);
    }
    // The guard cancels pinches on the controls' layer too (zoom.js).
    for (const type of ["gesturestart", "gesturechange", "gestureend"]) {
      const event = new Event(type, { bubbles: true, cancelable: true });
      controls.dispatchEvent(event);
      assert(event.defaultPrevented, `${type} not cancelled`);
    }
    assertEqual(visualViewport.scale, 1, "visualViewport.scale");
  },

  "the inventory: I opens it over the running level (a held W walks on, Control crouches under it), the outfit dragged to the backpack, bread eaten, the touch button opens it, the touch controls' stick and Click select, Right-click opens the menu, Click held drags, Escape and the exit button close it": async () => {
    needsGame();
    const controls = document.getElementById("touch-controls");
    const centreOf4 = ([x, y, w, h], within) => {
      // Within the list (the outfit's cell item is wider than it).
      const [lx, ly, lw, lh] = within ?? [x, y, w, h];
      return [Math.min(x + w / 2, lx + lw / 2), Math.min(y + h / 2, ly + lh / 2)];
    };
    const items = (s) => s.level.character.items;
    const item = (s, section) => items(s).find((i) => i.section === section);
    const live = async (on) => {
      for (let i = 0; i < 100 && controls.dataset.live !== String(on); i++) await sleep(20);
      assertEqual(controls.dataset.live, String(on), "the level's controls live");
    };
    // The key bound to `inventory` (I, docs/behavior/inventory-screen.md
    // 1.1) opens it; the game runs on behind it.
    let s = await state();
    const clock = s.level.game_time;
    // A click in the level takes pointer lock where the browser grants it;
    // opening the screen leaves it without releasing held keys: W held
    // before I walks on (inventory-screen.md 1.3).
    const down = (code, init = {}) => document.body.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true, ...init }));
    const up = (code, init = {}) => document.body.dispatchEvent(new KeyboardEvent("keyup", { code, bubbles: true, cancelable: true, ...init }));
    // The walk goes back the way the keyboard test came, the one way known
    // clear: ahead the turn's overshoot (a frame's turn: a quarter radian
    // at the iOS Simulator's 80 ms frames) can leave a wall 0.2 m away
    // (measured there), which stops the player under the screen.
    const yaw = s.level.yaw;
    const turnedBy = (s) => (((s.level.yaw - yaw) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
    down("ArrowRight");
    await until((s) => turnedBy(s) > 2.9, "the right arrow to turn round", 3000);
    up("ArrowRight");
    // The page asks for unadjusted movement (input.md, The level's
    // controls); each request's options are recorded.
    const root = document.documentElement;
    const requests = [];
    const real = root.requestPointerLock;
    if (real) {
      root.requestPointerLock = function (options) {
        requests.push(options);
        return real.call(this, options);
      };
    }
    // Headless Chrome grants no pointer lock: then it is stubbed (the
    // lock element, its request and exit, each exit's change event), so the
    // page's exit path runs. The stub rejects unadjusted movement as
    // unsupported, so the fallback to a plain lock is what locks.
    let stubbed = null;
    const stub = () => {
      stubbed = null;
      requests.length = 0;
      const change = () => document.dispatchEvent(new Event("pointerlockchange"));
      Object.defineProperty(document, "pointerLockElement", { get: () => stubbed, configurable: true });
      root.requestPointerLock = (options) => {
        requests.push(options);
        if (options?.unadjustedMovement) return Promise.reject(new DOMException("unsupported", "NotSupportedError"));
        stubbed = root;
        change();
        return Promise.resolve();
      };
      document.exitPointerLock = () => {
        stubbed = null;
        change();
      };
    };
    const unstub = () => {
      delete document.pointerLockElement;
      delete root.requestPointerLock;
      delete document.exitPointerLock;
    };
    clickAt([512, 600]);
    for (let i = 0; i < 25 && !document.pointerLockElement; i++) await sleep(20);
    const granted = Boolean(document.pointerLockElement);
    if (!granted) {
      stub();
      clickAt([512, 600]);
      for (let i = 0; i < 25 && !document.pointerLockElement; i++) await sleep(20);
      assertEqual(JSON.stringify(requests), '[{"unadjustedMovement":true},null]',"an unadjusted request, then a plain one");
    } else if (real) {
      assert(requests[0]?.unadjustedMovement === true, `the lock asked for unadjusted movement: ${JSON.stringify(requests)}`);
    }
    assert(document.pointerLockElement === root, "the pointer locked");
    down("KeyW");
    await until((s) => s.level.player.moving, "W to walk");
    key("KeyI");
    s = await until((s) => s.inventory, "the inventory screen");
    await live(false);
    assert(!document.pointerLockElement, "the pointer released");
    if (!granted) unstub();
    else delete root.requestPointerLock;
    await sleep(300);
    s = await state();
    assert(s.inventory && s.level.player.moving, `W walks on under the screen: ${JSON.stringify(s.level.player)}`);
    up("KeyW");
    await until((s) => !s.level.player.moving, "the player to stop");
    // Control (the crouch key; its keydown carries ctrlKey) crouches under
    // the screen: the page forwards it while the character takes keys.
    down("ControlLeft", { ctrlKey: true });
    await until((s) => s.level.player.crouch, "Control to crouch under the screen");
    up("ControlLeft");
    await until((s) => !s.level.player.crouch, "the crouch released");
    const outfitList = s.inventory.lists.find((l) => l.list === "outfit");
    const outfit = outfitList.cells[0];
    assertEqual(outfit.section, "novice_outfit", "the worn outfit's cell item");
    assertEqual(s.inventory.money, "40 RU", "money");
    assert(s.inventory.weight_line.includes("4.3"), `the weight line: ${s.inventory.weight_line}`);
    s = await until((s) => s.level.game_time > clock, "game time to run under the screen");
    // A mouse drag of the outfit onto the backpack list undresses it.
    const from = client(...centreOf4(outfit.rect, outfitList.rect));
    pointer("pointermove", from);
    pointer("pointerdown", from, { button: 0, buttons: 1 });
    pointer("pointermove", [from[0] + 6, from[1] + 6], { buttons: 1 });
    await until((s) => s.inventory?.dragging === outfit.items[0], "the drag to begin");
    const to = client(100, 600);
    pointer("pointermove", to, { buttons: 1 });
    pointer("pointerup", to, { button: 0, buttons: 0 });
    s = await until((s) => item(s, "novice_outfit")?.place === "backpack", "the outfit in the backpack");
    assert(s.inventory.lists.find((l) => l.list === "bag").cells.some((c) => c.section === "novice_outfit"), "drawn in the backpack");
    assertEqual(s.inventory.protections, [], "no protection without an outfit");
    // Bread (given through the test interface: nothing to pick up yet) is
    // eaten with a double click: satiety rises, the bread is gone.
    const bread = await volya.call("debug_give_item", { section: "bread" });
    s = await until((s) => s.inventory.lists.find((l) => l.list === "bag").cells.some((c) => c.items.includes(bread)), "the bread in the backpack");
    const satiety = s.level.character.satiety;
    const cell = s.inventory.lists.find((l) => l.list === "bag").cells.find((c) => c.items.includes(bread));
    const at = client(...centreOf4(cell.rect));
    pointer("pointermove", at);
    for (let i = 0; i < 2; i++) {
      pointer("pointerdown", at, { button: 0, buttons: 1 });
      pointer("pointerup", at, { button: 0, buttons: 0 });
    }
    s = await until((s) => !items(s).some((i) => i.id === bread), "the bread eaten");
    assert(s.level.character.satiety > satiety, `satiety ${satiety} → ${s.level.character.satiety}`);
    // Escape closes it (the screen has the key: no menu opens).
    key("Escape");
    s = await until((s) => s.phase === "level" && !s.inventory, "the screen closed");
    assert(!s.menu, "Escape opened the menu");
    await live(true);
    // On a touch device, the HUD button at the top right opens it; the
    // touch controls go away while it shows.
    const tap = { pointerType: "touch", pointerId: 60, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    const button = controls.querySelector(".touch-button.inventory");
    // In the game's picture, turned with it on upright phones.
    const local = localRect(button, controls);
    const look = localRect(controls.querySelector(".touch-look"), controls);
    assert(local.left > look.left && local.top < 100, `at the top right: ${JSON.stringify(local)}`);
    const r = button.getBoundingClientRect();
    const centre = [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
    for (const type of ["pointerdown", "pointerup"]) {
      button.dispatchEvent(
        new PointerEvent(type, { clientX: centre[0], clientY: centre[1], pointerId: 61, pointerType: "touch", isPrimary: true, button: 0, buttons: type === "pointerdown" ? 1 : 0, bubbles: true, cancelable: true }),
      );
    }
    s = await until((s) => s.inventory, "the touch button to open the screen");
    // The cursor's controls show over it (#856): the stick, Menu, Click
    // and Right-click.
    await cursorControls();
    const shown = (name) => controls.querySelector(`.touch-button.${name}`).getClientRects().length > 0;
    assert(["menu", "click", "right-click"].every(shown) && !["jump", "crouch", "inventory"].some(shown), "the cursor's buttons");
    // The stick moves the cursor onto the outfit in the backpack, Click
    // selects it, Right-click opens its menu, whose first row («одеть
    // костюм») Click chooses.
    const bag = s.inventory.lists.find((l) => l.list === "bag");
    const suitCell = bag.cells.find((c) => c.section === "novice_outfit");
    await stickTo(centreOf4(suitCell.rect));
    await tapClick();
    s = await until((s) => s.inventory?.selected === suitCell.items[0], "Click to select the outfit");
    await tapClick("right-click");
    s = await until((s) => s.inventory?.menu, "Right-click to open the context menu");
    assertEqual(s.inventory.menu.entries, ["st_dress_outfit", "st_drop"], "the outfit's entries");
    await stickTo(centreOf({ rect: s.inventory.menu.rows[0] }));
    await tapClick();
    s = await until((s) => item(s, "novice_outfit")?.place === "slot", "the outfit worn again");
    // Click held on the character picture drags the outfit with the
    // stick; let go on the backpack, it is undressed.
    const worn = s.inventory.lists.find((l) => l.list === "outfit");
    await stickTo(centreOf4(worn.cells[0].rect, worn.rect));
    await sleep(300);
    await cursorButton("click", true);
    const bagNow = s.inventory.lists.find((l) => l.list === "bag");
    await stickTo(centreOf4(bagNow.rect));
    s = await until((s) => s.inventory?.dragging === worn.cells[0].items[0], "Click held to drag");
    await cursorButton("click", false);
    s = await until((s) => item(s, "novice_outfit")?.place === "backpack", "the drag to the backpack");
    s = await until((s) => s.inventory && s.inventory.dragging == null, "the drag to end");
    // Dragged back onto the character, worn again.
    const inBag = s.inventory.lists.find((l) => l.list === "bag").cells.find((c) => c.section === "novice_outfit");
    await stickTo(centreOf4(inBag.rect));
    await sleep(300);
    await cursorButton("click", true);
    await stickTo(centreOf4(worn.rect));
    await cursorButton("click", false);
    s = await until((s) => item(s, "novice_outfit")?.place === "slot", "dragged back on");
    // The exit button closes it.
    clickAt(centreOf({ rect: s.inventory.exit }));
    s = await until((s) => s.phase === "level" && !s.inventory, "the exit button to close the screen");
    assert(!s.menu, "no menu");
    await live(true);
    return `game time ran ${((s.level.game_time - clock) / 1000).toFixed(1)} s under the screen; pointer lock ${granted ? "granted" : "stubbed"}`;
  },

  "the PDA: P opens it on the tasks tab, M switches to the map and closes it, a click on a caption switches tabs, Escape closes it without the menu; on touch the PDA button opens it, the stick and Click switch tabs, and the PDA button over it closes it": async () => {
    needsGame();
    const controls = document.getElementById("touch-controls");
    const live = async (on) => {
      for (let i = 0; i < 100 && controls.dataset.live !== String(on); i++) await sleep(20);
      assertEqual(controls.dataset.live, String(on), "the level's controls live");
    };
    // The keys bound to `active_jobs` (P) and `map` (M)
    // (docs/behavior/pda-window.md 4.1).
    key("KeyP");
    let s = await until((s) => s.pda, "P to open the PDA");
    assertEqual(s.pda.tab, 0, "the tasks tab");
    await live(false);
    s = await until((s) => s.pda?.clock, "the clock");
    assert(/^\d\d:\d\d \d\d\/\d\d\/\d{4}$/.test(s.pda.clock), `the clock: ${s.pda.clock}`);
    key("KeyM");
    s = await until((s) => s.phase === "level" && !s.pda, "M to close it");
    key("KeyP");
    s = await until((s) => s.pda, "P to open it again");
    assertEqual(s.pda.tab, 0, "P switched back to the tasks tab");
    // The page takes the mouse as the cursor once the engine says so; the
    // click comes past the double-click time of the inventory test's last
    // (a press within it is a double click, which selects no tab).
    await live(false);
    await sleep(300);
    // A click on the diary's caption switches to it (3.3).
    clickAt(centreOf({ rect: s.pda.tabs[2].rect }));
    s = await until((s) => s.pda?.tab === 2, "a click on the diary's caption");
    key("Escape");
    s = await until((s) => s.phase === "level" && !s.pda, "Escape to close it");
    assert(!s.menu, "Escape opened the menu");
    await live(true);
    // Touch: the PDA button at the top, left of Inventory.
    const tap = { pointerType: "touch", pointerId: 65, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    const pda = controls.querySelector(".touch-button.pda");
    const inventory = localRect(controls.querySelector(".touch-button.inventory"), controls);
    const local = localRect(pda, controls);
    assert(local.right < inventory.left && local.top < 100, `left of Inventory at the top: ${JSON.stringify(local)}`);
    const press = () => {
      const r = pda.getBoundingClientRect();
      for (const type of ["pointerdown", "pointerup"]) {
        pda.dispatchEvent(
          new PointerEvent(type, { clientX: (r.left + r.right) / 2, clientY: (r.top + r.bottom) / 2, pointerId: 66, pointerType: "touch", isPrimary: true, button: 0, buttons: type === "pointerdown" ? 1 : 0, bubbles: true, cancelable: true }),
        );
      }
    };
    press();
    s = await until((s) => s.pda, "the PDA button to open it");
    assertEqual(s.pda.tab, 0, "the tasks tab, as P");
    for (let i = 0; i < 50 && !controls.classList.contains("pda-open"); i++) await sleep(20);
    assert(!controls.hidden && controls.classList.contains("pda-open"), "the PDA button over the window");
    // With the cursor's controls (#856).
    const shown = [...controls.querySelectorAll(".touch-button")].filter((b) => b.getClientRects().length > 0).map((b) => b.dataset.button);
    assertEqual(shown.sort(), ["click", "menu", "pda", "right_click"], "the PDA button and the cursor's");
    // The stick onto the contacts' caption, Click switches to it.
    await stickTo(centreOf({ rect: s.pda.tabs[3].rect }));
    await tapClick();
    s = await until((s) => s.pda?.tab === 3, "Click to switch to the contacts");
    press();
    s = await until((s) => s.phase === "level" && !s.pda, "the PDA button to close it");
    await live(true);
    assert(!controls.classList.contains("pda-open"), "the level's controls again");
    return "";
  },

  "the PDA map: M opens it, flying to the player's marks; the wheel forward zooms out; on touch two fingers pinched apart zoom in": async () => {
    needsGame();
    const controls = document.getElementById("touch-controls");
    key("KeyM");
    let s = await until((s) => s.pda?.tab === 1 && s.pda.map?.shown, "M to open the map");
    // docs/behavior/pda-map.md 3: the first showing flies to the player.
    s = await until((s) => s.pda?.map && !s.pda.map.moving, "the flight to the player", 15000);
    const map = s.pda.map;
    assert(Math.abs(map.scale - 6) < 1e-3, `at the largest zoom: ${map.scale}`);
    assertEqual(map.marks.length, 2, "the player's two marks");
    // The mouse: the wheel forward (up) zooms out by 2/3 (5.5).
    for (let i = 0; i < 100 && controls.dataset.live !== "false"; i++) await sleep(20);
    await sleep(300);
    pointer("pointermove", client(480, 334));
    await sleep(100);
    view().dispatchEvent(new WheelEvent("wheel", { deltaY: -1, deltaMode: 1, bubbles: true, cancelable: true }));
    s = await until((s) => Math.abs(s.pda.map.zoom - 4) < 1e-3, "the wheel forward to zoom out");
    // Touch (our design, #743): two fingers pinched apart are wheel down.
    const ids = [nextTouch(), nextTouch()];
    const t = (i) => ({ pointerType: "touch", pointerId: ids[i], isPrimary: i === 0 });
    const at = (i, f) => client(480 + (i ? 1 : -1) * (40 + 80 * f), 334);
    [0, 1].forEach((i) => pointer("pointerdown", at(i, 0), { ...t(i), button: 0, buttons: 1 }));
    for (let n = 1; n <= 10; n++) {
      await sleep(25);
      [0, 1].forEach((i) => pointer("pointermove", at(i, n / 10), { ...t(i), buttons: 1 }));
    }
    [0, 1].forEach((i) => pointer("pointerup", at(i, 1), { ...t(i), button: 0, buttons: 0 }));
    s = await until((s) => s.pda.map.zoom > 5.9, "two fingers pinched apart to zoom in");
    key("Escape");
    s = await until((s) => s.phase === "level" && !s.pda, "Escape to close it");
    for (let i = 0; i < 100 && controls.dataset.live !== "true"; i++) await sleep(20);
    return "";
  },

  "touch: Torch and Night vision give the torch's commands as L and N do; Night vision shows only with an outfit that has it": async () => {
    needsGame();
    const controls = document.getElementById("touch-controls");
    // A touch anywhere shows the controls (a touch device).
    const tap = { pointerType: "touch", pointerId: 70, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    const torch = controls.querySelector(".touch-button.torch");
    const night = controls.querySelector(".touch-button.night-vision");
    const press = (el) => {
      const r = el.getBoundingClientRect();
      for (const type of ["pointerdown", "pointerup"]) {
        el.dispatchEvent(
          new PointerEvent(type, {
            clientX: (r.left + r.right) / 2,
            clientY: (r.top + r.bottom) / 2,
            pointerId: 71,
            pointerType: "touch",
            isPrimary: true,
            button: 0,
            buttons: type === "pointerdown" ? 1 : 0,
            bubbles: true,
            cancelable: true,
          }),
        );
      }
    };
    const shown = async (el, pressed, what) => {
      for (let i = 0; i < 50 && (el.hidden || el.getAttribute("aria-pressed") !== String(pressed)); i++) await sleep(20);
      assert(!el.hidden, `${what} shown`);
      assertEqual(el.getAttribute("aria-pressed"), String(pressed), `${what} shown ${pressed ? "on" : "off"}`);
    };
    // The torch attached and off: Torch shows, off; no outfit with night
    // vision is worn, so Night vision is hidden.
    let s = await state();
    assert(s.level.torch.attached && !s.level.torch.on, "the torch attached and off");
    await shown(torch, false, "Torch");
    assert(night.hidden, "Night vision hidden without an outfit that has it");
    const active = s.level.lights.active;
    press(torch);
    s = await until((s) => s.level.torch.on, "Torch to switch the torch on");
    assert(s.level.torch.main_active && s.level.torch.glow_on, "its main light and glow");
    assertEqual(s.level.lights.active, active + 1, "one more active light record");
    await shown(torch, true, "Torch");
    press(torch);
    await until((s) => !s.level.torch.on && s.level.lights.active === active, "Torch to switch it off");
    await shown(torch, false, "Torch");
    // An outfit with night vision worn: Night vision shows and switches its
    // effect on and off (torch.md 15.2).
    const outfit = await volya.call("debug_nightvision_outfit", { wear: true });
    await shown(night, false, "Night vision");
    press(night);
    s = await until((s) => s.level.torch.night_vision_effect, "Night vision to start the effect");
    assert(s.level.torch.night_vision_requested, "requested");
    await shown(night, true, "Night vision");
    press(night);
    s = await until((s) => !s.level.torch.night_vision_effect, "Night vision to stop it");
    assert(!s.level.torch.night_vision_requested && !s.level.torch.on, "not requested, the torch still off");
    await shown(night, false, "Night vision");
    // The outfit gone: hidden again.
    await volya.call("debug_nightvision_outfit", { wear: false });
    for (let i = 0; i < 50 && !night.hidden; i++) await sleep(20);
    assert(night.hidden, "Night vision hidden once the outfit is gone");
    return `outfit ${outfit}`;
  },

  "touch, experimental (#802): a look drag keeps the scene under the finger, a flick coasts on as a scroll view does (#754)": async () => {
    needsGame();
    // The setting applies at once in the running level: on a touch device
    // the game upright, at the canvas's own size (#806).
    await setTouchMode("experimental");
    assertEqual(volya.touchControls.running, "experimental", "the touch controls' mode");
    const controls = document.getElementById("touch-controls");
    assert(controls, "no touch controls");
    const tap = { pointerType: "touch", pointerId: 30, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    if (coarse()) {
      assert(upright() && !turned(view()) && !turned(controls), "the game and its controls upright");
      if (innerHeight > innerWidth) assert(render[1] > render[0], `drawn upright: ${render}`);
    }
    const look = controls.querySelector(".touch-look");
    const r = localRect(look, controls);
    // Drags from the right half's centre, as long as on #754's look area
    // there (the whole picture looks since #772).
    const span = (r.right - r.left) / 2;
    const [lx, ly] = [r.left + 1.5 * span, (r.top + r.bottom) / 2];
    // The picture in the controls' own layout, the game's (upright on an
    // upright phone).
    const [width, height] = [controls.clientWidth, controls.clientHeight];
    const touchOn = (type, at) => {
      const [x, y] = toClient(controls, at);
      look.dispatchEvent(
        new PointerEvent(type, {
          clientX: x,
          clientY: y,
          pointerId: 81,
          pointerType: "touch",
          isPrimary: true,
          button: type === "pointermove" ? -1 : 0,
          buttons: type === "pointerup" ? 0 : 1,
          bubbles: true,
          cancelable: true,
        }),
      );
    };
    const angles = async () => {
      const { yaw, pitch } = (await state()).level;
      return { yaw, pitch };
    };
    const settled = async () => {
      let last = await angles();
      for (let i = 0; i < 200; i++) {
        await sleep(50);
        const now = await angles();
        if (now.yaw === last.yaw && now.pitch === last.pitch) return now;
        last = now;
      }
      assert(false, "the view to stop turning");
    };
    // The turn that keeps the scene's direction under a finger moved from
    // `from` to `to` (controls px) at `pitch`: the engine's grab_turn, its
    // 67.5° field of view over the picture's height.
    const tanHalf = Math.tan(((67.5 / 2) * Math.PI) / 180);
    const norm = ([x, y]) => [(x - width / 2) / (height / 2), (y - height / 2) / (height / 2)];
    const grab = (from, to, pitch) => {
      const camera = ([x, y]) => {
        const v = [x * tanHalf, -y * tanHalf, 1];
        const l = Math.hypot(...v);
        return v.map((c) => c / l);
      };
      const world = (c, p) => [c[0], c[1] * Math.cos(p) - c[2] * Math.sin(p), c[1] * Math.sin(p) + c[2] * Math.cos(p)];
      const d = world(camera(norm(from)), pitch);
      const c = camera(norm(to));
      const p = Math.atan2(c[1], c[2]) - Math.asin(d[1] / Math.hypot(c[1], c[2]));
      const w = world(c, p);
      return { yaw: Math.atan2(d[0], d[2]) - Math.atan2(w[0], w[2]), pitch: p - pitch };
    };
    const deg = (rad) => (rad * 180) / Math.PI;
    // A drag from the right half's centre by `by` px at `speed` px/s, its
    // moves placed by the clock (about every 8 ms); held still `hold` ms
    // before the lift. The turn while held (settled), the turn in all, and
    // the finger's speed at the end.
    const drag = async (by, speed, hold) => {
      const before = await settled();
      const length = Math.hypot(...by);
      touchOn("pointerdown", [lx, ly]);
      const t0 = performance.now();
      let f = 0;
      while (f < 1) {
        await sleep(8);
        f = Math.min(1, ((performance.now() - t0) / 1000) * (speed / length));
        touchOn("pointermove", [lx + by[0] * f, ly + by[1] * f]);
      }
      const took = (performance.now() - t0) / 1000;
      let held = null;
      if (hold) {
        await sleep(hold);
        held = await settled();
      }
      touchOn("pointerup", [lx + by[0], ly + by[1]]);
      const after = await settled();
      const minus = (a) => ({ yaw: a.yaw - before.yaw, pitch: a.pitch - before.pitch });
      return { before, held: held && minus(held), after: minus(after), speed: length / took };
    };
    const start = await settled();
    const runs = [];
    // Slow drags held still before the lift: the scene stays under the
    // finger (within 1%, or 0.2°), and the lift flings nothing.
    for (const [what, by] of [
      ["right", [span * 0.4, 0]],
      ["down", [0, height * 0.3]],
      ["diagonal", [-span * 0.6, -height * 0.25]],
    ]) {
      const run = await drag(by, 250, 150);
      const want = grab([lx, ly], [lx + by[0], ly + by[1]], run.before.pitch);
      for (const k of ["yaw", "pitch"]) {
        const [got, exp] = [deg(run.held[k]), deg(want[k])];
        assert(Math.abs(got - exp) <= Math.max(0.2, 0.01 * Math.abs(exp)), `slow ${what} ${k}: ${got.toFixed(2)}° for ${exp.toFixed(2)}°`);
        assert(Math.abs(run.after[k] - run.held[k]) < 1e-4, `slow ${what}: the lift turned ${deg(run.after[k] - run.held[k])}°`);
      }
      runs.push(`slow ${what} ${deg(run.held.yaw).toFixed(1)}°/${deg(run.held.pitch).toFixed(1)}°`);
      // Back the same way, so the next tests find the view as it was.
      await drag([-by[0], -by[1]], 250, 150);
    }
    // Flicks lifted moving: the drag's turn, then v / −ln 0.998 more, v
    // the finger's angular speed at the lift (rad/ms, at most 4π/1000):
    // within 15%, the release velocity being fitted to clock-placed moves.
    // Then flicked back and touched 100 ms into the coast: it stops there.
    const half = span;
    const flicked = async (length, seconds) => {
      const run = await drag([-length, 0], length / seconds, 0);
      const end = [lx - length, ly];
      const dragged = grab([lx, ly], end, run.before.pitch).yaw;
      const rate = grab([end[0] + 1, ly], end, run.before.pitch).yaw; // rad a px
      const v = Math.min((rate * run.speed) / 1000, (4 * Math.PI) / 1000);
      return { run, dragged, projected: v / -Math.log(0.998) };
    };
    for (const [what, seconds] of [
      ["steady", 0.4],
      ["quick", 0.15],
    ]) {
      const length = half * 0.8;
      const { run, dragged, projected } = await flicked(length, seconds);
      const [got, want] = [run.after.yaw, dragged + projected];
      runs.push(
        `${what} flick of ${Math.round(length)} px (${Math.round(run.speed)} px/s) ${deg(got).toFixed(0)}° (drag ${deg(dragged).toFixed(0)}° + coast ${deg(projected).toFixed(0)}°)`,
      );
      assert(Math.abs(got - want) <= 0.15 * Math.abs(want), `${what}: ${deg(got).toFixed(1)}° for ${deg(want).toFixed(1)}°; ${runs.join(", ")}`);
      // Back: the same flick to the right, touched 100 ms after the lift.
      const before = await settled();
      touchOn("pointerdown", [lx - length, ly]);
      const t0 = performance.now();
      for (let f = 0; f < 1; ) {
        await sleep(8);
        f = Math.min(1, (performance.now() - t0) / 1000 / seconds);
        touchOn("pointermove", [lx - length + length * f, ly]);
      }
      touchOn("pointerup", [lx, ly]);
      await sleep(100);
      touchOn("pointerdown", [lx, ly]);
      const stopped = await settled();
      // Held past a tap's time, so the lift is no tap (#772).
      await sleep(550);
      touchOn("pointerup", [lx, ly]);
      const back = Math.abs(stopped.yaw - before.yaw);
      assert(back <= Math.abs(dragged) + 0.4 * Math.abs(projected), `the touch to stop the coast: ${deg(back).toFixed(1)}° back; ${runs.join(", ")}`);
    }
    // The view as it was for the next tests: slow drags held still, the
    // turn each needs at the middle (tan(33.75°) / half the height a px).
    const perPixel = tanHalf / (height / 2);
    for (let i = 0; i < 12; i++) {
      const now = await settled();
      const wrap = (a) => Math.atan2(Math.sin(a), Math.cos(a));
      const [dy, dp] = [wrap(start.yaw - now.yaw), start.pitch - now.pitch];
      if (Math.abs(dy) < 0.002 && Math.abs(dp) < 0.002) break;
      const most = half * 0.4;
      const clamp = (v) => Math.max(-most, Math.min(most, v));
      await drag([clamp(-dy / perPixel), clamp(-dp / perPixel)], 400, 120);
    }
    return `${width}×${height}: ${runs.join(", ")}`;
  },

  "touch, experimental: a tap on the floor runs there and stops near it, a double tap sprints, a tap on the marker cancels, drags look (from Jump too), Crouch and its swipe down, Menu opens the menu (#772)": async () => {
    needsGame();
    assertEqual(volya.touchControls.running, "experimental", "the touch controls' mode");
    const controls = document.getElementById("touch-controls");
    assert(controls, "no touch controls");
    // A touch anywhere shows them (a touch device).
    const tap = { pointerType: "touch", pointerId: 30, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    // Points in the controls' own layout (the game's picture, turned with
    // it on upright phones), given to the page in its coordinates.
    const centre = (el) => {
      const r = localRect(el, controls);
      return [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
    };
    const touchOn = (el, type, id, at) => {
      const [x, y] = toClient(controls, at);
      return el.dispatchEvent(
        new PointerEvent(type, {
          clientX: x,
          clientY: y,
          pointerId: id,
          pointerType: "touch",
          isPrimary: id === 31,
          button: type === "pointermove" ? -1 : 0,
          buttons: type === "pointerup" ? 0 : 1,
          bubbles: true,
          cancelable: true,
        }),
      );
    };
    const look = controls.querySelector(".touch-look");
    const marker = controls.querySelector(".touch-destination");
    const [width, height] = [controls.clientWidth, controls.clientHeight];
    // A tap at `y` (half-heights below the picture's centre), straight
    // ahead.
    let id = 60;
    const tapAhead = (y) => {
      const at = [width / 2, height / 2 + (y * height) / 2];
      touchOn(look, "pointerdown", ++id, at);
      touchOn(look, "pointerup", id, at);
    };
    const keyDown = (code) => document.body.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true }));
    const keyUp = (code) => document.body.dispatchEvent(new KeyboardEvent("keyup", { code, bubbles: true, cancelable: true }));
    // Look down a little, so the floor a metre and a half ahead shows; then
    // find a flat way: the way the inventory test's walk came first (behind,
    // where something 2.3 m on stops the keyboard's walk too),
    // then the sides, turned to with the arrow keys. A tap there whose
    // destination is not level with the feet is let go by a key (the
    // keyboard takes over a trip).
    let s = await state();
    const pitch = s.level.pitch;
    keyDown("ArrowDown");
    await until((s) => s.level.pitch > pitch + 0.3, "the down arrow to look down", 3000);
    keyUp("ArrowDown");
    const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
    let before = null;
    let destination = null;
    let away = 0;
    const tried = [];
    for (const turn of [Math.PI, Math.PI / 2, Math.PI]) {
      const facing = (await state()).level.yaw;
      keyDown("ArrowRight");
      await until((s) => s.level.yaw > facing + turn - 0.15, "the right arrow to turn", 3000);
      keyUp("ArrowRight");
      s = await until((s) => !s.level.player.moving, "standing still");
      before = s.level;
      tapAhead(0.8);
      s = await until((s) => s.level.player.trip, "the tap to set a destination");
      destination = s.level.player.trip;
      away = flat(destination, before.player.position);
      const rise = destination[1] - before.player.position[1];
      tried.push(`${away.toFixed(2)} m away, ${rise.toFixed(2)} m up`);
      if (away > 0.8 && away < 3 && Math.abs(rise) < 0.3) break;
      keyDown("KeyS");
      await until((s) => !s.level.player.trip, "a key to end the trip");
      keyUp("KeyS");
      destination = null;
    }
    assert(destination, `no flat floor ahead: ${tried.join("; ")}`);
    // A tap: run to at a run (no sprint) with its marker shown; the view
    // doesn't turn; it stops within 0.3 m.
    s = await until((s) => s.level.player.moving && s.level.player.running, "the tap to run");
    assert(!s.level.player.sprint_requested, "a single tap runs");
    for (let i = 0; i < 50 && marker.hidden; i++) await sleep(20);
    assert(!marker.hidden, "the destination marked");
    // The closest the polls saw it (they come a frame or more apart, at a
    // run's 4 m/s or so) and where it came to rest.
    let closest = Infinity;
    s = await until((s) => {
      closest = Math.min(closest, flat(s.level.player.position, destination));
      return !s.level.player.trip && !s.level.player.moving;
    }, "the trip to end and the player to stop", 6000);
    const atRest = flat(s.level.player.position, destination);
    const [how, atEnd] = s.level.player.trip_end ?? [];
    assert(how === "arrived", `the trip ended ${how} ${atEnd?.toFixed(2)} m from its destination ${JSON.stringify(destination)} (the closest seen ${closest.toFixed(2)} m), from ${JSON.stringify(before.player.position)} to ${JSON.stringify(s.level.player.position)}, pitch ${before.pitch}`);
    assert(atRest <= 0.8, `came to rest ${atRest.toFixed(2)} m from the destination`);
    assert(Math.abs(s.level.yaw - before.yaw) < 1e-4, "the view didn't turn");
    for (let i = 0; i < 50 && !marker.hidden; i++) await sleep(20);
    assert(marker.hidden, "the marker goes with the trip");
    const ran = flat(s.level.player.position, before.player.position);
    // Turned round, a double tap back the way it came: the trip sprints.
    const back = s.level.yaw;
    keyDown("ArrowRight");
    await until((s) => s.level.yaw > back + Math.PI - 0.15, "the right arrow to turn round", 3000);
    keyUp("ArrowRight");
    await until((s) => !s.level.player.moving, "standing still again");
    tapAhead(0.8);
    tapAhead(0.8);
    s = await until((s) => s.level.player.sprint && s.level.player.trip, "the double tap to sprint");
    s = await until((s) => !s.level.player.trip && !s.level.player.moving, "the sprint's trip to end", 6000);
    assert(!s.level.player.sprint_requested, "the sprint ends with its trip");
    // Turned round onto the way the trips came, a trip again, creeping in
    // the low crouch (Crouch swiped down) so it lasts; a tap on its marker
    // past the double tap's time cancels it, and the player stops short of
    // it; then standing up and turned back.
    const turnRound = async () => {
      const from = (await state()).level.yaw;
      keyDown("ArrowRight");
      await until((s) => s.level.yaw > from + Math.PI - 0.15, "the right arrow to turn round", 3000);
      keyUp("ArrowRight");
      await until((s) => !s.level.player.moving, "standing still");
    };
    await turnRound();
    const crouchButton = controls.querySelector(".touch-button.crouch");
    const [kx, ky] = centre(crouchButton);
    touchOn(crouchButton, "pointerdown", ++id, [kx, ky]);
    touchOn(crouchButton, "pointermove", id, [kx, ky + 30]);
    touchOn(crouchButton, "pointerup", id, [kx, ky + 30]);
    await until((s) => s.level.player.posture === 2 && s.level.player.crouch_toggled, "the swipe to crouch low");
    tapAhead(0.45);
    s = await until((s) => s.level.player.trip && s.level.player.moving, "a tap to creep there");
    const cancelled = s.level.player.trip;
    for (let i = 0; i < 50 && marker.hidden; i++) await sleep(20);
    await sleep(270);
    assert(!marker.hidden && (await state()).level.player.trip, "still under way, marked");
    const m = localRect(marker, controls);
    const onMarker = [(m.left + m.right) / 2 + 6, (m.top + m.bottom) / 2];
    touchOn(look, "pointerdown", ++id, onMarker);
    touchOn(look, "pointerup", id, onMarker);
    s = await until((s) => !s.level.player.trip && !s.level.player.moving, "the tap on the marker to stop the trip", 3000);
    const [cancelHow, cancelAt] = s.level.player.trip_end ?? [];
    assertEqual(cancelHow, "cancelled", "how the trip ended");
    const short = flat(s.level.player.position, cancelled);
    assert(short > 0.3, `stopped ${short.toFixed(2)} m short`);
    for (let i = 0; i < 50 && !marker.hidden; i++) await sleep(20);
    assert(marker.hidden, "the marker goes with the cancelled trip");
    // Standing up again (a tap on Crouch while crouched, on its release).
    touchOn(crouchButton, "pointerdown", ++id, [kx, ky]);
    touchOn(crouchButton, "pointerup", id, [kx, ky]);
    await until((s) => !s.level.player.crouch_toggled && s.level.player.posture === 0, "standing up");
    await turnRound();
    s = await state();
    // A drag looks and moves nothing.
    // About a quarter of the picture's height left (100 px on a phone held
    // across), so it turns as far upright (#806).
    const [lx, ly] = centre(look);
    const [dx, dy] = [height * 0.23, height * 0.045];
    const start = s.level;
    touchOn(look, "pointerdown", 32, [lx, ly]);
    touchOn(look, "pointermove", 32, [lx - dx, ly - dy]);
    // Held still, then lifted: no coast. The scene follows the finger left
    // and up: the view turns right and looks down.
    s = await until((s) => s.level.yaw > start.yaw + 0.2, "the drag to turn right");
    await sleep(120);
    touchOn(look, "pointerup", 32, [lx - dx, ly - dy]);
    assert(s.level.pitch > start.pitch, "the drag looks down");
    s = await state();
    assert(!s.level.player.trip && !s.level.player.moving, `a drag moved: ${JSON.stringify(s.level.player)}`);
    const turned = s.level.yaw - start.yaw;

    // A drag begun on Jump jumps and turns too.
    const jump = controls.querySelector(".touch-button.jump");
    const [jx, jy] = centre(jump);
    // A jump needs the player standing on the ground, not landing: wait for
    // that, then press; a press the movement layer didn't take (a frame
    // still landing) is let go and tried again.
    await until((s) => !s.level.player.airborne && !s.level.player.landing && !s.level.player.moving, "a state to jump from", 3000);
    const yaw = (await state()).level.yaw;
    let jumped = false;
    for (let attempt = 0, id = 380; attempt < 3 && !jumped; attempt++, id++) {
      touchOn(jump, "pointerdown", id, [jx, jy]);
      touchOn(jump, "pointermove", id, [jx + 20, jy]);
      touchOn(jump, "pointermove", id, [jx + height * 0.45, jy]);
      jumped = await until((s) => s.level.player.airborne, "Jump to jump", 1500).then(
        () => true,
        () => false,
      );
      await sleep(120);
      touchOn(jump, "pointerup", id, [jx + height * 0.45, jy]);
    }
    assert(jumped, "Jump to jump");
    // The scene dragged right: the view turns left.
    s = await until((s) => s.level.yaw < yaw - 0.15, "the drag from Jump to turn left").catch(async (e) => {
      throw new Error(`${e.message.slice(0, 60)}: turned ${((await state()).level.yaw - yaw).toFixed(3)} rad`);
    });
    s = await until((s) => !s.level.player.airborne, "the jump to land", 3000);
    // Crouch toggles: the engine reports it back to the button.
    const crouch = controls.querySelector(".touch-button.crouch");
    touchOn(crouch, "pointerdown", 33, centre(crouch));
    touchOn(crouch, "pointerup", 33, centre(crouch));
    await until((s) => s.level.player.crouch, "Crouch to crouch");
    for (let i = 0; i < 50 && crouch.getAttribute("aria-pressed") !== "true"; i++) await sleep(20);
    assertEqual(crouch.getAttribute("aria-pressed"), "true", "Crouch shown on");
    // Crouch swiped down: the low crouch, shown on the button.
    const [cx, cy] = centre(crouch);
    touchOn(crouch, "pointerdown", 39, [cx, cy]);
    touchOn(crouch, "pointermove", 39, [cx, cy + 30]);
    touchOn(crouch, "pointerup", 39, [cx, cy + 30]);
    await until((s) => s.level.player.posture === 2 && s.level.player.crouch_toggled, "the swipe to crouch low");
    for (let i = 0; i < 50 && !crouch.classList.contains("low"); i++) await sleep(20);
    assert(crouch.classList.contains("low") && crouch.getAttribute("aria-pressed") === "true", "low crouch shown");
    // Crouch while crouched waits for its gesture: nothing on the press,
    // a swipe down stays crouched, a tap stands up on release.
    touchOn(crouch, "pointerdown", 42, [cx, cy]);
    await sleep(100);
    assert(!(await volya.call("input_state")).held.includes("touch_crouch"), "no press while crouched");
    touchOn(crouch, "pointermove", 42, [cx, cy + 30]);
    touchOn(crouch, "pointerup", 42, [cx, cy + 30]);
    await sleep(150);
    s = await state();
    assert(s.level.player.crouch_toggled && s.level.player.posture === 2, `still low: ${JSON.stringify(s.level.player)}`);
    touchOn(crouch, "pointerdown", 44, [cx, cy]);
    touchOn(crouch, "pointercancel", 44, [cx, cy]);
    await sleep(150);
    assert((await state()).level.player.crouch_toggled, "a cancelled touch on Crouch is no tap");
    touchOn(crouch, "pointerdown", 43, [cx, cy]);
    await sleep(100);
    assert((await state()).level.player.crouch_toggled, "a tap waits for its release");
    touchOn(crouch, "pointerup", 43, [cx, cy]);
    await until((s) => !s.level.player.crouch_toggled, "a tap while crouched to stand up");
    for (let i = 0; i < 50 && crouch.getAttribute("aria-pressed") !== "false"; i++) await sleep(20);
    assertEqual(crouch.getAttribute("aria-pressed"), "false", "Crouch shown off");
    // Jump held down while Menu is pressed: when the controls hide in the
    // menu, nothing stays held.
    touchOn(jump, "pointerdown", 35, centre(jump));
    await sleep(100);
    assert((await volya.call("input_state")).held.includes("touch_jump"), "Jump held");
    const menu = controls.querySelector(".touch-button.menu");
    touchOn(menu, "pointerdown", 34, centre(menu));
    touchOn(menu, "pointerup", 34, centre(menu));
    const opened = await until((s) => inGame(s), "the in-game menu");
    assertEqual(inGame(opened).list, "menu_main_single", "list");
    for (let i = 0; i < 50 && !controls.hidden; i++) await sleep(20);
    assert(controls.hidden, "the touch controls hide in the menu");
    await sleep(100);
    const held = (await volya.call("input_state")).held;
    assert(!held.some((h) => h.startsWith("touch_")), `held after hiding: ${held}`);
    // In the ergonomic mode a touch clicks where it lands (#807): a tap on
    // "Возврат к игре" selects it, and once the bar rests one there
    // activates it; the controls show again.
    const item = client(...centreOfRow(0));
    for (let i = 0; i < 10 && inGame(await state()); i++) {
      await sleep(300);
      const finger = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
      pointer("pointerdown", item, { ...finger, buttons: 1 });
      pointer("pointerup", item, { ...finger, buttons: 0 });
      await until((s) => !inGame(s) || (inGame(s).selected === 0 && !inGame(s).moving), "item 0 at rest", 3000);
    }
    await until((s) => s.phase === "level" && !s.menu, "back in the game");
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show again");
    await quitToTitle();
    return `ran ${ran.toFixed(2)} m to a tap ${away.toFixed(2)} m away: it ended ${atEnd.toFixed(2)} m from it, came to rest ${atRest.toFixed(2)} m from it; a tap on the marker cancelled a trip ${cancelAt.toFixed(2)} m short, at rest ${short.toFixed(2)} m short; turned ${turned.toFixed(2)} rad`;
  },

  "touch, ergonomic mode (#807): a pinch in the level zooms the view about the picture's centre within 20° to 67.5°; on the in-game menu no cursor is drawn, a pinch magnifies the screen, and a tap clicks where it lands through the zoom": async () => {
    needsGame();
    await volya.touchControls.set("experimental");
    if ((await state()).phase === "menu") await enterLevel();
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    assertEqual(volya.touchControls.running, "experimental", "the mode");
    const controls = document.getElementById("touch-controls");
    const tap = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    const look = controls.querySelector(".touch-look");
    const [width, height] = [controls.clientWidth, controls.clientHeight];
    const fingers = [nextTouch(), nextTouch()];
    const on = (type, i, at) => {
      const [x, y] = toClient(controls, at);
      look.dispatchEvent(
        new PointerEvent(type, {
          clientX: x,
          clientY: y,
          pointerId: fingers[i],
          pointerType: "touch",
          isPrimary: i === 0,
          button: type === "pointermove" ? -1 : 0,
          buttons: type === "pointerup" ? 0 : 1,
          bubbles: true,
          cancelable: true,
        }),
      );
    };
    // Two fingers about the picture's centre, `gap` CSS pixels apart.
    const pair = (type, gap) => [0, 1].forEach((i) => on(type, i, [width / 2 + (i ? 1 : -1) * (gap / 2), height / 2]));
    const spread = async (from, to) => {
      for (let n = 1; n <= 10; n++) {
        await sleep(25);
        pair("pointermove", from + ((to - from) * n) / 10);
      }
    };
    await sleep(500);
    const before = (await state()).level;
    assertEqual(before.fov, 67.5, "the game's field of view at first");
    pair("pointerdown", 100);
    await spread(100, 200);
    await until((s) => s.level.fov < 60, "pinched apart to zoom in");
    await sleep(200);
    let s = await state();
    // Twice as far apart: the tangent of half the field of view halved.
    const want = (2 * Math.atan(Math.tan((67.5 * Math.PI) / 360) / 2) * 180) / Math.PI;
    const zoomedIn = s.level.fov;
    assert(Math.abs(zoomedIn - want) < 0.5, `the field of view ${zoomedIn}, not ${want.toFixed(2)}`);
    const turn = Math.abs(s.level.yaw - before.yaw) + Math.abs(s.level.pitch - before.pitch);
    assert(turn < 0.01, `a pinch about the centre doesn't turn: ${turn}`);
    // Far apart: no narrower than 20°; together: no wider than 67.5°.
    await spread(200, 1000);
    s = await until((s) => s.level.fov < 20.01, "far apart");
    assert(s.level.fov >= 20 - 1e-3, `at least 20°: ${s.level.fov}`);
    await spread(1000, 20);
    s = await until((s) => s.level.fov > 67.49, "together");
    assert(s.level.fov <= 67.5 + 1e-3, `no wider than the game's: ${s.level.fov}`);
    pair("pointerup", 20);

    // The in-game menu (it shows the game's cursor): no cursor drawn.
    key("Escape");
    await until((s) => inGame(s) && !inGame(s).moving, "the in-game menu");
    for (let i = 0; i < 50 && !controls.hidden; i++) await sleep(20);
    await sleep(100);
    assertEqual((await volya.call("input_state")).cursor_drawn, false, "no cursor drawn");
    // Two fingers pinched apart about "Возврат к игре" magnify the screen
    // twice, the item staying under them.
    const screen = document.getElementById("screen");
    const item = client(...centreOfRow(0));
    const ids = [nextTouch(), nextTouch()];
    const at = (i, gap) => [item[0] + (i ? 1 : -1) * (gap / 2), item[1]];
    const t = (i) => ({ pointerType: "touch", pointerId: ids[i], isPrimary: i === 0 });
    [0, 1].forEach((i) => pointer("pointerdown", at(i, 60), { ...t(i), button: 0, buttons: 1 }));
    for (let n = 1; n <= 10; n++) {
      await sleep(25);
      [0, 1].forEach((i) => pointer("pointermove", at(i, 60 + 6 * n), { ...t(i), buttons: 1 }));
    }
    [0, 1].forEach((i) => pointer("pointerup", at(i, 120), { ...t(i), button: 0, buttons: 0 }));
    const match = screen.style.transform.match(/translate\((-?[\d.e-]+)px, (-?[\d.e-]+)px\) scale\(([\d.]+)\)/);
    assert(match, `magnified: "${screen.style.transform}"`);
    const [x, y, scale] = match.slice(1).map(Number);
    assert(Math.abs(scale - 2) < 1e-6, `twice: ${scale}`);
    const shown = [x + scale * item[0], y + scale * item[1]];
    assert(Math.hypot(shown[0] - item[0], shown[1] - item[1]) < 1, `the item under the fingers: ${shown} ${item}`);
    await sleep(100);
    assert(inGame(await state()), "a pinch clicks nothing");
    // A tap on the item where it shows magnified: the cursor goes there
    // (selecting it), and a tap on it returns to the game.
    await sleep(300);
    const tapOn = () => {
      const finger = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
      pointer("pointerdown", shown, { ...finger, buttons: 1 });
      pointer("pointerup", shown, { ...finger, buttons: 0 });
    };
    tapOn();
    const { ui } = await volya.call("input_state");
    const [ix, iy] = centreOfRow(0);
    assert(Math.abs(ui[0] - ix) < 3 && Math.abs(ui[1] - iy) < 3, `the cursor at the tap: ${ui}, not ${[ix, iy]}`);
    for (let i = 0; i < 10; i++) {
      s = await until((s) => !inGame(s) || (inGame(s).selected === 0 && !inGame(s).moving), "item 0 at rest", 3000);
      if (!inGame(s)) break;
      await sleep(300);
      tapOn();
      await sleep(200);
    }
    await until((s) => s.phase === "level" && !s.menu, "a tap returns to the game");
    for (let i = 0; i < 50 && screen.style.transform !== ""; i++) await sleep(20);
    assertEqual(screen.style.transform, "", "the zoom ends with the cursor");
    // The standard tests start from a New Game of their own, as before.
    await quitToTitle();
    await setTouchMode("standard");
    return `pinched twice as far apart about the centre: ${zoomedIn.toFixed(2)}° (want ${want.toFixed(2)}°), turned ${turn.toFixed(4)} rad`;
  },

  "touch, standard (the default): a slow look drag across the look half turns about 180°, quicker drags up to twice as far (the look curve, #747)": async () => {
    needsGame();
    // A New Game's level after the experimental tests' quit to the title
    // menu, in the default controls.
    await setTouchMode("standard");
    if ((await state()).phase === "menu") await enterLevel();
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    assertEqual(volya.touchControls.running, "standard", "the touch controls' mode");
    const controls = document.getElementById("touch-controls");
    assert(controls, "no touch controls");
    // On an upright phone the game turned again, drawn landscape (#806).
    if (coarse() && innerHeight > innerWidth) {
      assert(!upright() && turned(view()), "the game turned");
      assert(render[0] > render[1], `drawn landscape: ${render}`);
    }
    const tap = { pointerType: "touch", pointerId: 30, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    const look = controls.querySelector(".touch-look");
    const r = localRect(look, controls);
    const [lx, ly] = [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
    // The screen's width in the game's own (landscape) layout: the phone's
    // long side, upright (turned) or not.
    const width = controls.clientWidth;
    const touchOn = (type, at) => {
      const [x, y] = toClient(controls, at);
      look.dispatchEvent(
        new PointerEvent(type, {
          clientX: x,
          clientY: y,
          pointerId: 81,
          pointerType: "touch",
          isPrimary: true,
          button: type === "pointermove" ? -1 : 0,
          buttons: type === "pointerup" ? 0 : 1,
          bubbles: true,
          cancelable: true,
        }),
      );
    };
    const settled = async () => {
      let last = (await state()).level.yaw;
      for (let i = 0; i < 100; i++) {
        await sleep(50);
        const yaw = (await state()).level.yaw;
        if (yaw === last) return yaw;
        last = yaw;
      }
      assert(false, "the view to stop turning");
    };
    // One swipe of `length` px at `speed` CSS px/s, its moves placed by the
    // clock (about every 8 ms) so jitter changes their count, not the speed;
    // the degrees it turned and the speed it took.
    const sweep = async (length, speed, dir) => {
      const before = await settled();
      touchOn("pointerdown", [lx, ly]);
      const t0 = performance.now();
      let x = 0;
      while (x < length) {
        await sleep(8);
        x = Math.min(length, ((performance.now() - t0) / 1000) * speed);
        touchOn("pointermove", [lx + dir * x, ly]);
      }
      const took = (performance.now() - t0) / 1000;
      touchOn("pointerup", [lx + dir * length, ly]);
      const turned = Math.abs((await settled()) - before) * (180 / Math.PI);
      return { turned, speed: length / took };
    };
    // The look area: the right half.
    const half = r.right - r.left;
    const runs = {};
    for (const [what, length, speed] of [
      ["slow", width, 250],
      ["steady", width, 1000],
      ["quick", width, width / 0.2],
      ["half slow", half, 250],
      ["half quick", half, half / 0.15],
    ]) {
      // Right, then back left alike, so the next tests find the view as it was.
      runs[what] = await sweep(length, speed, 1);
      await sweep(length, speed, -1);
    }
    const report = `${width} px wide, look half ${Math.round(half)}: ${Object.entries(runs)
      .map(([what, r]) => `${what} (${Math.round(r.speed)} px/s) ${r.turned.toFixed(0)}°`)
      .join(", ")}`;
    // At the default mouse_sens 0.12 (0.1375° a count): slow turns the base
    // 3.25 counts a pixel, so a slow drag across the look half about 180°; a
    // quick flick across it at least as far; nothing past the 6.5 cap.
    const degree = 0.1375;
    const perPixel = runs.slow.turned / width;
    assert(Math.abs(perPixel - 3.25 * degree) < 0.01, `slow: ${perPixel.toFixed(4)}° a pixel; ${report}`);
    const halfSlow = runs["half slow"].turned;
    assert(halfSlow >= 150 && halfSlow <= 200, `the slow half drag: ${report}`);
    assert(runs["half quick"].turned >= halfSlow, `the half flick: ${report}`);
    assert(runs.quick.turned <= 6.5 * degree * width + 1, `the quick swipe within the cap: ${report}`);
    return report;
  },

  "touch, standard: the stick walks and its sprint lock sprints, drags look (from Jump too), Crouch and its swipe down, Menu opens the menu, where the stick and Click return to the game, and Menu does": async () => {
    needsGame();
    const controls = document.getElementById("touch-controls");
    assert(controls, "no touch controls");
    // A touch anywhere shows them (a touch device).
    const tap = { pointerType: "touch", pointerId: 30, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    // Points in the controls' own layout (the game's picture, turned with
    // it on upright phones), given to the page in its coordinates.
    const centre = (el) => {
      const r = localRect(el, controls);
      return [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
    };
    const touchOn = (el, type, id, at) => {
      const [x, y] = toClient(controls, at);
      return el.dispatchEvent(
        new PointerEvent(type, {
          clientX: x,
          clientY: y,
          pointerId: id,
          pointerType: "touch",
          isPrimary: id === 31,
          button: type === "pointermove" ? -1 : 0,
          buttons: type === "pointerup" ? 0 : 1,
          bubbles: true,
          cancelable: true,
        }),
      );
    };
    const start = (await state()).level;
    // The stick pulled fully down runs back the way the inventory test's
    // walk came (clear for its whole run), held until the player has gone
    // half a metre from where this test started, however long the frames
    // take; a second finger then drags right to look.
    const move = controls.querySelector(".touch-move");
    const look = controls.querySelector(".touch-look");
    const [mx, my] = centre(move);
    const from = (s) => Math.hypot(s.level.player.position[0] - start.player.position[0], s.level.player.position[2] - start.player.position[2]);
    touchOn(move, "pointerdown", 31, [mx, my]);
    touchOn(move, "pointermove", 31, [mx, my + 80]);
    let s = await until((s) => s.level.player.moving && s.level.player.running, "the stick to run");
    s = await until((s) => from(s) > 0.5, "the stick to run half a metre", 4000);
    touchOn(move, "pointerup", 31, [mx, my + 80]);
    s = await until((s) => !s.level.player.moving, "the player to stop");
    const p = s.level.player;
    const moved = from(s);
    assert(moved > 0.5, `moved ${moved.toFixed(2)} m: ${JSON.stringify(p)}`);
    const [lx, ly] = centre(look);
    touchOn(look, "pointerdown", 32, [lx, ly]);
    touchOn(look, "pointermove", 32, [lx + 100, ly + 20]);
    touchOn(look, "pointerup", 32, [lx + 100, ly + 20]);
    // 100 px at mouse_sens 0.12: 0.24 rad right or more (the look curve),
    // a little down.
    s = await until((s) => s.level.yaw > start.yaw + 0.2, "the drag to turn right");
    assert(s.level.pitch > start.pitch, "the drag looks down");
    const turned = s.level.yaw - start.yaw;
    // The sprint lock: the stick pushed forward past its rim (1.5 radii)
    // sprints; let go on the lock target (2.1 radii up) it keeps sprinting
    // with nobody on it; a touch on the stick ends it.
    const ring = controls.querySelector(".touch-stick");
    touchOn(move, "pointerdown", 36, [mx, my]);
    touchOn(move, "pointermove", 36, [mx, my - 84]);
    await until((s) => s.level.player.sprint_requested, "past the rim to request the sprint");
    assert(ring.classList.contains("past"), "the lock target shown past the rim");
    touchOn(move, "pointermove", 36, [mx, my - 118]);
    touchOn(move, "pointerup", 36, [mx, my - 118]);
    s = await until((s) => s.level.player.sprint_locked && s.level.player.sprint, "the lock to sprint hands-free");
    const sprintAt = s.level.player.position;
    await sleep(150);
    s = await state();
    assert(s.level.player.sprint_locked, `the lock held: ${JSON.stringify(s.level.player)}`);
    for (let i = 0; i < 50 && !ring.classList.contains("locked"); i++) await sleep(20);
    assert(ring.classList.contains("locked"), "the stick shown locked");
    touchOn(move, "pointerdown", 37, [mx, my]);
    touchOn(move, "pointerup", 37, [mx, my]);
    s = await until((s) => !s.level.player.sprint_locked && !s.level.player.sprint_requested, "a touch to end the lock");
    s = await until((s) => !s.level.player.moving, "the player to stop after the lock");
    const sprinted = Math.hypot(s.level.player.position[0] - sprintAt[0], s.level.player.position[2] - sprintAt[2]);
    for (let i = 0; i < 50 && ring.classList.contains("locked"); i++) await sleep(20);
    assert(!ring.classList.contains("locked"), "the stick shown unlocked");
    // Thumbs landing high: the target is drawn closer (1.95 radii, its
    // drop area just below the safe area's top edge), or at the very top
    // turned aside; either way on the screen, and the engine locks where
    // it is drawn.
    const top = localRect(controls.querySelector(".touch-button.menu"), controls).top;
    const lockTarget = controls.querySelector(".touch-lock");
    let id = 50;
    for (const [y, what] of [
      [top + 142, "closer"],
      [top + 4, "aside"],
    ]) {
      touchOn(move, "pointerdown", ++id, [mx, y]);
      const r = localRect(lockTarget, controls);
      assert(r.top >= top - 1 && r.left >= 0, `the ${what} lock target on the screen: ${JSON.stringify(r)}`);
      const [tx, ty] = [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
      const d = Math.hypot(tx - mx, ty - y);
      touchOn(move, "pointermove", id, [mx + ((tx - mx) / d) * 84, y + ((ty - y) / d) * 84]);
      await until((s) => s.level.player.sprint_requested, `past the rim toward the ${what} target`);
      touchOn(move, "pointermove", id, [tx, ty]);
      touchOn(move, "pointerup", id, [tx, ty]);
      await until((s) => s.level.player.sprint_locked && s.level.player.sprint, `the ${what} target to lock`);
      touchOn(move, "pointerdown", ++id, [mx, my]);
      touchOn(move, "pointerup", id, [mx, my]);
      await until((s) => !s.level.player.sprint_locked && !s.level.player.moving, "a touch to end the lock and stop");
    }
    // A cancelled touch on the target neither locks nor keeps sprinting.
    touchOn(move, "pointerdown", ++id, [mx, my]);
    touchOn(move, "pointermove", id, [mx, my - 84]);
    await until((s) => s.level.player.sprint_requested, "past the rim to request the sprint again");
    touchOn(move, "pointermove", id, [mx, my - 118]);
    touchOn(move, "pointercancel", id, [mx, my - 118]);
    s = await until((s) => !s.level.player.sprint_requested && !s.level.player.moving, "the cancel to end the sprint");
    assert(!s.level.player.sprint_locked && !ring.classList.contains("locked"), "a cancelled touch locked");

    // A drag begun on Jump jumps and turns too.
    const jump = controls.querySelector(".touch-button.jump");
    const [jx, jy] = centre(jump);
    // A jump needs the player standing on the ground, not landing: wait for
    // that, then press; a press the movement layer didn't take (a frame
    // still landing, a take-off refused on a steep contact where the
    // sprints left the player) is let go and tried again, the release and
    // the new press often in one frame on a loaded machine (#852).
    await until((s) => !s.level.player.airborne && !s.level.player.landing && !s.level.player.moving, "a state to jump from", 3000);
    const yaw = (await state()).level.yaw;
    let jumped = false;
    const seen = [];
    for (let attempt = 0, id = 380; attempt < 3 && !jumped; attempt++, id++) {
      touchOn(jump, "pointerdown", id, [jx, jy]);
      touchOn(jump, "pointermove", id, [jx - 20, jy]);
      touchOn(jump, "pointermove", id, [jx - 100, jy]);
      // What the attempt saw: the jump flag (a take-off refused still
      // sets it), the button held, the latest actions.
      const look = { jump: false };
      jumped = await until(
        (s) => {
          look.jump ||= s.level.player.jump;
          return s.level.player.airborne;
        },
        "Jump to jump",
        1500,
      ).then(
        () => true,
        () => false,
      );
      const i = await volya.call("input_state");
      look.held = i.held.includes("touch_jump");
      look.recent = i.recent.slice(-4).map((a) => `${a.action} ${a.control ?? ""}`);
      seen.push(look);
      touchOn(jump, "pointerup", id, [jx - 100, jy]);
    }
    if (!jumped) {
      const { held } = await volya.call("input_state");
      const s = await state();
      const p = s.level.player;
      assert(false, `Jump to jump: held ${held}; ${JSON.stringify({ airborne: p.airborne, landing: p.landing, moving: p.moving, crouch: p.crouch, posture: p.posture, jump: p.jump, falling: p.falling, climbing: p.climbing, velocity: p.velocity, position: p.position, menu: Boolean(s.menu), seen })}; jump at ${[jx, jy]}, controls ${controls.clientWidth}×${controls.clientHeight}, ${JSON.stringify(controls.querySelector(".touch-button.jump").getBoundingClientRect())}`);
    }
    s = await until((s) => s.level.yaw < yaw - 0.15, "the drag from Jump to turn left");
    s = await until((s) => !s.level.player.airborne, "the jump to land", 3000);
    // Crouch toggles: the engine reports it back to the button.
    const crouch = controls.querySelector(".touch-button.crouch");
    touchOn(crouch, "pointerdown", 33, centre(crouch));
    touchOn(crouch, "pointerup", 33, centre(crouch));
    await until((s) => s.level.player.crouch, "Crouch to crouch");
    for (let i = 0; i < 50 && crouch.getAttribute("aria-pressed") !== "true"; i++) await sleep(20);
    assertEqual(crouch.getAttribute("aria-pressed"), "true", "Crouch shown on");
    // Crouch swiped down: the low crouch, shown on the button.
    const [cx, cy] = centre(crouch);
    touchOn(crouch, "pointerdown", 39, [cx, cy]);
    touchOn(crouch, "pointermove", 39, [cx, cy + 30]);
    touchOn(crouch, "pointerup", 39, [cx, cy + 30]);
    await until((s) => s.level.player.posture === 2 && s.level.player.crouch_toggled, "the swipe to crouch low");
    for (let i = 0; i < 50 && !crouch.classList.contains("low"); i++) await sleep(20);
    assert(crouch.classList.contains("low") && crouch.getAttribute("aria-pressed") === "true", "low crouch shown");
    // Crouch while crouched waits for its gesture: nothing on the press,
    // a swipe down stays crouched, a tap stands up on release.
    touchOn(crouch, "pointerdown", 42, [cx, cy]);
    await sleep(100);
    assert(!(await volya.call("input_state")).held.includes("touch_crouch"), "no press while crouched");
    touchOn(crouch, "pointermove", 42, [cx, cy + 30]);
    touchOn(crouch, "pointerup", 42, [cx, cy + 30]);
    await sleep(150);
    s = await state();
    assert(s.level.player.crouch_toggled && s.level.player.posture === 2, `still low: ${JSON.stringify(s.level.player)}`);
    touchOn(crouch, "pointerdown", 44, [cx, cy]);
    touchOn(crouch, "pointercancel", 44, [cx, cy]);
    await sleep(150);
    assert((await state()).level.player.crouch_toggled, "a cancelled touch on Crouch is no tap");
    touchOn(crouch, "pointerdown", 43, [cx, cy]);
    await sleep(100);
    assert((await state()).level.player.crouch_toggled, "a tap waits for its release");
    touchOn(crouch, "pointerup", 43, [cx, cy]);
    await until((s) => !s.level.player.crouch_toggled, "a tap while crouched to stand up");
    for (let i = 0; i < 50 && crouch.getAttribute("aria-pressed") !== "false"; i++) await sleep(20);
    assertEqual(crouch.getAttribute("aria-pressed"), "false", "Crouch shown off");
    // Jump held down while Menu is pressed: when the controls hide in the
    // menu, nothing stays held.
    touchOn(jump, "pointerdown", 35, centre(jump));
    await sleep(100);
    assert((await volya.call("input_state")).held.includes("touch_jump"), "Jump held");
    const menu = controls.querySelector(".touch-button.menu");
    touchOn(menu, "pointerdown", 34, centre(menu));
    touchOn(menu, "pointerup", 34, centre(menu));
    const opened = await until((s) => inGame(s), "the in-game menu");
    assertEqual(inGame(opened).list, "menu_main_single", "list");
    for (let i = 0; i < 50 && controls.dataset.view !== "cursor"; i++) await sleep(20);
    assertEqual(controls.dataset.view, "cursor", "the cursor's controls in the menu");
    await sleep(100);
    const held = (await volya.call("input_state")).held;
    assert(!held.some((h) => h.startsWith("touch_")), `held after the level's controls went: ${held}`);
    // The stick (#856): "Возврат к игре" under the cursor, selected and
    // under the glass once the bar rests; Click activates it; the level's
    // controls show again.
    await stickTo(centreOfRow(0));
    await until((s) => inGame(s)?.selected === 0 && !inGame(s).moving, "item 0 at rest", 3000);
    await tapClick();
    await until((s) => s.phase === "level" && !s.menu, "back in the game");
    for (let i = 0; i < 50 && controls.dataset.view !== "level"; i++) await sleep(20);
    assertEqual(controls.dataset.view, "level", "the level's controls again");
    // Menu again; on the menu Menu is back (the gamepad's east): the game
    // again.
    touchOn(menu, "pointerdown", 36, centre(menu));
    touchOn(menu, "pointerup", 36, centre(menu));
    await until((s) => inGame(s), "the in-game menu again");
    for (let i = 0; i < 50 && controls.dataset.view !== "cursor"; i++) await sleep(20);
    touchOn(menu, "pointerdown", 37, centre(menu));
    touchOn(menu, "pointerup", 37, centre(menu));
    await until((s) => s.phase === "level" && !s.menu, "Menu on the menu to return to the game");
    await quitToTitle();
    return `moved ${moved.toFixed(2)} m, turned ${turned.toFixed(2)} rad, sprint-locked ${sprinted.toFixed(2)} m`;
  },

  "touch and gamepad: Use shows while the trader's door is under the crosshair; a tap and the d-pad right use it, leaving it locked": async () => {
    needsGame();
    if ((await state()).phase === "menu") await enterLevel();
    let s = await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    const controls = document.getElementById("touch-controls");
    const use = controls.querySelector(".touch-button.use");
    // A touch anywhere shows the controls (a touch device).
    const t = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...t, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...t, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    // Facing the start view, nothing to use; the door is behind (a half
    // turn with the right arrow).
    assert(!s.level.player.hint && use.hidden, `Use hidden: hint ${s.level.player.hint}`);
    const down = (code) => document.body.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true }));
    const up = (code) => document.body.dispatchEvent(new KeyboardEvent("keyup", { code, bubbles: true, cancelable: true }));
    const yaw = s.level.yaw;
    down("ArrowRight");
    await until((s) => s.level.yaw > yaw + Math.PI - 0.08, "the right arrow to turn round", 3000);
    up("ArrowRight");
    await until((s) => s.level.player.hint === "tip_door_locked", "the door-locked hint");
    for (let i = 0; i < 50 && use.hidden; i++) await sleep(20);
    assert(!use.hidden, "Use shown");
    const r = use.getBoundingClientRect();
    for (const type of ["pointerdown", "pointerup"]) {
      use.dispatchEvent(
        new PointerEvent(type, {
          clientX: (r.left + r.right) / 2,
          clientY: (r.top + r.bottom) / 2,
          pointerId: nextTouch(),
          pointerType: "touch",
          isPrimary: true,
          button: 0,
          buttons: type === "pointerdown" ? 1 : 0,
          bubbles: true,
          cancelable: true,
        }),
      );
    }
    await sleep(300);
    s = await state();
    assertEqual([s.level.doors[0].locked, s.level.player.hint], [true, "tip_door_locked"], "the door after the touch Use");
    // The gamepad's d-pad right (button 15 of the standard mapping).
    const pad = { index: 0, mapping: "standard", axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) };
    const getGamepads = navigator.getGamepads;
    navigator.getGamepads = () => [pad];
    try {
      pad.buttons[15] = { pressed: true, value: 1 };
      await sleep(100);
      pad.buttons[15] = { pressed: false, value: 0 };
      await sleep(300);
      s = await state();
    } finally {
      navigator.getGamepads = getGamepads;
    }
    assertEqual([s.level.doors[0].locked, s.level.player.hint], [true, "tip_door_locked"], "the door after the d-pad right");
    // Turned back to the start view, nothing has a hint.
    const back = s.level.yaw;
    down("ArrowLeft");
    await until((s) => s.level.yaw < back - Math.PI + 0.08, "the left arrow to turn back", 3000);
    up("ArrowLeft");
    await until((s) => !s.level.player.hint, "no hint turned away");
    for (let i = 0; i < 50 && !use.hidden; i++) await sleep(20);
    assert(use.hidden, "Use hidden once nothing has a hint");
    return "";
  },

  "a gamepad: the left stick runs, the d-pad up switches the torch; where the game shows its cursor the gamepad is a mouse: the stick moves the cursor to a PDA caption and A presses it, the shoulders zoom the map as the wheel, A held drags an inventory item, A presses a menu item; B closes and opens the menu": async () => {
    needsGame();
    // A New Game's level after the touch test's quit to the title menu.
    if ((await state()).phase === "menu") await enterLevel();
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    // A gamepad (the Gamepad API, faked): the left stick pushed up runs.
    const start = (await state()).level.player;
    const pad = { index: 0, mapping: "standard", axes: [0, -1, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) };
    const getGamepads = navigator.getGamepads;
    navigator.getGamepads = () => [pad];
    const hold = (i, pressed) => (pad.buttons[i] = { pressed, value: pressed ? 1 : 0 });
    const tap = async (i) => {
      hold(i, true);
      await sleep(100);
      hold(i, false);
      await sleep(100);
    };
    // The virtual cursor (#835): the left stick steers the game's cursor,
    // read back by `cursor`, to UI point `to`, easing off near it.
    const stickTo = async (cursor, to, what) => {
      for (let i = 0; i < 400; i++) {
        const [x, y] = cursor(await state());
        const [dx, dy] = [to[0] - x, to[1] - y];
        const d = Math.hypot(dx, dy);
        if (d < 6) {
          pad.axes[0] = pad.axes[1] = 0;
          await sleep(100);
          return;
        }
        const tilt = d > 100 ? 0.75 : d > 30 ? 0.5 : 0.4;
        pad.axes[0] = (dx / d) * tilt;
        pad.axes[1] = (dy / d) * tilt;
        await sleep(30);
      }
      pad.axes[0] = pad.axes[1] = 0;
      throw new Error(`the stick to bring the cursor to ${what}: at ${cursor(await state())}`);
    };
    const centre4 = ([x, y, w, h], within) => {
      const [lx, ly, lw, lh] = within ?? [x, y, w, h];
      return [Math.min(x + w / 2, lx + lw / 2), Math.min(y + h / 2, ly + lh / 2)];
    };
    try {
      await until((s) => s.level.player.moving && s.level.player.running, "the left stick to run");
      await sleep(250);
      pad.axes[1] = 0;
      const stopped = await until((s) => !s.level.player.moving, "the player to stop");
      const p = stopped.level.player;
      const moved = Math.hypot(p.position[0] - start.position[0], p.position[2] - start.position[2]);
      assert(moved > 0.1, `the gamepad moved ${moved.toFixed(2)} m`);
      // The d-pad up switches the torch on and off (our design).
      for (const on of [true, false]) {
        hold(12, true);
        await until((s) => s.level.torch.on === on, `the d-pad up to switch the torch ${on ? "on" : "off"}`);
        hold(12, false);
        await sleep(100);
      }
      // The PDA (our design): d-pad left opens it as P does; in it the left
      // stick moves the cursor, south presses, east closes it as Escape
      // does.
      pointer("pointermove", client(512, 384));
      await tap(14);
      let s = await until((s) => s.pda, "the d-pad left to open the PDA");
      assertEqual(s.pda.tab, 0, "the tasks tab");
      await stickTo((s) => s.pda.cursor, centreOf(s.pda.tabs[6]), "the last caption");
      await tap(0);
      s = await until((s) => s.pda?.tab === 6, "A to switch to the last tab");
      // The d-pad nudges the cursor.
      const before = s.pda.cursor;
      await tap(15);
      s = await until((s) => s.pda.cursor[0] > before[0], "the d-pad right to nudge the cursor");
      assert(s.pda.cursor[0] - before[0] < 10, `a small step: ${before} to ${s.pda.cursor}`);
      await tap(1);
      s = await until((s) => s.phase === "level" && !s.pda, "B to close it");
      assert(!s.menu, "B opened the menu");
      // The map (our design, #743): over it the right shoulder zooms in at
      // the cursor, the left out, as the wheel.
      key("KeyM");
      s = await until((s) => s.pda?.map?.shown && !s.pda.map.moving, "M to open the map", 15000);
      const [mx, my, mw, mh] = s.pda.map.main;
      await stickTo((s) => s.pda.cursor, [mx + mw / 2, my + mh / 2], "the map");
      const zoom = (await state()).pda.map.zoom;
      await tap(4);
      s = await until((s) => s.pda.map.zoom < zoom - 1e-3, "the left shoulder to zoom out");
      await tap(5);
      s = await until((s) => Math.abs(s.pda.map.zoom - zoom) < 1e-3, "the right shoulder to zoom in");
      await tap(1);
      s = await until((s) => s.phase === "level" && !s.pda, "B to close the map");
      // The inventory: north opens it; A held over the worn outfit drags it
      // to the backpack, and back.
      await tap(3);
      s = await until((s) => s.inventory, "Y to open the inventory");
      const place = (s) => s.level.character.items.find((i) => i.section === "novice_outfit")?.place;
      assertEqual(place(s), "slot", "the outfit worn");
      const outfitList = s.inventory.lists.find((l) => l.list === "outfit");
      await stickTo((s) => s.inventory.cursor, centre4(outfitList.cells[0].rect, outfitList.rect), "the outfit");
      hold(0, true);
      await sleep(100);
      await tap(13);
      await until((s) => s.inventory?.dragging != null, "A held and a nudge to drag the outfit");
      await stickTo((s) => s.inventory.cursor, [100, 600], "the backpack");
      hold(0, false);
      s = await until((s) => place(s) === "backpack", "the outfit dropped in the backpack");
      const bagCell = s.inventory.lists.find((l) => l.list === "bag").cells.find((c) => c.section === "novice_outfit");
      await stickTo((s) => s.inventory.cursor, centre4(bagCell.rect), "the outfit in the backpack");
      hold(0, true);
      await sleep(100);
      await tap(13);
      await until((s) => s.inventory?.dragging != null, "the drag back");
      await stickTo((s) => s.inventory.cursor, centre4(outfitList.rect), "the outfit list");
      hold(0, false);
      await until((s) => place(s) === "slot", "the outfit worn again");
      await tap(1);
      s = await until((s) => s.phase === "level" && !s.inventory, "B to close the inventory");
      assert(!s.menu, "B opened the menu");
      // B in the level opens the menu; the stick moves the cursor to the
      // return item and A presses it.
      await tap(1);
      s = await until((s) => inGame(s) && !inGame(s).moving, "B to open the menu");
      const back = inGame(s).items.findIndex((i) => i.name === "btn_ret");
      assert(back >= 0, `the return item: ${inGame(s).items.map((i) => i.name)}`);
      await stickTo((s) => inGame(s).cursor, centreOfRow(back), "the return item");
      await until((s) => inGame(s)?.selected === back && !inGame(s).moving, "the return item at rest under the glass", 3000);
      await tap(0);
      await until((s) => s.phase === "level" && !s.menu, "A to return to the game");
    } finally {
      pad.axes = [0, 0, 0, 0];
      navigator.getGamepads = getGamepads;
    }
  },

  ...keptTeardown(),
};
