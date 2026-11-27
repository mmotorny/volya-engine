// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Our frames of the views in tests/fidelity/views.rs against the original's
// (`//tests/fidelity:frames`, from the local cache, served as this suite's
// references): per view the pixels that differ, the mean and largest
// difference, and a heat map. A report with thresholds, not a gate: the
// original's frames are its Direct3D 9 driver's, which ours need not match
// pixel for pixel (docs/testing.md). Kept frames
// (VOLYA_DUMP_DIR): `<view>` ours, `<view>-original`, `<view>-heat`.
//
// The guard (#561): our check starts from the configuration file the
// original's run started from (`<view>.user.ltx`), and the file our console
// then writes must equal byte for byte the one the original rewrote when it
// quit (`<view>.saved.ltx`). Otherwise the run is invalid: the test fails
// with the first difference and no pixel is compared. Kept files: ours
// `<view>.saved.ltx`, the original's `<view>-original.saved.ltx`.

import { dump, dumpFile } from "./harness.js";
import { gameChecks, reference, referenceFile } from "./render-game.js";

/** A pixel differs when a channel differs by more than this. */
const TOLERANCE = 16;
/** Within thresholds: at most this share of pixels differ ... */
const DIFFERING_SHARE = 0.05;
/** ... and the mean difference (largest channel, 0 – 255) is at most this. */
const MEAN = 8;

/** The heat map of `d` (per pixel, the largest channel's difference) over
 * our frame: matching pixels show ours dimmed, differing ones from yellow
 * (just over the tolerance) to red (128 or more). */
function heatMap(width, height, ours, d) {
  const rgba = new Uint8Array(width * height * 4);
  for (let i = 0; i < d.length; i++) {
    const o = i * 4;
    if (d[i] <= TOLERANCE) {
      const luma = (ours[o] * 0.3 + ours[o + 1] * 0.59 + ours[o + 2] * 0.11) * 0.35;
      rgba.set([luma, luma, luma], o);
    } else {
      const t = Math.min(1, (d[i] - TOLERANCE) / (128 - TOLERANCE));
      rgba.set([255, 255 * (1 - t), 0], o);
    }
    rgba[o + 3] = 255;
  }
  return rgba;
}

async function compare(frame) {
  const original = await reference("fidelity", frame.name);
  if (!original) return "no frame of the original (//tests/fidelity:frames)";
  if (original.width !== frame.width || original.height !== frame.height) {
    return `the original's frame is ${original.width}x${original.height}, ours ${frame.width}x${frame.height}`;
  }
  const { width, height, rgba } = frame;
  const d = new Uint8Array(width * height);
  let sum = 0;
  let max = 0;
  let differ = 0;
  for (let i = 0; i < d.length; i++) {
    const o = i * 4;
    const v = Math.max(
      Math.abs(rgba[o] - original.rgba[o]),
      Math.abs(rgba[o + 1] - original.rgba[o + 1]),
      Math.abs(rgba[o + 2] - original.rgba[o + 2]),
    );
    d[i] = v;
    sum += v;
    if (v > max) max = v;
    if (v > TOLERANCE) differ++;
  }
  const opaque = new Uint8Array(original.rgba);
  for (let i = 3; i < opaque.length; i += 4) opaque[i] = 255;
  await dump(`${frame.name}-original`, { width, height, rgba: opaque });
  await dump(`${frame.name}-heat`, { width, height, rgba: heatMap(width, height, rgba, d) });
  const share = differ / d.length;
  const mean = sum / d.length;
  const within = share <= DIFFERING_SHARE && mean <= MEAN;
  return (
    `against the original: ${differ} pixels (${(100 * share).toFixed(1)} %) differ by more than ${TOLERANCE}, ` +
    `mean ${mean.toFixed(1)}, max ${max}: ${within ? "within" : "beyond"} thresholds ` +
    `(${100 * DIFFERING_SHARE} %, mean ${MEAN})`
  );
}

/** The view's configuration file to start from, as bytes. */
async function args(view) {
  const user = await referenceFile("fidelity", `${view}.user.ltx`);
  if (!user) throw new Error(`run invalid: no ${view}.user.ltx of the original's run`);
  return { user_config: Array.from(user) };
}

/** Line `n` (from 1) of `bytes` for a report. */
function line(bytes, n) {
  const lines = new TextDecoder("latin1").decode(bytes).split("\n");
  return JSON.stringify(lines[n - 1] ?? "(end of file)");
}

/** Throws unless our rewritten configuration file equals the original's. */
async function verify(view, files) {
  const original = await referenceFile("fidelity", `${view}.saved.ltx`);
  const ours = files[`${view}.saved.ltx`];
  if (ours) await dumpFile(`${view}.saved.ltx`, ours);
  if (original) await dumpFile(`${view}-original.saved.ltx`, original);
  if (!original) throw new Error(`run invalid: no ${view}.saved.ltx of the original's run`);
  if (!ours) throw new Error("run invalid: our check wrote no configuration file");
  const length = Math.min(original.length, ours.length);
  let at = 0;
  while (at < length && original[at] === ours[at]) at++;
  if (at === original.length && at === ours.length) return;
  let n = 1;
  for (let i = 0; i < at; i++) if (original[i] === 10) n++;
  throw new Error(
    `run invalid, no pixel compared: the configuration files differ (original ${original.length} bytes, ` +
      `ours ${ours.length}), first at byte ${at}, line ${n}: original ${line(original, n)}, ours ${line(ours, n)}`,
  );
}

export const tests = await gameChecks("fidelity", compare, { args, verify });
