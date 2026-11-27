// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// One frame of the UX audit page: the app's real interface (style.css,
// game-ui.js) in one state, with a fake engine and fake updates. The part of
// the address after `#` names the device and the state: `#iphone/home`.

import { screens, start } from "../game-ui.js";
import { createFullscreenOffer } from "../fullscreen.js";
import { createTouchControls } from "../touch-controls.js";
import { createConsoleTouch } from "../console-touch.js";
import { zoomTransform } from "../direct-touch.js";
import { versionText } from "../update.js";
import { DEVICES, STATES } from "./states.js";

const [deviceId, stateId] = location.hash.slice(1).split("/");
const device = DEVICES.find((d) => d.id === deviceId) ?? DEVICES[0];
const state = STATES.find((s) => s.id === stateId) ?? STATES[0];

// Cellular, Wi-Fi and battery, as drawn in iOS's status bar.
const SYSTEM_ICONS = `<svg width="78" height="13" viewBox="0 0 78 13" fill="#fff" aria-hidden="true">
  <rect x="0" y="8" width="3" height="4" rx="1"/><rect x="5" y="6" width="3" height="6" rx="1"/>
  <rect x="10" y="3.5" width="3" height="8.5" rx="1"/><rect x="15" y="1" width="3" height="11" rx="1"/>
  <path d="M31 3.2a9 9 0 0 1 12 0l-1.3 1.4a7 7 0 0 0-9.4 0zM33.4 5.8a5.6 5.6 0 0 1 7.2 0l-1.3 1.4a3.7 3.7 0 0 0-4.6 0zM37 8.2l1.8 1.9L37 12l-1.8-1.9z"/>
  <rect x="50" y="1" width="24" height="11" rx="3" fill="none" stroke="#fff" stroke-opacity=".45"/>
  <rect x="52" y="3" width="17" height="7" rx="1.5"/><rect x="75" y="4.5" width="2" height="4" rx="1" fill-opacity=".45"/>
</svg>`;

// The device's safe areas (env() is 0 in a frame) and system chrome.
const root = document.documentElement.style;
for (const [side, px] of Object.entries(device.insets)) root.setProperty(`--inset-${side}`, `${px}px`);
if (device.insets.top > 0) {
  document.body.insertAdjacentHTML(
    "beforeend",
    `<div class="ux-status-bar"><span>9:41</span><span class="island"></span>${SYSTEM_ICONS}</div>`,
  );
}
if (device.touch) document.body.insertAdjacentHTML("beforeend", `<div class="ux-home-indicator"></div>`);

// The device's platform; installed unless the state says it's a browser
// tab; Chrome's install offer on desktop.
globalThis.volyaPlatform = device.touch ? "ios" : "desktop";
globalThis.volyaChromium = !device.touch;
globalThis.volyaInstalled = state.tab ? false : true;
if (state.tab && !device.touch) globalThis.volyaInstallOffer = { prompt: async () => {}, userChoice: Promise.resolve({ outcome: "dismissed" }) };

// The performance log, always on (#669), and whether the game's log is
// kept (#646: once the game has run).
const log = { on: true, entries: true, ...state.log };
let gameLog = state.gameLog ?? true;
// Whether saves or settings are stored, until Delete saves, settings, and logs.
let userData = state.userData ?? true;

// Whether an import finished here before (the localStorage mark in game-ui.js).
globalThis.volyaImportedBefore = Boolean(state.deleted);

// Storage persistence as the state says (the installed app keeps it).
Object.defineProperty(navigator.storage, "persisted", { value: async () => state.persisted ?? true });
Object.defineProperty(navigator.storage, "persist", { value: async () => state.persisted ?? true });

