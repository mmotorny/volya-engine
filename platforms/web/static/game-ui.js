// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Game UI: importing the game from a GOG installer and launching it. Plain
// DOM with native buttons, so touch, mouse and keyboard all work.
//
// The app always opens here; the game starts from the Play button. That tap
// also lets the browser play sound (it stays silent until the first user
// gesture), and keeps this screen, where the engine's settings live, in
// view. While the game runs the engine takes the screen and its input, and
// this interface hides until the player quits (the Q key or the title
// menu's last item).
//
// Home holds Play only; secondary actions (an update offer, Settings) sit at
// the bottom, away from it. Settings holds updates, storage and, in a danger
// zone, removing the import.

import { BUILD, versionText } from "./update.js";
import { SHARE_ICON, canOffer, chromium, install, installed, platform } from "./install.js";


let call;
let updates = null;
const root = () => document.getElementById("game");
/** Replaces the game section's content, skipping absent (null) parts. */
const show = (...children) => {
  delete document.documentElement.dataset.screen;
  root().replaceChildren(...children.filter((c) => c !== null && c !== undefined));
};

/**
 * Moves to another screen, sliding forward (deeper, e.g. into Settings) or
 * back; instant where the browser can't animate it or the player asked for
 * reduced motion.
 */
function navigate(direction, render) {
  // A new screen starts at its top, not where the last one was scrolled to.
  const go = async () => {
    await render();
    scrollTo(0, 0);
  };
  if (!document.startViewTransition || matchMedia("(prefers-reduced-motion: reduce)").matches) return go();
  document.documentElement.dataset.nav = direction;
  const transition = document.startViewTransition(go);
  // A quick second tap skips this animation; the screen still changes.
  transition.ready.catch(() => {});
  return transition.finished.finally(() => delete document.documentElement.dataset.nav);
}

const formatBytes = (n) =>
  n >= 2 ** 30 ? `${(n / 2 ** 30).toFixed(2)} GiB` : n >= 2 ** 20 ? `${(n / 2 ** 20).toFixed(1)} MiB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${n} B`;

function element(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter((c) => c !== null && c !== undefined));
  return node;
}

/** Entry point, called by main.js once the engine runs. */
export async function start(engineCall, updateState = null) {
  call = engineCall;
  updates = updateState;
  globalThis.volya.importGame = importGame;
  globalThis.volya.showSettings = showSettings;
  updates?.onChange(showUpdateStatus);
  showUpdateStatus();
  await refresh();
}

/** The footer's version line, saying what updates are doing. */
function showUpdateStatus() {
  const status = document.getElementById("status");
  if (!status?.dataset.text || status.dataset.state === "error") return;
  if (updates?.status === "downloading") {
    status.textContent = "Updating…";
  } else if (updates?.updatedFrom && !status.dataset.updatedShown) {
    status.dataset.updatedShown = "1";
    status.textContent = `Updated to ${BUILD}`;
    setTimeout(() => {
      if (status.textContent === `Updated to ${BUILD}`) status.textContent = status.dataset.text;
    }, 5000);
  } else if (status.textContent === "Updating…") {
    status.textContent = status.dataset.text;
  }
}

/** The update offer when automatic updates are off (hidden while none). */
function updateOffer() {
  if (!updates) return null;
  const button = element("button", { className: "primary wide" });
  const render = () => {
    const { status, bytes, auto } = updates;
    button.hidden = auto || !["available", "downloading", "ready"].includes(status);
    button.disabled = status === "downloading";
    button.textContent =
      status === "downloading" ? "Downloading update…" : status === "ready" ? "Update now" : `Download update · ${formatBytes(bytes)}`;
  };
  button.addEventListener("click", () => (updates.status === "ready" ? updates.apply() : updates.download()));
  const stop = updates.onChange(() => (button.isConnected ? render() : stop()));
  render();
  return button;
}

/** Secondary actions at the bottom of a screen. */
const bottom = (...children) => element("div", { className: "bottom" }, ...children);

function settingsButton() {
  const button = element("button", { className: "wide", textContent: "Settings" });
  button.addEventListener("click", () => navigate("forward", showSettings));
  return button;
}

/** Whether the engine can draw (without WebGPU the game would run unseen). */
const graphics = async () => (await call("status")).gpu.state === "ready";

