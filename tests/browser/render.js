// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Rendering in the engine worker: startup with WebGPU, else WebGL 2 (#113),
// else without graphics; the frame loop on the transferred canvas, and the
// path from an imported archive through DDS (or Theora) to the screen, read
// back through the renderer (`debug_capture`), not the DOM.

import { assert, assertEqual, skip } from "./harness.js";

const ARCHIVE = "gamedata.dbb";
const UNCOMPRESSED = "gamedata/textures/internal/internal_light_attpoint.dds";
const BC = "gamedata/textures/flare/flare_02.dds";
const VIDEO = "gamedata/textures/intro/intro_gsc-wp.ogm";
let installer;
let gpu;

function installerBase() {
  const path = location.pathname;
  return `${path.slice(0, path.indexOf("/__test/"))}/__test/installer`;
}

const source = (name) => ({
  url: `${installerBase()}/${encodeURIComponent(name)}`,
  size: installer.find((f) => f.name === name).size,
});

/** The RGBA texels of an uncompressed DDS file's top level (public format). */
function decodeMasked(bytes) {
  const view = new DataView(new Uint8Array(bytes).buffer);
  const u32 = (offset) => view.getUint32(offset, true);
  const [height, width, flags, bits] = [u32(12), u32(16), u32(80), u32(88)];
  assert((flags & 0x4) === 0 && bits === 32, `not 32-bit uncompressed: flags ${flags}, ${bits} bits`);
  const masks = [92, 96, 100, 104].map(u32);
  if ((flags & 0x1) === 0) masks[3] = 0;
  const texels = [];
  for (let i = 0; i < width * height; i++) {
    const value = u32(128 + 4 * i);
    texels.push(
      masks.map((mask, c) => {
        if (mask === 0) return c === 3 ? 255 : 0;
        const shift = 31 - Math.clz32(mask & -mask);
        return ((value & mask) >>> shift) * 255 / (mask >>> shift);
      }),
    );
  }
  return { width, height, texels };
}

/** Fails unless graphics are up (WebGPU, or WebGL 2 where there's no WebGPU adapter). */
function needsGpu() {
  assertEqual(gpu.state, "ready", `graphics: ${gpu.message}`);
}

async function capture(width, height) {
  const { rgba } = await volya.call("debug_capture", { width, height });
  return rgba;
}