// The engine's own message for a browser without WebGL 2 (platforms/web/src/gpu.rs).
const gpu = state.gpu === false ? { state: "unavailable", message: "no WebGL 2 adapter" } : { state: "ready" };
const call = async (op, args) => {
  switch (op) {
    case "status":
      return { gpu };
    case "cover":
      return placeholderArt();
    case "launch":
      if (state.launchError) throw new Error(state.launchError);
      // Starting never settles.
      if (state.starting) return new Promise(() => {});
      return null;
    case "diagnostic_log_status":
      return log;
    case "diagnostics_set":
      // Off deletes the log, unless the state says it can't; on, a new one
      // starts.
      if (args?.on) return log;
      return { on: false, entries: Boolean(log.deleteFails), deleted: !log.deleteFails };
    case "game_log_status":
      return { present: gameLog };
    case "forget_user_data":
      gameLog = false;
      userData = false;
      return null;
    case "logs":
      // Preparing never settles.
      if (log.download === "preparing") return new Promise(() => {});
      throw new Error("the logs are held by another copy of the app");
    case "game_status":
      if (state.game instanceof Error) throw state.game;
      return state.game;
    case "forget_game":
      if (state.deleteGameFails) throw new Error("the game's files are held by another copy of the app");
      return null;
    case "user_data_status":
      return { present: userData };
    default:
      return null;
  }
};

/**
 * Stand-in for the art the engine reads from the player's game (the page
 * can't ship game assets): a dark, grainy plate with a note.
 */
function placeholderArt() {
  // The splash's size (startup.md section 3).
  const [width, height] = [500, 268];
  const canvas = new OffscreenCanvas(width, height);
  const g = canvas.getContext("2d");
  const gradient = g.createLinearGradient(0, 0, width, height);
  gradient.addColorStop(0, "#3a2f22");
  gradient.addColorStop(1, "#14110c");
  g.fillStyle = gradient;
  g.fillRect(0, 0, width, height);
  for (let i = 0; i < 4000; i++) {
    g.fillStyle = `rgba(255, 230, 190, ${Math.random() * 0.06})`;
    g.fillRect(Math.random() * width, Math.random() * height, 2, 2);
  }
  g.fillStyle = "rgba(255, 235, 200, 0.55)";
  g.font = "600 30px system-ui, sans-serif";
  g.textAlign = "center";
  g.fillText("The game's splash picture", width / 2, height / 2);
  const rgba = g.getImageData(0, 0, width, height).data;
  let binary = "";
  for (let i = 0; i < rgba.length; i += 0x8000) binary += String.fromCharCode(...rgba.subarray(i, i + 0x8000));
  return { width, height, rgba: btoa(binary) };
}

// Fake updates with the state's values.
const listeners = new Set();
const updates = {
  status: "current",
  bytes: 0,
  build: "c0ffee1",
  auto: true,
  ...state.updates,
  async setAuto(on) {
    updates.auto = on;
    for (const f of listeners) f(updates);
  },
  async download() {},
  apply() {},
  async check() {},
  // The state's check outcome; "checking" never settles.
  checkNow: () => (state.check === "checking" ? new Promise(() => {}) : Promise.resolve(state.check ?? "none")),
  onChange(f) {
    listeners.add(f);
    return () => listeners.delete(f);
  },
};

