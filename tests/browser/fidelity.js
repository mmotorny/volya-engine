// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Our frames of the views in tests/fidelity/views.rs against the original's
// (Bazel's capture actions, //tests/fidelity:frames, served as this
// suite's references by //tests/browser:fidelity_chrome): per view the pixels that differ, the mean and largest
// difference, and a heat map. A report with thresholds, not a gate: the
// original runs on Wine's Direct3D 9, which isn't pixel-exact. Kept frames
// (VOLYA_DUMP_DIR): `<view>` ours, `<view>-original`, `<view>-heat`.

import { dump } from "./harness.js";
import { gameChecks, reference } from "./render-game.js";

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

export const tests = await gameChecks("fidelity", compare);
