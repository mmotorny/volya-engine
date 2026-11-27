// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// What the render suites on game data share (`render-screens`,
// `render-level`, `draw-calls`): the whole game imported (once, in
// Chrome's kept profile: what is imported already is skipped), then the
// suite's render checks (`volya_engine::selftest`, through
// `render_game_selftest`), each a test, its kept frames dumped
// (`VOLYA_DUMP_DIR`) and those it asks for compared with an earlier run's
// (`VOLYA_COMPARE_DIR`), or handed to the suite's own comparison
// (`fidelity.js`).

import { dump, keepsDumps, skip } from "./harness.js";

function testBase() {
  const path = location.pathname;
  return path.slice(0, path.indexOf("/__test/") + "/__test/".length);
}

let comparing = null;
/** Whether the dev server serves an earlier run's frames. */
function comparesFrames() {
  comparing ??= fetch(`${testBase()}reference/`).then(
    (r) => r.status === 204,
    () => false,
  );
  return comparing;
}

/** The earlier run's RGBA of frame `name` of `suite`, or null. */
export async function reference(suite, name) {
  const response = await fetch(`${testBase()}reference/${suite}/${name}.png`);
  if (!response.ok) return null;
  const options = { colorSpaceConversion: "none", premultiplyAlpha: "none" };
  const bitmap = await createImageBitmap(await response.blob(), options);
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  const context = canvas.getContext("2d");
  context.drawImage(bitmap, 0, 0);
  const { data } = context.getImageData(0, 0, bitmap.width, bitmap.height);
  return { width: bitmap.width, height: bitmap.height, rgba: data };
}

/** The bytes of file `name` (with its extension) the suite is served
 * beside its reference frames, or null. */
export async function referenceFile(suite, name) {
  const response = await fetch(`${testBase()}reference/${suite}/${name}`);
  if (!response.ok) return null;
  return new Uint8Array(await response.arrayBuffer());
}

/** The bytes of base64 `text`. */
function bytesOf(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Pixels of `rgba` whose colour differs from `before`'s. */
function differingPixels(frame, rgba, before) {
  if (before.width !== frame.width || before.height !== frame.height) return frame.width * frame.height;
  let differ = 0;
  for (let i = 0; i < rgba.length; i += 4) {
    if (rgba[i] !== before.rgba[i] || rgba[i + 1] !== before.rgba[i + 1] || rgba[i + 2] !== before.rgba[i + 2]) differ++;
  }
  return differ;
}

let imported = false;

/**
 * The tests of render suite `suite`. With `compare`, every check keeps its
 * frames and each is handed to `compare(frame)` (`{ name, width, height,
 * rgba }`, opaque), whose note joins the test's. `args(check)` gives a
 * check's further arguments; `verify(check, files)` sees the files it kept
 * (by name, bytes) before any frame is compared and gives `{ note,
 * failure }`: a note that joins the test's, and a failure (or null) that
 * fails it once its frames are compared; it throws when they make the run
 * invalid, before any frame.
 */
export async function gameChecks(suite, compare = null, { args = null, verify = null } = {}) {
  const checks = await volya.call("render_game_selftest", { suite });
  const tests = {
    "imports the game": async () => {
      const response = await fetch(`${testBase()}installer/index.json`);
      if (response.status === 404) skip("VOLYA_GOG_INSTALLER is unset");
      if ((await volya.status()).gpu.state !== "ready") skip("no graphics: nothing to draw");
      const installer = await response.json();
      const source = (name) => ({
        url: `${testBase()}installer/${encodeURIComponent(name)}`,
        size: installer.find((f) => f.name === name).size,
      });
      const exe = installer.find((f) => f.name.endsWith(".exe")).name;
      const bin = installer.find((f) => f.name.endsWith("-1.bin")).name;
      const started = performance.now();
      await volya.importGame({ exe: source(exe), bin: source(bin) });
      imported = true;
      return `${((performance.now() - started) / 1000).toFixed(1)} s`;
    },
  };
  for (const check of checks) {
    tests[check] = async () => {
      if (!imported) skip("the game isn't imported");
      const comparing = await comparesFrames();
      const frames = Boolean(compare) || comparing || (await keepsDumps());
      const extra = args ? await args(check) : {};
      const result = await volya.call("render_game_selftest", { ...extra, check, frames });
      let verdict = { note: "", failure: null };
      if (verify && !result.skip) {
        const files = Object.fromEntries((result.files ?? []).map((f) => [f.name, bytesOf(f.base64)]));
        verdict = await verify(check, files);
      }
      const compared = [];
      const differing = [];
      const notes = verdict.note ? [verdict.note] : [];
      for (const frame of result.frames ?? []) {
        const rgba = bytesOf(frame.rgba_base64);
        // Opaque, as the canvas shows it (its alpha mode is opaque): a 2D
        // canvas keeps colours premultiplied, so a translucent pixel's
        // colour would not survive the PNG's round trip (#495).
        for (let i = 3; i < rgba.length; i += 4) rgba[i] = 255;
        await dump(frame.name, { width: frame.width, height: frame.height, rgba });
        if (compare) {
          notes.push(await compare({ name: frame.name, width: frame.width, height: frame.height, rgba }));
          continue;
        }
        // The parity suite holds every frame (#478, #495) but those whose
        // check says they don't repeat; the others, those their checks ask
        // for.
        const held = frame.compare ?? suite === "parity";
        if (!comparing || (frame.compare == null && suite !== "parity")) continue;
        const before = await reference(suite, frame.name);
        if (!before) continue;
        const differ = differingPixels(frame, rgba, before);
        compared.push(`${frame.name} ${differ}`);
        if (held && differ) differing.push(`${frame.name}: ${differ} pixels differ`);
      }
      if (differing.length) throw new Error(`against the earlier run's frames: ${differing.join("; ")}`);
      if (verdict.failure) throw new Error([verdict.failure, ...notes].join("; "));
      if (result.skip) skip(result.skip);
      if (notes.length) return [result.note, ...notes].join("; ");
      if (!compared.length) return result.note;
      return `${result.note}; pixels differing from the earlier run's: ${compared.join(", ")}`;
    };
  }
  return tests;
}
