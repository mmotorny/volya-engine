// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Import worker: extracts the game from a GOG installer into OPFS
// (docs/architecture.md, Game data). Messages from the page:
//   { type: "plan", exe }            → { type: "plan", plan } | { type: "error", message }
//   { type: "run", bin, only }       → { type: "progress", done, total, path }…, { type: "done" } | error
// `exe` and `bin` are Files (or, in tests, `{ url, size }` served with range
// support); `only` optionally restricts the import to some paths.

import init, { plan_import, run_import } from "./pkg/volya_engine_web.js";

const ready = init();

/** A synchronous byte source over a File or a range-capable URL. */
function source(input) {
  if (input instanceof Blob) {
    const reader = new FileReaderSync();
    return {
      size: input.size,
      read: (offset, length) => new Uint8Array(reader.readAsArrayBuffer(input.slice(offset, offset + length))),
    };
  }
  return {
    size: input.size,
    read: (offset, length) => {
      const request = new XMLHttpRequest();
      request.open("GET", input.url, false);
      request.responseType = "arraybuffer";
      request.setRequestHeader("Range", `bytes=${offset}-${offset + length - 1}`);
      request.send();
      if (request.status !== 206) throw new Error(`range request: HTTP ${request.status}`);
      return new Uint8Array(request.response);
    },
  };
}

onmessage = async ({ data }) => {
  try {
    await ready;
    if (data.type === "plan") {
      postMessage({ type: "plan", plan: JSON.parse(plan_import(source(data.exe))) });
    } else if (data.type === "run") {
      let last = 0;
      await run_import(source(data.bin), data.only ?? null, (done, total, path) => {
        const now = performance.now();
        if (now - last > 100 || done === total) {
          last = now;
          postMessage({ type: "progress", done, total, path });
        }
      });
      postMessage({ type: "done" });
    }
  } catch (error) {
    postMessage({ type: "error", message: String(error?.message ?? error) });
  }
};
