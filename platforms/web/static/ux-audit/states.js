// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The app states shown on the UX audit page: each is what the fake engine
// and updates report, and which screen to show. Grouped into the board's
// sections. Devices: desktop first, for balance.

const GAME = 5.56 * 2 ** 30;
const imported = { imported: true, written: 3020, total: 3020, bytes: GAME };
const fresh = { imported: false, written: 0, total: 0, bytes: 0 };
const TOUCH = ["iphone", "iphone-landscape"];

export const SECTIONS = [
  {
    title: "Import",
    states: [
      { id: "import", name: "First start", game: fresh },
      { id: "import-tab", name: "First start in a browser tab", game: fresh, tab: true, persisted: false },
      { id: "import-deleted", name: "The browser deleted the game files", game: fresh, tab: true, persisted: false, deleted: true },
      { id: "import-no-graphics", name: "WebGL 2 not working (before or after import)", game: fresh, gpu: false },
      { id: "import-not-isolated", name: "Not cross-origin isolated after the one reload (no shared memory for sound)", game: fresh, isolated: false },
      { id: "import-not-isolated-no-graphics", name: "Not isolated, and no WebGL 2", game: fresh, isolated: false, gpu: false },
      { id: "import-pick", name: "Picked the wrong files", game: fresh, snapshot: { error: "pick" } },
      { id: "import-not-installer", name: "Not the game's installer", game: fresh, snapshot: { error: "Inno Setup headers: unexpected signature" } },
      {
        id: "import-storage",
        name: "Not enough storage",
        game: fresh,
        snapshot: { error: "Not enough storage: the game files need 5.56 GiB, and this browser allows Volya Engine 3.10 GiB more." },
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
      {
        id: "import-record-damaged",
        name: "Game files damaged (their record unreadable)",
        game: { ...fresh, problem: "import.json: expected value at line 1 column 1" },
      },
      { id: "import-resume", name: "Interrupted import", game: { ...fresh, written: 1210, bytes: 2.31 * 2 ** 30, total_bytes: GAME } },
    ],
  },
  {
    title: "Home",
    states: [
      { id: "home", name: "Ready", game: imported },
      { id: "home-starting", name: "Starting the game (Play pressed)", game: imported, starting: true },
      { id: "home-play-failed", name: "Play failed", game: imported, launchError: "level.geom: unexpected end of file" },
      // An update, in order: offered (automatic downloads off), downloading
      // on request or after Check for updates found it, downloaded (either way).
      {
        id: "home-offer",
        name: "Update available (automatic downloads off)",
        game: imported,
        updates: { auto: false, status: "available", bytes: 1.9 * 2 ** 20 },
      },
      {
        id: "home-offer-downloading",
        name: "Downloading the update on request",
        game: imported,
        updates: { auto: false, status: "downloading", bytes: 1.9 * 2 ** 20 },
      },
      {
        id: "home-checked-downloading",
        name: "Downloading an update found by Check for updates",
        game: imported,
        updates: { status: "downloading", requested: true, bytes: 1.9 * 2 ** 20 },
      },
      { id: "home-finish", name: "Update downloaded", game: imported, updates: { status: "ready" } },
    ],
  },
  // In the level, over a plain backdrop (the board can't show the game's
  // own frames): on desktop the full screen offer, on touch devices the
  // touch controls (approximations of the game's textures: the board has no
  // game data).
  {
    title: "In the level",
    note: "Touch controls: approximations; the app draws them from the textures in the game files.",
    states: [
      { id: "level-fullscreen-offer", name: "Full screen offered (desktop Chrome)", level: { offer: true }, devices: ["desktop"] },
      { id: "level-touch", name: "Idle", level: {}, devices: TOUCH },
      { id: "level-touch-faded", name: "Idle a while: faded", level: { fade: true }, devices: TOUCH },
      { id: "level-touch-moving", name: "Moving with the stick", level: { stick: [0.45, -0.85] }, devices: TOUCH },
      { id: "level-touch-jump", name: "Jump held", level: { down: "jump" }, devices: TOUCH },
      { id: "level-touch-inventory", name: "Inventory held", level: { down: "inventory" }, devices: TOUCH },
      { id: "level-touch-pda", name: "PDA held", level: { down: "pda" }, devices: TOUCH },
      {
        id: "level-touch-pda-open",
        name: "PDA open (the game draws it): PDA alone, closing it; elsewhere the touchpad",
        level: { level: false, pda: true },
        devices: TOUCH,
      },
      {
        id: "level-touch-past-rim",
        name: "Past the rim: sprinting, the lock target shown",
        level: { stick: [0.1, -1.5], sprint: true },
        devices: TOUCH,
      },
      {
        id: "level-touch-on-target",
        name: "On the lock target",
        level: { stick: "target", sprint: true },
        devices: TOUCH,
      },
      {
        id: "level-touch-past-rim-high",
        name: "Past the rim with the thumb high: the lock target closer",
        level: { stick: "target", sprint: true, thumbY: 150 },
        devices: TOUCH,
      },
      {
        id: "level-touch-past-rim-top",
        name: "Past the rim with the thumb at the very top: the lock target beside it",
        level: { stick: "target", sprint: true, thumbY: 20 },
        devices: TOUCH,
      },
      { id: "level-touch-locked", name: "Sprint locked", level: { lock: true, sprint: true }, devices: TOUCH },
      { id: "level-touch-crouched", name: "Crouched", level: { crouch: true }, devices: TOUCH },
      { id: "level-touch-crouch-held", name: "Crouched, Crouch held", level: { down: "crouch", crouch: true }, devices: TOUCH },
      {
        id: "level-touch-low",
        name: "Crouch swiped down: low crouch",
        level: { crouch: true, low: true },
        devices: TOUCH,
      },
      { id: "level-touch-torch", name: "Torch on", level: { torch: true }, devices: TOUCH },
      {
        id: "level-touch-night-vision",
        name: "Wearing an outfit with night vision: Night vision shown",
        level: { nightVision: false },
        devices: TOUCH,
      },
      { id: "level-touch-night-vision-on", name: "Night vision on", level: { nightVision: true }, devices: TOUCH },
      {
        id: "level-touch-no-torch",
        name: "Torch moved to the backpack: Torch hidden",
        level: { torch: null },
        devices: TOUCH,
      },
    ],
  },
  // The console on touch screens (#642): the game draws the console and
  // the main menu, the system the device's keyboard and its Paste bubble;
  // the board shows stand-ins for them.
  {
    title: "Console (touch)",
    note: "Stand-ins for what the game draws (the console, the main menu) and for the system's keyboard (with its form bar, which a page can't hide) and Paste bubble.",
    states: [
      { id: "console-level", name: "Level: Console beside Menu", level: {}, devices: TOUCH },
      { id: "console-menu", name: "Main menu: Console at the top left", console: { menu: true }, devices: TOUCH },
      { id: "console-typing", name: "Open over the level, the keyboard up, a line typed: one compact row of keys on it (landscape: the picture lifted, the prompt just above the keys)", console: { keyboard: true }, devices: TOUCH },
      { id: "console-key-held", name: "A strip key held", console: { keyboard: true, down: "up" }, devices: TOUCH },
      {
        id: "console-paste",
        name: "Paste pressed: the system's Paste bubble",
        console: { keyboard: true, paste: true },
        devices: TOUCH,
      },
      { id: "console-keyboard-hidden", name: "Keyboard hidden: the console only", console: {}, devices: TOUCH },
    ],
  },
  {
    title: "Settings",
    states: [
      { id: "settings", name: "Game files imported", game: imported, screen: "settings" },
      { id: "settings-checking", name: "Check for updates pressed: checking", game: imported, screen: "settings", check: "checking" },
      { id: "settings-up-to-date", name: "Checked: up to date (for a few seconds)", game: imported, screen: "settings", check: "none" },
      { id: "settings-check-offline", name: "Checked while offline (for a few seconds)", game: imported, screen: "settings", check: "offline" },
      {
        id: "settings-check-failed",
        name: "Check failed, not known to be offline (for a few seconds)",
        game: imported,
        screen: "settings",
        check: "failed",
      },
      // The logs (#646, #669): the game's log and the performance log,
      // always kept (the default states have both; before the game has
      // run, only the performance log, as with no game files below).
      {
        id: "settings-log-game-only",
        name: "Only the game's log kept (the performance log empty, just started anew)",
        game: imported,
        screen: "settings",
        log: { entries: false },
      },
      {
        id: "settings-log-preparing",
        name: "Download logs pressed: preparing the file",
        game: imported,
        screen: "settings",
        log: { download: "preparing" },
      },
      {
        id: "settings-log-failed",
        name: "Download failed: the logs couldn't be read (for a few seconds)",
        game: imported,
        screen: "settings",
        log: { download: "failed" },
      },
      // Danger zone: the game's saves and settings (#535), and the logs
      // (#669, #681).
      {
        id: "settings-delete-user-data",
        name: "Delete saves, settings, and logs pressed: confirming",
        game: imported,
        screen: "settings",
        press: ["Delete saves, settings, and logs"],
      },
      {
        id: "settings-user-data-deleted",
        name: "Saves, settings, and logs deleted",
        game: imported,
        screen: "settings",
        press: ["Delete saves, settings, and logs", "Delete"],
      },
      {
        id: "settings-user-data-not-deleted",
        name: "Delete pressed, but the performance log couldn't be deleted",
        game: imported,
        screen: "settings",
        log: { deleteFails: true },
        press: ["Delete saves, settings, and logs", "Delete"],
      },
      // Danger zone: the game files (#538).
      {
        id: "settings-delete-game",
        name: "Delete game files pressed: confirming",
        game: imported,
        screen: "settings",
        press: ["Delete game files"],
      },
      {
        id: "settings-delete-game-failed",
        name: "Delete pressed, but the game files couldn't be deleted",
        game: imported,
        screen: "settings",
        deleteGameFails: true,
        press: ["Delete game files", "Delete"],
      },
      {
        id: "settings-delete-game-no-logs",
        name: "No logs kept: Delete game files pressed, confirming",
        game: imported,
        screen: "settings",
        gameLog: false,
        log: { entries: false },
        press: ["Delete game files"],
      },
      { id: "settings-no-user-data", name: "No settings or saves stored", game: imported, screen: "settings", userData: false },
      {
        id: "settings-delete-game-no-user-data",
        name: "No settings or saves stored: Delete game files pressed, confirming",
        game: imported,
        screen: "settings",
        userData: false,
        press: ["Delete game files"],
      },
      { id: "settings-no-game", name: "No game files, settings or saves stored", game: fresh, screen: "settings", gameLog: false },
      { id: "settings-nothing-stored", name: "No game files, nothing stored", game: fresh, screen: "settings", userData: false, gameLog: false },
    ],
  },
  {
    title: "Problems",
    states: [
      { id: "start-failed", name: "Volya Engine couldn't start", startError: "the wasm module could not be compiled" },
    ],
  },
];

export const STATES = SECTIONS.flatMap((section) => section.states);

/** Devices: CSS size, safe-area insets, how input works, and whether the
 * board starts with it unticked (`hidden`). */
export const DEVICES = [
  { id: "desktop", name: "Desktop Chrome", width: 1280, height: 800, insets: { top: 0, right: 0, bottom: 0, left: 0 }, touch: false },
  { id: "iphone", name: "iPhone 17 Pro Max", width: 440, height: 956, insets: { top: 62, right: 0, bottom: 34, left: 0 }, touch: true },
  { id: "iphone-landscape", name: "iPhone 17 Pro Max · landscape", width: 956, height: 440, insets: { top: 0, right: 62, bottom: 21, left: 62 }, touch: true, hidden: true },
];
