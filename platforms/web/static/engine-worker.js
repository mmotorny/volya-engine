// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The engine thread (docs/architecture.md). Messages from the page:
//   { type: "start", canvas }       → { type: "started" } | { type: "error", message }
//   { type: "call", id, op, args }  → { type: "result", id, result } | { type: "result", id, error }

import init, { start, call } from "./pkg/volya_engine_web.js";

const message = (error) => String(error?.message ?? error);

onmessage = async ({ data }) => {
  if (data.type === "start") {
    try {
      await init();
      start(data.canvas);
      postMessage({ type: "started" });
    } catch (error) {
      postMessage({ type: "error", message: message(error) });
    }
  } else if (data.type === "call") {
    try {
      postMessage({ type: "result", id: data.id, result: JSON.parse(await call(data.op, JSON.stringify(data.args ?? {}))) });
    } catch (error) {
      postMessage({ type: "result", id: data.id, error: message(error) });
    }
  }
};