/** Shows the home or the import screen. */
async function refresh() {
  root().hidden = false;
  try {
    const status = await call("game_status");
    if (status.in_use) showInUse();
    else if (status.imported) await showHome(status);
    else showImport(status);
  } catch (error) {
    showDamaged(error);
  }
}

/**
 * Imports the game. `exe` and `bin` are the installer's Files (or, in tests,
 * `{ url, size }` sources); `only` optionally limits the paths imported.
 */
export async function importGame({ exe, bin, only }, onProgress = () => {}) {
  // Busy: an update must not reload the page meanwhile (update.js).
  root().dataset.busy = "import";
  try {
    return await runImport({ exe, bin, only }, onProgress);
  } finally {
    delete root().dataset.busy;
  }
}

async function runImport({ exe, bin, only }, onProgress) {
  await call("unmount_game");
  const worker = new Worker(new URL("./import-worker.js", import.meta.url), { type: "module" });
  const request = (message) =>
    new Promise((resolve, reject) => {
      worker.onmessage = ({ data }) => {
        if (data.type === "progress") onProgress(data);
        else if (data.type === "error") reject(new Error(data.message));
        else resolve(data);
      };
      worker.onerror = (event) => {
        event.preventDefault();
        reject(new Error(`import worker: ${event.message ?? "failed to load"}`));
      };
      worker.postMessage(message);
    });
  try {
    const { plan } = await request({ type: "plan", exe });
    const needed = only ? plan.files.filter((f) => only.includes(f.path)).reduce((s, f) => s + f.size, 0) : plan.total;
    const { usage, quota } = await navigator.storage.estimate();
    if (quota - usage < needed) {
      // The browser's allowance for this app, not the disk's free space
      // (browsers don't tell): say so.
      throw new Error(`Not enough storage: the game needs ${formatBytes(needed)}, and this browser allows Volya Engine ${formatBytes(quota - usage)} more.`);
    }
    const persisted = await navigator.storage.persist();
    await request({ type: "run", bin, only });
    return { plan, persisted, status: await call("game_status") };
  } finally {
    worker.terminate();
  }
}

/**
 * What went wrong with an import, for people: a title and what to do. The
 * engine's messages are technical (they name formats and checksums).
 */
function importProblem(message) {
  // Messages name files exactly as the steps do, in their style.
  const again = () => ["Choose ", ...installerFiles(), " again: the import continues where it stopped."];
  if (message === "pick") {
    return { title: "Pick both installer files", text: ["Choose ", ...installerFiles(), " together in the file picker."] };
  }
  if (/^Inno Setup headers|no game archives|not an Inno/i.test(message)) {
    return {
      title: "These aren't the game's installer files",
      text: ["Choose ", ...installerFiles(), " downloaded from GOG."],
    };
  }
  const mismatch = message.match(/^(.*): MD5 mismatch/);
  if (mismatch) {
    return {
      title: "The installer files are damaged",
      text: [
        fileName(mismatch[1]),
        " doesn't match the installer's checksum. Download ",
        ...installerFiles(),
        " from GOG again, then choose them here.",
      ],
    };
  }
  if (message.startsWith("Not enough storage")) {
    return { title: "Not enough storage", text: `${message.replace(/^Not enough storage:\s*/, "").replace(/^./, (c) => c.toUpperCase())} Free some space, then try again.` };
  }
  if (/^installer read/i.test(message)) {
    return { title: "The installer files couldn't be read", text: again() };
  }
  return { title: "The import failed", text: [`The engine reported: ${message}. `, ...again()] };
}

/**
 * `snapshot` shows a moment of an import on the UX page: under way
 * ({ done, total, path }) or failed ({ done, total, error }).
 */
