// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The engine comes in two wasm builds (#186): `webgpu` (pkg/), and `webgl`
// (pkg-webgl/), which adds wgpu's WebGL 2 backend for browsers that give no
// WebGPU adapter (#113). Each browser downloads only the one it runs; the
// service worker keeps that one (sw.js). Chosen as the engine chooses its
// backend: by asking for a WebGPU adapter (the iOS Simulator's WebKit has
// `navigator.gpu` but gives none, measured), which needs no canvas.

/** `webgpu` where the browser gives a WebGPU adapter, else `webgl`. */
export async function wasmBuild() {
  const adapter = await navigator.gpu?.requestAdapter().catch(() => null);
  return adapter ? "webgpu" : "webgl";
}

/** The wasm-bindgen module of `build`, loaded. */
export function wasmModule(build) {
  return import(build === "webgpu" ? "./pkg/volya_engine_web.js" : "./pkg-webgl/volya_engine_web.js");
}
