// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Our audio of the walks in tests/fidelity/walks.rs against the original's
// (#838; `//tests/fidelity:audio`, from the local cache, served as this
// suite's references): each walk's check
// (`volya_engine::selftest::audio_fidelity`) renders ours offline in step
// with the walk's frames and, given the original's capture (`<walk>.wav`,
// `<walk>.frames.txt`), compares their onsets step by step: frame,
// material, level. Its frames where the player starts and ends
// (`<walk>-start`, `<walk>-end`) are compared with the capture's as the
// fidelity suite compares its views (`againstOriginal`): whether both
// players moved the same way. A report, not a gate. Kept
// (VOLYA_DUMP_DIR): ours `<walk>.wav` and `<walk>.frames.txt`,
// `<walk>.report.txt`, the frames, the original's and their heat maps.

import { dumpFile } from "./harness.js";
import { againstOriginal, gameChecks, referenceFile } from "./render-game.js";

/** `bytes` as base64, a piece at a time. */
function base64(bytes) {
  let text = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    text += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  }
  return btoa(text);
}

/** The original's capture of the walk, where the cache has it. */
async function args(walk) {
  const references = {};
  for (const name of [`${walk}.wav`, `${walk}.frames.txt`]) {
    const bytes = await referenceFile("audio-fidelity", name);
    if (bytes) references[name] = base64(bytes);
  }
  return { references };
}

/** Our recording, its frames and the report, kept, with or without the
 * original's. */
async function kept(walk, files) {
  for (const [name, bytes] of Object.entries(files)) await dumpFile(name, bytes);
}

export const tests = await gameChecks("audio-fidelity", (frame) => againstOriginal("audio-fidelity", frame), {
  args,
  kept,
});
