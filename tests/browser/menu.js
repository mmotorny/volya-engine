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
  // The levels (a New Game's level, docs/behavior/level-loading.md).
  "gamedata.db0",
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

  "a difficulty starts a new game: the level's start view is drawn": async () => {
    needsGame();
    pointer("pointermove", row(0));
    await until((s) => s.menu.selected === 0, "item 0");
    key("Enter");
    await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
    key("Enter");
    // The loading screen (docs/behavior/loading-screen.md): a caption per
    // stage over the background and the progress strip. Whichever loading
    // image is current when the scene is read (a stage's, or after the last
    // stage the overlay's over the level's first 30 frames) is checked.
    const loading = await until((s) => s.phase === "loading" && s.loading, "the loading screen", 10000);
    assertEqual(loading.loading.expected, 14, "expected captions");
    let frame;
    for (const deadline = performance.now() + 10000; performance.now() < deadline; await sleep(30)) {
      frame = await volya.call("menu_scene", { width: 1024, height: 768 });
      if (frame.sprites.some((s) => s.layer.startsWith("loading"))) break;
    }
    assertEqual(frame.sprites.filter((s) => s.layer === "loading strip").length, 40, "strip cells");
    assert(frame.sprites.some((s) => s.layer === "loading background"), "background");
    assert(frame.sprites.some((s) => s.layer === "loading caption"), "caption");
    // After the last stage the image covers the level's first 30 frames;
    // the intro starts under the last two (loading-screen.md section 7).
    const s = await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", 60000);
    assertEqual([s.level.name, s.level.draws], ["l01_escape", 7782], "level");
    assertEqual([s.intro?.items, s.intro?.kind], [1, "video"], "the intro plays");
    // The intro's 120 MB video streams from the file (#53): playing it
    // keeps the wasm heap near its size before (about 137 MB).
    await until((s) => s.intro?.elapsed_ms > 1500, "the intro to play a while", 10000);
    const playing = await volya.call("level_stats");
    assert(playing.wasm_heap_bytes < 180 * 2 ** 20, `wasm heap ${(playing.wasm_heap_bytes / 2 ** 20).toFixed(0)} MiB during the intro`);
    // A key bound to quit skips it (startup.md section 6.5).
    key("Escape");
    const shown = await until((s) => s.phase === "level" && !s.intro, "the intro skipped");
    assertEqual(shown.phase, "level", "still in the level");
    let note = `"${loading.loading.caption}" seen; first caption to first level frame in ${s.load_ms.toFixed(0)} ms; intro skipped, wasm heap ${(playing.wasm_heap_bytes / 2 ** 20).toFixed(0)} MiB while it played`;
    if (gpu.state === "ready") {
      const { rgba } = await volya.call("debug_capture", { width: 256, height: 192 });
      const colours = new Set();
      for (let i = 0; i < rgba.length; i += 4) colours.add((rgba[i] << 16) | (rgba[i + 1] << 8) | rgba[i + 2]);
      assert(colours.size > 200, `${colours.size} colours`);
      note += `, ${colours.size} colours`;
    }
    const stats = await volya.call("level_stats");
    return `${note}; ${JSON.stringify(stats)}`;
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
    // No touch in the level yet: desktop Chrome shows no touch controls.
    assert(document.getElementById("touch-controls").hidden, "touch controls shown without a touch");
    return `ran ${moved.toFixed(2)} m, jumped ${height.toFixed(2)} m`;
  },

  "touch: the move stick walks, a drag looks, Menu returns to the title menu": async () => {
    needsGame();
    const controls = document.getElementById("touch-controls");
    assert(controls, "no touch controls");
    // A touch anywhere shows them (a touch device).
    const tap = { pointerType: "touch", pointerId: 30, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 100), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 100), { ...tap, buttons: 0 });
    for (let i = 0; i < 50 && controls.hidden; i++) await sleep(20);
    assert(!controls.hidden, "the touch controls show");
    const centre = (el) => {
      const r = el.getBoundingClientRect();
      return [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
    };
    const touchOn = (el, type, id, [x, y]) =>
      el.dispatchEvent(
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
    const start = (await state()).level;
    // The stick pushed fully up runs forward (along the view the keyboard
    // test turned to); a second finger drags right to look.
    const move = controls.querySelector(".touch-move");
    const look = controls.querySelector(".touch-look");
    const [mx, my] = centre(move);
    touchOn(move, "pointerdown", 31, [mx, my]);
    touchOn(move, "pointermove", 31, [mx, my - 80]);
    let s = await until((s) => s.level.player.moving && s.level.player.running, "the stick to run");
    await sleep(200);
    touchOn(move, "pointerup", 31, [mx, my - 80]);
    s = await until((s) => !s.level.player.moving, "the player to stop");
    const p = s.level.player;
    const moved = Math.hypot(p.position[0] - start.player.position[0], p.position[2] - start.player.position[2]);
    assert(moved > 0.3, `moved ${moved.toFixed(2)} m: ${JSON.stringify(p)}`);
    const [lx, ly] = centre(look);
    touchOn(look, "pointerdown", 32, [lx, ly]);
    touchOn(look, "pointermove", 32, [lx + 100, ly + 20]);
    touchOn(look, "pointerup", 32, [lx + 100, ly + 20]);
    // 100 px at mouse_sens 0.12: 0.24 rad right, a little down.
    s = await until((s) => s.level.yaw > start.yaw + 0.2, "the drag to turn right");
    assert(s.level.pitch > start.pitch, "the drag looks down");
    // Crouch toggles: the engine reports it back to the button.
    const crouch = controls.querySelector(".touch-button.crouch");
    touchOn(crouch, "pointerdown", 33, centre(crouch));
    touchOn(crouch, "pointerup", 33, centre(crouch));
    await until((s) => s.level.player.crouch, "Crouch to crouch");
    for (let i = 0; i < 50 && crouch.getAttribute("aria-pressed") !== "true"; i++) await sleep(20);
    assertEqual(crouch.getAttribute("aria-pressed"), "true", "Crouch shown on");
    // Jump held down while Menu is pressed: when the controls hide in the
    // menu, nothing stays held.
    const jump = controls.querySelector(".touch-button.jump");
    touchOn(jump, "pointerdown", 35, centre(jump));
    await sleep(100);
    assert((await volya.call("input_state")).held.includes("touch_jump"), "Jump held");
    const menu = controls.querySelector(".touch-button.menu");
    touchOn(menu, "pointerdown", 34, centre(menu));
    touchOn(menu, "pointerup", 34, centre(menu));
    const back = await until((s) => s.phase === "menu", "the menu");
    assertEqual(back.menu.list, "menu_main", "list");
    for (let i = 0; i < 50 && !controls.hidden; i++) await sleep(20);
    assert(controls.hidden, "the touch controls hide in the menu");
    await sleep(100);
    const held = (await volya.call("input_state")).held;
    assert(!held.some((h) => h.startsWith("touch_")), `held after hiding: ${held}`);
    return `moved ${moved.toFixed(2)} m, turned ${(s.level.yaw - start.yaw).toFixed(2)} rad`;
  },

  "New Game again after returning: the level loads anew": async () => {
    needsGame();
    pointer("pointermove", row(0));
    await until((s) => s.menu.selected === 0, "item 0");
    key("Enter");
    await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
    key("Enter");
    const s = await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level again", 60000);
    assertEqual([s.level.name, s.level.draws], ["l01_escape", 7782], "level");
    // The intro again; a tap skips it (our design for touch).
    assert(s.intro, "the intro plays");
    const tap = { pointerType: "touch", pointerId: 21, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 384), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 384), { ...tap, buttons: 0 });
    await until((s) => s.phase === "level" && !s.intro, "the tap to skip the intro");
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
    } finally {
      navigator.getGamepads = getGamepads;
    }
    if (gpu.state === "ready") {
      // The new world reaches the GPU on the next frame drawn.
      let stats;
      for (let i = 0; i < 50 && stats?.world_draws !== 7782; i++) {
        stats = await volya.call("level_stats");
        await sleep(50);
      }
      assertEqual(stats.world_draws, 7782, "world draws on the GPU");
    }
    key("Escape");
    await until((s) => s.phase === "menu", "the menu");
    return `loaded again in ${s.load_ms.toFixed(0)} ms`;
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