export const tests = {
  "app starts with a clear graphics status": async () => {
    gpu = (await volya.status()).gpu;
    const line = document.getElementById("status").textContent;
    if (gpu.state === "ready") {
      assert(!line.includes("unavailable"), `status line: ${line}`);
      return `${gpu.backend}, ${gpu.format}, ${gpu.size.join("×")}, textures ${gpu.textures}`;
    }
    assertEqual(gpu.state, "unavailable", "state");
    assert(gpu.message && line.includes(`graphics unavailable: ${gpu.message}`), `status line: ${line}`);
    return gpu.message;
  },

  "renderer runs in the engine worker": async () => {
    needsGpu();
    // The canvas covers the viewport in device pixels.
    // The page reports its size after startup; read the status anew until
    // the worker has applied it.
    // The game is drawn in landscape: an upright phone's canvas is turned.
    const css = innerWidth >= innerHeight ? [innerWidth, innerHeight] : [innerHeight, innerWidth];
    const expected = css.map((v) => Math.round(v * devicePixelRatio));
    let size;
    for (let i = 0; i < 40; i++) {
      size = (await volya.status()).gpu.size;
      if (size.join() === expected.join()) break;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    assertEqual(size, expected, "canvas size");
  },

  "empty draw list renders black": async () => {
    needsGpu();
    await volya.call("debug_draw", {});
    const rgba = await capture(8, 6);
    assert(rgba.every((v, i) => v === (i % 4 === 3 ? 255 : 0)), `pixels ${rgba.slice(0, 8)}`);
  },

  "imports an archive with textures": async () => {
    const response = await fetch(`${installerBase()}/index.json`);
    if (response.status === 404) skip("VOLYA_GOG_INSTALLER is unset");
    if (gpu.state !== "ready") skip("no graphics: nothing to draw");
    installer = await response.json();
    const exe = installer.find((f) => f.name.endsWith(".exe")).name;
    const bin = installer.find((f) => f.name.endsWith("-1.bin")).name;
    await volya.call("forget_game");
    await volya.importGame({ exe: source(exe), bin: source(bin), only: [ARCHIVE] });
  },

  "draws an uncompressed game texture exactly": async () => {
    needsGpu();
    if (!installer) skip("no installer");
    const head = await volya.call("read_head", { path: UNCOMPRESSED, max: 1 << 20 });
    const { width, height, texels } = decodeMasked(head.bytes);
    const info = await volya.call("debug_draw", { path: UNCOMPRESSED });
    assertEqual([info.width, info.height, info.format], [width, height, "32-bit"], "texture");
    // Full screen at the texture's size: one pixel per texel, alpha-blended
    // over black.
    const rgba = await capture(width, height);
    let worst = 0;
    texels.forEach((t, i) => {
      const expected = [0, 1, 2].map((c) => (t[c] * t[3]) / 255).concat(255);
      expected.forEach((e, c) => (worst = Math.max(worst, Math.abs(rgba[4 * i + c] - e))));
    });
    assert(worst <= 1.5, `largest difference ${worst}`);
    return `${width}×${height}, largest difference ${worst.toFixed(2)}`;
  },

  "draws a BC game texture (as stored, transcoded to ETC2 or decoded)": async () => {
    needsGpu();
    if (!installer) skip("no installer");
    const info = await volya.call("debug_draw", { path: BC });
    assertEqual(info.format, "Bc1", "format");
    const rgba = await capture(info.width, info.height);
    const distinct = new Set();
    for (let i = 0; i < rgba.length; i += 4) distinct.add(rgba.slice(i, i + 3).join());
    assert(distinct.size > 4, `${distinct.size} distinct colours`);
    return `${gpu.textures}: ${distinct.size} distinct colours`;
  },

  "plays a game video as a texture": async () => {
    needsGpu();
    if (!installer) skip("no installer");
    const info = await volya.call("debug_draw", { path: VIDEO });
    assertEqual(
      info.texture,
      [info.width, info.height].map((n) => 2 ** Math.ceil(Math.log2(n))),
      "power-of-two texture",
    );
    // Frames advance on the frame loop: the picture changes.
    await new Promise((resolve) => setTimeout(resolve, 200));
    const first = await capture(64, 48);
    let changed = false;
    for (let i = 0; i < 50 && !changed; i++) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      const next = await capture(64, 48);
      changed = next.some((v, j) => v !== first[j]);
    }
    assert(changed, "video frames do not advance");
    return `${info.width}×${info.height} at ${info.fps.toFixed(2)} fps`;
  },

  // docs/behavior/particle-drawing.md 4.5 – 7 through this browser's
  // backend (WebGL 2 in the iOS Simulator: no storage buffers, no base
  // vertex): the scenes of the native render test.
  "draws particle effects: back to front, the SET alpha test, culling, distortion": async () => {
    needsGpu();
    const draw = async (name, size) => {
      const info = await volya.call("debug_draw_particles", { case: name });
      const rgba = await capture(size, size);
      return { compat: info.compat, at: (x, y) => Array.from(rgba.slice(4 * (y * size + x), 4 * (y * size + x) + 3)) };
    };
    const near = (got, want, what) => assert(got.every((v, i) => Math.abs(v - want[i]) <= 3), `${what}: ${got}, expected ${want}`);
    const blend = await draw("blend", 64);
    // Red at half over black, then green at half over that.
    near(blend.at(32, 32), [64, 128, 0], "green over red");
    const set = await draw("set", 64);
    assertEqual(set.at(28, 32), [0, 0, 0], "alpha 199 is cut");
    assertEqual(set.at(36, 32), [255, 255, 255], "alpha 202 passes");
    // A white top left quarter: the scene composed the right way up.
    const plain = await draw("plain", 256);
    assertEqual([plain.at(124, 64), plain.at(124, 192)], [[255, 255, 255], [0, 0, 0]], "the top left quarter");
    // The distortion texel's red of 1 reads the scene (1 − 0.5) · 0.05 of
    // the width to the right: about 6 pixels. A frame drawn straight to the
    // screen doesn't show its distortion; the one after goes through the
    // target, presented with postprocess_d (frame-presentation.md 3.1),
    // at the renderer's own colour base (254/255).
    await draw("distort", 256);
    const rgba = await capture(256, 256);
    const shifted = { at: (x, y) => Array.from(rgba.slice(4 * (y * 256 + x), 4 * (y * 256 + x) + 3)) };
    assertEqual(shifted.at(124, 64), [0, 0, 0], "shifted");
    near(shifted.at(112, 64), [254, 254, 254], "still white");
    assertEqual(shifted.at(112, 192), [0, 0, 0], "the right way up");
    await volya.call("debug_draw", {});
    return `${gpu.backend}${blend.compat ? ", compat path" : ""}`;
  },

  "draws dynamic light passes and glows": async () => {
    needsGpu();
    const draw = async (name) => {
      const info = await volya.call("debug_draw_lights", { case: name });
      const rgba = await capture(16, 16);
      return { compat: info.compat, centre: Array.from(rgba.slice(4 * (8 * 16 + 8), 4 * (8 * 16 + 8) + 3)) };
    };
    const near = (got, want, what) => assert(got.every((v, i) => Math.abs(v - want[i]) <= 3), `${what}: ${got}, expected ${want}`);
    const none = await draw("none");
    near(none.centre, [0, 0, 0], "unlit");
    // 2 · B · c with white attenuation textures: (¼, ⅛, 0) doubled.
    near((await draw("point")).centre, [128, 64, 0], "a point light pass");
    near((await draw("spot")).centre, [128, 64, 0], "a spot light pass");
    near((await draw("glow")).centre, [64, 64, 64], "a glow in front");
    near((await draw("hidden")).centre, [0, 0, 0], "a glow behind the triangle");
    await volya.call("debug_draw", {});
    return `${gpu.backend}${none.compat ? ", compat path" : ""}`;
  },

  "clears and forgets the game": async () => {
    if (gpu.state === "ready") await volya.call("debug_draw", {});
    if (installer) await volya.call("forget_game");
  },
};
