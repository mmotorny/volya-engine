// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Our frames of the views in tests/fidelity/views.rs against the original's
// (`//tests/fidelity:frames`, from the local cache, served as this suite's
// references): per view the pixels that differ, the mean and largest
// difference, and a heat map (`againstOriginal`). A report with
// thresholds, not a gate: the original's frames are its Direct3D 9
// driver's, which ours need not match pixel for pixel (docs/testing.md).
// Kept frames
// (VOLYA_DUMP_DIR): `<view>` ours, `<view>-original`, `<view>-heat`.
//
// The guard (#561): our check starts from the configuration file the
// original's run started from (`<view>.user.ltx`), at its size, and the
// file our console then writes is compared line by line with the one the
// original rewrote when it quit (`<view>.saved.ltx`). Every differing line
// is reported; the tracked ones (`KNOWN`) pass, any other fails the test
// once its pixels are compared too. Kept files: ours `<view>.saved.ltx`,
// the original's `<view>-original.saved.ltx`.

import { dumpFile } from "./harness.js";
import { againstOriginal, gameChecks, referenceFile } from "./render-game.js";

/** The view's configuration file to start from, as bytes. */
async function args(view) {
  const user = await referenceFile("fidelity", `${view}.user.ltx`);
  if (!user) throw new Error(`run invalid: no ${view}.user.ltx of the original's run`);
  return { user_config: Array.from(user) };
}

/**
 * The tracked differences of the rewritten configuration files, each a
 * line naming `command` on the `side` alone, changing no pixel: reported,
 * not failing. An entry whose difference is gone fails, to be removed.
 */
const KNOWN = [
  // The computer name: ours empty and never saved until multiplayer (the
  // user's decision, #618).
  { command: "mm_net_srv_name", side: "original", issue: 618 },
  { command: "load_last_save", side: "original", issue: 777 },
];

/** A configuration file's lines, without their line ends. */
function lines(bytes) {
  return new TextDecoder("latin1")
    .decode(bytes)
    .split("\n")
    .map((l) => l.replace(/\r$/, ""))
    .filter((l) => l !== "");
}

/** The lines of `a` that `b` lacks, counting repeats. */
function missing(a, b) {
  const counts = new Map();
  for (const l of b) counts.set(l, (counts.get(l) ?? 0) + 1);
  return a.filter((l) => {
    const n = counts.get(l) ?? 0;
    if (n) counts.set(l, n - 1);
    return !n;
  });
}

/**
 * Compares our rewritten configuration file with the original's line by
 * line: `{ note, failure }`, the known differences and every other (null
 * when none). Throws when either file is missing: nothing to compare.
 */
async function verify(view, files) {
  const original = await referenceFile("fidelity", `${view}.saved.ltx`);
  const ours = files[`${view}.saved.ltx`];
  if (ours) await dumpFile(`${view}.saved.ltx`, ours);
  if (original) await dumpFile(`${view}-original.saved.ltx`, original);
  if (!original) throw new Error(`run invalid: no ${view}.saved.ltx of the original's run`);
  if (!ours) throw new Error("run invalid: our check wrote no configuration file");
  const theirs = lines(original);
  const mine = lines(ours);
  const diffs = [
    ...missing(theirs, mine).map((line) => ({ side: "original", line })),
    ...missing(mine, theirs).map((line) => ({ side: "ours", line })),
  ];
  const tracked = (d) => KNOWN.find((k) => k.command === d.line.split(" ")[0] && k.side === d.side);
  const known = [];
  const real = [];
  for (const d of diffs) {
    const k = tracked(d);
    if (k) known.push(`${d.side} alone ${JSON.stringify(d.line)} (#${k.issue})`);
    else real.push(`${d.side} alone ${JSON.stringify(d.line)}`);
  }
  for (const k of KNOWN) {
    if (!diffs.some((d) => tracked(d) === k)) {
      real.push(`no ${k.command} line on the ${k.side}'s side alone any more (#${k.issue}): remove it from KNOWN`);
    }
  }
  const sameBytes = original.length === ours.length && original.every((b, i) => b === ours[i]);
  if (!diffs.length && !sameBytes) {
    real.push(`the same lines in other bytes (original ${original.length} bytes, ours ${ours.length})`);
  }
  return {
    note: known.length ? `configuration files' known differences: ${known.join(", ")}` : "",
    failure: real.length ? `the configuration files differ: ${real.join(", ")}` : null,
  };
}

export const tests = await gameChecks("fidelity", (frame) => againstOriginal("fidelity", frame), { args, verify });
