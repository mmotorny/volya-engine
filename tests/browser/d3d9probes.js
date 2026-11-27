// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The Direct3D 9 probes' scenes (tests/d3d9probes/) replayed through our GL
// layer (`d3d9probes_replay`: volya-engine-render's d3d9probes.rs) against
// each reference's frames (`//tests/d3d9probes:references`, served as this
// suite's references: Wine's results, then each GPU's committed under
// tests/d3d9probes/hardware/), each reference's own `scenes.bin` replayed:
// per probe and case the pixels differing in any channel (RGBA) and the
// largest difference. A report, not a gate: a test fails only when a case
// can't be replayed or compared. The PNGs are decoded here, as stored (no
// canvas, which would premultiply). Kept frames (VOLYA_DUMP_DIR):
// `<reference>-<probe>-<case>` the reference's, `<reference>-<probe>-<case>-ours`
// ours, both made opaque.

import { dump } from "./harness.js";

/** The probes, in the order the runner draws them (probe.c). */
const PROBES = ["texel_centres", "raster", "bilinear", "mip", "aniso", "bc", "alpha_test", "blend", "fog", "gamma"];

function base() {
  const path = location.pathname;
  return `${path.slice(0, path.indexOf("/__test/"))}/__test/reference/d3d9probes/`;
}

/** The bytes of the reference file at `path`, or null. */
async function file(path) {
  const response = await fetch(`${base()}${path}`);
  return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
}

/** An 8-bit RGBA PNG's pixels, rows from the top (the PNG specification:
 * zlib data, filters 0 – 4, no interlacing). */
async function decodePng(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  if (signature.some((b, i) => bytes[i] !== b)) throw new Error("not a PNG");
  let width = 0;
  let height = 0;
  const data = [];
  for (let at = 8; at + 8 <= bytes.length; ) {
    const length = view.getUint32(at);
    const type = String.fromCharCode(...bytes.subarray(at + 4, at + 8));
    const body = bytes.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      width = view.getUint32(at + 8);
      height = view.getUint32(at + 12);
      const [depth, colour, , , interlace] = body.subarray(8, 13);
      if (depth !== 8 || colour !== 6 || interlace !== 0) {
        throw new Error(`PNG of depth ${depth}, colour type ${colour}, interlace ${interlace}: not 8-bit RGBA`);
      }
    } else if (type === "IDAT") {
      data.push(body);
    } else if (type === "IEND") {
      break;
    }
    at += 12 + length;
  }
  const stream = new Blob(data).stream().pipeThrough(new DecompressionStream("deflate"));
  const raw = new Uint8Array(await new Response(stream).arrayBuffer());
  const stride = width * 4;
  if (raw.length < (stride + 1) * height) throw new Error(`PNG data of ${raw.length} bytes for ${width}×${height}`);
  const rgba = new Uint8Array(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const row = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const out = y * stride;
    for (let i = 0; i < stride; i++) {
      const a = i >= 4 ? rgba[out + i - 4] : 0;
      const b = y > 0 ? rgba[out - stride + i] : 0;
      const c = i >= 4 && y > 0 ? rgba[out - stride + i - 4] : 0;
      let predicted = 0;
      if (filter === 1) predicted = a;
      else if (filter === 2) predicted = b;
      else if (filter === 3) predicted = (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
        predicted = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      } else if (filter !== 0) throw new Error(`PNG filter ${filter}`);
      rgba[out + i] = (row[i] + predicted) & 0xff;
    }
  }
  return { width, height, rgba };
}

function base64Of(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

function bytesOf(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

const replays = new Map();

/** Our frames of `reference`'s scenes, by probe: `[{case, width, height,
 * rgba | error}]`. */
function replay(reference) {
  if (!replays.has(reference)) {
    replays.set(
      reference,
      (async () => {
        const scenes = await file(`${reference}/scenes.bin`);
        if (!scenes) throw new Error(`${reference} has no scenes.bin`);
        const frames = await volya.call("d3d9probes_replay", { scenes_base64: base64Of(scenes) });
        const byProbe = new Map();
        for (const f of frames) {
          if (!byProbe.has(f.probe)) byProbe.set(f.probe, []);
          byProbe.get(f.probe).push({ ...f, rgba: f.rgba_base64 && bytesOf(f.rgba_base64) });
        }
        return byProbe;
      })(),
    );
  }
  return replays.get(reference);
}

const opaque = (rgba) => rgba.map((v, i) => (i % 4 === 3 ? 255 : v));

/** How `ours` differs from `theirs` (same size): the pixels differing in
 * any channel, the largest difference and the first differing pixel. */
function difference(width, ours, theirs) {
  let differ = 0;
  let max = 0;
  let first = null;
  for (let i = 0; i < ours.length; i += 4) {
    let d = 0;
    for (let c = 0; c < 4; c++) d = Math.max(d, Math.abs(ours[i + c] - theirs[i + c]));
    if (!d) continue;
    differ++;
    max = Math.max(max, d);
    if (!first) {
      const p = i / 4;
      first = `(${p % width}, ${Math.floor(p / width)}) ours ${ours.subarray(i, i + 4).join(" ")}, theirs ${theirs.subarray(i, i + 4).join(" ")}`;
    }
  }
  return { differ, max, first };
}

/** Compares `probe`'s cases with `reference`'s frames: the report's note. */
async function compareProbe(reference, probe) {
  const cases = (await replay(reference)).get(probe);
  if (!cases) throw new Error(`no case of ${probe} in ${reference}'s scenes.bin`);
  const notes = [];
  const problems = [];
  for (const c of cases) {
    if (c.error) {
      problems.push(`${c.case}: not replayed: ${c.error}`);
      continue;
    }
    const png = await file(`${reference}/${probe}/${c.case}.png`);
    if (!png) {
      problems.push(`${c.case}: ${reference} has no frame`);
      continue;
    }
    const theirs = await decodePng(png);
    if (theirs.width !== c.width || theirs.height !== c.height) {
      problems.push(`${c.case}: ${reference}'s frame is ${theirs.width}×${theirs.height}, the case ${c.width}×${c.height}`);
      continue;
    }
    const name = `${reference}-${probe}-${c.case}`;
    await dump(name, { width: c.width, height: c.height, rgba: opaque(theirs.rgba) });
    await dump(`${name}-ours`, { width: c.width, height: c.height, rgba: opaque(c.rgba) });
    const d = difference(c.width, c.rgba, theirs.rgba);
    const total = c.width * c.height;
    notes.push(
      d.differ ? `${c.case}: ${d.differ} of ${total} differ, max ${d.max}, first ${d.first}` : `${c.case}: 0 of ${total} differ`,
    );
  }
  if (problems.length) throw new Error([...problems, ...notes].join("; "));
  return notes.join("; ");
}

const index = await file("references.json");
const references = index ? JSON.parse(new TextDecoder().decode(index)) : [];

export const tests = {
  "references are served": async () => {
    if (!references.length) throw new Error("no references.json (//tests/d3d9probes:references)");
    return references.join(", ");
  },
};
for (const reference of references) {
  tests[`${reference}: its scenes replay, each probe compared`] = async () => {
    const probes = [...(await replay(reference)).keys()];
    const unknown = probes.filter((p) => !PROBES.includes(p));
    if (unknown.length) throw new Error(`probes this suite doesn't list: ${unknown.join(", ")}`);
    return `${probes.length} probes`;
  };
  for (const probe of PROBES) {
    tests[`${probe} against ${reference}`] = () => compareProbe(reference, probe);
  }
}
