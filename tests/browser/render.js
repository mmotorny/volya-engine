// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Rendering in the engine worker: startup with or without WebGPU, the frame
// loop on the transferred canvas, and the path from an imported archive
// through DDS (or Theora) to the screen, read back through the renderer
// (`debug_capture`), not the DOM.

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

/** Fails unless WebGPU is up (the iOS Simulator has none: a known failure). */
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
      return `${gpu.format}, ${gpu.size.join("×")}, BC ${gpu.bc ? "yes" : "no"}`;
    }
    assertEqual(gpu.state, "unavailable", "state");
    assert(gpu.message && line.includes(`graphics unavailable: ${gpu.message}`), `status line: ${line}`);
    return gpu.message;
  },

  "WebGPU renderer runs in the engine worker": async () => {
    needsGpu();
    // The canvas covers the viewport in device pixels.
    const expected = [innerWidth, innerHeight].map((css) => Math.round(css * devicePixelRatio));
    assertEqual(gpu.size, expected, "canvas size");
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
    if (gpu.state !== "ready") skip("no WebGPU: nothing to draw");
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

  "draws a BC game texture as stored": async () => {
    needsGpu();
    if (!installer) skip("no installer");
    if (!gpu.bc) {
      const error = await volya.call("debug_draw", { path: BC }).then(() => null, (e) => e.message);
      assert(error?.includes("texture-compression-bc"), `expected a clear error, got ${error}`);
      return "no BC support: clear error";
    }
    const info = await volya.call("debug_draw", { path: BC });
    assertEqual(info.format, "Bc1", "format");
    const rgba = await capture(info.width, info.height);
    const distinct = new Set();
    for (let i = 0; i < rgba.length; i += 4) distinct.add(rgba.slice(i, i + 3).join());
    assert(distinct.size > 4, `${distinct.size} distinct colours`);
    return `${distinct.size} distinct colours`;
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

  "clears and forgets the game": async () => {
    if (gpu.state === "ready") await volya.call("debug_draw", {});
    if (installer) await volya.call("forget_game");
  },
};
