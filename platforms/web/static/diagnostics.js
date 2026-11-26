// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The page's side of the diagnostic log (#346;
// docs/architecture/diagnostics.md). The log lives in the engine worker;
// the page sends its own events there, only while the player's setting is on (off by
// default), so nothing is done for it otherwise:
//
//   event(name, args)    an instant event (args: a flat object)
//   metric(name, value)  a value; its minimum, mean and maximum per second
//                        are logged (aggregated here, sent once a second)
//   count(name, n)       adds to a count, logged per second
//   end(reason, message) how the engine ended (`engine.end`, stored at once)
//
// The setting is kept in localStorage (namespaced: the github.io origin is
// shared); the UX audit page sets `volyaDiagnostics` per frame.
//
// As the page is hidden or closed, its engine worker may be ended before it
// stores its own `engine.end`: the page keeps one in localStorage too
// (`keepEnd`, written synchronously in the event), which the next session
// adds to the log unless the worker stored its own (`takeKeptEnd`).

import { versionText } from "./update.js";

const KEY = "volya-engine.diagnostic-log";
const END_KEY = "volya-engine.diagnostic-log-end";
/** Set (sessionStorage) as a page restored from the back-forward cache reloads. */
const RESTORED_KEY = "volya-engine.diagnostic-log-restored";
const PERIOD_MS = 1000;
/** This session's `{pid, cat}` once its log is on (the engine's status). */
let current = null;

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

/**
 * Whether this page load is main.js's reload of a page restored from the
 * back-forward cache (`restoredReload`); read as the page loads, whether or
 * not the log is on, so no later load inherits it.
 */
const restored = (() => {
  try {
    const value = sessionStorage.getItem(RESTORED_KEY) === "1";
    sessionStorage.removeItem(RESTORED_KEY);
    return value;
  } catch {
    return false;
  }
})();

/** What the page tells the engine of the session. */
export function session() {
  const standalone = matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;
  return {
    version: versionText(),
    user_agent: navigator.userAgent,
    screen: `${screen.width}×${screen.height}`,
    dpr: devicePixelRatio,
    cores: navigator.hardwareConcurrency ?? 0,
    // How this page load started: navigate, reload or back_forward
    // (Navigation Timing Level 2).
    navigation: performance.getEntriesByType("navigation")[0]?.type ?? "unknown",
    restored_reload: restored,
    display_mode: standalone ? "standalone" : "browser",
  };
}

/**
 * Notes, for the next page's session, that this page was restored from the
 * back-forward cache and reloads (its engine worker ended as it was hidden).
 */
export function restoredReload() {
  try {
    sessionStorage.setItem(RESTORED_KEY, "1");
  } catch {
    // Storage refused: the next session shows a reload only.
  }
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

/** Takes the log's status (`diagnostic_log_status`): this session's number. */
export function setStatus(status) {
  current = status?.on ? (status.session ?? null) : null;
}

/** Microseconds since the Unix epoch, as the engine's events. */
const now = () => (performance.timeOrigin + performance.now()) * 1000;

/** Keeps how this session ended, should its engine worker be ended before storing it. */
export function keepEnd(reason, message) {
  if (!current || !enabled()) return;
  try {
    // A closing page's `visibilitychange` may come after its `pagehide`
    // (seen in headless Chrome: a reload kept `page_hidden`): closed stays
    // closed.
    const kept = JSON.parse(localStorage.getItem(END_KEY));
    if (kept?.pid === current.pid && kept.reason === "page_closed") return;
    localStorage.setItem(END_KEY, JSON.stringify({ ...current, ts: now(), reason, message }));
  } catch {
    // Storage refused: only the worker's record remains.
  }
}

/** Forgets the kept end: the session goes on (the page shown again). */
export function forgetEnd() {
  try {
    localStorage.removeItem(END_KEY);
  } catch {
    // Nothing kept.
  }
}

/** The end an earlier session kept, once (for the engine to add). */
export function takeKeptEnd() {
  try {
    const kept = JSON.parse(localStorage.getItem(END_KEY));
    localStorage.removeItem(END_KEY);
    return kept;
  } catch {
    return null;
  }
}

/** Sends the page's events with `postMessage` (main.js: to the engine worker). */
export function connect(postMessage) {
  post = postMessage;
  follow();
}

/**
 * Stores the setting; `call` tells the engine (the `diagnostics_set` op),
 * whose answer is the log's status. Off, the engine deletes the log and
 * the kept end goes too (#536).
 */
export async function setEnabled(on, call) {
  try {
    if (on) localStorage.setItem(KEY, "on");
    else localStorage.removeItem(KEY);
  } catch {
    // Storage refused: the setting lasts this session.
  }
  if (globalThis.volyaDiagnostics !== undefined) globalThis.volyaDiagnostics = on;
  if (!on) forgetEnd();
  const status = await call("diagnostics_set", { on, session: session(), previous_end: on ? takeKeptEnd() : null });
  setStatus(status);
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

/** How the engine ended, seen from the page: recorded and stored at once. */
export function end(reason, message) {
  if (post && enabled()) post({ type: "end", reason, message: String(message) });
}
