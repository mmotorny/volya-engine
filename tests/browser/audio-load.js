// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Audio under load (#812): a New Game's level played for SESSION_S with
// its own sounds (the static sources, the ambient), a music track changed
// every MUSIC_S, a busy moment every BUSY_S (BUSY sounds of the level's
// files started by one update) and a run back and forth between them.
// Sampled every second: the audio worker's mixing time and frames, its
// underruns, the sound cache's bytes, its wasm memory and the engine's
// frame times (`level_stats`). Where the page's AudioContext does not run
// (the iOS Simulator holds it "interrupted" without a gesture), an
// OfflineAudioContext rendered in steps of STEP_S paced by as long a
// pause stands in for it, as the audio suite's renders do.

import { assert } from "./harness.js";
import { keptSetup, keptTeardown, needsGame, sleep } from "./game.js";

const SESSION_S = 720;
const MUSIC_S = 180;
const BUSY_S = 60;
const RATE = 48000;
const CHUNK_S = 30;
const STEP_S = 0.02;
const MUSIC = ["music\\amb01", "music\\amb20", "music\\wasteland2"];
// The level's files (as the audio suite's busy update), and those it
// played so far, started 2D, not looped.
const FILES = [
  "nature\\new_flies1",
  "ambient\\sparks1",
  "scripts\\magnitofon\\magnitofon_2",
  "script_replics\\radio\\message\\radio_noise_1",
  "ambient\\indoors\\small_fan",
  "music\\amb01_l",
  "music\\amb20_l",
  "music\\wasteland2_l",
];
const BUSY = 24;

function hold(code, down) {
  const type = down ? "keydown" : "keyup";
  document.body.dispatchEvent(new KeyboardEvent(type, { code, bubbles: true, cancelable: true }));
}

// Renders OfflineAudioContexts of CHUNK_S one after another until `done()`.
async function offlineOutput(done) {
  while (!done()) {
    const context = new OfflineAudioContext(2, RATE * CHUNK_S, RATE);
    await volya.connectAudio(context);
    for (let t = STEP_S; t < CHUNK_S; t += STEP_S) {
      context.suspend(t).then(async () => {
        await sleep(STEP_S * 1000);
        context.resume();
      });
    }
    await context.startRendering();
  }
}

const quantile = (values, q) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
};

