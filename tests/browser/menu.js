// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Startup sequence and title menu (#32): the archives they read are imported
// from the real installer, the game is launched as the app launches it, the
// logo sequence is skipped with synthesized Escape presses, and the menu is
// driven with synthesized keyboard, mouse and touch input. State comes from
// the `menu_state` and `menu_scene` ops; pixels from `debug_capture` (the
// renderer, not the DOM) where WebGPU exists. Without WebGPU (the iOS
// Simulator) everything but the pixels still runs.

import { assert, assertEqual, skip } from "./harness.js";

// What docs/behavior/startup.md and title-menu.md read (found by the native
// game-data test), plus the splash's executable and the default user
// settings.
const ONLY = [
  "gamedata.db5",
  "gamedata.db6",
  "gamedata.db7",
  "gamedata.db8",
  "gamedata.db9",
  "gamedata.dba",
  "gamedata.dbb",
  "gamedata.dbc",
  "gamedata.dbd",
  "bin/XR_3DA.exe",
  "__support/userdocs/user.ltx",
];
const CAPTIONS = ["Новая игра", "Загрузить игру", "Сетевая игра", "Опции", "Титры", "Выйти в Windows"];
let installer;
let gpu;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const state = () => volya.call("menu_state");
const view = () => document.getElementById("view");

function installerBase() {
  const path = location.pathname;
  return `${path.slice(0, path.indexOf("/__test/"))}/__test/installer`;
}

const source = (name) => ({
  url: `${installerBase()}/${encodeURIComponent(name)}`,
  size: installer.find((f) => f.name === name).size,
});

/** Polls the menu state until `test` holds, or fails after `ms`. */
async function until(test, what, ms = 5000) {
  const deadline = performance.now() + ms;
  let s;
  while (performance.now() < deadline) {
    s = await state();
    if (test(s)) return s;
    await sleep(30);
  }
  throw new Error(`timed out waiting for ${what}; state ${JSON.stringify(s).slice(0, 400)}`);
}

function key(code) {
  for (const type of ["keydown", "keyup"]) {
    document.body.dispatchEvent(new KeyboardEvent(type, { code, bubbles: true, cancelable: true }));
  }
}

/** Page position of UI point (x, y) (1024 × 768 stretched to the canvas). */
function client(x, y) {
  const [cx, cy] = [(x * view().clientWidth) / 1024, (y * view().clientHeight) / 768];
  const rect = view().getBoundingClientRect();
  // Turned 90° clockwise on upright phones (style.css).
  if (rect.width < rect.height !== view().clientWidth < view().clientHeight) return [rect.right - cy, rect.top + cx];
  return [rect.left + cx, rect.top + cy];
}

function pointer(type, [x, y], init = {}) {
  document.body.dispatchEvent(
    new PointerEvent(type, {
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
    }),
  );
}

/** The centre of item row `i` (title-menu.md section 4.2). */
const row = (i) => client(782, 419 + 30 * i + 15);

function needsGame() {
  if (!installer) skip("no installer");
}

