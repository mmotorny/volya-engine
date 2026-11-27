// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The game's screens from the kept import: the save screen's name (the
// device keyboard's input on touch, the on-screen keyboard with a gamepad),
// the in-game menu and its load screen, the options screen (touch controls,
// sliders, the key table, rebinding), Apply storing `user.ltx`, the
// rebound key in a level and the settings read back at the next launch.

import { assert, assertEqual } from "./harness.js";
import {
  answerBox,
  bounds,
  button,
  centreOf,
  clickAt,
  client,
  control,
  cssPerUi,
  gpu,
  inGame,
  keptSetup,
  keptTeardown,
  key,
  launchToMenu,
  loadMs,
  needsGame,
  nextTouch,
  openOptions,
  options,
  pointAt,
  pointer,
  quitToTitle,
  render,
  selectInGame,
  sleep,
  state,
  storedEnds,
  swipe,
  stickTo,
  tapClick,
  cursorButton,
  until,
  untilOptions,
  view,
} from "./game.js";

export const tests = {
  ...keptSetup({ level: true, tap: true }),

  "the save screen's name: typed through the device keyboard's input on touch, and the on-screen keyboard with a gamepad": async () => {
    needsGame();
    key("Escape");
    await until((s) => inGame(s), "the in-game menu");
    await selectInGame("btn_save");
    key("Enter");
    const save = async () => (await until((s) => s.menu?.save_screen, "the save screen")).menu.save_screen;
    const [fx, fy, fw, fh] = (await save()).field;
    // Touch (seen earlier in this page): the text entry is where the field
    // is, but takes no taps itself (#94).
    const entry = document.querySelector("#text-entry input");
    for (let i = 0; i < 50 && entry.parentElement.hidden; i++) await sleep(20);
    assert(!entry.parentElement.hidden, "the text entry shows");
    const centre = [fx + fw / 2, fy + fh / 2];
    assert(document.elementFromPoint(...client(...centre)) !== entry, "the text entry takes taps");
    // A tap on the field, or Click with the cursor elsewhere, focuses
    // nothing.
    await stickTo([100, 100]);
    const t = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
    pointer("pointerdown", client(...centre), { ...t, buttons: 1 });
    pointer("pointerup", client(...centre), { ...t, buttons: 0 });
    await tapClick();
    await sleep(200);
    assert(document.activeElement !== entry && !(await save()).focused, "a tap on the field focused it");
    // The stick over the field: Click clicks it, and the page focuses its
    // input in Click's lift (the device's keyboard rises only so).
    await stickTo(centre);
    await sleep(200);
    await tapClick();
    assertEqual(document.activeElement, entry, "the input focused in Click's lift");
    await until((s) => s.menu?.save_screen?.focused, "the field focused by Click");
    // Typed text reaches the field by its own rules (Cyrillic types nothing).
    entry.value = "Ab1ж";
    entry.dispatchEvent(new InputEvent("input", { inputType: "insertText", data: "Ab1ж", bubbles: true }));
    await until((s) => s.menu?.save_screen?.name === "Ab1", "the typed name");
    assertEqual(entry.value, "", "the input emptied");
    // An input method's composition: nothing until it ends, then its text.
    entry.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    entry.value = "x";
    entry.dispatchEvent(new InputEvent("input", { inputType: "insertCompositionText", data: "x", isComposing: true, bubbles: true }));
    await sleep(100);
    assertEqual((await save()).name, "Ab1", "nothing while composing");
    assertEqual(entry.value, "x", "the composition kept in the input");
    entry.dispatchEvent(new CompositionEvent("compositionend", { data: "x", bubbles: true }));
    await until((s) => s.menu?.save_screen?.name === "Ab1x", "the composed text");
    assertEqual(entry.value, "", "the input emptied after composing");
    // Typographic quotes as plain ones.
    entry.dispatchEvent(new InputEvent("input", { inputType: "insertText", data: "\u2019", bubbles: true }));
    await until((s) => s.menu?.save_screen?.name === "Ab1x'", "a curly quote typed as '");
    const del = () => entry.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", code: "", bubbles: true, cancelable: true }));
    del();
    del();
    await until((s) => s.menu?.save_screen?.name === "Ab1", "deleted again");
    // The device keyboard's Backspace and Enter without key codes.
    const press = (key) => entry.dispatchEvent(new KeyboardEvent("keydown", { key, code: "", bubbles: true, cancelable: true }));
    press("Backspace");
    await until((s) => s.menu?.save_screen?.name === "Ab", "Backspace");
    press("Enter");
    await until((s) => s.menu?.save_screen && !s.menu.save_screen.focused, "Enter ends the focus");
    for (let i = 0; i < 50 && document.activeElement === entry; i++) await sleep(20);
    assert(document.activeElement !== entry, "the device keyboard closes");
    // A gamepad (faked): south on the field opens the on-screen keyboard.
    const pad = { index: 0, mapping: "standard", axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) };
    const getGamepads = navigator.getGamepads;
    navigator.getGamepads = () => [pad];
    // The page polls gamepads once an animation frame: each state is held
    // for two frames at least, however slow the page's frames are (the
    // iOS Simulator's can outlast a fixed wait).
    const frames = (n) => new Promise((resolve) => requestAnimationFrame(() => (n > 1 ? frames(n - 1).then(resolve) : resolve())));
    const button = async (i) => {
      pad.buttons[i].pressed = true;
      await sleep(80);
      await frames(2);
      pad.buttons[i].pressed = false;
      await sleep(80);
      await frames(2);
    };
    try {
      await button(0);
      let s = await save();
      assert(s.focused && s.keyboard, `the keyboard opens: ${JSON.stringify({ focused: s.focused, keyboard: s.keyboard })}`);
      assertEqual(s.keyboard.selected, "1", "the first key highlighted");
      // Down twice (13), south (0): "a"; west (2): Backspace.
      await button(13);
      await button(13);
      assertEqual((await save()).keyboard.selected, "a", "highlighted");
      await button(0);
      await until((s) => s.menu?.save_screen?.name === "Aba", "a typed");
      await button(2);
      await until((s) => s.menu?.save_screen?.name === "Ab", "Backspace");
      if (gpu.state === "ready") {
        const scene = await volya.call("menu_scene", { width: 1024, height: 768 });
        assert(scene.sprites.some((p) => p.layer === "keyboard key"), "the keyboard drawn");
      }
      // East (1): Done; the screen stays.
      await button(1);
      s = (await until((s) => s.menu?.save_screen && !s.menu.save_screen.keyboard, "the keyboard closes")).menu.save_screen;
      await sleep(200);
      assert((await state()).menu?.save_screen, "east's release leaves the screen open");
      assert(!s.focused, "the focus ends");
      assertEqual(s.name, "Ab", "the name kept");
    } finally {
      navigator.getGamepads = getGamepads;
    }
    // Back to the menu, then the game.
    key("Escape");
    await until((s) => inGame(s) && !s.menu.save_screen, "the in-game menu");
    key("Escape");
    await until((s) => s.phase === "level" && !s.menu, "back in the game");
  },

  "the in-game menu: Escape opens it and stops the clock, Return resumes, Load lists no saves, quitting asks": async () => {
    needsGame();
    key("Escape");
    let s = await until((s) => inGame(s), "the in-game menu");
    // title-menu.md section 4.1: the data's list, the last-save item included.
    assertEqual(
      inGame(s).items.map((i) => i.name),
      ["btn_ret", "btn_lastsave", "btn_load", "btn_save", "btn_options", "btn_credits", "btn_quit_to_mm", "btn_quit"],
      "items",
    );
    const stopped = s.level.game_time;
    const effects = (await volya.call("particles_state")).clock;
    await sleep(300);
    assertEqual((await state()).level.game_time, stopped, "the game clock stops");
    assertEqual((await volya.call("particles_state")).clock, effects, "the effects' clock stops");
    if (gpu.state === "ready") {
      // The menu covers the screen; the level stays on the GPU.
      const stats = await volya.call("level_stats");
      assertEqual(stats.world_draws, 7782, "the world kept on the GPU");
    }
    // "Возврат к игре": the game runs again.
    key("Enter");
    await until((s) => s.phase === "level" && !s.menu, "back in the game");
    await until((s) => s.level.game_time > stopped, "the clock to run again");
    // Load: the load screen with no saves; Escape returns to the menu.
    key("Escape");
    await until((s) => inGame(s), "the in-game menu");
    await selectInGame("btn_load");
    key("Enter");
    s = await until((s) => s.menu?.load_screen, "the load screen");
    assertEqual(s.menu.load_screen.list.rows.length, 0, "no saves");
    assertEqual(s.menu.load_screen.list.rect, [490, 399, 429, 180], "the list");
    key("Escape");
    await until((s) => inGame(s) && !s.menu.load_screen, "back to the menu");
    // Quit game: the box; "Нет" keeps the game, "Да" ends it.
    await selectInGame("btn_quit_to_mm");
    key("Enter");
    s = await until((s) => s.menu?.message, "the quit box");
    assertEqual(s.menu.message.name, "message_box_quit_game", "box");
    await answerBox("no");
    await until((s) => inGame(s) && !s.menu.message, "the box to close");
    assertEqual((await state()).phase, "level", "still in the game");
    key("Enter");
    await answerBox("yes");
    const title = await until((s) => s.phase === "menu", "the title menu");
    assertEqual(title.menu.list, "menu_main", "list");
    const end = (await storedEnds()).at(-1);
    assertEqual([end?.args.reason, end?.args.message], ["quit", "quit to the title menu"], "the end in the diagnostic log");
  },

  "Options opens from the title menu": async () => {
    needsGame();
    await openOptions();
    const o = (await state()).options;
    assertEqual(o.controls.length, 31, "option controls");
    assertEqual(o.cells.length, 118, "key cells");
    const scene = await volya.call("menu_scene", { width: 1024, height: 768 });
    assertEqual(bounds(scene, "dialog").slice(0, 2), [414.5, 167.5], "dialog");
    assert(scene.sprites.some((s) => s.layer === "list_renderer line"), "renderer line");
    // A tap in the black bars beside a 4:3 picture clicks nothing (the
    // standard touch controls, #856), and the cursor stays where it is.
    const rect = view().getBoundingClientRect();
    const rotated = rect.width < rect.height !== view().clientWidth < view().clientHeight;
    if (rotated || view().clientWidth / view().clientHeight < render[0] / render[1] + 0.05) {
      return "no bars at the sides here";
    }
    // The cursor on nothing (the screen's top left).
    pointer("pointermove", client(5, 5));
    await sleep(100);
    const before = await volya.call("input_state");
    await sleep(300);
    const touch = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
    const bar = [rect.left + 2, rect.top + rect.height / 2];
    pointer("pointerdown", bar, { ...touch, buttons: 1 });
    pointer("pointerup", bar, { ...touch, buttons: 0 });
    await sleep(100);
    const after = await volya.call("input_state");
    const actions = after.recent.filter((a) => a.seq > before.count).map((a) => `${a.action} ${a.control}`);
    assertEqual(actions, [], "nothing");
    assertEqual(after.ui, before.ui, "the cursor after a tap in the bar");
    return "a tap in the bar clicks nothing";
  },

  "touch: Click held drags a slider with the stick; a two-finger swipe scrolls the key table": async () => {
    needsGame();
    await stickTo(centreOf(button(await options(), "tab 1")));
    await tapClick();
    let o = await untilOptions((o) => o.page === "sound", "the sound tab");
    // On the music volume's bar: Click held, the stick left past the
    // bar's start.
    const bar = control(o, "track_musicvolume");
    await stickTo(centreOf(bar));
    const value = () => options().then((o) => parseFloat(control(o, "track_musicvolume").value));
    await sleep(300);
    await cursorButton("click", true);
    await stickTo([bar.rect[0] - 30, centreOf(bar)[1]]);
    await sleep(100);
    assertEqual((await volya.call("input_state")).held, ["mouse_left"], "held while dragging");
    assertEqual(await value(), 0, "dragged to the bar's start");
    // The page losing focus mid-drag releases it; Click's lift then does
    // nothing.
    dispatchEvent(new Event("blur"));
    await sleep(100);
    const blurred = await volya.call("input_state");
    assertEqual(blurred.held, [], "released by the blur");
    await cursorButton("click", false);
    await sleep(100);
    const after = await volya.call("input_state");
    assertEqual([after.ui, after.count], [blurred.ui, blurred.count], "Click's lift after the blur");
    // The controls tab; two fingers on the key table swiped up scroll it
    // down, swiped down back.
    await stickTo(centreOf(button(await options(), "tab 3")));
    await tapClick();
    o = await untilOptions((o) => o.page === "controls", "the controls tab");
    const cell = o.cells.find((c) => c.action === "forward" && c.slot === 0);
    await stickTo(centreOf(cell));
    const top = o.scroll[1];
    await sleep(300);
    await swipe([[400, 500], [470, 500]], [0, -120 / cssPerUi()]);
    o = await untilOptions((o) => o.scroll[1] > top, "the key table to scroll");
    const scrolled = o.scroll[1];
    await sleep(300);
    await swipe([[400, 300], [470, 300]], [0, 300 / cssPerUi()]);
    o = await untilOptions((o) => o.scroll[1] === top, "the key table to scroll back");
    assertEqual(o.page, "controls", "nothing clicked");
    return `scrolled ${scrolled - top} positions`;
  },
  "a setting, a tap and a key changed, Apply stores them": async () => {
    needsGame();
    // The controls tab; the mouse-sensitivity bar clicked at a third.
    clickAt(centreOf(button(await options(), "tab 3")));
    let o = await untilOptions((o) => o.page === "controls", "the controls tab");
    // Frame times with the key table shown (the latest 60 frames).
    await sleep(1500);
    const stats = await volya.call("level_stats");
    const bar = control(o, "track_mousesens");
    clickAt([bar.rect[0] + 12 + (bar.rect[2] - 24) / 3, bar.rect[1] + 10]);
    o = await untilOptions((o) => control(o, "track_mousesens").value !== bar.value, "the bar to move");
    const sens = control(o, "track_mousesens").value;
    // kb_forward's primary cell waits for a key: T.
    const cell = o.cells.find((c) => c.action === "forward" && c.slot === 0);
    assertEqual(cell.key, "kW", "forward's key");
    clickAt(centreOf(cell));
    await untilOptions((o) => o.waiting === "forward:0", "the cell to wait");
    key("KeyT");
    o = await untilOptions((o) => o.cells.find((c) => c.action === "forward" && c.slot === 0).key === "kT", "kT");
    // Touch (the touch controls' stick and Click): the cursor to the sound
    // tab, Click; then to the EAX check box, Click.
    await stickTo(centreOf(button(o, "tab 1")));
    await tapClick();
    o = await untilOptions((o) => o.page === "sound", "the sound tab");
    const eax = control(o, "check_eax").value;
    await stickTo(centreOf(control(o, "check_eax")));
    await tapClick();
    o = await untilOptions((o) => control(o, "check_eax").value !== eax, "the tap to toggle EAX");
    clickAt(centreOf(button(o, "btn_accept")));
    await until((s) => s.phase === "menu" && !s.options, "Apply to close the screen");
    let stored;
    for (let i = 0; i < 50 && !stored?.includes("bind forward kT"); i++) {
      stored = await volya.call("user_config");
      await sleep(50);
    }
    assert(stored?.includes("bind forward kT\r\n"), "the binding stored");
    assert(stored.includes(`mouse_sens ${Number(sens).toFixed(5).replace(/0+$/, "")}\r\n`), `mouse_sens ${sens} stored`);
    assert(stored.includes(`snd_efx ${eax === "on" ? "off" : "on"}\r\n`), "snd_efx stored");
    return `mouse_sens ${sens}; controls tab frame CPU ${stats.frame_cpu_ms.toFixed(2)} ms (max ${stats.frame_cpu_max_ms.toFixed(2)})`;
  },

  "the rebound key walks in the level": async () => {
    needsGame();
    await pointAt(0);
    key("Enter");
    await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
    key("Enter");
    await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", loadMs());
    const tap = { pointerType: "touch", pointerId: 22, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 384), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 384), { ...tap, buttons: 0 });
    await until((s) => s.phase === "level" && !s.intro, "the intro skipped");
    const down = (code) => document.body.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true }));
    const up = (code) => document.body.dispatchEvent(new KeyboardEvent("keyup", { code, bubbles: true, cancelable: true }));
    // W no longer moves.
    down("KeyW");
    await sleep(300);
    const still = (await state()).level.player;
    up("KeyW");
    assert(!still.moving, "W still walks");
    down("KeyT");
    await until((s) => s.level.player.moving, "T to walk forward");
    up("KeyT");
    await until((s) => !s.level.player.moving, "the player to stop");
    await quitToTitle();
  },

  "Полный экран is the page's full screen: Apply asks for it, leaving it clears the box; disabled where the page can't": async () => {
    needsGame();
    const video = async () => {
      // The cursor off the menu first, so it comes onto Options afresh.
      pointer("pointermove", client(5, 5));
      await sleep(300);
      await openOptions();
      clickAt(centreOf(button(await options(), "tab 0")));
      return untilOptions((o) => o.page === "video", "the video tab");
    };
    let o = await video();
    const box = (o) => control(o, "check_fullscreen");
    // As the page tells the engine (fullscreen.js).
    if (!(document.documentElement.requestFullscreen && document.fullscreenEnabled !== false)) {
      assert(!box(o).enabled, "disabled without the page's full screen");
      key("Escape");
      await until((s) => s.phase === "menu" && !s.options, "Escape to close the screen");
      return "disabled: this browser has no full screen for the page";
    }
    assert(box(o).enabled, "enabled");
    assertEqual(box(o).value, "off", "off while the page isn't in full screen");
    // Headless Chrome can't enter full screen without a real gesture: the
    // API is stubbed, recording its calls.
    const calls = [];
    const request = Element.prototype.requestFullscreen;
    const exit = document.exitFullscreen;
    Element.prototype.requestFullscreen = async function () {
      calls.push(["enter", this === document.documentElement]);
      Object.defineProperty(document, "fullscreenElement", { value: document.documentElement, configurable: true });
      document.dispatchEvent(new Event("fullscreenchange"));
    };
    document.exitFullscreen = async () => {
      calls.push(["exit"]);
      delete document.fullscreenElement;
      document.dispatchEvent(new Event("fullscreenchange"));
    };
    try {
      clickAt(centreOf(box(o)));
      await untilOptions((o) => box(o).value === "on", "the box to check");
      clickAt(centreOf(button(await options(), "btn_accept")));
      await until((s) => s.phase === "menu" && !s.options, "Apply to close the screen");
      for (let i = 0; i < 50 && !calls.length; i++) await sleep(20);
      assertEqual(calls, [["enter", true]], "the page's root asked for full screen");
      // Left as the browser lets the player (Escape): the box follows.
      await document.exitFullscreen();
      o = await video();
      assertEqual(box(o).value, "off", "off once the page left full screen");
      key("Escape");
      await until((s) => s.phase === "menu" && !s.options, "Escape to close the screen");
    } finally {
      Element.prototype.requestFullscreen = request;
      document.exitFullscreen = exit;
      delete document.fullscreenElement;
    }
    return "entered at Apply, cleared on leaving";
  },

  "the settings are read back when the game starts again": async () => {
    needsGame();
    key("KeyQ");
    await until((s) => s.phase === "none", "the game to end");
    await launchToMenu();
    await openOptions();
    clickAt(centreOf(button(await options(), "tab 3")));
    const o = await untilOptions((o) => o.page === "controls", "the controls tab");
    assertEqual(o.cells.find((c) => c.action === "forward" && c.slot === 0).key, "kT", "forward's key");
    // The defaults back (section 11.6), then Escape closes the screen.
    clickAt(centreOf(button(o, "btn_default")));
    await untilOptions((o) => o.cells.find((c) => c.action === "forward" && c.slot === 0).key === "kW", "the defaults");
    key("Escape");
    await until((s) => s.phase === "menu" && !s.options, "Escape to close the screen");
  },

  ...keptTeardown(),
};
