// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Game UI: importing the game from a GOG installer, launching it, and
// browsing its files. Plain DOM with native buttons, so touch, mouse and
// keyboard all work.
//
// Once the game is imported, the app launches it on start (and after an
// import): the engine takes the screen and its input, and this interface
// hides until the player quits (the Q key or the title menu's last item).

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

async function storageLine() {
  const persisted = await navigator.storage.persisted();
  const { usage, quota } = await navigator.storage.estimate();
  return `Storage: ${formatBytes(usage)} used of ${formatBytes(quota)}; ${
    persisted ? "persistent" : "not persistent: the browser may clear it"
  }.`;
}

/** Entry point, called by main.js once the engine runs. */
export async function start(engineCall) {
  call = engineCall;
  globalThis.volya.importGame = importGame;
  await refresh({ launch: true });
}

/** Whether the engine can draw (without WebGPU the game would run unseen). */
const graphics = async () => (await call("status")).gpu.state === "ready";

/**
 * Shows the browser or the import screen; `launch` starts an imported game
 * when the engine can draw.
 */
async function refresh({ launch = false } = {}) {
  root().hidden = false;
  try {
    const status = await call("game_status");
    if (status.imported) {
      await showBrowser("");
      if (launch && (await graphics())) await play();
    } else showImport(status);
  } catch (error) {
    showDamaged(error);
  }
}

/** Launches the game; on failure says why above the browser. */
async function play() {
  try {
    await call("launch");
  } catch (error) {
    root().prepend(element("p", { className: "message", textContent: `The game could not start: ${error.message}` }));
  }
}

/**
 * Imports the game. `exe` and `bin` are the installer's Files (or, in tests,
 * `{ url, size }` sources); `only` optionally limits the paths imported.
 */
export async function importGame({ exe, bin, only }, onProgress = () => {}) {
  // Files already imported are skipped (the import resumes), so only the
  // rest needs free space.
  const imported = (await call("game_status").catch(() => ({ bytes: 0 }))).bytes ?? 0;
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
    if (quota - usage < needed - imported) {
      throw new Error(`Not enough storage: the game needs ${formatBytes(needed)}, ${formatBytes(quota - usage)} is available.`);
    }
    const persisted = await navigator.storage.persist();
    await request({ type: "run", bin, only });
    return { plan, persisted, status: await call("game_status") };
  } finally {
    worker.terminate();
  }
}

/**
 * A button that picks the installer's two files and imports from them, with
 * progress and messages. Importing over an existing import adds only what is
 * missing (files already imported are skipped).
 */
function installerPicker(label, primary) {
  const message = element("p", { className: "message" });
  const progress = element("progress", { max: 1, value: 0, hidden: true });
  const input = element("input", { type: "file", multiple: true, hidden: true });
  const choose = element("button", { className: primary ? "primary" : "", textContent: label });
  choose.addEventListener("click", () => input.click());
  input.addEventListener("change", async () => {
    const files = [...input.files];
    const exe = files.find((f) => f.name.toLowerCase().endsWith(".exe"));
    const bin = files.find((f) => f.name.toLowerCase().endsWith(".bin"));
    input.value = "";
    if (!exe || !bin) {
      message.textContent = "Pick both installer files: the .exe and the -1.bin.";
      return;
    }
    choose.disabled = true;
    progress.hidden = false;
    message.textContent = "Reading the installer…";
    try {
      await importGame({ exe, bin }, ({ done, total, path }) => {
        progress.value = done / total;
        message.textContent = `Importing ${path}: ${formatBytes(done)} of ${formatBytes(total)}`;
      });
      await refresh({ launch: true });
    } catch (error) {
      message.textContent = `Import failed: ${error.message}`;
      choose.disabled = false;
      progress.hidden = true;
    }
  });
  return [choose, input, progress, message];
}

function showImport(status) {
  const resume = status.written > 0 ? element("p", { textContent: `A previous import stopped after ${status.written} files; choosing the installer again resumes it.` }) : null;
  show(
    element("h2", { textContent: "Import the game" }),
    element("p", {
      textContent:
        "Choose both files of the GOG offline installer of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): setup_….exe and setup_…-1.bin. The game files are extracted into this app's storage (about 5.6 GiB).",
    }),
    resume,
    ...installerPicker("Choose installer files", true),
  );
}

function showDamaged(error) {
  const remove = element("button", { textContent: "Remove imported data" });
  remove.addEventListener("click", async () => {
    await call("forget_game");
    await refresh();
  });
  show(element("h2", { textContent: "Imported game unavailable" }), element("p", { textContent: error.message }), remove);
}

async function showBrowser(path) {
  const entries = await call("read_dir", { path });
  const crumbs = element("nav", { className: "crumbs" });
  const parts = path ? path.split("/") : [];
  const crumb = (label, target) => {
    const button = element("button", { textContent: label });
    button.addEventListener("click", () => showBrowser(target));
    return button;
  };
  crumbs.append(crumb("game", ""));
  parts.forEach((part, i) => crumbs.append(" / ", crumb(part, parts.slice(0, i + 1).join("/"))));

  const preview = element("pre", { className: "preview", hidden: true });
  const list = element("ul", { className: "entries" });
  for (const entry of entries) {
    const target = path ? `${path}/${entry.name}` : entry.name;
    const button = element(
      "button",
      {},
      element("span", { className: "name", textContent: entry.dir ? `${entry.name}/` : entry.name }),
      entry.dir ? null : element("span", { className: "size", textContent: formatBytes(entry.size) }),
    );
    button.addEventListener("click", () => (entry.dir ? showBrowser(target) : showPreview(target, preview)));
    list.append(element("li", {}, button));
  }

  const launch = element("button", { className: "primary", textContent: "Play" });
  launch.addEventListener("click", play);
  if (!(await graphics())) {
    launch.disabled = true;
    launch.title = "The game needs WebGPU, which is unavailable here.";
  }
  const remove = element("button", { textContent: "Remove imported game" });
  remove.addEventListener("click", async () => {
    if (!confirm("Remove the imported game from this app's storage? You can import it again from the installer.")) return;
    await call("forget_game");
    await refresh();
  });
  show(
    launch,
    element("p", { className: "storage", textContent: await storageLine() }),
    crumbs,
    preview,
    list,
    // Adds files a newer version of the app imports (e.g. the splash
    // executable) without re-extracting the archives.
    ...installerPicker("Update from installer", false),
    remove,
  );
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
