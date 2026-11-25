// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The page's side of the diagnostic log (#346; docs/architecture.md,
// Diagnostic log). The log lives in the engine worker; the page sends its
// own events there, only while the player's setting is on (off by
// default), so nothing is done for it otherwise:
//
//   event(name, args)    an instant event (args: a flat object)
//   metric(name, value)  a value; its minimum, mean and maximum per second
//                        are logged (aggregated here, sent once a second)
//   count(name, n)       adds to a count, logged per second
//
// The setting is kept in localStorage (namespaced: the github.io origin is
// shared); the UX audit page sets `volyaDiagnostics` per frame.

import { versionText } from "./update.js";

const KEY = "volya-engine.diagnostic-log";
const PERIOD_MS = 1000;

let post = null;
let timer = null;
const metrics = new Map();

/** Whether the player keeps a diagnostic log. */
export function enabled() {
  if (globalThis.volyaDiagnostics !== undefined) return globalThis.volyaDiagnostics;
  try {
    return localStorage.getItem(KEY) === "on";
  } catch {
    return false;
  }
}

/** What the page tells the engine of the session. */
export function session() {
  return {
    version: versionText(),
    user_agent: navigator.userAgent,
    screen: `${screen.width}×${screen.height}`,
    dpr: devicePixelRatio,
    cores: navigator.hardwareConcurrency ?? 0,
  };
}

function flush() {
  for (const [name, m] of metrics) post?.({ type: "diag", kind: "metrics", name, ...m });
  metrics.clear();
}

function follow() {
  if (enabled() && post && !timer) timer = setInterval(flush, PERIOD_MS);
  else if (!enabled() && timer) {
    clearInterval(timer);
    timer = null;
    metrics.clear();
  }
}

/** Sends the page's events with `postMessage` (main.js: to the engine worker). */
export function connect(postMessage) {
  post = postMessage;
  follow();
}

/**
 * Stores the setting; `call` tells the engine (the `diagnostics_set` op),
 * whose answer is the log's status.
 */
export async function setEnabled(on, call) {
  try {
    if (on) localStorage.setItem(KEY, "on");
    else localStorage.removeItem(KEY);
  } catch {
    // Storage refused: the setting lasts this session.
  }
  if (globalThis.volyaDiagnostics !== undefined) globalThis.volyaDiagnostics = on;
  const status = await call("diagnostics_set", { on, session: session() });
  follow();
  return status;
}

export function event(name, args = {}) {
  if (post && enabled()) post({ type: "diag", kind: "event", name, args });
}

export function metric(name, value) {
  if (!post || !enabled() || !Number.isFinite(value)) return;
  const m = metrics.get(name);
  if (m) {
    m.n++;
    m.sum += value;
    m.min = Math.min(m.min, value);
    m.max = Math.max(m.max, value);
  } else metrics.set(name, { n: 1, sum: value, min: value, max: value });
}

export function count(name, n = 1) {
  if (post && enabled()) post({ type: "diag", kind: "count", name, n });
}