function showImport(status, snapshot = null) {
  const message = element("p", { className: "message", hidden: true });
  const progress = element("progress", { max: 1, value: 0, hidden: true });
  const notice = element("div", { hidden: true });
  // Declaring the installer's types sends iOS straight to the Files picker
  // (no Photos or camera menu); both files stay selectable (checked in the
  // simulator with real taps, `cargo xtask sim`).
  const input = element("input", { type: "file", multiple: true, accept: ".exe,.bin", hidden: true });
  const choose = element("button", { className: "primary wide", textContent: "Choose installer files" });
  const say = (text) => {
    message.textContent = text;
    message.hidden = !text;
  };
  /** A warning (amber) or a problem (red): a title and what to do. */
  const warn = ({ title, text }, kind = "problem") => {
    notice.className = `callout ${kind}`;
    progress.className = kind;
    notice.replaceChildren(element("strong", { textContent: title }), element("span", {}, ...[text].flat()));
    notice.hidden = false;
  };
  /** A failed import: the bar stays where it stopped, with what happened. */
  const fail = (error) => {
    warn(importProblem(error));
    choose.disabled = false;
    choose.textContent = "Choose installer files";
    if (progress.value > 0) say(message.textContent.replace(/ · .*$/, ""));
    else {
      progress.hidden = true;
      say("");
    }
  };
  choose.addEventListener("click", () => input.click());
  input.addEventListener("change", async () => {
    const files = [...input.files];
    const exe = files.find((f) => f.name.toLowerCase().endsWith(".exe"));
    const bin = files.find((f) => f.name.toLowerCase().endsWith(".bin"));
    input.value = "";
    if (!exe || !bin) {
      fail("pick");
      return;
    }
    notice.hidden = true;
    progress.className = "";
    choose.disabled = true;
    progress.hidden = false;
    say("Reading the installer…");
    try {
      await importGame({ exe, bin }, ({ done, total, path }) => {
        progress.value = done / total;
        say(`${formatBytes(done)} of ${formatBytes(total)} · ${path}`);
      });
      await refresh();
    } catch (error) {
      fail(error.message);
    }
  });
  if (snapshot) {
    choose.disabled = true;
    if (snapshot.total) {
      progress.hidden = false;
      progress.value = snapshot.done / snapshot.total;
      say(`${formatBytes(snapshot.done)} of ${formatBytes(snapshot.total)}${snapshot.path ? ` · ${snapshot.path}` : ""}`);
    }
    if (snapshot.error) fail(snapshot.error);
  }
  // On iPhone in a browser tab, an import would stay in the browser's storage,
  // apart from the Home Screen app's: say so before files are chosen.
  const installFirst =
    platform() === "ios" && !installed()
      ? element(
          "div",
          { className: "callout warning" },
          element("strong", { textContent: "Add to your Home Screen first" }),
          element(
            "span",
            {},
            "The Home Screen app keeps the game and has its own storage, so import there: tap ",
            shareIcon(),
            " in the browser, then ",
            element("strong", { textContent: "Add to Home Screen" }),
            ", and open Volya Engine from your Home Screen.",
          ),
        )
      : null;
  // An interrupted import looks like one under way, stopped where it was,
  // with what happened and how to go on.
  if (status.written > 0 && !snapshot) {
    choose.textContent = "Choose installer files to continue";
    if (status.total_bytes) {
      progress.hidden = false;
      progress.value = status.bytes / status.total_bytes;
      say(`${formatBytes(status.bytes)} of ${formatBytes(status.total_bytes)}`);
    } else {
      say(`${formatBytes(status.bytes)} imported`);
    }
    warn(
      {
        title: "The import stopped before it finished",
        text: [
          "This can happen when the app is closed or the device locks during an import. Choose ",
          ...installerFiles(),
          " again: it continues where it stopped.",
        ],
      },
      "warning",
    );
  }
  show(
    element(
      "div",
      { className: "panel" },
      element("p", { className: "label", textContent: "Bring your own game" }),
      element("h2", { textContent: "Import S.T.A.L.K.E.R.: Shadow of Chernobyl" }),
      element("p", { textContent: "Volya Engine runs the game from your own copy, imported from its GOG offline installer." }),
      installFirst,
      element(
        "ol",
        { className: "steps" },
        element(
          "li",
          {},
          "Download both installer files from your ",
          link("https://www.gog.com/en/account", "GOG library"),
          " (the game's offline backup installers): ",
          ...installerFiles(),
          ".",
        ),
        element("li", {
          textContent:
            platform() === "ios"
              ? "Choose both files below. The game is extracted into this app's storage, which needs room for it and for a temporary copy of the installer that iOS makes. Afterwards the installer files are no longer needed."
              : "Choose both files below. The game is extracted into this app's storage, which needs room for it. Afterwards the installer files are no longer needed.",
        }),
      ),
      progress,
      message,
      notice,
      choose,
      input,
    ),
    bottom(updateOffer(), settingsButton()),
  );
}

