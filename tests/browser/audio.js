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
// What a New Game reads (as the menu suite imports it).
const NEW_GAME = [
  "gamedata.db0",
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
let installer;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const state = () => volya.call("menu_state");

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

  // The positional sound system (docs/behavior/sound-positional.md) on the
  // same output: `intro_gsc_wp_r` has no parameters, so the defaults (min
  // distance 1, max 300). The listener stands at the origin looking along +z.
  "plays a sound at a position, heard on its side": async () => {
    if (!installer) skip("no installer");
    const name = "characters_voice\\scenario\\video\\intro_gsc_wp_r";
    let state;
    const [left, right] = await render(async () => {
      await volya.call("debug_play_sound", { name, position: [3, 0, 0] });
      // Started by the next sound update.
      for (let i = 0; i < 50; i++) {
        state = await volya.call("sound_state");
        if (state.emitters[0]?.phase === "playing") break;
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
    });
    assert(state.emitters[0]?.phase === "playing", `not playing: ${JSON.stringify(state)}`);
    assert(state.voices >= 4, `voices ${state.voices}`);
    assert(rmsOf(right) > 0.001, `right channel is silent (RMS ${rmsOf(right)})`);
    assert(rmsOf(left) < 1e-6, `left channel is not silent (RMS ${rmsOf(left)})`);
    return `RMS right ${rmsOf(right).toFixed(4)}, left ${rmsOf(left)}`;
  },

  "a sound beyond its max distance is silent": async () => {
    if (!installer) skip("no installer");
    const name = "characters_voice\\scenario\\video\\intro_gsc_wp_r";
    let state;
    const [left, right] = await render(async () => {
      await volya.call("debug_play_sound", { name, position: [400, 0, 0] });
      for (let i = 0; i < 50; i++) {
        state = await volya.call("sound_state");
        if (state.emitters[0]?.phase === "silent") break;
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
    });
    assert(state.emitters[0]?.phase === "silent", `not silent: ${JSON.stringify(state)}`);
    for (const [side, channel] of [["left", left], ["right", right]]) {
      assert(rmsOf(channel) === 0, `${side} channel is audible (RMS ${rmsOf(channel)})`);
    }
  },

  // The first level's own sounds (docs/behavior/sound-static-sources.md,
  // sound-weather-ambient.md, sound-level-music.md, sound-occlusion.md):
  // a New Game, as the menu suite starts one, then the sound system's state.
  "imports what a New Game reads": async () => {
    if (!installer) skip("no installer");
    const exe = installer.find((f) => f.name.endsWith(".exe")).name;
    const bin = installer.find((f) => f.name.endsWith("-1.bin")).name;
    await volya.call("forget_game");
    const started = performance.now();
    const result = await volya.importGame({ exe: source(exe), bin: source(bin), only: NEW_GAME });
    assert(result.status.written === NEW_GAME.length, `files written ${result.status.written}`);
    return `${(result.status.bytes / 2 ** 30).toFixed(2)} GiB in ${((performance.now() - started) / 1000).toFixed(1)} s`;
  },

  "a New Game's level plays its sounds": async () => {
    if (!installer) skip("no installer");
    await volya.call("forget_user_config");
    await volya.call("launch");
    // The logos, then the title menu's New Game with its first difficulty.
    await until((s) => s.phase === "startup", "the logo sequence");
    for (let i = 0; i < 40; i++) {
      if ((await state()).phase === "menu") break;
      key("Escape");
      await sleep(150);
    }
    await until((s) => s.phase === "menu" && s.menu.list === "menu_main", "the menu");
    key("Enter");
    await until((s) => s.menu?.list === "menu_new_game", "the difficulty page");
    key("Enter");
    await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", 60000);
    await until((s) => s.intro, "the intro", 10000).catch(() => null);
    key("Escape");
    await until((s) => s.phase === "level" && !s.intro && !s.menu, "the level's controls", 10000);
    // S3 section 5: sources 0, 1, 2 and 4 start looped on the first frame
    // after the loading image (the radio waits for 02:00); S5 section 7: no
    // music at 05:30; S4 4.2: the first frame starts a random sound.
    const statics = [
      "nature\\new_flies1",
      "ambient\\sparks1",
      "scripts\\magnitofon\\magnitofon_2",
      "ambient\\indoors\\small_fan",
    ];
    let sounds;
    for (const deadline = performance.now() + 5000; performance.now() < deadline; await sleep(50)) {
      sounds = await volya.call("sound_state");
      if (statics.every((n) => sounds.emitters.some((e) => e.name === n))) break;
    }
    for (const name of statics) {
      const e = sounds.emitters.find((e) => e.name === name);
      assert(e?.looped && !e.two_d, `${name}: ${JSON.stringify(e)}`);
    }
    const names = sounds.emitters.map((e) => e.name);
    assert(!names.some((n) => n.includes("radio_noise")), `the radio plays: ${names}`);
    assert(!names.some((n) => n.startsWith("music\\")), `music plays: ${names}`);
    assert(
      names.some((n) => n.startsWith("ambient\\rnd_outdoor\\")),
      `no ambient sound: ${names}`,
    );
    const heard = sounds.emitters.filter((e) => e.phase === "playing").map((e) => e.name);
    assert(heard.length > 0, `nothing heard: ${JSON.stringify(sounds.emitters)}`);
    // The sound update with occlusion on the level, over the latest frames.
    await sleep(2000);
    const stats = await volya.call("level_stats");
    const sparks = (await volya.call("sound_state")).emitters.find((e) => e.name === "ambient\\sparks1");
    return (
      `playing: ${heard.join(", ")}; sparks1 occlusion ${sparks?.occlusion}; ` +
      `sound update ${(stats.sound_update_ms * 1000).toFixed(0)} µs average, ` +
      `${(stats.sound_update_max_ms * 1000).toFixed(0)} µs worst; ` +
      `frame CPU ${stats.frame_cpu_ms.toFixed(2)} ms; wasm heap ${(stats.wasm_heap_bytes / 2 ** 20).toFixed(0)} MiB`
    );
  },

  // The player's footsteps (docs/behavior/player-footsteps.md): running in
  // the level plays step sounds from the ground's material pair, at the
  // feet raised by 0.5 m, with the run's volume 1.
  "running in the level makes step sounds": async () => {
    if (!installer) skip("no installer");
    const down = (code) => document.body.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true }));
    const up = (code) => document.body.dispatchEvent(new KeyboardEvent("keyup", { code, bubbles: true, cancelable: true }));
    // Collision stops a run along the start view after about 0.8 m
    // (measured natively): run backwards, where the way is clear.
    const steps = new Map();
    const start = (await state()).level.player.position;
    down("KeyS");
    try {
      for (const deadline = performance.now() + 2000; performance.now() < deadline; await sleep(30)) {
        for (const e of (await volya.call("sound_state")).emitters) {
          if (e.name.startsWith("material\\")) steps.set(e.name + e.position.join(), e);
        }
      }
    } finally {
      up("KeyS");
    }
    const end = (await state()).level.player.position;
    const moved = Math.hypot(end[0] - start[0], end[2] - start[2]);
    assert(moved > 2, `ran ${moved.toFixed(2)} m`);
    assert(steps.size >= 2, `steps heard: ${JSON.stringify([...steps.values()])}`);
    for (const e of steps.values()) {
      assert(e.name.startsWith("material\\actor\\step\\"), `not a step sound: ${e.name}`);
      assert(!e.looped && !e.two_d && e.volume === 1, `step played as ${JSON.stringify(e)}`);
    }
    const stats = await volya.call("level_stats");
    return (
      `ran ${moved.toFixed(1)} m: ${[...steps.values()].map((e) => e.name.split("\\").pop()).join(", ")}; ` +
      `sound update ${(stats.sound_update_ms * 1000).toFixed(0)} µs average; frame CPU ${stats.frame_cpu_ms.toFixed(2)} ms`
    );
  },

  "forgets the game": async () => {
    await volya.call("quit_game").catch(() => null);
    if (!installer) skip("no installer");
    await volya.call("forget_game");
  },
};
