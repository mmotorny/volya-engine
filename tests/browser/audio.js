// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Audio path: a game sound (from the real installer, as in the import suite)
// decoded and mixed in the engine worker, pulled by the AudioWorklet and
// rendered by an OfflineAudioContext (no user gesture needed).

import { assert, skip } from "./harness.js";

// A stereo pair: mono left and right channels in `_l.ogg` and `_r.ogg`.
const TRACK = "gamedata/sounds/characters_voice/scenario/video/intro_gsc_wp";
// The pair as a music track name, relative to `sounds\`.
const MUSIC = "characters_voice\\scenario\\video\\intro_gsc_wp";
const RATE = 48000;
const SECONDS = 1;
let installer;

function installerBase() {
  const path = location.pathname;
  return `${path.slice(0, path.indexOf("/__test/"))}/__test/installer`;
}

const source = (name) => ({
  url: `${installerBase()}/${encodeURIComponent(name)}`,
  size: installer.find((f) => f.name === name).size,
});

// Renders SECONDS of the engine's output with sounds started by `start`
// (output connected first); returns the left and right channels after the
// first 0.2 s, then stops all sounds.
async function render(start) {
  const context = new OfflineAudioContext(2, RATE * SECONDS, RATE);
  await volya.connectAudio(context);
  await start();
  // Offline rendering outruns the worker's replies: pause regularly so
  // requested blocks arrive.
  for (let t = 0.02; t < SECONDS; t += 0.02) {
    context.suspend(t).then(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
      context.resume();
    });
  }
  const buffer = await context.startRendering();
  await volya.call("stop_sounds");
  return [0, 1].map((c) => buffer.getChannelData(c).subarray(RATE * 0.2));
}

// RMS of `a`, or of the difference `a - b`.
function rmsOf(a, b) {
  let sum = 0;
  for (let i = 0; i < a.length; i++) {
    const v = b ? a[i] - b[i] : a[i];
    sum += v * v;
  }
  return Math.sqrt(sum / a.length);
}

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
    const [left] = await render(() => volya.call("play_sound", { path: `${TRACK}_l.ogg` }));
    const rms = rmsOf(left);
    assert(rms > 0.001, `rendered audio is silent (RMS ${rms})`);
    return `RMS ${rms.toFixed(4)}`;
  },

  "pans a sound fully left": async () => {
    if (!installer) skip("no installer");
    const [left, right] = await render(() =>
      volya.call("play_sound", { path: `${TRACK}_l.ogg`, pan: -1, category: "music" }),
    );
    assert(rmsOf(left) > 0.001, `left channel is silent (RMS ${rmsOf(left)})`);
    assert(rmsOf(right) < 1e-6, `right channel is not silent (RMS ${rmsOf(right)})`);
    return `RMS left ${rmsOf(left).toFixed(4)}, right ${rmsOf(right)}`;
  },

  "plays a music track's files as its left and right channels": async () => {
    if (!installer) skip("no installer");
    // Each render adds one file, inverted and panned to its side, to the
    // track: if the file plays exactly that channel (from the same frame as
    // the track), the channel cancels out and the other stays.
    const cancel = (side, pan) => () =>
      volya
        .call("play_music", { track: MUSIC })
        .then(() => volya.call("play_sound", { path: `${TRACK}_${side}.ogg`, pan, gain: -1 }));
    const [leftCancelled, right] = await render(cancel("l", -1));
    const [left, rightCancelled] = await render(cancel("r", 1));
    for (const [name, rms] of [["left", rmsOf(left)], ["right", rmsOf(right)]]) {
      assert(rms > 0.001, `${name} channel is silent (RMS ${rms})`);
    }
    for (const [name, rms] of [["_l", rmsOf(leftCancelled)], ["_r", rmsOf(rightCancelled)]]) {
      assert(rms < 1e-6, `${name} is not alone in its channel (residual RMS ${rms})`);
    }
    return `RMS left ${rmsOf(left).toFixed(4)}, right ${rmsOf(right).toFixed(4)}`;
  },

  "music volume scales music only": async () => {
    if (!installer) skip("no installer");
    await volya.call("set_volume", { category: "music", volume: 0 });
    try {
      const [mutedLeft] = await render(() => volya.call("play_music", { track: MUSIC }));
      assert(rmsOf(mutedLeft) === 0, `muted music is audible (RMS ${rmsOf(mutedLeft)})`);
      const [effect] = await render(() => volya.call("play_sound", { path: `${TRACK}_l.ogg` }));
      assert(rmsOf(effect) > 0.001, "effects are muted too");
    } finally {
      await volya.call("set_volume", { category: "music", volume: 1 });
    }
  },

  "forgets the game": async () => {
    if (!installer) skip("no installer");
    await volya.call("forget_game");
  },
};
