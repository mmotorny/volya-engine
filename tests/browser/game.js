// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// What the game suites share (`game-startup`, `game-level`, `game-input`,
// `game-screens`, `game-memory`): the game is launched as the app launches
// it and driven with synthesized keyboard, mouse, touch and gamepad input.
// State comes from the `menu_state` and `menu_scene` ops; pixels from
// `debug_capture` (the renderer, not the DOM) where graphics exist, with the
// textures transcoded to ETC2 where the GPU has no BC (the iOS Simulator).
// Without graphics everything but the pixels still runs. `game-startup`
// imports into a fresh profile; the others start from Chrome's kept import
// (`keptSetup`), each from the state its tests need.

import { assert, assertEqual, skip } from "./harness.js";

// What docs/behavior/startup.md and title-menu.md read (found by the native
// game-data test), plus the splash's executable and the default user
// settings.
export const ONLY = [
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
// uploaded a few pieces per task turn); a regression past
// this is reported as an "OVER BUDGET" note, not failed: budgets are
// measured and tracked in issues, never a release gate (AGENTS.md). The
// audio worker's instance (#354) counts too (`audio_heap_bytes`, which
// `heap` adds).
export const WASM_HEAP_CAP = 110 * 2 ** 20;
/** Both wasm instances' heaps in `level_stats`: the engine's and the audio worker's. */
export const heap = (stats) => stats.wasm_heap_bytes + stats.audio_heap_bytes;
// The wasm heap at the end of the last frame under each caption, MiB, from
// a `heap_log`; in test builds with the most it held live meanwhile
// (`heap/live`), so fragmentation shows as the difference (#357).
export const byCaption = (log) => {
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
export const overBudget = (bytes, what) => (bytes > WASM_HEAP_CAP ? `OVER BUDGET: ${what} ${mib(bytes)} > ${mib(WASM_HEAP_CAP)}; ` : "");
// Touch devices: no full screen offer, touch controls from the start.
export const coarse = () => matchMedia("(pointer: coarse)").matches;
// How long a level may take to load: transcoding its textures (WebGL 2
// without BC, #113) takes about 7 s in the iOS Simulator, on every load
// (#183).
export const loadMs = () => (gpu?.textures === "etc2" ? 120000 : 60000);
export const mib = (bytes) => `${(bytes / 2 ** 20).toFixed(0)} MiB`;
export const CAPTIONS = ["Новая игра", "Загрузить игру", "Сетевая игра", "Опции", "Титры", "Выйти в Windows"];
export let installer;
export let gpu;
// The size the game draws at (#49), shown scaled to fit the canvas.
export let render;
export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
/**
 * WebGL 2's calls per frame over 2 s (`debug_gl_calls`): the total and the
 * most frequent kinds; and, from the diagnostic log meanwhile, where it is
 * on, the mean per frame of each span of the frame taking 0.1 ms or more.
 */
export async function glCalls() {
  const since = (performance.timeOrigin + performance.now()) * 1000;
  await volya.call("debug_gl_calls", { on: true });
  await sleep(2000);
  const { frames, calls } = await volya.call("debug_gl_calls", { on: false });
  const per = Object.entries(calls ?? {})
    .map(([name, n]) => [name, n / frames])
    .sort((a, b) => b[1] - a[1]);
  const total = per.reduce((sum, [, n]) => sum + n, 0);
  // Without the counting's own cost: a second more, then the log's periods.
  await sleep(3000);
  const { text } = await volya.call("diagnostic_log");
  const spans = new Map();
  let frameCount = 0;
  for (const e of JSON.parse(text).traceEvents) {
    if (e.name !== "time ms" || e.ts < since + 2.5e6) continue;
    frameCount += e.args["frame n"] ?? 0;
    for (const [key, mean] of Object.entries(e.args)) {
      if (!key.endsWith(" mean")) continue;
      const name = key.slice(0, -" mean".length);
      spans.set(name, (spans.get(name) ?? 0) + mean * (e.args[`${name} n`] ?? 0));
    }
  }
  const times = [...spans]
    .map(([name, sum]) => [name, sum / frameCount])
    .filter(([, ms]) => ms >= 0.1)
    .sort((a, b) => b[1] - a[1]);
  const log = frameCount ? `; ms a frame: ${times.map(([name, ms]) => `${name} ${ms.toFixed(2)}`).join(", ")}` : "";
  return `${total.toFixed(0)} GL calls a frame (${per.slice(0, 10).map(([name, n]) => `${name} ${n.toFixed(0)}`).join(", ")})${log}`;
}
export const state = () => volya.call("menu_state");
// Where the wasm heap's live bytes sit before the New Game (#432), from the
// `heap_map` op: per MiB of the heap "#" for 1000 KiB or more live, "+" for
// 512, "." for 64, "_" for less; then the live blocks of 256 KiB or more
// (MiB address: KiB).
export const heapMaps = [];
export async function noteHeap(label) {
  const m = await volya.call("heap_map").catch(() => null);
  if (!m) return;
  const mib = (b) => Math.round(b / 2 ** 20);
  const occupancy = m.live_kib_per_mib.map((k) => (k >= 1000 ? "#" : k >= 512 ? "+" : k >= 64 ? "." : "_")).join("");
  const large = m.large.map(([at, kib]) => `${at.toFixed(1)}:${kib}`).join(" ");
  heapMaps.push(`${label} ${mib(m.wasm_heap_bytes)}/${mib(m.live_bytes)} MiB [${occupancy}] large ${large}`);
}
// The most the wasm heap held live during a load, and by what (#440): the
// `heap_record` op's bytes per block size and its largest blocks (KiB).
export async function heapRecord(reset) {
  const r = await volya.call("heap_record", { reset }).catch(() => null);
  if (!r || reset) return "";
  const kib = (k) => (k >= 1024 ? `${(k / 1024).toFixed(1)}M` : `${k}K`);
  const size = (b) => (b >= 2 ** 20 ? `${b / 2 ** 20}M` : b >= 1024 ? `${b / 1024}K` : `${b}`);
  const sizes = r.sizes_kib.map(([at, k]) => `≥${size(at)}: ${kib(k)}`).join(", ");
  const growths = (r.growths ?? []).map(([k, at, heap]) => `${kib(k)} at ${at.toFixed(1)} to ${heap.toFixed(0)}`).join(", ");
  return `the most live ${mib(r.live_bytes)} (by block size ${sizes}; largest ${r.large.slice(0, 40).map(([, k]) => kib(k)).join(" ")}); the heap grown by ${r.growth_count ?? 0} allocations (KiB asked for at MiB, to MiB): ${growths}`;
}
export const view = () => document.getElementById("view");
// The memory JavaScript holds, per context (#374): Chrome's
// `performance.measureUserAgentSpecificMemory()`, where the page is
// cross-origin isolated (documented, MDN); the page's and each worker's
// bytes by type, MiB, and the wasm heaps beside them.
export async function jsMemory(stats) {
  if (!performance.measureUserAgentSpecificMemory) return `JS memory not measurable here (crossOriginIsolated ${crossOriginIsolated})`;
  const start = performance.now();
  const m = await Promise.race([performance.measureUserAgentSpecificMemory(), sleep(60000).then(() => null)]).catch((e) => String(e));
  if (!m || typeof m === "string") return `JS memory not measured: ${m ?? "no result in 60 s"}`;
  const where = (b) => {
    const a = b.attribution[0];
    if (!a) return "shared";
    const name = a.url.split("/").pop() || "page";
    return `${a.scope} ${name}`;
  };
  const parts = m.breakdown
    .filter((b) => b.bytes >= 2 ** 19)
    .sort((a, b) => b.bytes - a.bytes)
    .map((b) => `${where(b)} [${b.types.join("+")}] ${(b.bytes / 2 ** 20).toFixed(1)}`);
  return `JS memory ${mib(m.bytes)} in all (${parts.join(", ")}; the wasm heaps ${mib(stats.wasm_heap_bytes)} and ${mib(stats.audio_heap_bytes)}), measured in ${((performance.now() - start) / 1000).toFixed(1)} s`;
}

export function installerBase() {
  const path = location.pathname;
  return `${path.slice(0, path.indexOf("/__test/"))}/__test/installer`;
}

export const source = (name) => ({
  url: `${installerBase()}/${encodeURIComponent(name)}`,
  size: installer.find((f) => f.name === name).size,
});

/** Polls the menu state until `test` holds, or fails after `ms`. */
export async function until(test, what, ms = 5000) {
  const deadline = performance.now() + ms;
  let s;
  while (performance.now() < deadline) {
    s = await state();
    if (test(s)) return s;
    await sleep(30);
  }
  throw new Error(`timed out waiting for ${what}; state ${JSON.stringify(s).slice(0, 400)}`);
}

export function key(code) {
  for (const type of ["keydown", "keyup"]) {
    document.body.dispatchEvent(new KeyboardEvent(type, { code, bubbles: true, cancelable: true }));
  }
}

/** Reads the size the game draws at. */
export async function readRender() {
  render = (await volya.status()).render;
}

/**
 * Page position of UI point (x, y): 1024 × 768 stretched to the drawn
 * picture, which fits the canvas with its aspect kept (#49).
 */
export function client(x, y) {
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
export const turned = (box) => {
  const r = box.getBoundingClientRect();
  return r.width < r.height !== box.clientWidth < box.clientHeight;
};

/** Page position of point `[x, y]` in `box`'s own layout, as the page's `local` reads it back. */
export function toClient(box, [x, y]) {
  const b = box.getBoundingClientRect();
  return turned(box) ? [b.right - y, b.top + x] : [b.left + x, b.top + y];
}

/** `el`'s rectangle in `box`'s own layout (not turned). */
export function localRect(el, box) {
  const [b, r] = [box.getBoundingClientRect(), el.getBoundingClientRect()];
  const [left, top, right, bottom] = turned(box)
    ? [r.top - b.top, b.right - r.right, r.bottom - b.top, b.right - r.left]
    : [r.left - b.left, r.top - b.top, r.right - b.left, r.bottom - b.top];
  return { left, top, right, bottom };
}

export function pointer(type, [x, y], init = {}) {
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
export const centreOfRow = (i) => [782, 419 + 30 * i + 15];
export const row = (i) => client(...centreOfRow(i));

/**
 * The mouse on item `i`, at rest there. The engine places the pointer by the
 * video mode it fits on its next frame (a mode just set is not yet), and the
 * item may be selected already, so the cursor itself is awaited on the item.
 */
export async function pointAt(i) {
  await readRender();
  const [x, y] = centreOfRow(i);
  for (const deadline = performance.now() + 5000; ; await sleep(30)) {
    pointer("pointermove", row(i));
    const { ui } = await volya.call("input_state");
    if (Math.abs(ui[0] - x) < 2 && Math.abs(ui[1] - y) < 2) break;
    if (performance.now() > deadline) throw new Error(`the cursor at ${ui}, not on item ${i}`);
  }
  await until((s) => s.menu.selected === i && !s.menu.moving, `item ${i} at rest`, 2000);
}

export function needsGame() {
  if (!installer) skip("no installer");
}

/** The axis-aligned bounds of a scene layer. */
export function bounds(scene, layer) {
  const sprites = scene.sprites.filter((s) => s.layer === layer);
  assert(sprites.length > 0, `no ${layer} in the scene`);
  const xs = sprites.flatMap((s) => s.corners.map((c) => c[0]));
  const ys = sprites.flatMap((s) => s.corners.map((c) => c[1]));
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

/** The options screen's state (it must show). */
export async function options() {
  const o = (await state()).options;
  assert(o, "the options screen does not show");
  return o;
}

/** Polls the options screen's state until `test` holds. */
export async function untilOptions(test, what, ms = 5000) {
  return (await until((s) => s.options && test(s.options), what, ms)).options;
}

export const control = (o, name) => o.controls.find((c) => c.name === name) ?? assert(false, `no ${name}`);
export const button = (o, name) => o.buttons.find((b) => b.name === name) ?? assert(false, `no ${name}`);
/** The centre of a reported rectangle, in UI units. */
export const centreOf = ({ rect: [x, y, w, h] }) => [x + w / 2, y + h / 2];

/** A mouse click at UI point `at`. */
export function clickAt(at) {
  const p = client(...at);
  pointer("pointermove", p);
  pointer("pointerdown", p, { button: 0, buttons: 1 });
  pointer("pointerup", p, { button: 0, buttons: 0 });
}

// Touch where the game shows its cursor is a touchpad (touchpad.js, #108).
export let touchId = 100;
/** Slow enough for the touchpad's one-to-one speed, CSS pixels per event
 * 25 ms apart (120 px/s; the curve rises from 200). */
export const SLOW_STEP = 3;
/** CSS pixels per UI unit of the drawn picture. */
export const cssPerUi = () => {
  const [a, b] = [client(0, 0), client(100, 0)];
  return Math.hypot(b[0] - a[0], b[1] - a[1]) / 100;
};

/** Fingers at UI points `starts` moving slowly by `travel` (UI units), then lifted. */
export async function swipe(starts, [tx, ty], { lift = true, ids } = {}) {
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
export async function swipeTo(target) {
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
export async function tapAt(at = [512, 384]) {
  await sleep(300);
  const t = { pointerType: "touch", pointerId: ++touchId, isPrimary: true, button: 0 };
  pointer("pointerdown", client(...at), { ...t, buttons: 1 });
  pointer("pointerup", client(...at), { ...t, buttons: 0 });
}

/** The in-game menu's item list (save-load-screens.md section 2). */
export const inGame = (s) => (s.phase === "level" ? s.menu?.menu : undefined);

/** Selects in-game menu item `name` with the arrow keys from where it is. */
export async function selectInGame(name) {
  const m = inGame(await state());
  const target = m.items.findIndex((i) => i.name === name);
  assert(target >= 0, `no ${name}`);
  for (let i = m.selected; i < target; i++) key("ArrowDown");
  for (let i = m.selected; i > target; i--) key("ArrowUp");
  await until((s) => inGame(s)?.selected === target, `${name} selected`);
}

/** Answers the box over the in-game menu by clicking `answer`. */
export async function answerBox(answer) {
  const box = (await until((s) => s.menu?.message, "the box")).menu.message;
  clickAt(centreOf(box.buttons.find((b) => b.answer === answer)));
}

/** From the level: the in-game menu, "Завершить игру", "Да": the title menu. */
export async function quitToTitle() {
  if (!inGame(await state())) key("Escape");
  await until((s) => inGame(s), "the in-game menu");
  await selectInGame("btn_quit_to_mm");
  key("Enter");
  await answerBox("yes");
  const menu = await until((s) => s.phase === "menu", "the title menu");
  await audioFlows("in the title menu after quitting");
  return menu;
}

/**
 * Where the AudioContext runs: over 1.5 s the audio worker mixes about as
 * much as plays, and the AudioWorklet plays nothing as silence for want of
 * it (#452: an iPhone's title menu went silent for 15 s after a quit).
 */
export async function audioFlows(where) {
  if (volya.audioState() !== "running") return;
  const before = await volya.call("level_stats");
  const started = performance.now();
  await sleep(1500);
  const after = await volya.call("level_stats");
  const seconds = (performance.now() - started) / 1000;
  const mixed = (after.audio_mixed_frames - before.audio_mixed_frames) / 48000;
  const silent = after.audio_underrun_frames - before.audio_underrun_frames;
  assert(mixed > seconds * 0.5, `audio ${where}: ${mixed.toFixed(2)} s mixed in ${seconds.toFixed(2)} s`);
  assertEqual(silent, 0, `audio ${where}: frames played as silence`);
}

/** From the title menu's main page: New game, the first difficulty. */
export async function newGame() {
  await pointAt(0);
  key("Enter");
  await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
  key("Enter");
}

/**
 * This session's `engine.end` events in the diagnostic log as stored now
 * (not those waiting for the period's end).
 */
export async function storedEnds() {
  const { text } = await volya.call("diagnostic_log", { stored_only: true });
  const events = JSON.parse(text).traceEvents;
  const pid = events.findLast((e) => e.name === "session").pid;
  return events.filter((e) => e.pid === pid && e.name === "engine.end");
}

/**
 * Simulates the page being hidden or shown: the visibility the page reads,
 * then the event (a real hide needs the browser's own controls).
 */
export function setVisibility(state) {
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  Object.defineProperty(document, "hidden", { value: state === "hidden", configurable: true });
  document.dispatchEvent(new Event("visibilitychange"));
  if (state === "visible") {
    delete document.visibilityState;
    delete document.hidden;
  }
}

/** Opens the options screen from the title menu's main page. */
export async function openOptions() {
  pointer("pointermove", row(3));
  await until((s) => s.menu?.selected === 3 && !s.menu.moving, "item 3 at rest", 3000);
  pointer("pointerdown", row(3), { button: 0, buttons: 1 });
  pointer("pointerup", row(3), { button: 0, buttons: 0 });
  return untilOptions(() => true, "the options screen");
}

/** A fresh touch's pointer id. */
export const nextTouch = () => ++touchId;

/** Reads the graphics the page has. */
export async function readGpu() {
  gpu = (await volya.status()).gpu;
}

/**
 * Imports what the game suites read from the installer (skipping when none
 * is served); in a kept profile only what it lacks. `fresh`: nothing was
 * imported before, so every file is written.
 */
export async function importGameData(fresh) {
  const response = await fetch(`${installerBase()}/index.json`);
  if (response.status === 404) skip("VOLYA_GOG_INSTALLER is unset");
  installer = await response.json();
  const exe = installer.find((f) => f.name.endsWith(".exe")).name;
  const bin = installer.find((f) => f.name.endsWith("-1.bin")).name;
  const started = performance.now();
  const result = await volya.importGame({ exe: source(exe), bin: source(bin), only: ONLY });
  if (fresh) assertEqual(result.status.written, ONLY.length, "files written");
  else assert(result.status.written >= ONLY.length, `${result.status.written} files written`);
  return `${(result.status.bytes / 2 ** 30).toFixed(2)} GiB in ${((performance.now() - started) / 1000).toFixed(1)} s`;
}

/** Launches the game and skips its logos with Escape: the title menu. */
export async function launchToMenu() {
  await volya.call("launch");
  await until((s) => s.phase === "startup", "the logo sequence", 2000);
  for (let item = 1; item <= 5; item++) {
    key("Escape");
    await until((s) => s.phase === "menu" || s.startup?.item === item, `item ${item}`);
  }
  await until((s) => s.phase === "menu", "the menu");
  await readRender();
}

/**
 * From the title menu: a New Game's level, its intro skipped with Escape, or
 * with a tap (`tap`: the page has then seen touch).
 */
export async function enterLevel({ tap = false } = {}) {
  await newGame();
  await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", loadMs());
  if (tap) {
    const t = { pointerType: "touch", pointerId: nextTouch(), isPrimary: true, button: 0 };
    pointer("pointerdown", client(512, 384), { ...t, buttons: 1 });
    pointer("pointerup", client(512, 384), { ...t, buttons: 0 });
  } else key("Escape");
  await until((s) => s.phase === "level" && !s.intro && !s.menu, "the intro skipped");
}

/**
 * The start of a suite on the kept import: the installer's user settings
 * (a stored copy from an earlier run would not apply), what the game reads
 * imported where the profile lacks it, the diagnostic log on, the title
 * menu; with `level`, a New Game's level (`tap`: its intro skipped by touch).
 */
export function keptSetup({ level = false, tap = false } = {}) {
  const tests = {
    "imports what the game reads, where the kept profile lacks it": async () => {
      await readGpu();
      await volya.call("forget_user_config");
      return importGameData(false);
    },
    "the diagnostic log is turned on for the games that follow": async () => {
      needsGame();
      const status = await volya.diagnostics.setEnabled(true, volya.call);
      assert(status.on, "not on");
    },
    "launches into the title menu": async () => {
      needsGame();
      await launchToMenu();
    },
  };
  if (level) {
    tests[`a New Game into the level, its intro skipped by ${tap ? "a tap" : "Escape"}`] = async () => {
      needsGame();
      await enterLevel({ tap });
    };
  }
  return tests;
}

/** The end of a suite on the kept import: the game ended, the log off, the user settings forgotten. */
export function keptTeardown() {
  return {
    "leaves the kept profile as it found it: the game ended, the diagnostic log off, the user settings forgotten": async () => {
      if (!installer) return;
      if ((await state()).phase !== "none") {
        await volya.call("quit_game");
        await until((s) => s.phase === "none", "the game to end");
      }
      await volya.diagnostics.setEnabled(false, volya.call);
      await volya.call("forget_user_config");
    },
  };
}
