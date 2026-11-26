// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Module worker for the platform suite: exercises the engine-worker
// architecture (docs/architecture/web-platform.md) from inside a worker.

// The app lives above `__test/` (which may be `__test/run/<id>/`). Imported
// lazily: `onmessage` must be set before any await, or early messages are lost.
const base = location.pathname.slice(0, location.pathname.indexOf("/__test/") + 1);
const engine = import(`${base}pkg/volya_engine_web.js`);

const ops = {
  async wasm() {
    const { default: init, call } = await engine;
    await init();
    return JSON.parse(await call("status", "{}"));
  },

  // Clears the transferred canvas to orange and reads the pixel back from
  // its drawing buffer, proving the presentation surface works.
  async webgl2({ canvas }) {
    const gl = canvas.getContext("webgl2");
    if (!gl) throw new Error("no webgl2 context on OffscreenCanvas");
    gl.clearColor(1, 128 / 255, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const rgba = new Uint8Array(4);
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
    const info = gl.getExtension("WEBGL_debug_renderer_info");
    const renderer = gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER);
    return { renderer, rgba: Array.from(rgba) };
  },

  async opfs() {
    const root = await navigator.storage.getDirectory();
    const dir = await root.getDirectoryHandle("volya-engine", { create: true });
    const name = `test-${Date.now()}`;
    const file = await dir.getFileHandle(name, { create: true });
    const handle = await file.createSyncAccessHandle();
    try {
      const data = new TextEncoder().encode("volya");
      handle.write(data, { at: 0 });
      handle.flush();
      const back = new Uint8Array(handle.getSize());
      handle.read(back, { at: 0 });
      return new TextDecoder().decode(back);
    } finally {
      handle.close();
      await dir.removeEntry(name);
    }
  },
};

onmessage = async ({ data }) => {
  try {
    postMessage({ ok: await ops[data.op](data) });
  } catch (error) {
    postMessage({ error: String(error?.message ?? error) });
  }
};