export const tests = {
  ...keptSetup({ level: true }),

  [`a level session of ${SESSION_S / 60} minutes: mixing, underruns, the sound cache, frame times`]: async () => {
    needsGame();
    let done = false;
    const live = volya.audioState() === "running";
    const output = live ? null : offlineOutput(() => done);
    const samples = [];
    const heard = new Set(FILES);
    const started = performance.now();
    let music = -1;
    let busy = 0;
    let runs = 0;
    try {
      for (let second = 0; second <= SESSION_S; second++) {
        await sleep(Math.max(0, started + second * 1000 - performance.now()));
        const s = await volya.call("level_stats");
        samples.push({
          t: (performance.now() - started) / 1000,
          mix: s.audio_mix_ms,
          frames: s.audio_mixed_frames,
          underrun: s.audio_underrun_frames,
          cache: s.audio_cache_bytes,
          inUse: s.audio_in_use_bytes,
          heap: s.audio_heap_bytes,
          cpu: s.frame_cpu_ms,
          cpuMax: s.frame_cpu_max_ms,
          interval: s.frame_interval_ms,
        });
        if (second % MUSIC_S === 0 && second < SESSION_S) {
          music = (music + 1) % MUSIC.length;
          await volya.call("play_music", { track: MUSIC[music] });
        }
        if (second % 5 === 0) {
          for (const e of (await volya.call("sound_state")).emitters) heard.add(e.name);
        }
        if (second % BUSY_S === BUSY_S / 2) {
          const names = [...heard];
          const pick = Array.from({ length: BUSY }, (_, i) => names[(busy * BUSY + i) % names.length]);
          busy++;
          await volya.call("debug_play_sound", { names: pick, two_d: true });
        }
        // A run back, then forward: the steps' sounds.
        const phase = second % BUSY_S;
        if (phase === 5) hold("KeyS", true);
        if (phase === 7) (hold("KeyS", false), hold("KeyW", true));
        if (phase === 9) (hold("KeyW", false), runs++);
      }
    } finally {
      hold("KeyS", false);
      hold("KeyW", false);
      done = true;
      await volya.call("stop_music").catch(() => null);
      await volya.call("stop_sounds").catch(() => null);
      await output;
    }
    const first = samples[0];
    const last = samples.at(-1);
    const audioS = (last.frames - first.frames) / RATE;
    assert(audioS > SESSION_S * 0.5, `${audioS.toFixed(0)} s of audio mixed in ${SESSION_S} s`);
    // Each second's mixing per second of audio mixed in it.
    const perSecond = [];
    for (let i = 1; i < samples.length; i++) {
      const frames = samples[i].frames - samples[i - 1].frames;
      if (frames >= RATE / 2) perSecond.push((samples[i].mix - samples[i - 1].mix) / (frames / RATE));
    }
    const mean = (last.mix - first.mix) / audioS;
    const underrunMs = ((last.underrun - first.underrun) / RATE) * 1000;
    const kib = (b) => `${(b / 1024).toFixed(0)} K`;
    const mib = (b) => `${(b / 2 ** 20).toFixed(1)} MiB`;
    const at = (m) => samples[Math.min(samples.length - 1, m * 60)];
    const minutes = [0, 1, 2, 4, 6, 8, 10, SESSION_S / 60];
    const cpu = samples.slice(1).map((s) => s.cpu);
    const intervals = samples.slice(1).map((s) => s.interval);
    const most = (k) => Math.max(...samples.map((s) => s[k]));
    const summary = {
      output: live ? "live" : "offline",
      audio_s: +audioS.toFixed(1),
      mix_ms_per_s: +mean.toFixed(2),
      mix_ms_per_s_p99: +quantile(perSecond, 0.99).toFixed(2),
      mix_ms_per_s_worst: +Math.max(...perSecond).toFixed(2),
      underrun_ms: +underrunMs.toFixed(1),
      cache_kib: minutes.map((m) => Math.round(at(m).cache / 1024)),
      cache_most_kib: Math.round(most("cache") / 1024),
      in_use_most_kib: Math.round(most("inUse") / 1024),
      audio_heap_mib: minutes.map((m) => +(at(m).heap / 2 ** 20).toFixed(1)),
      frame_cpu_ms: +(cpu.reduce((a, b) => a + b, 0) / cpu.length).toFixed(2),
      frame_cpu_p99_ms: +quantile(cpu, 0.99).toFixed(2),
      frame_cpu_worst_ms: +most("cpuMax").toFixed(1),
      frame_interval_ms: +(intervals.reduce((a, b) => a + b, 0) / intervals.length).toFixed(2),
      busy_moments: busy,
      runs,
      files: heard.size,
    };
    console.log(`audio-load ${JSON.stringify(summary)}`);
    return (
      `${summary.output} output, ${audioS.toFixed(0)} s of audio; mixing ${mean.toFixed(2)} ms per second of audio, ` +
      `${summary.mix_ms_per_s_p99} at the 99th percentile of seconds, ${summary.mix_ms_per_s_worst} worst; ` +
      `underruns ${underrunMs.toFixed(0)} ms; the sound cache at minutes ${minutes.join(", ")}: ` +
      `${minutes.map((m) => kib(at(m).cache)).join(", ")} (most ${kib(most("cache"))}), ` +
      `the audio worker's bytes in use most ${kib(most("inUse"))}; its wasm memory ${minutes.map((m) => mib(at(m).heap)).join(", ")}; ` +
      `frame CPU ${summary.frame_cpu_ms} ms average, ${summary.frame_cpu_p99_ms} at the 99th percentile of seconds, ` +
      `${summary.frame_cpu_worst_ms} worst frame; frame interval ${summary.frame_interval_ms} ms; ` +
      `${busy} busy moments of ${BUSY} sounds, ${runs} runs, ${heard.size} files; ${JSON.stringify(summary)}`
    );
  },

  ...keptTeardown(),
};
