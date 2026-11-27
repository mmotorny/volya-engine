// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Memory and repeat games from the kept import: the heap and what the GPU
// holds when the game left from its level is launched again and a New Game
// runs at another video mode, the in-game menu opened twice, the page
// hidden and shown, quitting to the title menu, a New Game again, a New
// Game failing for a missing file, quitting to the app, the game's log's
// memory figures, and the diagnostic log holding all of it.

import { assert, assertEqual } from "./harness.js";
import {
  answerBox,
  audioFlows,
  byCaption,
  captionMs,
  client,
  enterLevel,
  gpu,
  heapMaps,
  heapRecord,
  inGame,
  jsMemory,
  keptSetup,
  keptTeardown,
  key,
  launchToMenu,
  levelFrames,
  loadMs,
  mib,
  needsGame,
  newGame,
  noteHeap,
  againstOriginal,
  pointAt,
  pointer,
  quitToTitle,
  readRender,
  selectInGame,
  setVisibility,
  sleep,
  state,
  storedEnds,
  until,
} from "./game.js";

// What the GPU holds of the first New Game's level (`gpu_level`: not what
// the renderer keeps across games, which earlier tests may have grown),
// and what it keeps then, once its intro is skipped.
let firstLive;
let firstKept;

// What the GPU holds by owner (`gpu_owners`), those holding anything.
const owners = (st) =>
  Object.entries(st.gpu_owners ?? {})
    .filter(([, l]) => l.textures || l.buffers)
    .map(([o, l]) => `${o} ${l.textures} textures ${mib(l.texture_bytes)}, ${l.buffers} buffers ${mib(l.buffer_bytes)}`)
    .join("; ") || "nothing";

