// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// UX audit board: every state of the app's own interface on every device,
// laid out like a design file. Each frame is the live interface (frame.html)
// at the device's size, scaled to the zoom; open one for full size.

import { DEVICES, SECTIONS } from "./states.js";
import { versionText } from "../update.js";

const board = document.querySelector(".board");
const zoom = document.querySelector(".zoom input");
const zoomOut = document.querySelector(".zoom output");
const devicesBox = document.querySelector(".devices");
const nav = document.querySelector(".sections");
const full = document.querySelector(".full");
/** Each frame's full length, as it reports it. */
const lengths = new Map();
/** Each state's screen text per device, as its frames report it. */
const texts = new Map();

document
  .querySelector(".title")
  .insertAdjacentHTML("beforeend", `<span class="build">${versionText()}</span><button class="check" type="button">Check for update</button>`);

const shown = new Set(DEVICES.filter((d) => !d.hidden).map((d) => d.id));
for (const device of DEVICES) {
  const box = Object.assign(document.createElement("input"), { type: "checkbox", checked: shown.has(device.id) });
  box.addEventListener("change", () => {
    if (box.checked) shown.add(device.id);
    else shown.delete(device.id);
    render();
    for (const stateId of texts.keys()) compareText(stateId);
  });
  const label = document.createElement("label");
  label.append(box, device.name);
  devicesBox.append(label);
}

const scale = () => Number(zoom.value) / 100;

function frame(device, state) {
  const href = `frame.html#${device.id}/${state.id}`;
  const figure = document.createElement("figure");
  figure.className = "frame";
  figure.innerHTML = `<figcaption><a href="${href}" target="_blank" rel="noopener">${device.name}</a><span>${device.width} × ${device.height}</span></figcaption>
    <div class="viewport${device.touch ? " phone" : ""}" data-hash="#${device.id}/${state.id}" data-height="${device.height}" style="--w: ${device.width}px; --h: ${device.height}px"><iframe src="${href}" loading="lazy" title="${state.name} · ${device.name}"
      width="${device.width}" height="${device.height}"></iframe></div>`;
  return figure;
}

function render() {
  const s = scale();
  zoomOut.value = `${zoom.value}%`;
  document.documentElement.style.setProperty("--scale", s);
  board.replaceChildren();
  nav.replaceChildren();
  for (const section of SECTIONS) {
    const id = section.title.toLowerCase().replace(/[^a-z0-9]+/g, "-");
    nav.insertAdjacentHTML("beforeend", `<a href="#${id}">${section.title}</a>`);
    const block = document.createElement("section");
    block.id = id;
    block.innerHTML = `<h2>${section.title}</h2>`;
    for (const state of section.states) {
      const row = document.createElement("div");
      row.className = "state";
      row.id = state.id;
      row.innerHTML = `<h3><a href="#${state.id}">${state.name}</a><span class="badge" hidden>Text differs by device</span></h3>`;
      const frames = document.createElement("div");
      frames.className = "frames";
      for (const device of DEVICES.filter((d) => shown.has(d.id))) frames.append(frame(device, state));
      row.append(frames);
      block.append(row);
    }
    board.append(block);
  }
}

/** Viewport height, or the whole screen's with "Full length". */
function fit(viewport) {
  const device = Number(viewport.dataset.height);
  const height = full.checked ? Math.max(device, lengths.get(viewport.dataset.hash) ?? device) : device;
  viewport.style.setProperty("--h", `${height}px`);
  viewport.querySelector("iframe").height = height;
}

/**
 * Flags a state whose text differs between the devices shown (they load as
 * they scroll into view): a badge listing each device's own lines, and the
 * captions marked, so it's read on each.
 */
