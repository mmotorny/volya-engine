// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import { assert, assertEqual } from "./harness.js";

// The app's base path (tests are served under it, in __test/).
const base = location.pathname.slice(0, location.pathname.indexOf("/__test/") + 1);

export const tests = {
  "engine reports ready": async () => {
    const status = await volya.status();
    assertEqual(status.state, "ready", "state");
    assert(/^\d+\.\d+\.\d+$/.test(status.version), `version ${status.version}`);
  },

  "engine runs in a worker": async () => {
    assertEqual((await volya.status()).thread, "worker");
  },

  "status is shown": async () => {
    const el = document.getElementById("status");
    assertEqual(el.dataset.state, "ready", "data-state");
    const { version, gpu } = await volya.status();
    // Without WebGPU the line says why graphics are unavailable.
    const build = (await (await fetch(`${base}build.txt`, { cache: "no-store" })).text()).trim();
    assert(/^[0-9a-f]{7,}\+?$/.test(build), `build ${build}`);
    const name = `Volya Engine ${version} (${build})`;
    const expected = gpu.state === "ready" ? name : `${name} · graphics unavailable: ${gpu.message}`;
    assertEqual(el.textContent, expected, "text");
  },

  "a new deploy offers a reload, never reloads by itself": async () => {
    const { startUpdateCheck } = await import(`${base}update.js`);
    let deployed = "aaaaaaa";
    const footer = document.createElement("footer");
    const update = await startUpdateCheck(footer, async () => deployed);
    assertEqual(update.build, "aaaaaaa", "build");
    await update.check();
    assert(update.button.hidden, "offered without a new deploy");
    deployed = "bbbbbbb";
    await update.check();
    assert(!update.button.hidden, "not offered after a new deploy");
    assertEqual(update.button.parentElement, footer, "placement");
    assert(await startUpdateCheck(footer, async () => { throw new Error("offline"); }) === null, "no build: no check");
  },

  // GitHub Pages cannot send COOP/COEP; the engine must not depend on them.
  "page is not cross-origin isolated": () => {
    assertEqual(self.crossOriginIsolated, false);
  },

  "service worker controls the app scope": async () => {
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 10000));
    const registration = await Promise.race([navigator.serviceWorker.ready, timeout]);
    assert(registration.scope.endsWith("/volya-engine/"), `scope ${registration.scope}`);
  },

  "layout fits the viewport": () => {
    const overflow = document.documentElement.scrollWidth - window.innerWidth;
    assert(overflow <= 0, `horizontal overflow ${overflow}px`);
  },
};
