// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Import and browse: the real GOG installer (BYOA, served locally by the dev
// server from VOLYA_GOG_INSTALLER) goes through the same import path as
// picked files, limited to two small archives to stay fast.

import { assert, assertEqual, skip } from "./harness.js";

const ONLY = ["gamedata.dbc", "gamedata.dbd"];
let installer;

async function source(name) {
  const file = installer.find((f) => f.name === name);
  return { url: `${installerBase()}/${encodeURIComponent(name)}`, size: file.size };
}

function installerBase() {
  const path = location.pathname;
  return `${path.slice(0, path.indexOf("/__test/"))}/__test/installer`;
}

export const tests = {
  "installer is available": async () => {
    const response = await fetch(`${installerBase()}/index.json`);
    if (response.status === 404) skip("VOLYA_GOG_INSTALLER is unset");
    installer = await response.json();
    assert(installer.some((f) => f.name.endsWith(".exe")), "no .exe in the installer directory");
  },

  "imports archives into OPFS": async () => {
    if (!installer) skip("no installer");
    const exe = installer.find((f) => f.name.endsWith(".exe")).name;
    const bin = installer.find((f) => f.name.endsWith("-1.bin")).name;
    await volya.call("forget_game");
    const result = await volya.importGame({ exe: await source(exe), bin: await source(bin), only: ONLY });
    assert(result.plan.files.length >= 14, `planned ${result.plan.files.length} files`);
    assertEqual(result.status.imported, true, "imported");
    assertEqual(result.status.written, ONLY.length, "files written");
    return `planned ${result.plan.files.length} files, persisted: ${result.persisted}`;
  },

  "browses the game tree": async () => {
    if (!installer) skip("no installer");
    const root = await volya.call("read_dir", { path: "" });
    assert(root.some((e) => e.name === "gamedata" && e.dir), `root: ${JSON.stringify(root)}`);
    const config = await volya.call("read_dir", { path: "gamedata/config" });
    assert(config.length > 0, "empty gamedata/config");
  },

  "reads a game file": async () => {
    if (!installer) skip("no installer");
    const head = await volya.call("read_head", { path: "gamedata/config/system.ltx", max: 256 });
    assert(head.size > 0 && head.bytes.length > 0, "empty system.ltx");
  },

  "forgets the game": async () => {
    if (!installer) skip("no installer");
    await volya.call("forget_game");
    assertEqual((await volya.call("game_status")).imported, false, "imported after forget");
  },
};
