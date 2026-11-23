// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Audio path: a game sound (from the real installer, as in the import suite)
// decoded and mixed in the engine worker, pulled by the AudioWorklet and
// rendered by an OfflineAudioContext (no user gesture needed).

import { assert, skip } from "./harness.js";

const SOUND = "gamedata/sounds/characters_voice/scenario/video/intro_gsc_wp_l.ogg";
let installer;

function installerBase() {
  const path = location.pathname;
  return `${path.slice(0, path.indexOf("/__test/"))}/__test/installer`;
}

const source = (name) => ({
  url: `${installerBase()}/${encodeURIComponent(name)}`,
  size: installer.find((f) => f.name === name).size,
});

export const tests = {
  "imports an archive with sounds": async () => {
    const response = await fetch(`${installerBase()}/index.json`);
    if (response.status === 404) skip("VOLYA_GOG_INSTALLER is unset");
    installer = await response.json();
    const exe = installer.find((f) => f.name.endsWith(".exe")).name;
    const bin = installer.find((f) => f.name.endsWith("-1.bin")).name;
    await volya.call("forget_game");
    await volya.importGame({ exe: source(exe), bin: source(bin), only: ["gamedata.dba"] });
  },

  "plays a game sound through the AudioWorklet": async () => {
    if (!installer) skip("no installer");
    const rate = 48000;
    const seconds = 1;
    const context = new OfflineAudioContext(2, rate * seconds, rate);
    await volya.connectAudio(context);
    await volya.call("play_sound", { path: SOUND });
    // Offline rendering outruns the worker's replies: pause regularly so
    // requested blocks arrive.
    for (let t = 0.02; t < seconds; t += 0.02) {
      context.suspend(t).then(async () => {
        await new Promise((resolve) => setTimeout(resolve, 20));
        context.resume();
      });
    }
    const buffer = await context.startRendering();
    const samples = buffer.getChannelData(0).subarray(rate * 0.2);
    const rms = Math.sqrt(samples.reduce((s, v) => s + v * v, 0) / samples.length);
    assert(rms > 0.001, `rendered audio is silent (RMS ${rms})`);
    await volya.call("stop_sounds");
    return `RMS ${rms.toFixed(4)}`;
  },

  "forgets the game": async () => {
    if (!installer) skip("no installer");
    await volya.call("forget_game");
  },
};