/** A file's name, as the steps show it. */
const fileName = (name) => element("code", { textContent: name });

/** The two installer files, named as the steps name them. */
const installerFiles = () => [fileName("setup_….exe"), " and ", fileName("setup_…-1.bin")];

/** A link that opens outside the app. */
const link = (href, text) => element("a", { href, textContent: text, target: "_blank", rel: "noopener" });

/** Engine errors start lower-case; sentences in the interface don't. */
const sentence = (text) => text.charAt(0).toUpperCase() + text.slice(1);

/** The game is open in another tab or window, which holds its files. */
function showInUse() {
  const retry = element("button", { className: "primary wide", textContent: "Try again" });
  retry.addEventListener("click", () => location.reload());
  show(
    element(
      "div",
      { className: "panel" },
      element("p", { className: "label", textContent: "Open elsewhere" }),
      element("h2", { textContent: "The game is open in another tab or window" }),
      element("p", { textContent: "Close Volya Engine there, then try again. Your imported game is safe." }),
      retry,
    ),
  );
}

function showDamaged(error) {
  const remove = element("button", { className: "danger wide", textContent: "Remove imported data" });
  remove.addEventListener("click", async () => {
    await call("forget_game");
    await refresh();
  });
  show(
    element(
      "div",
      { className: "panel" },
      element("p", { className: "label", textContent: "Problem" }),
      element("h2", { textContent: "The imported game can't be used" }),
      element("p", { textContent: sentence(error.message) }),
      remove,
    ),
  );
}

async function showHome(status) {
  const note = element("p", { className: "note", hidden: true });
  const problem = element("div", { className: "callout problem", hidden: true });
  const launch = element("button", { className: "primary play", textContent: "Play" });
  launch.addEventListener("click", async () => {
    problem.hidden = true;
    try {
      await call("launch");
    } catch (error) {
      // Where damaged game files show: say what failed and the way out.
      problem.replaceChildren(
        element("strong", { textContent: "The game couldn't start" }),
        element("span", {
          textContent: `The engine reported: ${error.message}. If this keeps happening, the imported files may be damaged: remove the game in Settings, then import it again.`,
        }),
      );
      problem.hidden = false;
    }
  });
  const ready = await graphics();
  if (!ready) {
    launch.disabled = true;
    note.textContent = "The game needs WebGPU, which this browser doesn't offer.";
    note.hidden = false;
  }

  // Only worth saying when it is true: the browser may delete the import.
  const persisted = await navigator.storage.persisted();
  show(
    element(
      "div",
      { className: "panel ready" },
      await cover(),
      element("p", { className: "label", textContent: ready ? "Ready" : "Unavailable here" }),
      element("p", { className: "game-title", textContent: "S.T.A.L.K.E.R.: Shadow of Chernobyl" }),
      launch,
      note,
      problem,
    ),
    persisted ? null : storageWarning(),
    bottom(updateOffer(), settingsButton()),
  );
}

/** The imported game's splash picture (the engine decodes it), or nothing. */
let coverImage = null;
async function cover() {
  try {
    coverImage ??= await call("cover");
  } catch (error) {
    // Decoration only: a verified import always has it, so this is a bug or
    // storage damaged since; Play reports real damage.
    console.warn(`game art: ${error.message}`);
    return null;
  }
  if (!coverImage) return null;
  const { width, height, rgba } = coverImage;
  const bytes = Uint8ClampedArray.from(atob(rgba), (c) => c.charCodeAt(0));
  const canvas = element("canvas", { className: "cover", width, height });
  canvas.getContext("2d").putImageData(new ImageData(bytes, width, height), 0, 0);
  canvas.setAttribute("aria-hidden", "true");
  return canvas;
}

/** The Share symbol, inline in text. */
function shareIcon() {
  const span = element("span", { className: "inline-icon" });
  span.innerHTML = SHARE_ICON;
  return span;
}

/**
 * How to keep the game on this device: install the app (install.js), said
 * only for this device; nothing once installed. On iPhone the Home Screen
 * app has its own storage, so the game is imported again there.
 */
