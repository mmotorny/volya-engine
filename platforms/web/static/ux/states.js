// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The app states shown on the UX audit page: each is what the fake engine
// and updates report, and which screen to show. Grouped into the board's
// sections. Devices: desktop first, for balance.

const GAME = 5.56 * 2 ** 30;
const imported = { imported: true, written: 3020, total: 3020, bytes: GAME, in_use: false };
const fresh = { imported: false, written: 0, total: 0, bytes: 0, in_use: false };

export const SECTIONS = [
  {
    title: "Import",
    states: [
      { id: "import", name: "First start", game: fresh },
      { id: "import-pick", name: "Picked the wrong files", game: fresh, snapshot: { error: "pick" } },
      { id: "import-not-installer", name: "Not the game's installer", game: fresh, snapshot: { error: "Inno Setup headers: unexpected signature" } },
      {
        id: "import-storage",
        name: "Not enough storage",
        game: fresh,
        snapshot: { error: "Not enough storage: the game needs 5.56 GiB, 3.10 GiB is available." },
      },
      {
        id: "import-progress",
        name: "Importing",
        game: fresh,
        snapshot: { done: 2.31 * 2 ** 30, total: GAME, path: "gamedata.db5" },
      },
      {
        id: "import-damaged",
        name: "Damaged installer files",
        game: fresh,
        snapshot: { done: 2.31 * 2 ** 30, total: GAME, path: "gamedata.db5", error: "gamedata.db5: MD5 mismatch" },
      },
      { id: "import-resume", name: "Interrupted import", game: { ...fresh, written: 1210, bytes: 2.31 * 2 ** 30, total_bytes: GAME } },
    ],
  },
  {
    title: "Home",
    states: [
      { id: "home", name: "Ready", game: imported },
      { id: "home-not-kept", name: "Storage not kept (browser tab)", game: imported, persisted: false },
      { id: "home-no-webgpu", name: "No WebGPU", game: imported, gpu: false },
      { id: "home-play-failed", name: "Play failed", game: imported, launchError: "level.geom: unexpected end of file" },
      { id: "home-updating", name: "Updating (automatic)", game: imported, updates: { status: "downloading", bytes: 1.9 * 2 ** 20 } },
      { id: "home-updated", name: "Just updated", game: imported, updates: { updatedFrom: "a1b2c3d" } },
      {
        id: "home-offer",
        name: "Update available (automatic off)",
        game: imported,
        updates: { auto: false, status: "available", bytes: 1.9 * 2 ** 20 },
      },
      { id: "home-ready-offer", name: "Update downloaded (automatic off)", game: imported, updates: { auto: false, status: "ready" } },
    ],
  },
  {
    title: "Settings",
    states: [
      { id: "settings", name: "Up to date", game: imported, screen: "settings" },
      {
        id: "settings-offer",
        name: "Automatic off, update available",
        game: imported,
        screen: "settings",
        updates: { auto: false, status: "available", bytes: 1.9 * 2 ** 20 },
      },
      { id: "settings-downloading", name: "Downloading", game: imported, screen: "settings", updates: { status: "downloading" } },
      { id: "settings-not-kept", name: "Storage not kept", game: imported, screen: "settings", persisted: false },
      { id: "settings-no-game", name: "No game imported", game: fresh, screen: "settings" },
    ],
  },
  {
    title: "Problems",
    states: [
      { id: "in-use", name: "Open in another tab", game: { ...imported, in_use: true } },
      { id: "damaged", name: "Import can't be used", game: new Error("game.json is missing") },
      { id: "start-failed", name: "Engine failed to start", startError: "the wasm module could not be compiled" },
    ],
  },
];

export const STATES = SECTIONS.flatMap((section) => section.states);

/** Devices: CSS size, safe-area insets, and how input works. */
export const DEVICES = [
  { id: "desktop", name: "Desktop Chrome", width: 1280, height: 800, insets: { top: 0, right: 0, bottom: 0, left: 0 }, touch: false },
  { id: "iphone", name: "iPhone 17 Pro Max", width: 440, height: 956, insets: { top: 62, right: 0, bottom: 34, left: 0 }, touch: true },
  { id: "iphone-landscape", name: "iPhone 17 Pro Max · landscape", width: 956, height: 440, insets: { top: 0, right: 62, bottom: 21, left: 62 }, touch: true },
];
