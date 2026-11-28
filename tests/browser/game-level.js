// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// A New Game's level, from the kept import's title menu: the loading screen,
// the start view, its objects, lights and intro, the placed effect, the
// collision world's rays, the zones' idle effects, frame and GPU times at a
// few video modes (`level_stats`), transcoding costs where textures are
// transcoded; on touch devices the ergonomic mode's upright
// game at the screen's own size (#806), its frames kept; the trader's
// locked door stopping the player, its hint shown.

import { assert, assertEqual, dump, skip } from "./harness.js";
import {
  ORIGINAL,
  byCaption,
  captionMs,
  coarse,
  doorSound,
  enterLevel,
  followed,
  glCalls,
  quitToTitle,
  readRender,
  render,
  setTouchMode,
  turned,
  upright,
  view,
  gpu,
  heapMaps,
  levelFrames,
  heapRecord,
  jsMemory,
  keptSetup,
  keptTeardown,
  key,
  loadMs,
  mib,
  needsGame,
  noteHeap,
  againstOriginal,
  hNow,
  rNow,
  pointAt,
  sleep,
  state,
  until,
} from "./game.js";

export const tests = {
  ...keptSetup(),

  "a difficulty starts a new game: the level's start view is drawn": async () => {
    needsGame();
    await pointAt(0);
    key("Enter");
    await until((s) => s.menu.list === "menu_new_game", "the difficulty page");
    await noteHeap("before the New Game");
    const menuH = await hNow();
    await volya.call("heap_log", { on: true });
    await heapRecord(true);
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
    const log = await volya.call("heap_log", { on: false });
    const stages = byCaption(log);
    const record = await heapRecord(false);
    assertEqual([s.level.name, s.level.draws], ["l01_escape", 7782], "level");
    // The level's objects (docs/behavior/offline-simulation.md 9.1,
    // docs/behavior/object-visual-and-transform.md 5.1).
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
    const original = await againstOriginal();
    let note = `${original}; h in the title menu before ${menuH} K, the original's ${ORIGINAL.titleMenuH} K; ${heapMaps.join("; ")}; wasm heap by caption (MiB, heap/live): ${stages}; ${record}; "${loading.loading.caption}" seen; first caption to first level frame in ${s.load_ms.toFixed(0)} ms; time by caption (ms): ${captionMs(log)}; ${levelFrames(log)}; intro skipped, wasm heap ${mib(before.wasm_heap_bytes)} before it, ${mib(playing.wasm_heap_bytes)} while it played, the audio worker's ${mib(playing.audio_heap_bytes)}; at the start ${lights.passes} light passes, ${lights.sprites} glows drawn`;
    if (gpu.state === "ready") {
      const { rgba } = await volya.call("debug_capture", { width: 256, height: 192 });
      const colours = new Set();
      for (let i = 0; i < rgba.length; i += 4) colours.add((rgba[i] << 16) | (rgba[i + 1] << 8) | rgba[i + 2]);
      assert(colours.size > 200, `${colours.size} colours`);
      note += `, ${colours.size} colours`;
    }
    const stats = await volya.call("level_stats");
    if (gpu.textures === "etc2") assertEqual(stats.textures.textures > 300, true, `textures prepared: ${JSON.stringify(stats.textures)}`);
    return `textures ${gpu.textures}; ${note}; ${await jsMemory(stats)}; ${JSON.stringify(stats)}`;
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

  "the collision world's rays at the player's start; their cost in wasm": async () => {
    needsGame();
    const b = await volya.call("debug_rays_bench", { count: 2000 });
    assert(b.short.nearest_hits > 0 && b.long.any_hits > 0, JSON.stringify(b));
    const f = (r) => `ray_nearest ${r.nearest_us.toFixed(2)} µs (${r.nearest_hits} hit), ray_any ${r.any_us.toFixed(2)} µs (${r.any_hits} hit)`;
    return `${b.rays} rays each: short (1 mm – 0.5 m) ${f(b.short)}; long (5 – 100 m) ${f(b.long)}; collision world ${(b.index_bytes / 1e6).toFixed(1)} MB`;
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
    // sizes. The diagnostic log is on, so the GPU's done times are kept.
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
        // The video mode's targets (`gpu_owners`).
        const t = s.gpu_owners?.targets;
        const targets = t ? `, targets ${t.textures} textures ${mib(t.texture_bytes)}` : "";
        rows.push(`${width}×${height}: interval ${ms(s.frame_interval_ms)}, CPU ${ms(s.frame_cpu_ms)}, done ${ms(s.gpu_done_ms)}${targets}`);
        assert(s.gpu_done_ms > 0, `no GPU done time at ${width}×${height}`);
      }
      rows.push(`1024×768: ${await glCalls()}`);
      // At 1024×768: r__supersample 1 instead of 2, and the grass left
      // undrawn (#415).
      for (const [what, change] of [["r__supersample 1", { supersample: 1 }], ["no grass", { supersample: 2, details: false }]]) {
        await volya.call("debug_render", change);
        await sleep(1000 + 60 * 34);
        const s = await volya.call("level_stats");
        const ms = (v) => (v == null ? "–" : v.toFixed(1));
        rows.push(`1024×768, ${what}: interval ${ms(s.frame_interval_ms)}, CPU ${ms(s.frame_cpu_ms)}, done ${ms(s.gpu_done_ms)}`);
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
    return `ms per frame: ${rows.join("; ")} (before: done ${before.gpu_done_ms?.toFixed(1)})${commands}; ${await rNow()}`;
  },

  "the diagnostic log's cost per frame: off and on in turn, its GPU fence and its writes": async () => {
    needsGame();
    await until((s) => s.phase === "level" && !s.intro, "the level");
    // #669: the latest 60 frames' averages after a second to settle, with
    // the log off and on in turn (each turn off deletes it; back on, a new
    // one starts).
    const rounds = 4;
    const sums = { off: { cpu: 0, interval: 0 }, on: { cpu: 0, interval: 0 } };
    try {
      for (let i = 0; i < rounds; i++) {
        for (const on of [false, true]) {
          await volya.diagnostics.setEnabled(on, volya.call);
          await sleep(1000 + 60 * 34);
          const s = await volya.call("level_stats");
          const side = sums[on ? "on" : "off"];
          side.cpu += s.frame_cpu_ms / rounds;
          side.interval += s.frame_interval_ms / rounds;
        }
      }
    } finally {
      await volya.diagnostics.setEnabled(true, volya.call);
    }
    // The last turn on: its per-second counters, the fence's poll and the
    // write to OPFS among them.
    const { text } = await volya.call("diagnostic_log");
    const events = JSON.parse(text).traceEvents;
    const pid = events.findLast((e) => e.name === "session").pid;
    const mean = (key) => {
      const values = events.filter((e) => e.pid === pid && e.ph === "C" && key in e.args).map((e) => e.args[key]);
      return values.length ? (values.reduce((a, b) => a + b, 0) / values.length).toFixed(3) : "none";
    };
    const ms = (v) => v.toFixed(2);
    const { off, on } = sums;
    return (
      `ms per frame, off: CPU ${ms(off.cpu)}, interval ${ms(off.interval)}; on: CPU ${ms(on.cpu)}, interval ${ms(on.interval)} ` +
      `(CPU +${ms(on.cpu - off.cpu)}); on, per second: render.poll mean ${mean("render.poll mean")} ms, ` +
      `gpu.done_ms mean ${mean("gpu.done_ms mean")}, diagnostics.write_ms mean ${mean("diagnostics.write_ms mean")} max ${mean("diagnostics.write_ms max")}`
    );
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

  "touch, ergonomic mode (#806): the game upright at the screen's own size, which a rotation changes; the title menu, a New Game's level, its inventory and PDA drawn at it; Standard turns it again": async () => {
    needsGame();
    if (!coarse()) skip("touch devices only: the setting applies there");
    await until((s) => s.phase === "level" && !s.intro && !s.menu, "the level");
    const portrait = innerHeight > innerWidth;
    const pixels = () => [view().clientWidth, view().clientHeight].map((css) => Math.round(css * devicePixelRatio));
    const frames = [];
    // A frame at the size drawn, kept for inspection (VOLYA_DUMP_DIR).
    const frame = async (name) => {
      const [width, height] = render;
      const capture = await volya.call("debug_capture", { width, height });
      for (let i = 3; i < capture.rgba.length; i += 4) capture.rgba[i] = 255;
      await dump(`experimental-${name}`, capture);
      frames.push(`${name} ${width}×${height}`);
    };
    // The video mode Standard drew at, given back when it is chosen again.
    await readRender();
    const own = render;
    await setTouchMode("experimental");
    assert(upright() && !turned(view()), "the canvas upright");
    assertEqual(render, pixels(), "drawn at the canvas's own size");
    if (portrait) assert(render[1] > render[0], `drawn upright: ${render}`);
    // The level begun in Standard, its inventory and PDA laid out then.
    await frame("level-switched");
    // A rotation, as the canvas's size changing: followed each way.
    const style = view().style.cssText;
    Object.assign(view().style, { inset: "0 auto auto 0", width: `${innerHeight}px`, height: `${innerWidth}px` });
    const turnedTo = await followed("the video mode to follow the rotated canvas", pixels());
    await readRender();
    assertEqual(turnedTo, pixels(), "the rotated size");
    assert((turnedTo[0] > turnedTo[1]) === portrait, `drawn across: ${turnedTo}`);
    await frame("level-rotated");
    view().style.cssText = style;
    const back = await followed("the video mode to follow the canvas back", pixels());
    await readRender();
    assertEqual(back, pixels(), "back at the canvas's size");
    // A New Game in the ergonomic mode: its interface laid out at the upright size.
    await quitToTitle();
    await readRender();
    await sleep(500);
    await frame("title-menu");
    await enterLevel();
    await sleep(1000);
    await frame("level");
    key("KeyI");
    await until((s) => s.inventory, "the inventory screen");
    await sleep(500);
    await frame("inventory");
    key("Escape");
    await until((s) => s.phase === "level" && !s.inventory, "the inventory closed");
    key("KeyP");
    await until((s) => s.pda, "the PDA");
    await sleep(500);
    await frame("pda");
    key("Escape");
    await until((s) => s.phase === "level" && !s.pda, "the PDA closed");
    await setTouchMode("standard");
    assertEqual(render, own, "Standard: its own video mode again");
    if (portrait) {
      assert(!upright() && turned(view()), "Standard: the canvas turned");
      assert(render[0] > render[1], `Standard: drawn landscape: ${render}`);
    }
    return frames.join(", ");
  },

  "the trader's door: turned to it from the start, running forward stops at it and the HUD shows the door-locked hint; Use plays the servomotor at it and leaves it locked": async () => {
    needsGame();
    let s = await until((s) => s.phase === "level" && !s.intro && !s.menu, "the level's controls");
    // The locked door (its scheme `ph_door@locked` holds its hinge with
    // the motor 10000 / 1, script API Y 11), 1.7 m from the start,
    // is behind the start view: turned half round with the right arrow (π
    // rad/s), the run meets it within a metre and a half.
    const door = s.level.doors.find((d) => d.name === "trader_door");
    assertEqual([door?.motor, door?.tip], [[10000, 1], "tip_door_locked"], "the door's scheme");
    const down = (code) => document.body.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, cancelable: true }));
    const up = (code) => document.body.dispatchEvent(new KeyboardEvent("keyup", { code, bubbles: true, cancelable: true }));
    const yaw = s.level.yaw;
    down("ArrowRight");
    await until((s) => s.level.yaw > yaw + Math.PI - 0.08, "the right arrow to turn round", 3000);
    up("ArrowRight");
    const start = (await until((s) => !s.level.player.moving, "the turn to end")).level.player.position;
    const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
    down("KeyW");
    let p;
    try {
      await until((s) => s.level.player.moving, "W to run");
      // Run until a second's run moves the player less than 5 cm.
      let last = start;
      for (let i = 0; i < 8; i++) {
        await sleep(1000);
        p = (await state()).level.player.position;
        if (flat(p, last) < 0.05) break;
        last = p;
      }
      s = await state();
    } finally {
      up("KeyW");
    }
    const ran = flat(p, start);
    assert(ran > 0.5 && ran < 1.5, `stopped ${ran.toFixed(2)} m from where the run started (the door's face is about 1.1 m away)`);
    assertEqual(s.level.player.hint, "tip_door_locked", "the player's hint");
    // The HUD's hint static draws it (`hud-scope` 13).
    const frame = await volya.call("menu_scene", { width: 1024, height: 768 });
    const glyphs = frame.sprites.filter((g) => g.layer === "hud quick info");
    assert(glyphs.length >= 10, `${glyphs.length} glyphs of the hint`);
    // Use (F, its default key) plays device\door_servomotor at the door
    // (use-action.md 11.1), leaves it locked, and the run still stops at it.
    await doorSound(false);
    key("KeyF");
    const servo = await doorSound();
    s = await state();
    const after = s.level.doors.find((d) => d.name === "trader_door");
    assertEqual([after.motor, after.tip, s.level.player.hint], [[10000, 1], "tip_door_locked", "tip_door_locked"], "the door after Use");
    const near = flat(servo.position, s.level.player.position);
    assert(!servo.two_d && !servo.looped && near < 3, `the sound 3D at the door: ${JSON.stringify(servo)}, ${near.toFixed(2)} m from the player`);
    down("KeyW");
    try {
      await sleep(1000);
      s = await state();
    } finally {
      up("KeyW");
    }
    const on = flat(s.level.player.position, start);
    assert(on < 1.5, `ran ${on.toFixed(2)} m on after Use: not through the door`);
    return `stopped ${ran.toFixed(2)} m on, the hint drawn in ${glyphs.length} glyphs; still locked after Use`;
  },

  "the physics world: made with the level, stepped 100 times a second of play": async () => {
    needsGame();
    // docs/behavior/physics-world.md 1.1, 2.2: made under
    // st_client_spawning, whole steps of h = 0.01 s from the display frame
    // time (smoothed, at most 0.1 s a frame), on the engine clock.
    const first = await until((s) => s.phase === "level" && !s.intro && !s.menu, "the level");
    assertEqual(Math.fround(first.level.physics.step), Math.fround(0.01), "the step");
    await sleep(1000);
    const s = await state();
    const ms = s.clock - first.clock;
    const steps = s.level.physics.steps - first.level.physics.steps;
    // Within a frame's smoothing of the clock's 100 a second.
    assert(steps > 0 && Math.abs(steps - ms / 10) <= 10, `${steps} steps in ${ms} ms of the engine clock`);
    const p = s.level.physics;
    return `${steps} steps in ${ms} ms of the engine clock; ${p.iterations} iterations; the floor at y ${p.floor.toFixed(2)} m`;
  },

  ...keptTeardown(),
};