function keepHelp() {
  if (installed()) return [];
  if (platform() === "ios") {
    return [
      element(
        "p",
        {},
        "To keep it, add Volya Engine to your Home Screen: tap ",
        shareIcon(),
        " in the browser, then ",
        element("strong", { textContent: "Add to Home Screen" }),
        ". Then import the game in the Home Screen app: it has its own storage.",
      ),
    ];
  }
  if (canOffer()) {
    const button = element("button", { className: "wide", textContent: "Install app" });
    button.addEventListener("click", async () => {
      if (await install()) {
        await navigator.storage.persist();
        await refresh();
      }
    });
    return [element("p", { textContent: "Install Volya Engine as an app to keep it." }), button];
  }
  return [
    element("p", {
      textContent: chromium()
        ? "Install Volya Engine as an app to keep it: click the install icon at the right of the address bar."
        : "Install Volya Engine as an app to keep it, if this browser can.",
    }),
  ];
}

const storageWarning = () =>
  element(
    "div",
    { className: "panel warning" },
    element("p", { className: "label", textContent: "Storage" }),
    element("p", { textContent: "This browser may delete the imported game when it needs space." }),
    ...keepHelp(),
  );

/** Settings: updates, storage, and removing the import (danger zone). */
async function showSettings() {
  const status = await call("game_status");
  const persisted = await navigator.storage.persisted();
  const back = element("button", { className: "back", textContent: "‹ Back" });
  back.addEventListener("click", () => navigate("back", refresh));

  const auto = element("input", { type: "checkbox", checked: updates?.auto ?? true, disabled: !updates });
  const updateLine = element("p", { className: "note" });
  const renderUpdates = () => {
    if (!updates) {
      updateLine.textContent = "Updates need a browser with service workers.";
      return;
    }
    auto.checked = updates.auto;
    const { status: s, bytes } = updates;
    updateLine.textContent =
      s === "available"
        ? `An update is available (${formatBytes(bytes)}).`
        : s === "downloading"
          ? "Downloading an update…"
          : s === "ready"
            ? "An update is ready."
            : "Up to date.";
  };
  auto.addEventListener("change", () => updates?.setAuto(auto.checked));
  const stop = updates?.onChange(() => (updateLine.isConnected ? renderUpdates() : stop()));
  renderUpdates();

  const remove = element("button", { className: "danger filled", textContent: "Remove imported game" });
  remove.addEventListener("click", async () => {
    if (!confirm("Remove the imported game from this app's storage? You can import it again from the installer.")) return;
    await call("forget_game");
    coverImage = null;
    await refresh();
  });

  show(
    element("div", { className: "screen-head" }, back, element("h2", { textContent: "Settings" })),
    element(
      "div",
      { className: "panel" },
      element("p", { className: "label", textContent: "Updates" }),
      element("p", { className: "value", textContent: `Version ${versionText()}` }),
      element(
        "label",
        { className: "switch" },
        auto,
        element(
          "span",
          {},
          element("strong", { textContent: "Update automatically" }),
          element("small", { textContent: "When off, nothing downloads until you choose to (for metered connections)." }),
        ),
      ),
      updateLine,
      updateOffer(),
    ),
    element(
      "div",
      { className: "panel" },
      element("p", { className: "label", textContent: "Storage" }),
      element("p", { className: "value", textContent: status.imported ? `S.T.A.L.K.E.R.: Shadow of Chernobyl · ${formatBytes(status.bytes)}` : "No game imported." }),
      element("p", {
        className: "note",
        textContent: persisted ? "Kept: the browser won't delete it to free space." : "Not kept: the browser may delete it to free space.",
      }),
      ...(persisted ? [] : keepHelp()),
    ),
    status.imported
      ? element(
          "div",
          { className: "panel danger-zone" },
          element("p", { className: "label", textContent: "Danger zone" }),
          element("p", { className: "value", textContent: "Remove imported game" }),
          element("p", {
            className: "note",
            textContent: `Deletes the game's ${formatBytes(status.bytes)} from this app's storage. You can import it again from the installer.`,
          }),
          remove,
        )
      : null,
  );
  // Settings is a deeper screen: no logo (it's Home's), Back at the top.
  document.documentElement.dataset.screen = "settings";
  back.focus();
}

/** Every screen, for the UX audit page (static/ux/), which shows each state. */
export const screens = { showImport, showHome, showSettings, showInUse, showDamaged, showUpdateStatus };