const status = document.getElementById("status");
// The touch controls' setting (#802), on touch devices.
let touchMode = state.touchMode ?? "standard";
globalThis.volya = {
  touchControls: {
    get mode() {
      return touchMode;
    },
    available: () => device.touch,
    async set(mode) {
      touchMode = mode;
    },
  },
};
if (state.level) {
  // In the level: the page's own interface hides; over a plain backdrop the
  // full screen offer shows (desktop) or the touch controls do.
  document.documentElement.dataset.uxLevel = "";
  if (device.height > device.width) document.documentElement.dataset.uxTurned = "";
  document.querySelector("main").hidden = true;
  const { mode, stick, lock, thumbY, trip, fade, down, offer, ...toggles } = state.level;
  if (offer) {
    // As on desktop Chrome (fullscreen.js).
    const fullscreen = createFullscreenOffer(async () => [], { show: true });
    document.body.append(fullscreen.element);
    fullscreen.update({ level: true });
  } else {
    const touch = createTouchControls(() => {}, { touch: true, fade: false });
    document.body.append(touch.element);
    // Standard unless the state says experimental (#802).
    touch.setMode(mode ?? "standard");
    // As at a New Game's start unless the state says otherwise: the torch
    // attached and off, no outfit with night vision.
    touch.update({ level: true, torch: false, nightVision: null, ...toggles });
    const box = touch.element;
    const landscape = [Math.max(box.clientWidth, box.clientHeight), Math.min(box.clientWidth, box.clientHeight)];
    // A thumb a little above the stick's resting place, or where the state
    // says (high on the screen).
    const thumb = [140, thumbY ?? landscape[1] - 150];
    if (stick) touch.showStick(thumb, stick);
    if (lock) touch.showLocked(thumb);
    // A trip's destination where the state says, in the picture (from its
    // centre, in half its height).
    if (trip) touch.trip(trip);
    if (fade) touch.fadeNow();
    if (down) box.querySelector(`.touch-button.${down}`).classList.add("down");
  }
} else if (state.screenZoom) {
  // A screen with the game's cursor in the ergonomic mode (#807): a
  // stand-in for the game's picture, turned as the canvas is on upright
  // phones, magnified in the screen's layer (direct-touch.js) by `scale`
  // with the picture's point `at` (fractions of the screen) at the same
  // place.
  document.documentElement.dataset.uxLevel = "";
  if (device.height > device.width) document.documentElement.dataset.uxTurned = "";
  document.querySelector("main").hidden = true;
  const screen = document.getElementById("screen");
  screen.insertAdjacentHTML(
    "beforeend",
    `<div class="ux-game"><div class="ux-stand-in ux-menu">The game's screen (${state.screenZoom.cursor ? "its cursor drawn" : "no cursor drawn"})</div></div>`,
  );
  const { scale, at } = state.screenZoom;
  const [w, h] = [screen.clientWidth, screen.clientHeight];
  screen.style.transform = zoomTransform({ scale, x: (1 - scale) * w * at[0], y: (1 - scale) * h * at[1] });
} else if (state.console) {
  // The console on touch screens (#642), over a plain backdrop: over the
  // main menu its Console button, or the console open (a stand-in for
  // what the game draws) with the device's keyboard (a stand-in) and the
  // key strip on it.
  document.documentElement.dataset.uxLevel = "";
  if (device.height > device.width) document.documentElement.dataset.uxTurned = "";
  document.querySelector("main").hidden = true;
  const { menu, keyboard, down, paste } = state.console;
  const consoleTouch = createConsoleTouch(() => {}, { press() {}, type() {}, paste() {} }, { touch: true });
  document.body.append(...consoleTouch.elements);
  consoleTouch.setCapture(true);
  consoleTouch.update({ open: !menu, menu: Boolean(menu) });
  const [root, strip] = consoleTouch.elements;
  if (menu) {
    root.insertAdjacentHTML("afterbegin", `<div class="ux-stand-in ux-menu">The game's main menu</div>`);
  } else {
    // The console's look (console-window.md 4.1, 4.3): the top half, its
    // log above the prompt line.
    root.insertAdjacentHTML(
      "afterbegin",
      `<div class="ux-console"><p>~ cfg_load</p><p>! Unknown command:  zz</p><p class="prompt">&gt;&gt;&gt; hud_crosshair_</p></div>`,
    );
  }
  if (keyboard) {
    // The iPhone's keyboard: in landscape 244 px of Safari's page (measured,
    // iOS 27.2 Simulator, iPhone 17 Pro Max; console-touch.js), upright
    // about 300 (our estimate). In landscape the picture lifts (#682).
    const height = device.height > device.width ? 300 : 244;
    document.body.insertAdjacentHTML(
      "beforeend",
      `<div class="ux-stand-in ux-keyboard" style="height: ${height}px">Device keyboard (under its form bar)</div>`,
    );
    consoleTouch.showKeys({ height, down });
    if (paste) {
      const key = strip.querySelector('[data-key="insert"]').getBoundingClientRect();
      document.body.insertAdjacentHTML(
        "beforeend",
        `<div class="ux-paste-bubble" style="left: ${key.left + key.width / 2}px; top: ${key.top - 8}px">Paste</div>`,
      );
    }
  }
} else if (state.startError) {
  status.textContent = `Volya Engine · ${versionText()}`;
  status.dataset.state = "ready";
  screens.showProblem(new Error(state.startError));
} else {
  const name = `Volya Engine · ${versionText()}`;
  status.textContent = name;
  status.dataset.text = status.textContent;
  status.dataset.state = "ready";
  await start(call, updates, { isolated: state.isolated !== false });
  if (state.screen === "settings") await globalThis.volya.showSettings();
  // Licences: the site's own licences, their first lines (the board shows
  // each state whole); loading never settles; failed as when unreadable.
  if (state.screen === "licences") {
    const read = async () => {
      if (state.notices === "failed") throw new Error("HTTP 404");
      const response = await fetch("../third-party-licenses.txt");
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return (await response.text()).split("\n").slice(0, 60).join("\n");
    };
    const notices = state.notices === "loading" ? new Promise(() => {}) : read();
    screens.showLicences(() => notices);
    if (state.notices !== "loading") await notices.catch(() => {});
    await new Promise((resolve) => setTimeout(resolve));
  }
  // Check for updates pressed, its result held on screen.
  if (state.check) {
    globalThis.volyaHoldCheckResult = true;
    [...document.querySelectorAll("#game button")].find((b) => b.textContent === "Check for updates").click();
    await new Promise((resolve) => setTimeout(resolve));
  }
  // Download logs pressed, its outcome held on screen.
  if (state.log?.download) {
    globalThis.volyaHoldCheckResult = true;
    [...document.querySelectorAll("#game button")].find((b) => b.textContent === "Download logs").click();
    await new Promise((resolve) => setTimeout(resolve));
  }
  // Buttons pressed in turn, by their text (a confirmation's, say), or a
  // switch by its title.
  for (const label of state.press ?? []) {
    const button = [...document.querySelectorAll("#game button")].find((b) => b.textContent === label);
    const toggle = [...document.querySelectorAll("#game .switch")].find((l) => l.querySelector("strong")?.textContent === label);
    (button ?? toggle.querySelector("input")).click();
    await new Promise((resolve) => setTimeout(resolve));
  }
  if (state.snapshot) screens.showImport(state.game, state.snapshot);
  // Play pressed: the game failed to start, or is starting.
  if (state.launchError || state.starting) {
    document.querySelector("button.play").click();
    await new Promise((resolve) => setTimeout(resolve));
  }
}
document.title = `${state.name} · ${device.name}`;
document.documentElement.dataset.ready = "true";
// The page's full length, for the board's "Full length" view, and the
// screen's text (its content, not the status bar or footer), for the board
// to flag states whose text differs by device.
/** The content's text, with inline symbols named (e.g. "[Share]"). */
function screenText() {
  const game = document.getElementById("game");
  const icons = [...game.querySelectorAll(".inline-icon")];
  const marks = icons.map((icon) => {
    const mark = document.createTextNode(`[${icon.querySelector("[aria-label]")?.getAttribute("aria-label") ?? "icon"}]`);
    icon.after(mark);
    return mark;
  });
  const text = game.innerText.trim();
  for (const mark of marks) mark.remove();
  return text;
}

const report = () =>
  parent.postMessage(
    {
      type: "ux-frame",
      hash: location.hash,
      height: document.documentElement.scrollHeight,
      text: screenText(),
    },
    "*",
  );
new ResizeObserver(report).observe(document.body);
report();
