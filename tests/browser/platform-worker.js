// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Module worker for the platform suite: exercises the engine-worker
// architecture (docs/architecture.md) from inside a worker.

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
  // the canvas texture itself, proving the presentation surface works.
  async webgpu({ canvas }) {
    if (!navigator.gpu) throw new Error("navigator.gpu missing in worker");
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("no WebGPU adapter");
    const device = await adapter.requestDevice();
    const context = canvas.getContext("webgpu");
    if (!context) throw new Error("no webgpu context on OffscreenCanvas");
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({
      device,
      format,
      alphaMode: "opaque",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
    const texture = context.getCurrentTexture();
    const encoder = device.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: [{
        view: texture.createView(),
        loadOp: "clear",
        storeOp: "store",
        clearValue: { r: 1, g: 0.5, b: 0, a: 1 },
      }],
    });
    pass.end();
    const buffer = device.createBuffer({
      size: 256,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    encoder.copyTextureToBuffer({ texture }, { buffer, bytesPerRow: 256 }, { width: 1, height: 1 });
    device.queue.submit([encoder.finish()]);
    await buffer.mapAsync(GPUMapMode.READ);
    const px = Array.from(new Uint8Array(buffer.getMappedRange(), 0, 4));
    buffer.unmap();
    const rgba = format.startsWith("bgra") ? [px[2], px[1], px[0], px[3]] : px;
    return { format, vendor: adapter.info?.vendor ?? "?", rgba };
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