export const tests = {
  ...keptSetup(),

  "the first New Game's level: what the GPU holds, for the repeat games": async () => {
    needsGame();
    // As the first New Game's test measures it (game-level): the intro
    // played a while, skipped, a frame captured.
    const menu = await volya.call("level_stats");
    await newGame();
    await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", loadMs());
    await until((s) => s.intro?.elapsed_ms > 1500, "the intro to play a while", 10000);
    key("Escape");
    await until((s) => s.phase === "level" && !s.intro, "the intro skipped");
    if (gpu.state === "ready") await volya.call("debug_capture", { width: 256, height: 192 });
    const stats = await volya.call("level_stats");
    firstLive = stats.gpu_level;
    firstKept = stats.gpu_kept;
    return `wasm heap ${mib(stats.wasm_heap_bytes)}, the audio worker's ${mib(stats.audio_heap_bytes)}; ${await jsMemory(stats)}; GPU by owner: the title menu before (${owners(menu)}), the level (${owners(stats)})`;
  },


  "the game left from its level and launched again: a New Game there at 2868×1320 is served by the heap the first grew": async () => {
    needsGame();
    // A phone's flow (#451): the game left from its level, launched again,
    // the video mode changed in the title menu, then a New Game.
    await volya.call("quit_game");
    await until((s) => s.phase === "none", "the game to end");
    const logos = await launchToMenu();
    await audioFlows("in the title menu of the game launched again");
    const before = await volya.call("level_stats");
    await volya.call("debug_video_mode", { width: 2868, height: 1320 });
    await noteHeap("before the New Game in the game launched again");
    await volya.call("heap_log", { on: true });
    await heapRecord(true);
    try {
      await newGame();
      await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", loadMs());
    } finally {
      await volya.call("debug_video_mode", { width: 1024, height: 768 });
      await readRender();
    }
    const stages = byCaption(await volya.call("heap_log", { on: false }));
    const record = await heapRecord(false);
    const stats = await volya.call("level_stats");
    const original = await againstOriginal();
    await noteHeap("in that level");
    key("Escape");
    await until((s) => s.phase === "level" && !s.intro, "the intro skipped");
    // The first level's buffers, textures and vertex arrays are gone: the
    // GPU holds of this level what it held of the first.
    await sleep(500);
    const now = await volya.call("level_stats");
    const live = now.gpu_level;
    if (firstLive) assertEqual(live, firstLive, "what the GPU holds of the level against the first New Game's");
    // What the GPU still holds of the level left (#436): the world stays
    // until the next one is uploaded while the presenter keeps it.
    const held = before.gpu_world_bytes;
    const kept = (k) => `${k.textures} textures (${mib(k.texture_bytes)}), ${k.buffers} buffers (${mib(k.buffer_bytes)})`;
    const liveNote = live ? `the GPU holds of the level ${live.textures} textures (${mib(live.texture_bytes)}), ${live.buffers} buffers (${mib(live.buffer_bytes)}), ${live.vertex_arrays} vertex arrays, as in the first; the renderer keeps ${kept(now.gpu_kept)}${firstKept ? `, ${kept(firstKept)} in the first` : ""}; GPU by owner: the title menu after the first level (${owners(before)}), this level at 1024×768 (${owners(now)}); ` : "";
    return `the logo sequence ${logos.toFixed(0)} ms after the launch; ${original}; ${liveNote}wasm heap ${mib(before.wasm_heap_bytes)} before, ${mib(stats.wasm_heap_bytes)} after; the GPU world in the title menu before: ${held == null ? "none" : mib(held)}; ${heapMaps.slice(-2).join("; ")}; by caption (MiB, heap/live): ${stages}; ${record}`;
  },
  "the in-game menu opened twice over the level: what its videos take each time (#517)": async () => {
    needsGame();
    await until((s) => s.phase === "level" && !s.menu && !s.intro, "the level's controls");
    const mb = (b) => `${(b / 1e6).toFixed(1)} MB`;
    const live = async () => (await volya.call("heap_map")).live_bytes;
    const openings = [];
    for (let i = 0; i < 2; i++) {
      const before = await live();
      const heapBefore = (await volya.call("level_stats")).wasm_heap_bytes;
      await heapRecord(true);
      key("Escape");
      await until((s) => inGame(s), "the in-game menu");
      // The videos play a while.
      await sleep(1500);
      const open = await live();
      const most = (await volya.call("heap_record", { reset: false })).live_bytes;
      key("Escape");
      await until((s) => s.phase === "level" && !s.menu, "back in the game");
      await sleep(200);
      const after = await live();
      const heapAfter = (await volya.call("level_stats")).wasm_heap_bytes;
      openings.push(
        `opening ${i + 1}: live +${mb(open - before)} while open (+${mb(most - before)} at most), ${mb(after - before)} after closing; the heap ${mib(heapBefore)} → ${mib(heapAfter)}`,
      );
    }
    return openings.join("; ");
  },

  "a phone's second game: the first game played 20 s, quit to the title menu, a New Game 5 s later": async () => {
    needsGame();
    // The path of a device's log (#487): the game quit to the title menu
    // (not left), then a New Game.
    await until((s) => s.phase === "level" && !s.intro, "the level");
    const start = await volya.call("level_stats");
    // Walking and turning about, as a player does.
    const hold = (code, down) => document.body.dispatchEvent(new KeyboardEvent(down ? "keydown" : "keyup", { code, bubbles: true, cancelable: true }));
    for (let i = 0; i < 10; i++) {
      hold("KeyW", true);
      await sleep(1500);
      hold("KeyW", false);
      hold("ArrowRight", true);
      await sleep(500);
      hold("ArrowRight", false);
    }
    const played = await volya.call("level_stats");
    key("Escape");
    await until((s) => inGame(s), "the in-game menu");
    await selectInGame("btn_quit_to_mm");
    key("Enter");
    await answerBox("yes");
    await until((s) => s.phase === "menu", "the title menu");
    await sleep(5000);
    const title = await volya.call("level_stats");
    await noteHeap("the title menu 5 s after quitting");
    await volya.call("heap_log", { on: true });
    await heapRecord(true);
    await newGame();
    await until((s) => s.phase === "level" && s.overlay_frames === 0, "the level", loadMs());
    const stages = byCaption(await volya.call("heap_log", { on: false }));
    const record = await heapRecord(false);
    const loaded = await volya.call("level_stats");
    await noteHeap("the level after quitting to the title menu");
    key("Escape");
    await until((s) => s.phase === "level" && !s.intro, "the intro skipped");
    const gpu = (st) => `textures ${mib(st.gpu_texture_bytes ?? 0)}, world ${mib(st.gpu_world_bytes ?? 0)}`;
    return `wasm heap: the level ${mib(start.wasm_heap_bytes)}, after 20 s ${mib(played.wasm_heap_bytes)}, the title menu ${mib(title.wasm_heap_bytes)}, the second game ${mib(loaded.wasm_heap_bytes)}; GPU: the level ${gpu(start)}, the title menu ${gpu(title)}, the second game ${gpu(loaded)}; GPU by owner in the title menu after quitting: ${owners(title)}; ${heapMaps.slice(-2).join("; ")}; by caption (MiB, heap/live): ${stages}; ${record}`;
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

  "the in-game menu's quit returns to the title menu from the level, audio flowing": async () => {
    needsGame();
    await quitToTitle();
  },

  "New Game again after returning: the level loads anew; Escape under the loading image opens the menu once the controls are live": async () => {
    needsGame();
    await pointAt(0);
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
    const log = await volya.call("heap_log", { on: false });
    const stages = byCaption(log);
    const original = await againstOriginal();
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
    return `${original}; loaded again in ${s.load_ms.toFixed(0)} ms; time by caption (ms): ${captionMs(log)}; ${levelFrames(log)}; wasm heap ${mib(stats.wasm_heap_bytes)}, the audio worker's ${mib(stats.audio_heap_bytes)}; by caption (MiB, heap/live): ${stages}`;
  },

  "the in-game menu's quit returns to the title menu after the New Game again, audio flowing": async () => {
    needsGame();
    await quitToTitle();
  },

  "a New Game quit while its level is prepared leaves nothing prepared on the GPU": async () => {
    needsGame();
    // #262: what was prepared for a load that is abandoned is freed at once.
    const before = await volya.call("level_stats");
    await newGame();
    // Well into the textures (the level has 458), or its geometry.
    const preparing = (st) => st.prepared_textures >= 100 || st.preparing_world;
    let during;
    for (const deadline = performance.now() + loadMs(); performance.now() < deadline; await sleep(20)) {
      during = await volya.call("level_stats");
      if (preparing(during) || (await state()).phase === "level") break;
    }
    assert(preparing(during), `nothing seen being prepared: ${JSON.stringify(during.textures)}`);
    await volya.call("quit_game");
    await until((s) => s.phase === "none", "the game to end");
    await sleep(300);
    const after = await volya.call("level_stats");
    const live = (st) => st.gpu_live?.texture_bytes ?? 0;
    assert(live(after) <= live(before), `textures held: ${mib(live(after))} after, ${mib(live(before))} in the title menu before`);
    assertEqual([after.prepared_textures, after.preparing_world], [0, false], "prepared textures and world held");
    await launchToMenu();
    return `quit with ${during.prepared_textures} textures prepared${during.preparing_world ? " and the world being uploaded" : ""}; the GPU's textures ${mib(live(before))} in the title menu before, ${mib(live(after))} after`;
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

  "quitting stores the game's log file, logs/xray_player.log, every line ended by CR LF": async () => {
    needsGame();
    const read = async () => {
      try {
        const dir = await (await (await navigator.storage.getDirectory()).getDirectoryHandle("volya-engine")).getDirectoryHandle("logs");
        return new Uint8Array(await (await (await dir.getFileHandle("xray_player.log")).getFile()).arrayBuffer());
      } catch {
        return null;
      }
    };
    let bytes = await read();
    for (const end = performance.now() + 5000; !(bytes?.length > 0); bytes = await read()) {
      assert(performance.now() < end, "no log file stored at the quit");
      await sleep(30);
    }
    assertEqual([bytes.at(-2), bytes.at(-1)], [13, 10], "the last line's end");
    for (let i = 0; i < bytes.length; i++) if (bytes[i] === 10) assertEqual(bytes[i - 1], 13, `a bare line feed at ${i}`);
    assert((await volya.call("game_log_status")).present, "not reported as stored");
    return `${bytes.length} bytes`;
  },

  "the game's log reports the memory in use and the renderer's resources (stat_memory, phase cmem, dump_resources)": async () => {
    needsGame();
    const dir = await (await (await navigator.storage.getDirectory()).getDirectoryHandle("volya-engine")).getDirectoryHandle("logs");
    const read = () => dir.getFileHandle("xray_player.log").then((f) => f.getFile()).then((f) => f.text()).catch(() => "");
    const before = await read();
    // A game with a New Game's load, quit from the title menu: its log.
    // Launched after the last game's quit released the renderer's
    // resources, made again here.
    await launchToMenu();
    await enterLevel();
    await quitToTitle();
    key("KeyQ");
    await until((s) => s.phase === "none", "the game to end");
    for (const end = performance.now() + 5000; ; await sleep(30)) {
      const text = await read();
      if (text !== before && text.includes("* RM_Dump: v_shaders")) break;
      assert(performance.now() < end, "no log file stored at the quit");
    }
    // The figures' lines are ASCII; the captions' CP1251 is not read here.
    const lines = new TextDecoder("latin1").decode(await (await (await dir.getFileHandle("xray_player.log")).getFile()).arrayBuffer()).split("\r\n");
    const figure = (line, name) => Number(line.match(new RegExp(`${name}\\[(-?\\d+) K\\]`))?.[1]);
    // The last load that finished: its end's phase pair, then
    // `stat_memory` (console-diagnostics.md 2.4); its phases from its
    // first `* phase time: 0 ms`.
    const end = lines.findLastIndex((l, i) => l.startsWith("* [win32]: ") && lines[i - 1]?.startsWith("* phase cmem: "));
    assert(end > 0, "no load finished in the stored log");
    const start = lines.slice(0, end).findLastIndex((l) => l === "* phase time: 0 ms");
    const cmem = lines.slice(start, end).filter((l) => l.startsWith("* phase cmem: ")).map((l) => Number(l.slice(14, -2)));
    assert(cmem.length >= 2 && cmem.every((k) => k > 0), `phase cmem: ${cmem}`);
    const [win32, d3d, xray] = lines.slice(end, end + 3);
    assert(figure(win32, "committed") > 0 && figure(win32, "free") > 0, win32);
    assert(figure(d3d, "textures") > 0, d3d);
    assert(figure(xray, "crt heap") > 0, xray);
    // The quit's resource counts (console-diagnostics.md 2.5): after the
    // renderer's resources were released, none left, so none listed by
    // name (as in every measured run of the original).
    const from = lines.findLastIndex((l) => l.startsWith("* RM_Dump: textures  : "));
    const dump = lines.slice(from, lines.findIndex((l, i) => i > from && l.startsWith("* RM_Dump: v_shaders : ")) + 1);
    assertEqual(dump.length, 13, `the quit's lines: ${dump.join("; ")}`);
    assert(dump.every((l) => l.startsWith("* RM_Dump: ") && l.endsWith(": 0")), `left at the quit: ${dump.join("; ")}`);
    return `phase cmem (K) ${cmem.join(", ")}; ${win32}; ${d3d}; ${xray}; at the quit ${dump.map((l) => l.slice(11).replace(/ +:/, ":")).join(", ")}`;
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
    for (const key of ["renderer", "vid_mode", "r__supersample", "_preset", "r__detail_density", "r1_dlights", "rs_c_gamma", "rs_vis_distance"]) {
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
    // The GPU's times (#355): the submission to the GPU done; what was
    // drawn at.
    const session = mine.find((e) => e.name === "session").args;
    if (session.gpu_state === "ready") {
      assert(series("gpu.done_ms max") > 10, `gpu.done_ms: ${series("gpu.done_ms max")}`);
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
    const worst = Math.max(...long.map((e) => e.dur)) / 1000;
    // The log's rate while a game runs: this session's bytes over its time.
    const seconds = (mine.at(-1).ts - mine.find((e) => e.ts > 0).ts) / 1e6;
    const bytes = mine.reduce((sum, e) => sum + JSON.stringify(e).length + 2, 0);
    const gpu = `GPU submitted to done ${means("gpu.done_ms")} ms (means of periods)`;
    return `${mine.length} events, ${(bytes / seconds).toFixed(0)} B/s over ${seconds.toFixed(0)} s; ${long.length} long frames (worst ${worst.toFixed(0)} ms); ${gpu}; audio ${audio ? "running" : volya.audioState()}`;
  },

  ...keptTeardown(),
};