function compareText(stateId) {
  const row = board.querySelector(`.state[id="${stateId}"]`);
  const byDevice = texts.get(stateId);
  if (!row || !byDevice) return;
  const shownTexts = [...byDevice].filter(([device]) => shown.has(device));
  const differs = new Set(shownTexts.map(([, text]) => text)).size > 1;
  const badge = row.querySelector(".badge");
  badge.hidden = !differs;
  for (const caption of row.querySelectorAll("figcaption")) caption.classList.toggle("differs", differs);
  if (!differs) return;
  const lines = (text) => new Set(text.split("\n").map((l) => l.trim()).filter(Boolean));
  const sets = shownTexts.map(([device, text]) => [device, lines(text)]);
  badge.title = sets
    .map(([device, own]) => {
      const others = sets.filter(([d]) => d !== device).map(([, set]) => set);
      const unique = [...own].filter((line) => others.some((set) => !set.has(line)));
      const name = DEVICES.find((d) => d.id === device).name;
      return `${name}:\n${unique.map((l) => `  ${l}`).join("\n") || "  (nothing of its own)"}`;
    })
    .join("\n\n");
}

addEventListener("message", ({ data }) => {
  if (data?.type !== "ux-frame") return;
  lengths.set(data.hash, data.height);
  const [device, stateId] = data.hash.slice(1).split("/");
  if (!texts.has(stateId)) texts.set(stateId, new Map());
  texts.get(stateId).set(device, data.text);
  compareText(stateId);
  const viewport = board.querySelector(`.viewport[data-hash="${data.hash}"]`);
  if (viewport) fit(viewport);
});
full.addEventListener("change", () => board.querySelectorAll(".viewport").forEach(fit));

// Zooming only rescales; frames keep their state.
zoom.addEventListener("input", () => {
  zoomOut.value = `${zoom.value}%`;
  document.documentElement.style.setProperty("--scale", scale());
});
// The view (zoom, devices, full length, scroll) is kept across reloads and
// visits: saved on every change and when the page is left, restored on load.
// Namespaced: the github.io origin is shared.
const VIEW = "volya-engine.ux-audit.view";
const saveView = () =>
  localStorage.setItem(VIEW, JSON.stringify({ zoom: zoom.value, shown: [...shown], full: full.checked, y: scrollY }));
zoom.addEventListener("change", saveView);
full.addEventListener("change", saveView);
devicesBox.addEventListener("change", saveView);
addEventListener("pagehide", saveView);
document.addEventListener("visibilitychange", () => document.visibilityState === "hidden" && saveView());
// The board is built after load, so the browser can't restore the scroll.
history.scrollRestoration = "manual";

// Check for update: on request only, every 10 s up to 12 times (GitHub Pages
// serves a deploy a minute or two later, from many servers); a newer build
// reloads the board, keeping the view. This page isn't kept offline, so
// checks reach the network.
const CHECKS = 12;
const CHECK_EVERY_MS = 10_000;
const build = () => document.querySelector('meta[name="build"]').content;
async function deployedBuild() {
  const html = await (await fetch(location.pathname, { cache: "no-store" })).text();
  return new DOMParser().parseFromString(html, "text/html").querySelector('meta[name="build"]')?.content;
}
const checkButton = document.querySelector(".check");
checkButton.addEventListener("click", async () => {
  checkButton.disabled = true;
  for (let attempt = 1; attempt <= CHECKS; attempt++) {
    checkButton.textContent = `Checking… ${attempt}/${CHECKS}`;
    const latest = await deployedBuild().catch(() => null);
    if (latest && latest !== build()) {
      saveView();
      location.reload();
      return;
    }
    if (attempt < CHECKS) await new Promise((resolve) => setTimeout(resolve, CHECK_EVERY_MS));
  }
  checkButton.textContent = "No newer build · Check again";
  checkButton.disabled = false;
});
let saved = null;
try {
  saved = JSON.parse(localStorage.getItem(VIEW) ?? "null");
} catch {
  // A damaged entry: start from the defaults.
}
if (saved) {
  zoom.value = saved.zoom;
  full.checked = saved.full;
  shown.clear();
  for (const id of saved.shown ?? []) if (DEVICES.some((d) => d.id === id)) shown.add(id);
  devicesBox.querySelectorAll("input").forEach((box, i) => (box.checked = shown.has(DEVICES[i].id)));
  zoomOut.value = `${zoom.value}%`;
  document.documentElement.style.setProperty("--scale", scale());
}

render();
if (saved) requestAnimationFrame(() => scrollTo(0, saved.y));
