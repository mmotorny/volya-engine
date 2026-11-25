// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Startup sequence and title menu (#32): the archives they read are imported
// from the real installer, the game is launched as the app launches it, the
// logo sequence is skipped with synthesized Escape presses, and the menu is
// driven with synthesized keyboard, mouse and touch input. State comes from
// the `menu_state` and `menu_scene` ops; pixels from `debug_capture` (the
// renderer, not the DOM) where graphics exist: WebGPU, or WebGL 2 with the
// textures transcoded to ETC2 (the iOS Simulator, #113). Without graphics
// everything but the pixels still runs.

import { assert, assertEqual, skip } from "./harness.js";

// What docs/behavior/startup.md and title-menu.md read (found by the native
// game-data test), plus the splash's executable and the default user
// settings.
const ONLY = [
  // The levels (a New Game's level, docs/behavior/level-loading.md).
  "gamedata.db0",
  // The level's objects' models (docs/behavior/level-objects.md).
  "gamedata.db4",
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
// The wasm heap never shrinks: its size after a New Game is the page's for
// good. Measured at 98 MiB in headless Chrome after one and after two New
// Games (#79; 193 and 199 MiB before), and at 107–110 MiB after the first in
// the iOS Simulator, drawing through WebGL 2 (#184: the level's geometry
// uploaded a few pieces per task turn, since wgpu's GL backend keeps what
// is written in the heap until WebGL reports it done); a regression past
// this is reported as an "OVER BUDGET" note, not failed: budgets are
// measured and tracked in issues, never a release gate (AGENTS.md). The
// audio worker's instance (#354) counts too (`audio_heap_bytes`, which
// `heap` adds).
const WASM_HEAP_CAP = 110 * 2 ** 20;
/** Both wasm instances' heaps in `level_stats`: the engine's and the audio worker's. */
const heap = (stats) => stats.wasm_heap_bytes + stats.audio_heap_bytes;
// The wasm heap at the end of the last frame under each caption, MiB, from
// a `heap_log`; in test builds with the most it held live meanwhile
// (`heap/live`), so fragmentation shows as the difference (#357).
const byCaption = (log) => {
  const stages = [];
  for (const e of log) {
    const name = e.caption ?? e.phase;
    const heap = Math.round(e.wasm_heap_bytes / 2 ** 20);
    const live = e.peak_bytes === undefined ? undefined : Math.round(e.peak_bytes / 2 ** 20);
    const last = stages.at(-1);
    if (last?.name === name) {
      last.heap = heap;
      last.live = Math.max(last.live ?? 0, live ?? 0);
    } else stages.push({ name, heap, live });
  }
  return stages.map((s) => `${s.name} ${s.heap}${s.live === undefined ? "" : `/${s.live}`}`).join(", ");
};
const overBudget = (bytes, what) => (bytes > WASM_HEAP_CAP ? `OVER BUDGET: ${what} ${mib(bytes)} > ${mib(WASM_HEAP_CAP)}; ` : "");
// Touch devices: no full screen offer, touch controls from the start.
const coarse = () => matchMedia("(pointer: coarse)").matches;
// How long a level may take to load: transcoding its textures (WebGL 2
// without BC, #113) takes about 7 s in the iOS Simulator, on every load
// (#183).
const loadMs = () => (gpu?.textures === "etc2" ? 120000 : 60000);
const mib = (bytes) => `${(bytes / 2 ** 20).toFixed(0)} MiB`;
const CAPTIONS = ["Новая игра", "Загрузить игру", "Сетевая игра", "Опции", "Титры", "Выйти в Windows"];
let installer;
let gpu;
// The size the game draws at (#49), shown scaled to fit the canvas.
let render;
// The title menu's frames drawn through WebGPU are compared with the same
// scenes drawn through WebGL 2 (#121, #425), at the installer's video mode
// and a phone's size. They are drawn where no wasm heap is measured after
// (the readback grows the heap, which never shrinks).
const MENU_FRAME_SIZES = [
  [1024, 768],
  [2868, 1320],
];
// How far WebGL 2's frame may be from WebGPU's: measured identical in
// headless Chrome on the Mac (both on BC textures as stored, the same UI
// program and sampler), so only rounding is allowed.
const MENU_FRAME_MAX_DIFFERENCE = 2;
const MENU_FRAME_MAX_SHARE = 0.001;
/**
 * The title menu as it is now at each size, its videos at 2 s from their
 * start whatever the clock, drawn through this backend (the
 * `debug_capture_scene` op).
 */
async function drawMenuFrames(scenes) {
  scenes ??= await Promise.all(
    MENU_FRAME_SIZES.map(async ([width, height]) => {
      const scene = await volya.call("menu_scene", { width, height });
      for (const s of scene.sprites) if (s.texture.kind === "video") s.texture.start_ms = 0;
      return scene;
    }),
  );
  const frames = [];
  for (const scene of scenes) frames.push({ scene, rgba: (await volya.call("debug_capture_scene", { scene, clock_ms: 2000 })).rgba });
  return frames;
}

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

/** Reads the size the game draws at. */
async function readRender() {
  render = (await volya.status()).render;
}

/**
 * Page position of UI point (x, y): 1024 × 768 stretched to the drawn
 * picture, which fits the canvas with its aspect kept (#49).
 */
function client(x, y) {
  const [w, h] = [view().clientWidth, view().clientHeight];
  const [rw, rh] = render ?? [w, h];
  const scale = Math.min(w / rw, h / rh);
  const [fw, fh] = [rw * scale, rh * scale];
  const [cx, cy] = [(w - fw) / 2 + (x * fw) / 1024, (h - fh) / 2 + (y * fh) / 768];
  const rect = view().getBoundingClientRect();
  // Turned 90° clockwise on upright phones (style.css).
  if (rect.width < rect.height !== view().clientWidth < view().clientHeight) return [rect.right - cy, rect.top + cx];
  return [rect.left + cx, rect.top + cy];
}

/** Whether `box` is turned 90° clockwise on the screen (upright phones, style.css). */
const turned = (box) => {
  const r = box.getBoundingClientRect();
  return r.width < r.height !== box.clientWidth < box.clientHeight;
};

/** Page position of point `[x, y]` in `box`'s own layout, as the page's `local` reads it back. */
function toClient(box, [x, y]) {
  const b = box.getBoundingClientRect();
  return turned(box) ? [b.right - y, b.top + x] : [b.left + x, b.top + y];
}

/** `el`'s rectangle in `box`'s own layout (not turned). */
function localRect(el, box) {
  const [b, r] = [box.getBoundingClientRect(), el.getBoundingClientRect()];
  const [left, top, right, bottom] = turned(box)
    ? [r.top - b.top, b.right - r.right, r.bottom - b.top, b.right - r.left]
    : [r.left - b.left, r.top - b.top, r.right - b.left, r.bottom - b.top];
  return { left, top, right, bottom };
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

/** The centre of item row `i` (title-menu.md section 4.2), UI units and page. */
const centreOfRow = (i) => [782, 419 + 30 * i + 15];
const row = (i) => client(...centreOfRow(i));

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

/** The options screen's state (it must show). */
async function options() {
  const o = (await state()).options;
  assert(o, "the options screen does not show");
  return o;
}

/** Polls the options screen's state until `test` holds. */
async function untilOptions(test, what, ms = 5000) {
  return (await until((s) => s.options && test(s.options), what, ms)).options;
}

const control = (o, name) => o.controls.find((c) => c.name === name) ?? assert(false, `no ${name}`);
const button = (o, name) => o.buttons.find((b) => b.name === name) ?? assert(false, `no ${name}`);
/** The centre of a reported rectangle, in UI units. */
const centreOf = ({ rect: [x, y, w, h] }) => [x + w / 2, y + h / 2];

/** A mouse click at UI point `at`. */
function clickAt(at) {
  const p = client(...at);
  pointer("pointermove", p);
  pointer("pointerdown", p, { button: 0, buttons: 1 });
  pointer("pointerup", p, { button: 0, buttons: 0 });
}

// Touch where the game shows its cursor is a touchpad (touchpad.js, #108).
let touchId = 100;
/** Slow enough for the touchpad's one-to-one speed, CSS pixels per event
 * 25 ms apart (120 px/s; the curve rises from 200). */
const SLOW_STEP = 3;
/** CSS pixels per UI unit of the drawn picture. */
const cssPerUi = () => {
  const [a, b] = [client(0, 0), client(100, 0)];
  return Math.hypot(b[0] - a[0], b[1] - a[1]) / 100;
};

/** Fingers at UI points `starts` moving slowly by `travel` (UI units), then lifted. */
async function swipe(starts, [tx, ty], { lift = true, ids } = {}) {
  ids ??= starts.map(() => ++touchId);
  const t = (id) => ({ pointerType: "touch", pointerId: id, isPrimary: id === ids[0], button: -1 });
  const at = (i, f) => client(starts[i][0] + tx * f, starts[i][1] + ty * f);
  starts.forEach((_, i) => pointer("pointerdown", at(i, 0), { ...t(ids[i]), button: 0, buttons: 1 }));
  const steps = Math.max(1, Math.ceil((Math.hypot(tx, ty) * cssPerUi()) / SLOW_STEP));
  for (let n = 1; n <= steps; n++) {
    await sleep(25);
    starts.forEach((_, i) => pointer("pointermove", at(i, n / steps), { ...t(ids[i]), buttons: 1 }));
  }
  if (lift) starts.forEach((_, i) => pointer("pointerup", at(i, 1), { ...t(ids[i]), button: 0, buttons: 0 }));
  return ids;
}

/** Moves the cursor to UI point `target` with slow touchpad swipes. */
async function swipeTo(target) {
  // Past the double-tap time of an earlier tap (a touch then would drag).
  await sleep(300);
  for (let attempt = 0; attempt < 4; attempt++) {
    const { ui, sensitivity } = await volya.call("input_state");
    const [dx, dy] = [target[0] - ui[0], target[1] - ui[1]];
    if (Math.hypot(dx, dy) < 1.5) return;
    // One to one when slow: the cursor moves s/2 times the finger's travel
    // in UI units (title-menu.md section 7, over 2 normalised units).
    const travel = [(2 * dx) / sensitivity, (2 * dy) / sensitivity];
    await swipe([[512 - travel[0] / 2, 384 - travel[1] / 2]], travel);
    await sleep(50);
  }
  const { ui } = await volya.call("input_state");
  throw new Error(`the cursor at ${ui}, not ${target}`);
}

/** A touchpad tap at UI point `at` (anywhere: it clicks at the cursor). */
async function tapAt(at = [512, 384]) {
  await sleep(300);
  const t = { pointerType: "touch", pointerId: ++touchId, isPrimary: true, button: 0 };
  pointer("pointerdown", client(...at), { ...t, buttons: 1 });
  pointer("pointerup", client(...at), { ...t, buttons: 0 });
}

/** The in-game menu's item list (save-load-screens.md section 2). */
const inGame = (s) => (s.phase === "level" ? s.menu?.menu : undefined);

/** Selects in-game menu item `name` with the arrow keys from where it is. */
async function selectInGame(name) {
  const m = inGame(await state());
  const target = m.items.findIndex((i) => i.name === name);
  assert(target >= 0, `no ${name}`);
  for (let i = m.selected; i < target; i++) key("ArrowDown");
  for (let i = m.selected; i > target; i--) key("ArrowUp");
  await until((s) => inGame(s)?.selected === target, `${name} selected`);
}

/** Answers the box over the in-game menu by clicking `answer`. */
async function answerBox(answer) {
  const box = (await until((s) => s.menu?.message, "the box")).menu.message;
  clickAt(centreOf(box.buttons.find((b) => b.answer === answer)));
}

/** From the level: the in-game menu, "Завершить игру", "Да": the title menu. */
async function quitToTitle() {
  if (!inGame(await state())) key("Escape");
  await until((s) => inGame(s), "the in-game menu");
  await selectInGame("btn_quit_to_mm");
  key("Enter");
  await answerBox("yes");
  return until((s) => s.phase === "menu", "the title menu");
}

/** From the title menu's main page: New game, the first difficulty. */
async function newGame() {
  pointer("pointermove", row(0));
  await until((s) => s.menu.selected === 0, "item 0");
  key("Enter");
  await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
  key("Enter");
}

/**
 * This session's `engine.end` events in the diagnostic log as stored now
 * (not those waiting for the period's end).
 */
async function storedEnds() {
  const { text } = await volya.call("diagnostic_log", { stored_only: true });
  const events = JSON.parse(text).traceEvents;
  const pid = events.findLast((e) => e.name === "session").pid;
  return events.filter((e) => e.pid === pid && e.name === "engine.end");
}

/**
 * Simulates the page being hidden or shown: the visibility the page reads,
 * then the event (a real hide needs the browser's own controls).
 */
function setVisibility(state) {
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  Object.defineProperty(document, "hidden", { value: state === "hidden", configurable: true });
  document.dispatchEvent(new Event("visibilitychange"));
  if (state === "visible") {
    delete document.visibilityState;
    delete document.hidden;
  }
}

/** Opens the options screen from the title menu's main page. */
async function openOptions() {
  pointer("pointermove", row(3));
  await until((s) => s.menu?.selected === 3 && !s.menu.moving, "item 3 at rest", 3000);
  pointer("pointerdown", row(3), { button: 0, buttons: 1 });
  pointer("pointerup", row(3), { button: 0, buttons: 0 });
  return untilOptions(() => true, "the options screen");
}

export const tests = {
  "launch without an imported game fails and keeps the page": async () => {
    gpu = (await volya.status()).gpu;
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

  "the diagnostic log is turned on for the games that follow": async () => {
    needsGame();
    const status = await volya.diagnostics.setEnabled(true, volya.call);
    assert(status.on, "not on");
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
    await readRender();
    // The installer's settings: 1024 × 768 (options-screens.md 12.4).
    assertEqual(render, [1024, 768], "drawn at vid_mode");
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
    await volya.call("heap_log", { on: true });
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
    // Transcoding the level's textures (WebGL 2 without BC, #113) takes
    // longer, on every load.
    const s = await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", loadMs());
    const stages = byCaption(await volya.call("heap_log", { on: false }));
    assertEqual([s.level.name, s.level.draws], ["l01_escape", 7782], "level");
    // The level's objects (docs/behavior/level-objects.md section 7.1).
    assertEqual([s.level.objects.online, s.level.objects.with_model], [671, 352], "objects");
    // Its lamps on, with their seven records; chunk 6's 40 records and the
    // sun's two besides; 22 glows of its own and the lamps' 5
    // (hanging-lamps.md 9.3, dynamic-lights.md 3.3, glows.md 7); the
    // player's torch's two records and glow, off (torch.md 12); the record
    // of the one artefact online, esc_af_medusa (artefact-idle-effects.md).
    const lights = s.level.lights;
    assertEqual([lights.lamps, lights.lamps_on, lights.records, lights.glows], [5, 5, 52, 28], "lights");
    assert(s.level.torch?.attached && !s.level.torch.on, "the torch attached and off");
    assertEqual([s.intro?.items, s.intro?.kind], [1, "video"], "the intro plays");
    // The intro's 120 MB video streams from the file (#53): playing it
    // keeps the wasm heap near its size before.
    const before = await volya.call("level_stats");
    await until((s) => s.intro?.elapsed_ms > 1500, "the intro to play a while", 10000);
    const playing = await volya.call("level_stats");
    const grown = (playing.wasm_heap_bytes - before.wasm_heap_bytes) / 2 ** 20;
    assert(grown < 40, `wasm heap grew ${grown.toFixed(0)} MiB during the intro`);
    // The whole level's heap, with the level's geometry streamed from its
    // file to the GPU (#79): under its cap.
    // A key bound to quit skips it (startup.md section 6.5), before the
    // heap is judged, so a failure leaves the game where later tests expect.
    key("Escape");
    const shown = await until((s) => s.phase === "level" && !s.intro, "the intro skipped");
    assertEqual(shown.phase, "level", "still in the level");
    const heapNote = overBudget(heap(playing), "wasm heaps during the intro");
    let note = `${heapNote}wasm heap by caption (MiB, heap/live): ${stages}; "${loading.loading.caption}" seen; first caption to first level frame in ${s.load_ms.toFixed(0)} ms; intro skipped, wasm heap ${mib(before.wasm_heap_bytes)} before it, ${mib(playing.wasm_heap_bytes)} while it played, the audio worker's ${mib(playing.audio_heap_bytes)}; at the start ${lights.passes} light passes, ${lights.sprites} glows drawn`;
    if (gpu.state === "ready") {
      const { rgba } = await volya.call("debug_capture", { width: 256, height: 192 });
      const colours = new Set();
      for (let i = 0; i < rgba.length; i += 4) colours.add((rgba[i] << 16) | (rgba[i + 1] << 8) | rgba[i + 2]);
      assert(colours.size > 200, `${colours.size} colours`);
      note += `, ${colours.size} colours`;
    }
    const stats = await volya.call("level_stats");
    if (gpu.textures === "etc2") assertEqual(stats.textures.textures > 300, true, `textures prepared: ${JSON.stringify(stats.textures)}`);
    return `${gpu.backend}, textures ${gpu.textures}; ${note}; ${JSON.stringify(stats)}`;
  },

  "the level's placed effect runs on the world clock; its update cost in wasm": async () => {
    needsGame();
    await until((s) => s.phase === "level" && !s.intro, "the level");
    // docs/behavior/level-placed-effects.md 8: l01_escape places tests\muhi.
    let fx = await volya.call("particles_state");
    assertEqual(fx.records.map((r) => [r.name, r.group, r.playing]), [["tests\\muhi", false, true]], "records");
    const clock = fx.clock;
    await sleep(500);
    fx = await volya.call("particles_state");
    assert(fx.clock > clock, "the world clock runs in the level");
    assert(fx.particles > 0 && fx.particles <= 7, `${fx.particles} particles`);
    const bench = await volya.call("debug_particles_bench", { frames: 20000 });
    return `${fx.particles} particles, period ${fx.records[0].period} ms; a frame's update in wasm: mean ${bench.mean_us.toFixed(2)} µs over ${bench.frames} frames, the slowest under ${bench.max_us.toFixed(0)} µs (the timer's resolution)`;
  },

  "the zones' idle effects play, the campfires among them": async () => {
    needsGame();
    // docs/behavior/zone-idle-effects.md 8: 178 zone records online, 160
    // with an idle effect; only the galantines switch off.
    const { zones } = await volya.call("zones_state");
    assertEqual(zones.length, 178, "zones");
    const withEffect = zones.filter((z) => z.effect);
    assertEqual(withEffect.length, 160, "zones with an idle effect");
    const off = withEffect.filter((z) => !z.playing);
    assert(off.every((z) => z.section === "zone_witches_galantine" && z.state === "disabled"), `off: ${JSON.stringify(off)}`);
    const fires = zones.filter((z) => z.effect === "explosions\\campfire_05");
    assertEqual(fires.map((z) => z.playing && z.sound_on), Array(11).fill(true), "campfires");
    const stats = await volya.call("level_stats");
    return `${withEffect.length - off.length} idle effects playing, ${off.length} galantines off; frame CPU ${stats.frame_cpu_ms.toFixed(2)} ms average, ${stats.frame_cpu_max_ms.toFixed(2)} ms worst; wasm heap ${mib(stats.wasm_heap_bytes)}`;
  },

  "the level's frame and GPU times at a few video modes": async () => {
    needsGame();
    await until((s) => s.phase === "level" && !s.intro, "the level");
    // #355: the phone's own size (landscape), the original's default target
    // (1024×768 with r__supersample 2: √2 per axis, made even), common
    // sizes. The diagnostic log is on, so the GPU is timed.
    const sizes = [[2868, 1320], [1448, 1086], [1280, 720], [1024, 768]];
    const before = await volya.call("level_stats");
    const rows = [];
    try {
      for (const [width, height] of sizes) {
        await volya.call("debug_video_mode", { width, height });
        // A second to settle, then the latest 60 frames' averages.
        await sleep(1000 + 60 * 34);
        const s = await volya.call("level_stats");
        const ms = (v) => (v == null ? "–" : v.toFixed(1));
        const passes = Object.entries(s.gpu_pass_ms ?? {}).map(([name, t]) => `${name} ${ms(t)}`).join(", ");
        rows.push(`${width}×${height}: interval ${ms(s.frame_interval_ms)}, CPU ${ms(s.frame_cpu_ms)}, GPU ${ms(s.gpu_time_ms)} (${passes}), done ${ms(s.gpu_done_ms)}`);
        assert(s.gpu_done_ms > 0, `no GPU done time at ${width}×${height}`);
      }
      // At 1024×768: r__supersample 1 instead of 2, and the grass left
      // undrawn (#415).
      for (const [what, change] of [["r__supersample 1", { supersample: 1 }], ["no grass", { supersample: 2, details: false }]]) {
        await volya.call("debug_render", change);
        await sleep(1000 + 60 * 34);
        const s = await volya.call("level_stats");
        const ms = (v) => (v == null ? "–" : v.toFixed(1));
        rows.push(`1024×768, ${what}: interval ${ms(s.frame_interval_ms)}, CPU ${ms(s.frame_cpu_ms)}, GPU ${ms(s.gpu_time_ms)}, done ${ms(s.gpu_done_ms)}`);
      }
    } finally {
      await volya.call("debug_render", { supersample: 2, details: true });
      await volya.call("debug_video_mode", { width: 1024, height: 768 });
    }
    // #387: what a frame asks of the GPU, in all and by part.
    const c = before.frame_commands;
    const parts = Object.entries(c?.parts ?? {})
      .filter(([, p]) => p.draws > 0)
      .map(([name, p]) => `${name} ${p.draws}`)
      .join(", ");
    const commands = c ? `; ${c.total.draws} draws (${parts}), ${c.total.pipelines} pipelines, ${c.total.bind_groups} bind groups, ${c.total.buffers} buffers set` : "";
    return `ms per frame: ${rows.join("; ")} (before: GPU ${before.gpu_time_ms?.toFixed(1)}, done ${before.gpu_done_ms?.toFixed(1)})${commands}`;
  },

  "transcoding to ETC2 costs: the level's textures and a few large ones": async () => {
    needsGame();
    await until((s) => s.phase === "level" && !s.intro, "the level");
    // Only where the level's textures are transcoded: elsewhere (Chrome
    // uploads BC) the op would block the engine worker for half a minute
    // in the middle of the level's tests.
    if (gpu?.textures !== "etc2") skip(`textures ${gpu?.textures ?? "none"}: nothing is transcoded`);
    const large = [
      "gamedata\\textures\\detail\\detail_grnd_grass.dds",
      "gamedata\\levels\\l01_escape\\terrain\\terrain_escape.dds",
      "gamedata\\levels\\l01_escape\\lmap#1_1.dds",
      "gamedata\\textures\\sky\\sky_2_cube.dds",
      "gamedata\\textures\\crete\\crete_walls16.dds",
    ];
    const each = await volya.call("debug_transcode", { paths: large });
    const level = await volya.call("debug_transcode", {});
    assert(level.textures > 300, `${level.textures} textures`);
    const per = each.per.map((p) => `${p.path.split("\\").pop()} ${p.size.join("×")} ${p.format} ${p.ms.toFixed(0)} ms (${p.ms_per_mpx.toFixed(0)} ms/Mpx)`);
    return `level: ${level.textures} textures, ${level.mpx.toFixed(1)} Mpx in ${(level.ms / 1000).toFixed(1)} s (${level.ms_per_mpx.toFixed(0)} ms/Mpx), wasm heap ${mib(level.heap_before)} → at most ${mib(level.heap_max)}; ${per.join("; ")}`;
  },

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
    const press = { pointerType: "touch", pointerId: ++touchId, isPrimary: true, button: 0 };
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
    pointer("pointerdown", client(...finger), { ...press, pointerId: ++touchId, buttons: 1 });
    pointer("pointerup", client(...finger), { ...press, pointerId: touchId, buttons: 0 });
    await sleep(50);
    const id = ++touchId;
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

  "New Game again after returning: the level loads anew; Escape under the loading image opens the menu once the controls are live": async () => {
    needsGame();
    pointer("pointermove", row(0));
    await until((s) => s.menu.selected === 0, "item 0");
    key("Enter");
    await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
    await volya.call("heap_log", { on: true });
    key("Enter");
    // Escape while the loading image covers the level's first frames,
    // before the intro starts under its last two: kept (#96).
    await until((s) => s.phase === "level" && s.overlay_frames > 6, "the loading image over the level", loadMs());
    key("Escape");
    const asked = await until((s) => s.menu_asked || s.intro || s.menu, "the Escape taken");
    const seen = { menu_asked: asked.menu_asked, overlay: asked.overlay_frames, intro: Boolean(asked.intro) };
    assert(asked.menu_asked && !asked.menu, `the menu asked for, not open: ${JSON.stringify(seen)}`);
    const s = await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level again", loadMs());
    const stages = byCaption(await volya.call("heap_log", { on: false }));
    assertEqual([s.level.name, s.level.draws], ["l01_escape", 7782], "level");
    // The intro again; a tap skips it (our design for touch). The controls
    // go live then, and the menu opens at once.
    assert(s.intro && !s.menu, "the intro plays, no menu");
    const tap = { pointerType: "touch", pointerId: 21, isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 384), { ...tap, buttons: 1 });
    pointer("pointerup", client(512, 384), { ...tap, buttons: 0 });
    const opened = await until((s) => s.phase === "level" && !s.intro, "the tap to skip the intro");
    assert(inGame(opened), "the in-game menu opened as the controls went live");
    key("Escape");
    await until((s) => s.phase === "level" && !s.menu, "back in the game");
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
    if (gpu.state === "ready") {
      // The new world reaches the GPU on the next frame drawn.
      let stats;
      for (let i = 0; i < 50 && stats?.world_draws !== 7782; i++) {
        stats = await volya.call("level_stats");
        await sleep(50);
      }
      assertEqual(stats.world_draws, 7782, "world draws on the GPU");
    }
    // A second load reuses the heap the first one grew.
    const stats = await volya.call("level_stats");
    const heapNote = overBudget(heap(stats), "wasm heaps after the second New Game");
    return `${heapNote}loaded again in ${s.load_ms.toFixed(0)} ms; wasm heap ${mib(stats.wasm_heap_bytes)}, the audio worker's ${mib(stats.audio_heap_bytes)}; by caption (MiB, heap/live): ${stages}`;
  },

  "hidden: the menu opens, the clock, the frame loop and audio stop; shown: the game takes on from there": async () => {
    needsGame();
    let s = await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    const audioBefore = volya.audioState();
    setVisibility("hidden");
    try {
      s = await until((s) => inGame(s), "the in-game menu");
      const { clock, level } = s;
      const frames = (await volya.status()).frames;
      const effects = (await volya.call("particles_state")).clock;
      await sleep(500);
      s = await state();
      assertEqual([s.clock, s.level.game_time], [clock, level.game_time], "the engine and game clocks while hidden");
      assertEqual((await volya.call("particles_state")).clock, effects, "the effects' clock while hidden");
      const idle = (await volya.status()).frames;
      assert(idle - frames <= 1, `${idle - frames} frames ran while hidden`);
      // Suspended if it ran; else as the browser holds it (below).
      assertEqual(volya.audioState(), audioBefore === "running" ? "suspended" : audioBefore, "audio while hidden");
      assertEqual((await storedEnds()).at(-1)?.args.reason, "page_hidden", "the end in the diagnostic log");
    } finally {
      setVisibility("visible");
    }
    const hiddenClock = s.clock;
    const frames = (await volya.status()).frames;
    for (let i = 0; i < 50 && (await volya.status()).frames < frames + 3; i++) await sleep(20);
    const back = await state();
    assert(inGame(back), "the menu stays open on return");
    // The first frame back has a zero delta; a few frames ran since.
    const ran = back.clock - hiddenClock;
    assert(ran < 300, `the engine clock ran ${ran} ms over the return`);
    assertEqual(back.level.game_time, s.level.game_time, "the game clock, paused under the menu");
    // Chrome runs with autoplay allowed (docs/testing.md), so audio was
    // running and resumes. Where the AudioContext needs a gesture it never
    // ran: WebKit holds it "interrupted" from its creation (measured in the
    // iOS Simulator, where synthesized input is no gesture), and there is
    // nothing to suspend or resume.
    assert(["running", "interrupted"].includes(audioBefore), `audio before hiding: ${audioBefore}`);
    if (audioBefore === "running") await until(() => volya.audioState() === "running", "audio to resume");
    // The player resumes deliberately.
    await sleep(100);
    key("Escape");
    await until((s) => s.phase === "level" && !s.menu, "back in the game");
    return `audio ${audioBefore} before hiding; engine clock +${ran} ms over the return`;
  },

  "the save screen's name: typed through the device keyboard's input on touch, and the on-screen keyboard with a gamepad": async () => {
    needsGame();
    key("Escape");
    await until((s) => inGame(s), "the in-game menu");
    await selectInGame("btn_save");
    key("Enter");
    const save = async () => (await until((s) => s.menu?.save_screen, "the save screen")).menu.save_screen;
    const [fx, fy, fw, fh] = (await save()).field;
    // Touch (seen earlier in this page): the text entry is where the field
    // is, but takes no taps itself (#108).
    const entry = document.querySelector("#text-entry input");
    for (let i = 0; i < 50 && entry.parentElement.hidden; i++) await sleep(20);
    assert(!entry.parentElement.hidden, "the text entry shows");
    const centre = [fx + fw / 2, fy + fh / 2];
    assert(document.elementFromPoint(...client(...centre)) !== entry, "the text entry takes taps");
    // A tap on the field with the cursor elsewhere focuses nothing.
    await swipeTo([100, 100]);
    await tapAt(centre);
    await sleep(200);
    assert(document.activeElement !== entry && !(await save()).focused, "a tap on the field focused it");
    // The cursor over the field: a tap anywhere clicks it, and the page
    // focuses its input in that tap (the device's keyboard rises only so).
    await swipeTo(centre);
    await sleep(200);
    await tapAt([fx + fw / 2, fy - 100]);
    assertEqual(document.activeElement, entry, "the input focused in the tap");
    await until((s) => s.menu?.save_screen?.focused, "the field focused by the tap");
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

  "a level sound file missing at a New Game fails the load; the diagnostic log stores why at once": async () => {
    needsGame();
    await volya.call("debug_hide_file", { suffix: "sounds/ambient/rain.ogg" });
    try {
      await newGame();
      await until((s) => s.phase !== "menu", "the New Game to start");
      const s = await until((s) => s.phase === "menu" && s.error, "the title menu saying why", loadMs());
      assert(s.error.includes("rain.ogg"), `error: ${s.error}`);
    } finally {
      await volya.call("debug_hide_file", {});
    }
    const end = (await storedEnds()).at(-1);
    assertEqual(end?.args.reason, "load_failed", "reason");
    assert(end.args.message.includes("rain.ogg"), `message: ${end.args.message}`);
    return end.args.message;
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
    // A tap in the black bars beside a 4:3 picture clicks at the cursor
    // (the touchpad, #108), which stays where it is.
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
    const touch = { pointerType: "touch", pointerId: ++touchId, isPrimary: true, button: 0 };
    const bar = [rect.left + 2, rect.top + rect.height / 2];
    pointer("pointerdown", bar, { ...touch, buttons: 1 });
    pointer("pointerup", bar, { ...touch, buttons: 0 });
    await sleep(100);
    const after = await volya.call("input_state");
    const actions = after.recent.filter((a) => a.seq > before.count).map((a) => `${a.action} ${a.control}`);
    assertEqual(actions, ["press mouse_left", "release mouse_left"], "a click");
    assertEqual(after.ui, before.ui, "the cursor after a tap in the bar");
    return "a tap in the bar clicks at the cursor";
  },

  "touchpad: a double-tap-hold drag moves a slider; a two-finger swipe scrolls the key table": async () => {
    needsGame();
    await swipeTo(centreOf(button(await options(), "tab 1")));
    await tapAt();
    let o = await untilOptions((o) => o.page === "sound", "the sound tab");
    // On the music volume's bar: a tap, then a touch at once, held and
    // moved left past the bar's start.
    const bar = control(o, "track_musicvolume");
    await swipeTo(centreOf(bar));
    const value = () => options().then((o) => parseFloat(control(o, "track_musicvolume").value));
    await sleep(300);
    const fingerAt = [512, 384];
    const t = { pointerType: "touch", isPrimary: true, button: 0 };
    pointer("pointerdown", client(...fingerAt), { ...t, pointerId: ++touchId, buttons: 1 });
    pointer("pointerup", client(...fingerAt), { ...t, pointerId: touchId, buttons: 0 });
    await sleep(50);
    const s = (await volya.call("input_state")).sensitivity;
    const id = ++touchId;
    const travel = (-2 * (bar.rect[2] / 2 + 40)) / s;
    await swipe([fingerAt], [travel, 0], { lift: false, ids: [id] });
    await sleep(100);
    assertEqual((await volya.call("input_state")).held, ["mouse_left"], "held while dragging");
    assertEqual(await value(), 0, "dragged to the bar's start");
    // The page losing focus mid-drag releases it, and the touchpad forgets
    // the finger: its further motion and its lift do nothing.
    dispatchEvent(new Event("blur"));
    await sleep(100);
    const blurred = await volya.call("input_state");
    assertEqual(blurred.held, [], "released by the blur");
    pointer("pointermove", client(fingerAt[0] + travel + 60, fingerAt[1]), { ...t, pointerId: id, buttons: 1 });
    pointer("pointerup", client(fingerAt[0] + travel + 60, fingerAt[1]), { ...t, pointerId: id, buttons: 0 });
    await sleep(100);
    const after = await volya.call("input_state");
    assertEqual([after.ui, after.count], [blurred.ui, blurred.count], "the forgotten finger");
    // The controls tab; two fingers on the key table swiped up scroll it
    // down, swiped down back.
    await swipeTo(centreOf(button(await options(), "tab 3")));
    await tapAt();
    o = await untilOptions((o) => o.page === "controls", "the controls tab");
    const cell = o.cells.find((c) => c.action === "forward" && c.slot === 0);
    await swipeTo(centreOf(cell));
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
    // Touch (the touchpad): the cursor to the sound tab, a tap; then to the
    // EAX check box, a tap.
    await swipeTo(centreOf(button(o, "tab 1")));
    await tapAt();
    o = await untilOptions((o) => o.page === "sound", "the sound tab");
    const eax = control(o, "check_eax").value;
    await swipeTo(centreOf(control(o, "check_eax")));
    await tapAt();
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
    pointer("pointermove", row(0));
    await until((s) => s.menu.selected === 0, "item 0");
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

  "the settings are read back when the game starts again": async () => {
    needsGame();
    key("KeyQ");
    await until((s) => s.phase === "none", "the game to end");
    await volya.call("launch");
    await until((s) => s.phase === "startup", "the logo sequence", 2000);
    for (let item = 1; item <= 5; item++) {
      key("Escape");
      await until((s) => s.phase === "menu" || s.startup?.item === item, `item ${item}`);
    }
    await until((s) => s.phase === "menu", "the menu");
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

  "Q quits to the app's own interface": async () => {
    needsGame();
    key("KeyQ");
    await until((s) => s.phase === "none", "the game to end");
    assertEqual((await volya.call("input_state")).capture, false, "capture");
    await sleep(50);
    assertEqual(document.querySelector("main").hidden, false, "page interface hidden");
    const end = (await storedEnds()).at(-1);
    assertEqual([end?.args.reason, end?.args.message], ["quit", "left the game"], "the end in the diagnostic log");
  },

  "the diagnostic log holds the games, their levels and their frames, after the game ended": async () => {
    needsGame();
    const { text } = await volya.call("diagnostic_log");
    const events = JSON.parse(text).traceEvents;
    const pid = events.findLast((e) => e.name === "session").pid;
    const mine = events.filter((e) => e.pid === pid);
    const count = (ph, name) => mine.filter((e) => e.ph === ph && e.name === name).length;
    // Two New Games and the options' game, each left; each level named.
    assert(count("i", "game started") >= 2, `games started ${count("i", "game started")}`);
    const ends = mine.filter((e) => e.name === "engine.end");
    const left = ends.filter((e) => e.args.message === "left the game").length;
    assertEqual(left, count("i", "game started"), "games left");
    // Each game starts with its settings; a change writes them again.
    const settings = mine.filter((e) => e.name === "settings");
    for (const start of mine.filter((e) => e.name === "game started")) {
      const next = mine.find((e) => e.ts >= start.ts && (e.name === "settings" || e.name === "engine.end"));
      assertEqual(next?.name, "settings", "the settings at a game's start");
    }
    for (const key of ["renderer", "vid_mode", "r__supersample", "_preset", "r__detail_density", "r1_dlights", "rs_c_gamma", "rs_vis_distance", "graphics", "backend"]) {
      assert(settings.every((e) => key in e.args), `settings without ${key}`);
    }
    assert(settings.some((e) => e.args.vid_mode === "2868x1320"), "no settings at a changed video mode");
    const reasons = new Set(ends.map((e) => e.args.reason));
    for (const reason of ["quit", "page_hidden", "load_failed"]) assert(reasons.has(reason), `no ${reason} end`);
    const levels = (name) => mine.filter((e) => e.name === name && e.args?.level === "l01_escape").length;
    assert(levels("level loading") >= 2 && levels("level made") >= 2, "levels");
    const series = (key) => mine.filter((e) => e.ph === "C" && key in e.args).length;
    for (const key of ["frame max", "engine max", "render max", "frame.interval_ms max", "MiB", "diagnostics.write_ms max"]) {
      assert(series(key) > 10, `${key}: ${series(key)}`);
    }
    // The GPU's times (#355): by timestamp queries where the device has
    // them, the submission to the GPU done everywhere; what was drawn at.
    const session = mine.find((e) => e.name === "session").args;
    const timed = session.gpu_timing === "timestamp-query";
    if (session.gpu_state === "ready") {
      assert(["timestamp-query", "submitted-work-done"].includes(session.gpu_timing), `gpu_timing ${session.gpu_timing}`);
      assert(series("gpu.done_ms max") > 10, `gpu.done_ms: ${series("gpu.done_ms max")}`);
      if (timed) assert(series("gpu.time_ms max") > 10, `gpu.time_ms: ${series("gpu.time_ms max")}`);
      const mode = mine.find((e) => e.name === "video mode")?.args;
      assert(mode?.width > 0 && mode?.canvas_width > 0, `video mode ${JSON.stringify(mode)}`);
    }
    const means = (key) => {
      const values = mine.filter((e) => e.ph === "C" && `${key} mean` in e.args).map((e) => e.args[`${key} mean`]);
      return values.length ? (values.reduce((a, b) => a + b, 0) / values.length).toFixed(2) : "none";
    };
    // The level's first frame creates its sounds: long, with what ran.
    const long = mine.filter((e) => e.ph === "X" && e.name === "frame");
    assert(long.length > 0, "no long frame");
    const audio = volya.audioState() === "running";
    if (audio) assert(series("audio.fill_ms min") > 10 && series("audio.mix_ms max") > 10, "audio");
    await volya.diagnostics.setEnabled(false, volya.call);
    const worst = Math.max(...long.map((e) => e.dur)) / 1000;
    // The log's rate while a game runs: this session's bytes over its time.
    const seconds = (mine.at(-1).ts - mine.find((e) => e.ts > 0).ts) / 1e6;
    const bytes = mine.reduce((sum, e) => sum + JSON.stringify(e).length + 2, 0);
    const gpu = `GPU ${session.gpu_timing}: time ${means("gpu.time_ms")} ms, submitted to done ${means("gpu.done_ms")} ms (means of periods)`;
    return `${mine.length} events, ${(bytes / seconds).toFixed(0)} B/s over ${seconds.toFixed(0)} s; ${long.length} long frames (worst ${worst.toFixed(0)} ms); ${gpu}; audio ${audio ? "running" : volya.audioState()}`;
  },

  "WebGL 2 chosen in Settings: one press of Play starts the engine anew on it and the game, and the level draws": async () => {
    needsGame();
    const { graphics } = volya;
    if (!graphics.available.webgl2) skip("no WebGL 2 here");
    if (!graphics.available.webgpu) skip("no WebGPU here: the New Games above drew through WebGL 2");
    // The title menu through WebGPU, for WebGL 2's frames of the same scenes
    // below.
    await volya.call("launch");
    await until((s) => s.phase === "startup" || s.phase === "menu", "the game", 10000);
    for (let s = await state(); s.phase !== "menu"; s = await state()) {
      key("Escape");
      await sleep(100);
    }
    assertEqual((await volya.status()).gpu.backend, "webgpu", "graphics");
    const menuFrames = await drawMenuFrames();
    await volya.call("quit_game");
    await until((s) => s.phase === "none", "the game to end");
    await volya.showSettings();
    document.querySelector('#game input[name="graphics"][value="webgl2"]').click();
    // The engine workers started from here on (#369).
    const Original = globalThis.Worker;
    let engines = 0;
    globalThis.Worker = class extends Original {
      constructor(url, options) {
        super(url, options);
        if (String(url).includes("engine-worker.js")) engines++;
      }
    };
    try {
      await volya.refresh();
      const play = document.querySelector("#game button.play");
      play.click();
      // Pressed again while the engine starts anew: nothing more happens.
      assert(play.disabled && play.textContent === "Starting…", `Play while starting: ${play.disabled} ${play.textContent}`);
      play.click();
      // Calls made while the engine starts anew fail: asked again.
      for (const deadline = performance.now() + 30000; ; await sleep(100)) {
        const s = await state().catch(() => null);
        if (s?.phase === "startup" || s?.phase === "menu") break;
        assert(performance.now() < deadline, `the game did not start: ${JSON.stringify(s)?.slice(0, 300)}`);
      }
      const { gpu: drawn } = await volya.status();
      assertEqual([drawn.state, drawn.backend], ["ready", "webgl2"], "graphics");
      assertEqual(engines, 1, "engines started");
      // Escape skips the logos, then the title menu's New Game.
      for (let s = await state(); s.phase !== "menu"; s = await state()) {
        key("Escape");
        await sleep(100);
      }
      await readRender();
      // A New Game on this engine, then another after the title menu: the
      // wasm heap through each load and its first frames (#357), against
      // the budget.
      const level = drawn.textures === "etc2" ? 120000 : 60000;
      const newGame = async () => {
        pointer("pointermove", row(0));
        await until((s) => s.menu?.selected === 0 && !s.menu.moving, "item 0", 5000);
        key("Enter");
        await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
        await volya.call("heap_log", { on: true });
        key("Enter");
        const s = await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", level);
        assertEqual([s.level.name, s.level.draws], ["l01_escape", 7782], "level");
        key("Escape");
        await until((s) => s.phase === "level" && !s.intro, "the intro skipped");
        return s;
      };
      const s = await newGame();
      // Frame times over a few seconds of the level.
      await sleep(3000);
      const first = byCaption(await volya.call("heap_log", { on: false }));
      const { rgba } = await volya.call("debug_capture", { width: 256, height: 192 });
      const colours = new Set();
      for (let i = 0; i < rgba.length; i += 4) colours.add((rgba[i] << 16) | (rgba[i + 1] << 8) | rgba[i + 2]);
      assert(colours.size > 200, `${colours.size} colours`);
      const stats = await volya.call("level_stats");
      await quitToTitle();
      const again = await newGame();
      await sleep(1000);
      const second = byCaption(await volya.call("heap_log", { on: false }));
      const after = await volya.call("level_stats");
      const heapNote = overBudget(heap(stats), "wasm heaps after the first New Game") + overBudget(heap(after), "wasm heaps after the second New Game");
      // The title menu's scenes drawn through WebGPU above, drawn again
      // through WebGL 2: the same picture.
      await quitToTitle();
      const menuNote = [];
      const drawnAgain = await drawMenuFrames(menuFrames.map((f) => f.scene));
      menuFrames.forEach(({ scene, rgba }, i) => {
        const [width, height] = scene.resolution;
        const other = drawnAgain[i].rgba;
        let differ = 0;
        let max = 0;
        const box = [width, height, -1, -1];
        for (let p = 0; p < width * height; p++) {
          let d = 0;
          for (let c = 0; c < 3; c++) d = Math.max(d, Math.abs(rgba[4 * p + c] - other[4 * p + c]));
          if (d === 0) continue;
          differ++;
          max = Math.max(max, d);
          const [x, y] = [p % width, Math.floor(p / width)];
          [box[0], box[1], box[2], box[3]] = [Math.min(box[0], x), Math.min(box[1], y), Math.max(box[2], x), Math.max(box[3], y)];
        }
        const what = `title menu at ${width}×${height}: ${differ} pixels differ, by up to ${max}${differ ? ` within ${box}` : ""}`;
        assert(max <= MENU_FRAME_MAX_DIFFERENCE && differ <= MENU_FRAME_MAX_SHARE * width * height, `WebGL 2 against WebGPU, ${what}`);
        menuNote.push(what);
      });
      await volya.call("quit_game");
      return `${heapNote}${drawn.backend}, textures ${drawn.textures}; ${menuNote.map((n) => `${n}; `).join("")}first caption to first level frame in ${s.load_ms.toFixed(0)} ms, ${again.load_ms.toFixed(0)} ms again; ${colours.size} colours; wasm heap ${mib(stats.wasm_heap_bytes)}, ${mib(after.wasm_heap_bytes)} after the second; by caption (MiB, heap/live): ${first}; the second: ${second}; ${JSON.stringify(stats)}`;
    } finally {
      globalThis.Worker = Original;
      localStorage.removeItem("volya-engine.graphics");
      // Leaves the game for the tests after (a failure above is the one told).
      if ((await state()).phase !== "none") {
        if ((await state()).menu) key("Escape");
        await sleep(100);
        key("KeyQ");
        await until((s) => s.phase === "none", "the game to end").catch((e) => console.warn(e.message));
      }
    }
  },

  "forgets the game": async () => {
    if (installer) await volya.call("forget_game");
    await volya.call("forget_user_config");
  },
};
