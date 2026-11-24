// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// One frame of the UX audit page: the app's real interface (style.css,
// game-ui.js) in one state, with a fake engine and fake updates. The part of
// the address after `#` names the device and the state: `#iphone/home`.

import { screens, start } from "../game-ui.js";
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

// Storage persistence as the state says (the installed app keeps it).
Object.defineProperty(navigator.storage, "persisted", { value: async () => state.persisted ?? true });

const gpu = state.gpu === false ? { state: "unavailable", message: "no WebGPU adapter" } : { state: "ready" };
const call = async (op) => {
  switch (op) {
    case "status":
      return { gpu };
    case "game_status":
      if (state.game instanceof Error) throw state.game;
      return state.game;
    default:
      return null;
  }
};

// Fake updates with the state's values.
const listeners = new Set();
const updates = {
  status: "current",
  bytes: 0,
  build: "c0ffee1",
  updatedFrom: null,
  auto: true,
  ...state.updates,
  async setAuto(on) {
    updates.auto = on;
    for (const f of listeners) f(updates);
  },
  async download() {},
  apply() {},
  async check() {},
  onChange(f) {
    listeners.add(f);
    return () => listeners.delete(f);
  },
};

const status = document.getElementById("status");
globalThis.volya = {};
if (state.startError) {
  status.textContent = `Failed to start: ${state.startError}`;
  status.dataset.state = "error";
} else {
  const name = `Volya Engine · ${versionText()}`;
  status.textContent = gpu.state === "ready" ? name : `${name} · graphics unavailable: ${gpu.message}`;
  status.dataset.text = status.textContent;
  status.dataset.state = "ready";
  await start(call, updates);
  if (state.screen === "settings") await globalThis.volya.showSettings();
  if (state.snapshot) screens.showImport(state.game, state.snapshot);
}
document.title = `${state.name} · ${device.name}`;
document.documentElement.dataset.ready = "true";
// The page's full length, for the board's "Full length" view.
const report = () =>
  parent.postMessage({ type: "ux-frame", hash: location.hash, height: document.documentElement.scrollHeight }, "*");
new ResizeObserver(report).observe(document.body);
report();
