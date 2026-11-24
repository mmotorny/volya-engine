// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Game UI: importing the game from a GOG installer, launching it, and
// browsing its files. Plain DOM with native buttons, so touch, mouse and
// keyboard all work.
//
// The app always opens here; the game starts from the Play button. That tap
// also lets the browser play sound (it stays silent until the first user
// gesture), and keeps this screen, where the engine's settings live, in
// view. While the game runs the engine takes the screen and its input, and
// this interface hides until the player quits (the Q key or the title
// menu's last item).

const TEXT_EXTENSIONS = new Set(["ltx", "xml", "script", "txt", "ini", "ps", "vs", "s", "h"]);
const PREVIEW_BYTES = 4096;

let call;
const root = () => document.getElementById("game");
/** Replaces the game section's content, skipping absent (null) parts. */
const show = (...children) => root().replaceChildren(...children.filter((c) => c !== null && c !== undefined));

const formatBytes = (n) =>
  n >= 2 ** 30 ? `${(n / 2 ** 30).toFixed(2)} GiB` : n >= 2 ** 20 ? `${(n / 2 ** 20).toFixed(1)} MiB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${n} B`;

function element(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter((c) => c !== null && c !== undefined));
  return node;
}

/** Storage facts for the home screen. */
async function storageFacts(status) {
  const persisted = await navigator.storage.persisted();
  const { usage, quota } = await navigator.storage.estimate();
  return element(
    "dl",
    { className: "facts" },
    element("dt", { textContent: "Game data" }),
    element("dd", { textContent: formatBytes(status.bytes) }),
    element("dt", { textContent: "App storage" }),
    element("dd", { textContent: `${formatBytes(usage)} of ${formatBytes(quota)}` }),
    element("dt", { textContent: "Kept by the browser" }),
    element("dd", { textContent: persisted ? "Always" : "May be cleared" }),
  );
}

/** Entry point, called by main.js once the engine runs. */
export async function start(engineCall) {
  call = engineCall;
  globalThis.volya.importGame = importGame;
  await refresh();
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
      throw new Error(`Not enough storage: the game needs ${formatBytes(needed)}, ${formatBytes(quota - usage)} is available.`);
    }
    const persisted = await navigator.storage.persist();
    await request({ type: "run", bin, only });
    return { plan, persisted, status: await call("game_status") };
  } finally {
    worker.terminate();
  }
}

function showImport(status) {
  const message = element("p", { className: "message", hidden: true });
  const progress = element("progress", { max: 1, value: 0, hidden: true });
  const input = element("input", { type: "file", multiple: true, hidden: true });
  const choose = element("button", { className: "primary wide", textContent: "Choose installer files" });
  const say = (text) => {
    message.textContent = text;
    message.hidden = !text;
  };
  choose.addEventListener("click", () => input.click());
  input.addEventListener("change", async () => {
    const files = [...input.files];
    const exe = files.find((f) => f.name.toLowerCase().endsWith(".exe"));
    const bin = files.find((f) => f.name.toLowerCase().endsWith(".bin"));
    input.value = "";
    if (!exe || !bin) {
      say("Pick both installer files: the .exe and the -1.bin.");
      return;
    }
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
      say(`Import failed: ${sentence(error.message)}`);
      choose.disabled = false;
      progress.hidden = true;
    }
  });
  const resume =
    status.written > 0
      ? element("p", { className: "note", textContent: `A previous import stopped after ${status.written} files. Choosing the installer again resumes it.` })
      : null;
  show(
    element(
      "div",
      { className: "panel" },
      element("p", { className: "label", textContent: "Bring your own game" }),
      element("h2", { textContent: "Import S.T.A.L.K.E.R.: Shadow of Chernobyl" }),
      element("p", { textContent: "Volya Engine runs the game from your own copy, imported from its GOG offline installer." }),
      element(
        "ol",
        { className: "steps" },
        element("li", {}, "Download both installer files from GOG: ", element("code", { textContent: "setup_….exe" }), " and ", element("code", { textContent: "setup_…-1.bin" }), "."),
        element("li", { textContent: "Choose both files below. The game is extracted into this app's storage (about 5.6 GiB) and the installer files are no longer needed." }),
      ),
      resume,
      choose,
      input,
      progress,
      message,
    ),
  );
}

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
  const launch = element("button", { className: "primary play", textContent: "Play" });
  launch.addEventListener("click", async () => {
    note.hidden = true;
    try {
      await call("launch");
    } catch (error) {
      note.className = "note error";
      note.textContent = `The game could not start: ${sentence(error.message)}`;
      note.hidden = false;
    }
  });
  const ready = await graphics();
  if (!ready) {
    launch.disabled = true;
    note.textContent = "The game needs WebGPU, which this browser doesn't offer.";
    note.hidden = false;
  }

  const files = element("div", { className: "files" });
  const listing = element(
    "details",
    { className: "panel" },
    element("summary", {}, element("p", { className: "label", textContent: "Game files" })),
    files,
  );
  listing.addEventListener("toggle", () => listing.open && !files.childElementCount && showFiles(files, ""), { once: false });

  const remove = element("button", { className: "danger wide", textContent: "Remove imported game" });
  remove.addEventListener("click", async () => {
    if (!confirm("Remove the imported game from this app's storage? You can import it again from the installer.")) return;
    await call("forget_game");
    await refresh();
  });

  show(
    element(
      "div",
      { className: "panel" },
      element("p", { className: "label", textContent: ready ? "Ready" : "Unavailable here" }),
      element("p", { className: "game-title", textContent: "S.T.A.L.K.E.R.: Shadow of Chernobyl" }),
      launch,
      note,
    ),
    listing,
    element("div", { className: "panel" }, element("p", { className: "label", textContent: "Storage" }), await storageFacts(status), remove),
  );
}

