// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Playing a level from the kept import, with every input path: the desktop's
// full screen offer and Keyboard Lock, the keyboard (walk, jump, torch), the
// touch controls and zoom guard, the inventory screen (pointer lock, mouse,
// touch, touchpad), the touch stick, look, sprint lock, Jump, Crouch and
// Menu, and a gamepad.

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
  sleep,
  state,
  swipe,
  swipeTo,
  tapAt,
  toClient,
  touchId,
  turned,
  until,
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
    const sources = ["frame", "crouch", "creep", "sprint", "segments", "spot"].reduce((n, k) => n + art[k].width * art[k].height * 4, 0);
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

  "the inventory: I opens it over the running level (a held W walks on, Control crouches under it), the outfit dragged to the backpack, bread eaten, the touch button opens it, the touchpad selects, opens the menu and drags, Escape and the exit button close it": async () => {
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
    // Headless Chrome grants no pointer lock: then it is stubbed (the
    // lock element, its request and exit, each exit's change event), so the
    // page's exit path runs.
    const root = document.documentElement;
    let stubbed = null;
    const stub = () => {
      stubbed = null;
      const change = () => document.dispatchEvent(new Event("pointerlockchange"));
      Object.defineProperty(document, "pointerLockElement", { get: () => stubbed, configurable: true });
      root.requestPointerLock = () => {
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
    }
    assert(document.pointerLockElement === root, "the pointer locked");
    down("KeyW");
    await until((s) => s.level.player.moving, "W to walk");
    key("KeyI");
    s = await until((s) => s.inventory, "the inventory screen");
    await live(false);
    assert(!document.pointerLockElement, "the pointer released");
    if (!granted) unstub();
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
    for (let i = 0; i < 50 && !controls.hidden; i++) await sleep(20);
    assert(controls.hidden, "the touch controls hide under the screen");
    // Touch is the touchpad here (#108): the cursor swiped onto the outfit
    // in the backpack, a tap selects it, a long press opens its menu, whose
    // first row («одеть костюм») a tap at the cursor chooses.
    const bag = s.inventory.lists.find((l) => l.list === "bag");
    const suitCell = bag.cells.find((c) => c.section === "novice_outfit");
    await swipeTo(centreOf4(suitCell.rect));
    await tapAt();
    s = await until((s) => s.inventory?.selected === suitCell.items[0], "a tap to select the outfit");
    await sleep(300);
    const press = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 384), { ...press, buttons: 1 });
    await sleep(650);
    pointer("pointerup", client(512, 384), { ...press, buttons: 0 });
    s = await until((s) => s.inventory?.menu, "a long press to open the context menu");
    assertEqual(s.inventory.menu.entries, ["st_dress_outfit", "st_drop"], "the outfit's entries");
    await swipeTo(centreOf({ rect: s.inventory.menu.rows[0] }));
    await tapAt();
    s = await until((s) => item(s, "novice_outfit")?.place === "slot", "the outfit worn again");
    // A double-tap-hold on the character picture drags the outfit; let go
    // on its own list, nothing moves.
    const worn = s.inventory.lists.find((l) => l.list === "outfit");
    await swipeTo(centreOf4(worn.cells[0].rect, worn.rect));
    await sleep(300);
    const finger = [512, 384];
    pointer("pointerdown", client(...finger), { ...press, pointerId: nextTouch(), buttons: 1 });
    pointer("pointerup", client(...finger), { ...press, pointerId: touchId, buttons: 0 });
    await sleep(50);
    const id = nextTouch();
    await swipe([finger], [0, 30], { lift: false, ids: [id] });
    s = await until((s) => s.inventory?.dragging === worn.cells[0].items[0], "a double-tap-hold to drag");
    pointer("pointerup", client(finger[0], finger[1] + 30), { ...press, pointerId: id, buttons: 0 });
    s = await until((s) => s.inventory && s.inventory.dragging == null, "the drag to end");
    assertEqual(item(s, "novice_outfit").place, "slot", "dropped on its own list");
    // The exit button closes it.
    clickAt(centreOf({ rect: s.inventory.exit }));
    s = await until((s) => s.phase === "level" && !s.inventory, "the exit button to close the screen");
    assert(!s.menu, "no menu");
    await live(true);
    return `game time ran ${((s.level.game_time - clock) / 1000).toFixed(1)} s under the screen; pointer lock ${granted ? "granted" : "stubbed"}`;
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

  "touch: the stick walks and its sprint lock sprints, drags look (from Jump too), Crouch and its swipe down, Menu opens the menu": async () => {
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
    // still landing) is let go and tried again.
    await until((s) => !s.level.player.airborne && !s.level.player.landing && !s.level.player.moving, "a state to jump from", 3000);
    const yaw = (await state()).level.yaw;
    let jumped = false;
    for (let attempt = 0, id = 380; attempt < 3 && !jumped; attempt++, id++) {
      touchOn(jump, "pointerdown", id, [jx, jy]);
      touchOn(jump, "pointermove", id, [jx - 20, jy]);
      touchOn(jump, "pointermove", id, [jx - 100, jy]);
      jumped = await until((s) => s.level.player.airborne, "Jump to jump", 1500).then(
        () => true,
        () => false,
      );
      touchOn(jump, "pointerup", id, [jx - 100, jy]);
    }
    assert(jumped, "Jump to jump");
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
    for (let i = 0; i < 50 && !controls.hidden; i++) await sleep(20);
    assert(controls.hidden, "the touch controls hide in the menu");
    await sleep(100);
    const held = (await volya.call("input_state")).held;
    assert(!held.some((h) => h.startsWith("touch_")), `held after hiding: ${held}`);
    // The touchpad (#108): "Возврат к игре" under the cursor, selected and
    // under the glass once the bar rests; a tap activates it; the controls
    // show again.
    await swipeTo(centreOfRow(0));
    await until((s) => inGame(s)?.selected === 0 && !inGame(s).moving, "item 0 at rest", 3000);
    await tapAt([200, 200]);
    await until((s) => s.phase === "level" && !s.menu, "back in the game");
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show again");
    await quitToTitle();
    return `moved ${moved.toFixed(2)} m, turned ${turned.toFixed(2)} rad, sprint-locked ${sprinted.toFixed(2)} m`;
  },

  "a gamepad: the left stick runs, the d-pad up switches the torch": async () => {
    needsGame();
    // A New Game's level after the touch test's quit to the title menu.
    if ((await state()).phase === "menu") await enterLevel();
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    // A gamepad (the Gamepad API, faked): the left stick pushed up runs.
    const start = (await state()).level.player;
    const pad = { index: 0, mapping: "standard", axes: [0, -1, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) };
    const getGamepads = navigator.getGamepads;
    navigator.getGamepads = () => [pad];
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
        pad.buttons[12] = { pressed: true, value: 1 };
        await until((s) => s.level.torch.on === on, `the d-pad up to switch the torch ${on ? "on" : "off"}`);
        pad.buttons[12] = { pressed: false, value: 0 };
        await sleep(100);
      }
    } finally {
      navigator.getGamepads = getGamepads;
    }
  },

  ...keptTeardown(),
};
