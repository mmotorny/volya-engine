// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Startup sequence and title menu (#32), in a fresh profile: launching
// without the game fails, the archives they read are imported from the real
// installer, the logo sequence is skipped with synthesized Escape presses,
// and the menu is driven with synthesized keyboard, mouse and touch (the
// touchpad) input.

import { assert, assertEqual } from "./harness.js";
import {
  CAPTIONS,
  bounds,
  centreOfRow,
  client,
  gpu,
  importGameData,
  installer,
  key,
  needsGame,
  noteHeap,
  pointAt,
  pointer,
  readGpu,
  readRender,
  render,
  row,
  sleep,
  state,
  swipeTo,
  tapAt,
  until,
} from "./game.js";

export const tests = {
  "launch without an imported game fails and keeps the page": async () => {
    await readGpu();
    await volya.call("forget_game");
    // The installer's user settings apply (a stored copy from an earlier
    // run would not).
    await volya.call("forget_user_config");
    const error = await volya.call("launch").then(() => null, (e) => e.message);
    assert(error?.includes("not imported"), `expected a clear error, got ${error}`);
    assertEqual((await volya.call("input_state")).capture, false, "capture");
    assertEqual(document.querySelector("main").hidden, false, "page hidden");
  },

  "imports what the startup and the menu read": async () => {
    return importGameData(true);
  },

  "launch loads under the splash and starts the logo sequence": async () => {
    needsGame();
    const launched = await volya.call("launch");
    assertEqual(launched.splash, true, "splash picture found");
    // The sequence starts on the frame after loading.
    const s = await until((s) => s.phase === "startup", "the logo sequence", 2000);
    assertEqual([s.startup.item, s.startup.items], [0, 5], "sequence");
    assertEqual((await volya.call("input_state")).capture, true, "capture");
    assertEqual(document.querySelector("main").hidden, true, "page interface hidden");
    await noteHeap("launched");
    return `loaded in ${launched.loading_ms.toFixed(0)} ms`;
  },

  "the first logo plays after a second of black": async () => {
    needsGame();
    await until((s) => s.startup?.elapsed_ms > 1100, "the THQ logo", 4000);
    const scene = await volya.call("menu_scene", { width: 1024, height: 768 });
    const video = scene.sprites.find((s) => s.layer === "video");
    assert(video, `no video: ${scene.sprites.map((s) => s.layer)}`);
    assertEqual(video.texture.path, "gamedata\\textures\\intro\\thq_logo_hyuv.ogm", "video");
    // startup.md section 6.3: 1024 × 688.4 units, centred.
    const [x0, y0, x1, y1] = bounds(scene, "video");
    assert(x0 === -0.5 && x1 === 1023.5 && y0 === 38.5 && Math.abs(y1 - 726.86) < 0.01, `video ${[x0, y0, x1, y1]}`);
  },

  "Escape skips each logo, then the title menu shows": async () => {
    needsGame();
    for (let item = 1; item <= 5; item++) {
      key("Escape");
      await until((s) => s.phase === "menu" || s.startup?.item === item, `item ${item}`);
    }
    const s = await until((s) => s.phase === "menu", "the menu");
    await readRender();
    // The installer's settings: 1024 × 768 (options-screens.md 12.4).
    assertEqual(render, [1024, 768], "drawn at vid_mode");
    assertEqual(s.menu.list, "menu_main", "list");
    await noteHeap("the title menu");
    assertEqual(
      s.menu.items.map((i) => i.caption),
      CAPTIONS,
      "captions",
    );
    assertEqual(s.menu.version, "ver. 1.0006", "version");
    assertEqual(s.menu.music.track, "music\\wasteland2", "music");
    assert(s.menu.music.voices?.length === 2, `music voices ${JSON.stringify(s.menu.music)}`);
  },

  "the bar slides onto the first item": async () => {
    needsGame();
    const s = await until((s) => !s.menu.moving, "the bar at rest", 2000);
    assertEqual([s.menu.selected, s.menu.bar_y], [0, -22], "bar");
    assertEqual(s.menu.glass, [675, 419, 232, 36], "glass");
  },

  "the menu scene follows the specs at 1024×768 and 1280×720": async () => {
    needsGame();
    // At the shipped r__supersample 2 the title menu is laid out on the
    // off-screen target (frame-presentation.md 5.1, text-and-fonts.md 4.4:
    // 1448 × 1086 here, 1810 × 1018 at 1280 × 720). The scene gives what is
    // drawn into the target in the screen pixels that the presenter maps to
    // its target position t: (t + 0.5) · W / W′ − 0.5.
    const onTarget = ([w, h], [tw, th]) => {
      const [kx, ky] = [w / tw, h / th];
      return ([x0, y0, x1, y1]) => [(x0 + 0.5) * kx - 0.5, (y0 + 0.5) * ky - 0.5, (x1 + 0.5) * kx - 0.5, (y1 + 0.5) * ky - 0.5];
    };
    // A picture at (x, y), w × h units (ui-layout.md 5.3) on the target.
    const picture = ([tw, th], x, y, w, h) => {
      const [sx, sy] = [tw / 1024, th / 768];
      const [left, top] = [Math.floor(x * sx) - 0.5, Math.floor(y * sy) - 0.5];
      return [left, top, left + w * sx, top + h * sy];
    };
    // A glyph box of text-and-fonts.md section 5, given there as layout
    // positions times W / W′.
    const text = ([w, h], [tw, th]) => {
      const t = onTarget([w, h], [tw, th]);
      return ([x0, y0, x1, y1]) => t([x0 * (tw / w), y0 * (th / h), x1 * (tw / w), y1 * (th / h)]);
    };
    const near = (layer, scene, want) => {
      const got = bounds(scene, layer);
      assert(got.every((v, i) => Math.abs(v - want[i]) < 1.5e-3), `${layer}: expected ${want}, got ${got}`);
    };
    const XGA = [1024, 768];
    const T = [1448, 1086];
    const xga = await volya.call("menu_scene", { width: 1024, height: 768 });
    const at = onTarget(XGA, T);
    near("background", xga, [-0.5, -0.5, 1023.5, 767.5]);
    near("back_movie", xga, at(picture(T, 148, 0, 876, 400)));
    near("fire_movie", xga, at(picture(T, 525, 459, 128, 256)));
    near("bar", xga, at(picture(T, 599, 397, 392, 73)));
    const glyphs = text(XGA, T);
    near("caption 0", xga, glyphs([739.436, 425.724, 824.436, 447.724]));
    near("caption 5", xga, glyphs([712.436, 575.646, 850.436, 597.646]));
    near("version", xga, glyphs([14.851, 739.713, 93.851, 758.713]));
    const wide = await volya.call("menu_scene", { width: 1280, height: 720 });
    // Computed from the same rules: pen y floor(1018 / 768 · 425.701) = 564
    // target pixels, 564 · 720 / 1018 on the screen.
    const wideGlyphs = text([1280, 720], [1810, 1018]);
    near("caption 0", wide, wideGlyphs([935.326, 398.9, 1020.326, 420.9]));
    near("version", wide, wideGlyphs([18.387, 693.124, 97.387, 712.124]));
  },

  "renders the menu": async () => {
    needsGame();
    if (gpu.state !== "ready") {
      assertEqual(gpu.state, "unavailable", "graphics");
      const line = document.getElementById("status").textContent;
      assert(line.includes("graphics unavailable"), `status line: ${line}`);
      return `no graphics (${gpu.message}): the menu runs without them`;
    }
    const { rgba } = await volya.call("debug_capture", { width: 1024, height: 768 });
    const luma = (x, y) => rgba[(y * 1024 + x) * 4] + rgba[(y * 1024 + x) * 4 + 1] + rgba[(y * 1024 + x) * 4 + 2];
    // The last caption (713–851, 573–595) is black text on the slats.
    let dark = 0;
    for (let y = 573; y < 595; y++) for (let x = 713; x < 851; x++) dark += luma(x, y) < 60;
    assert(dark > 0.1 * 138 * 22, `${dark} dark caption pixels`);
    const colours = new Set();
    for (let i = 0; i < rgba.length; i += 4) colours.add((rgba[i] << 16) | (rgba[i + 1] << 8) | rgba[i + 2]);
    assert(colours.size > 1000, `${colours.size} colours`);
    // The sky is the looping panorama video: it changes.
    const sky = async () => (await volya.call("debug_capture", { width: 64, height: 48 })).rgba.slice(0, 64 * 4 * 8);
    const first = await sky();
    let changed = false;
    for (let i = 0; i < 20 && !changed; i++) {
      await sleep(100);
      changed = (await sky()).some((v, j) => v !== first[j]);
    }
    assert(changed, "the sky does not move");
    return `${colours.size} colours`;
  },

  "arrow keys move the selection and turn the wheels": async () => {
    needsGame();
    key("ArrowDown");
    let s = await until((s) => s.menu.selected === 1, "item 1");
    assertEqual(s.menu.bar_target, 8, "target");
    s = await until((s) => !s.menu.moving, "the bar at rest", 2000);
    assertEqual(s.menu.bar_y, 8, "bar");
    assert(s.menu.wheel_angle !== 0, "the wheels did not turn");
    key("ArrowUp");
    key("ArrowUp");
    await until((s) => s.menu.selected === 0, "item 0");
  },

  "the mouse selects by hovering and activates with a click on the glass": async () => {
    needsGame();
    pointer("pointermove", row(3));
    await until((s) => s.menu.selected === 3 && !s.menu.moving, "item 3 at rest", 2000);
    pointer("pointerdown", row(3), { button: 0, buttons: 1 });
    pointer("pointerup", row(3), { button: 0, buttons: 0 });
    const s = await until((s) => s.menu.activations.length > 0, "an activation");
    assertEqual(s.menu.activations.at(-1).item, "btn_options", "activated");
    // It opened the options screen; Escape closes it.
    await until((s) => s.options, "the options screen");
    key("Escape");
    await until((s) => !s.options, "the options screen to close");
    // A click outside the glass activates nothing.
    pointer("pointerdown", client(100, 100), { button: 0, buttons: 1 });
    pointer("pointerup", client(100, 100), { button: 0, buttons: 0 });
    await sleep(100);
    assertEqual((await state()).menu.activations.length, 1, "activations");
  },

  "touch is a touchpad: a tap on an item clicks at the cursor, not there; a swipe moves the cursor, a tap anywhere clicks under it": async () => {
    needsGame();
    // The cursor away from the items.
    await swipeTo([300, 650]);
    let s = await state();
    const [activations, selected] = [s.menu.activations.length, s.menu.selected];
    const ui = (await volya.call("input_state")).ui;
    await tapAt(centreOfRow(4));
    await sleep(300);
    s = await state();
    assertEqual([s.menu.activations.length, s.menu.selected], [activations, selected], "a tap on item 4 clicked it");
    assertEqual((await volya.call("input_state")).ui, ui, "the tap moved the cursor");
    // A swipe onto item 4 selects it (hover); a tap elsewhere activates it.
    await swipeTo(centreOfRow(4));
    await until((s) => s.menu.selected === 4 && !s.menu.moving, "item 4 at rest", 2000);
    await tapAt([150, 150]);
    s = await until((s) => s.menu.activations.length === activations + 1, "an activation");
    assertEqual(s.menu.activations.at(-1).item, "btn_credits", "activated");
  },

  "Enter on New game opens the difficulty page; Escape returns": async () => {
    needsGame();
    await pointAt(0);
    key("Enter");
    let s = await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
    assertEqual(s.menu.items.map((i) => i.caption), ["Новичок", "Сталкер", "Ветеран", "Мастер", "Назад"], "captions");
    assertEqual(s.menu.selected, 0, "selected");
    key("Escape");
    s = await until((s) => s.menu.list === "menu_main", "the main page");
    // Escape on the main page does nothing on the title screen.
    key("Escape");
    await sleep(100);
    assertEqual((await state()).phase, "menu", "phase");
  },

  "forgets the game": async () => {
    if (installer) await volya.call("forget_game");
    await volya.call("forget_user_config");
  },
};
