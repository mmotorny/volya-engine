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

document.querySelector(".title").insertAdjacentHTML("beforeend", `<span class="build">${versionText()}</span>`);

const shown = new Set(DEVICES.map((d) => d.id));
for (const device of DEVICES) {
  const box = Object.assign(document.createElement("input"), { type: "checkbox", checked: true });
  box.addEventListener("change", () => {
    if (box.checked) shown.add(device.id);
    else shown.delete(device.id);
    render();
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
      row.innerHTML = `<h3><a href="#${state.id}">${state.name}</a></h3>`;
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

addEventListener("message", ({ data }) => {
  if (data?.type !== "ux-frame") return;
  lengths.set(data.hash, data.height);
  const viewport = board.querySelector(`.viewport[data-hash="${data.hash}"]`);
  if (viewport) fit(viewport);
});
full.addEventListener("change", () => board.querySelectorAll(".viewport").forEach(fit));

// Zooming only rescales; frames keep their state.
zoom.addEventListener("input", () => {
  zoomOut.value = `${zoom.value}%`;
  document.documentElement.style.setProperty("--scale", scale());
});
render();