/** The game's files under `path`, browsable, with a preview of the picked file. */
async function showFiles(container, path) {
  const entries = await call("read_dir", { path });
  const crumbs = element("nav", { className: "crumbs" });
  const parts = path ? path.split("/") : [];
  const crumb = (label, target) => {
    const button = element("button", { textContent: label });
    button.addEventListener("click", () => showFiles(container, target));
    return button;
  };
  crumbs.append(crumb("game", ""));
  parts.forEach((part, i) => crumbs.append("/", crumb(part, parts.slice(0, i + 1).join("/"))));

  const preview = element("pre", { className: "preview", hidden: true });
  const list = element("ul", { className: "entries" });
  for (const entry of entries) {
    const target = path ? `${path}/${entry.name}` : entry.name;
    const button = element(
      "button",
      {},
      element("span", { className: entry.dir ? "name dir" : "name", textContent: entry.dir ? `${entry.name}/` : entry.name }),
      entry.dir ? null : element("span", { className: "size", textContent: formatBytes(entry.size) }),
    );
    button.addEventListener("click", () => (entry.dir ? showFiles(container, target) : showPreview(target, preview)));
    list.append(element("li", {}, button));
  }
  container.replaceChildren(crumbs, list, preview);
}

async function showPreview(path, preview) {
  const { size, bytes } = await call("read_head", { path, max: PREVIEW_BYTES });
  const data = new Uint8Array(bytes);
  const extension = path.split(".").pop().toLowerCase();
  let text;
  if (TEXT_EXTENSIONS.has(extension)) {
    // The game's text files are Windows-1251 (Cyrillic).
    text = new TextDecoder("windows-1251").decode(data);
  } else {
    const lines = [];
    for (let i = 0; i < Math.min(data.length, 512); i += 16) {
      const row = data.subarray(i, i + 16);
      const hex = [...row].map((b) => b.toString(16).padStart(2, "0")).join(" ");
      const ascii = [...row].map((b) => (b >= 32 && b < 127 ? String.fromCharCode(b) : ".")).join("");
      lines.push(`${i.toString(16).padStart(6, "0")}  ${hex.padEnd(47)}  ${ascii}`);
    }
    text = lines.join("\n");
  }
  preview.textContent = `${path} (${formatBytes(size)})${size > data.length ? `, first ${formatBytes(data.length)}` : ""}\n\n${text}`;
  preview.hidden = false;
  preview.scrollIntoView({ block: "nearest" });
}