/** The axis-aligned bounds of a scene layer. */
function bounds(scene, layer) {
  const sprites = scene.sprites.filter((s) => s.layer === layer);
  assert(sprites.length > 0, `no ${layer} in the scene`);
  const xs = sprites.flatMap((s) => s.corners.map((c) => c[0]));
  const ys = sprites.flatMap((s) => s.corners.map((c) => c[1]));
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

export const tests = {
  "launch without an imported game fails and keeps the page": async () => {
    gpu = (await volya.status()).gpu;
    await volya.call("forget_game");
    const error = await volya.call("launch").then(() => null, (e) => e.message);
    assert(error?.includes("not imported"), `expected a clear error, got ${error}`);
    assertEqual((await volya.call("input_state")).capture, false, "capture");
    assertEqual(document.querySelector("main").hidden, false, "page hidden");
  },

  "imports what the startup and the menu read": async () => {
    const response = await fetch(`${installerBase()}/index.json`);
    if (response.status === 404) skip("VOLYA_GOG_INSTALLER is unset");
    installer = await response.json();
    const exe = installer.find((f) => f.name.endsWith(".exe")).name;
    const bin = installer.find((f) => f.name.endsWith("-1.bin")).name;
    const started = performance.now();
    const result = await volya.importGame({ exe: source(exe), bin: source(bin), only: ONLY });
    assertEqual(result.status.written, ONLY.length, "files written");
    return `${(result.status.bytes / 2 ** 30).toFixed(2)} GiB in ${((performance.now() - started) / 1000).toFixed(1)} s`;
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
    assertEqual(s.menu.list, "menu_main", "list");
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
    const xga = await volya.call("menu_scene", { width: 1024, height: 768 });
    assertEqual(bounds(xga, "background"), [-0.5, -0.5, 1023.5, 767.5], "background");
    assertEqual(bounds(xga, "back_movie"), [147.5, -0.5, 1023.5, 399.5], "back_movie");
    assertEqual(bounds(xga, "fire_movie"), [524.5, 458.5, 652.5, 714.5], "fire_movie");
    assertEqual(bounds(xga, "bar"), [598.5, 396.5, 990.5, 469.5], "bar");
    // docs/formats/text-and-fonts.md section 5.
    assertEqual(bounds(xga, "caption 0"), [740, 423, 825, 445], "caption 0");
    assertEqual(bounds(xga, "caption 5"), [713, 573, 851, 595], "caption 5");
    assertEqual(bounds(xga, "version"), [15, 740, 94, 759], "version");
    const wide = await volya.call("menu_scene", { width: 1280, height: 720 });
    assertEqual(bounds(wide, "caption 0"), [935, 395, 1020, 417], "wide caption 0");
    assertEqual(bounds(wide, "version"), [18, 693, 97, 712], "wide version");
  },

  "renders the menu": async () => {
    needsGame();
    if (gpu.state !== "ready") {
      assertEqual(gpu.state, "unavailable", "graphics");
      const line = document.getElementById("status").textContent;
      assert(line.includes("graphics unavailable"), `status line: ${line}`);
      return `no WebGPU (${gpu.message}): the menu runs without graphics`;
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
    // A click outside the glass activates nothing.
    pointer("pointerdown", client(100, 100), { button: 0, buttons: 1 });
    pointer("pointerup", client(100, 100), { button: 0, buttons: 0 });
    await sleep(100);
    assertEqual((await state()).menu.activations.length, 1, "activations");
  },

  "touch: a tap selects a row, a second tap activates it": async () => {
    needsGame();
    const touch = { pointerType: "touch", button: 0, buttons: 1 };
    const tap = (at) => {
      pointer("pointerdown", at, touch);
      pointer("pointerup", at, { ...touch, buttons: 0 });
    };
    await sleep(300);
    tap(row(4));
    let s = await until((s) => s.menu.selected === 4 && !s.menu.moving, "item 4 at rest", 2000);
    assertEqual(s.menu.activations.length, 1, "the first tap activated");
    tap(row(4));
    s = await until((s) => s.menu.activations.length === 2, "an activation");
    assertEqual(s.menu.activations.at(-1).item, "btn_credits", "activated");
  },

  "Enter on New game opens the difficulty page; Escape returns": async () => {
    needsGame();
    pointer("pointermove", row(0));
    await until((s) => s.menu.selected === 0, "item 0");
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

  "Q quits to the app's own interface": async () => {
    needsGame();
    key("KeyQ");
    await until((s) => s.phase === "none", "the game to end");
    assertEqual((await volya.call("input_state")).capture, false, "capture");
    await sleep(50);
    assertEqual(document.querySelector("main").hidden, false, "page interface hidden");
  },

  "forgets the game": async () => {
    if (installer) await volya.call("forget_game");
  },
};
