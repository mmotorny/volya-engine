// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The Graphics setting (#121): which backend the engine draws with. The
// engine worker finds what the browser supports and starts the engine with
// the choice; Settings shows the options. Only an explicit choice is stored
// (the page's localStorage, main.js), so a browser that gains WebGPU moves
// to it; a stored choice the browser can't run gives way to the first
// option it can, and stays stored.

/** The options, recommended first: `[backend, label]`. */
export const BACKENDS = [
  ["webgpu", "WebGPU"],
  ["webgl2", "WebGL 2"],
];

/**
 * What this browser supports: `{ webgpu, webgl2 }`. WebGPU by asking for an
 * adapter, as the engine does (the iOS Simulator's WebKit has `navigator.gpu`
 * but gives none, measured); WebGL 2 by making a context on a canvas of its
 * own, so the engine's canvas keeps none.
 */
export async function supported() {
  const adapter = await navigator.gpu?.requestAdapter().catch(() => null);
  let webgl2 = false;
  try {
    const gl = new OffscreenCanvas(1, 1).getContext("webgl2");
    webgl2 = Boolean(gl);
    gl?.getExtension("WEBGL_lose_context")?.loseContext();
  } catch {
    // No OffscreenCanvas or no WebGL 2.
  }
  return { webgpu: Boolean(adapter), webgl2 };
}

/**
 * The backend used: `stored` where the browser supports it, else the first
 * option it supports; `null` where it supports none.
 */
export function choose(stored, available) {
  if (stored && available[stored]) return stored;
  return BACKENDS.map(([backend]) => backend).find((backend) => available[backend]) ?? null;
}
