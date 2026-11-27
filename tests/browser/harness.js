// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// In-page browser test harness. Each suite has a page, `__test/<suite>.html`,
// that calls `runPage("<suite>")`: it captures page errors, starts the
// unchanged app, runs the suite and POSTs a plain-text report to the dev
// server (shown on the page as well):
//
//   run <id>
//   suite <name>
//   ok <test>[\t<note>]
//   skip <test>\t<reason>
//   fail <test>\t<message>
//
// A report with no `fail` lines and at least one `ok` line passes. The run id
// comes from the dev server path `__test/run/<id>/<suite>.html`, which lets
// the browser suites' runner (`xtask suite`) match reports to runs; otherwise it is `manual`.

const errors = [];

class Skip extends Error {}

/** Skips the current test, with a reason shown in the report. */
export function skip(reason) {
  throw new Skip(reason);
}

/** Throws unless `condition` holds. */
export function assert(condition, message) {
  if (!condition) throw new Error(message);
}

/** Throws unless `actual` and `expected` are equal by value. */
export function assertEqual(actual, expected, message = "") {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${message}${message ? ": " : ""}expected ${e}, got ${a}`);
}

/** The suite running and the URL prefix of the test infrastructure. */
let current = { suite: "", testBase: "" };
let dumping = null;

/**
 * Keeps a frame for inspection: `capture` (`{ width, height, rgba }`, as
 * `debug_capture` returns it) is encoded as PNG and posted to the dev
 * server, which writes it to `<VOLYA_DUMP_DIR>/<suite>/<name>.png` when the
 * variable is set; otherwise nothing is encoded. `name` is letters, digits,
 * `-` and `_`.
 */
export async function dump(name, { width, height, rgba }) {
  if (!(await keepsDumps())) return;
  const canvas = new OffscreenCanvas(width, height);
  const pixels = new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength);
  canvas.getContext("2d").putImageData(new ImageData(pixels, width, height), 0, 0);
  const png = await canvas.convertToBlob({ type: "image/png" });
  const response = await fetch(`${current.testBase}dump/${current.suite}/${name}.png`, { method: "POST", body: png });
  assert(response.ok, `dump ${name}: HTTP ${response.status}`);
}

/** Keeps file `name` (with its extension, `.ltx`) of `bytes` beside the
 * dumped frames, when they are kept. */
export async function dumpFile(name, bytes) {
  if (!(await keepsDumps())) return;
  const response = await fetch(`${current.testBase}dump/${current.suite}/${name}`, { method: "POST", body: bytes });
  assert(response.ok, `dump ${name}: HTTP ${response.status}`);
}

/** Whether the dev server keeps dumped frames (`VOLYA_DUMP_DIR`). */
export function keepsDumps() {
  dumping ??= fetch(`${current.testBase}dump/`).then(
    (r) => r.status === 204,
    () => false,
  );
  return dumping;
}

function captureErrors() {
  addEventListener("error", (e) => errors.push(`error: ${e.message}`));
  addEventListener("unhandledrejection", (e) => errors.push(`unhandled rejection: ${e.reason}`));
  const consoleError = console.error;
  console.error = (...args) => {
    errors.push(`console.error: ${args.join(" ")}`);
    consoleError(...args);
  };
}

/** Resources that failed to load, where the browser reports status codes. */
function failedResources() {
  return performance
    .getEntriesByType("resource")
    // Requests to the test infrastructure itself (e.g. probing whether an
    // installer is configured) are not page errors, nor headless Chrome's
    // own request for the origin's /favicon.ico, outside the app (#368).
    .filter((r) => {
      const path = new URL(r.name).pathname;
      return r.responseStatus >= 400 && !path.includes("/__test/") && path !== "/favicon.ico";
    })
    .map((r) => `HTTP ${r.responseStatus}: ${r.name}`);
}

/**
 * Silent on the machine running the tests: whatever connects to a live
 * AudioContext's speakers goes through a gain of 0 first. The iOS Simulator
 * plays through one of the Mac's real output devices and has no setting for
 * none (measured: CoreSimulator falls back to the default device for any
 * other route). Suites measure audio before it (the AudioWorklet's output,
 * `OfflineAudioContext`, which this leaves alone, and engine state); the
 * context's state is unaffected.
 */
function muteSpeakers() {
  const connect = AudioNode.prototype.connect;
  const mutes = new WeakMap();
  AudioNode.prototype.connect = function (target, ...rest) {
    const context = target?.context;
    if (!(target instanceof AudioDestinationNode && context instanceof AudioContext)) {
      return connect.call(this, target, ...rest);
    }
    let mute = mutes.get(context);
    if (!mute) {
      mute = new GainNode(context, { gain: 0 });
      connect.call(mute, target);
      mutes.set(context, mute);
    }
    connect.call(this, mute, ...rest);
    return target;
  };
}

/** Where a test that reloads the page keeps the run's state (sessionStorage). */
const RESUME = "volya-engine-test.resume";
/** The report so far and the test running, for `resumeAfterReload`. */
let running = null;

/**
 * For a test that reloads the page: keeps the report so far and `state`,
 * then runs `reload` (which reloads the page). The reloaded page runs the
 * same test again with `state` as its argument, then the tests after it.
 * Never resolves.
 */
export function resumeAfterReload(state, reload) {
  sessionStorage.setItem(RESUME, JSON.stringify({ lines: running.lines, test: running.name, state }));
  reload();
  return new Promise(() => {});
}

/** Runs `suite` against the app. Entry point of every suite page. */
export async function runPage(suite) {
  captureErrors();
  muteSpeakers();
  let resume = null;
  try {
    resume = JSON.parse(sessionStorage.getItem(RESUME));
    sessionStorage.removeItem(RESUME);
  } catch {
    // Nothing to resume.
  }
  // Each run tests the build being served: a service worker left by an
  // earlier run would serve its own build's copies (sw.js is cache-first, and
  // a new build waits until asked). Drop it and its copies, then start over;
  // not when resuming a test that reloaded the page.
  if (!resume && navigator.serviceWorker?.controller) {
    for (const registration of await navigator.serviceWorker.getRegistrations()) await registration.unregister();
    for (const key of await caches.keys()) if (key.startsWith("volya-engine")) await caches.delete(key);
    location.reload();
    return;
  }
  const path = location.pathname;
  const base = path.slice(0, path.indexOf("/__test/") + 1);
  const runId = path.match(/\/__test\/run\/([^/]+)\//)?.[1] ?? "manual";
  current = { suite, testBase: `${base}__test/` };

  // The app's DOM, as in index.html, with its viewport.
  const index = new DOMParser().parseFromString(await (await fetch(`${base}index.html`)).text(), "text/html");
  document.querySelector('meta[name="viewport"]').content = index.querySelector('meta[name="viewport"]').content;
  document.head.insertAdjacentHTML(
    "beforeend",
    `<link rel="icon" href="${base}icon.svg" type="image/svg+xml"><link rel="stylesheet" href="${base}style.css">`,
  );
  document.body.insertAdjacentHTML(
    "afterbegin",
    `<div id="screen"><canvas id="view"></canvas></div><main><header class="brand"><h1 aria-label="Volya Engine"><img class="logo" src="${base}logo.svg" alt=""><span class="lockup"><img src="${base}logo-drawing.svg" alt=""><img src="${base}logo-wordmark.svg" alt=""></span></h1></header><section id="game" hidden></section><footer><p id="status" data-state="loading">Loading…</p><a class="source" href="https://github.com/mmotorny/volya-engine" target="_blank" rel="noopener"><svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>Source on GitHub</a></footer></main>`,
  );

  let startupError = null;
  try {
    await import(`${base}main.js`);
  } catch (error) {
    startupError = error;
  }

  const lines = resume?.lines ?? [`run ${runId}`, `suite ${suite}`];
  const oneLine = (text) => String(text).replace(/\s+/g, " ");
  const record = (name, error, note) =>
    lines.push(
      error
        ? `fail ${name}\t${oneLine(error.message ?? error)}`
        : `ok ${name}${note ? `\t${oneLine(note)}` : ""}`,
    );

  if (startupError) {
    record("engine startup", startupError);
  } else {
    try {
      const { tests } = await import(`./${suite}.js`);
      // Resuming, the tests before the one that reloaded already ran.
      let skipping = Boolean(resume);
      for (const [name, test] of Object.entries(tests)) {
        if (skipping && name !== resume.test) continue;
        const resumed = skipping ? resume.state : undefined;
        skipping = false;
        running = { lines, name };
        try {
          // A test may return a note (e.g. a measured value) for the report.
          const note = await test(resumed);
          record(name, null, typeof note === "string" ? note : "");
        } catch (error) {
          if (error instanceof Skip) lines.push(`skip ${name}\t${oneLine(error.message)}`);
          else record(name, error);
        }
      }
    } catch (error) {
      record(`load suite ${suite}`, error);
    }
  }
  const pageErrors = [...errors, ...failedResources()];
  record("no page errors", pageErrors.length ? new Error(pageErrors.join("; ")) : null);

  const report = lines.join("\n");
  try {
    await fetch(`${base}__test/results`, { method: "POST", body: report });
  } catch {
    // No dev server (e.g. GitHub Pages): the on-page report is the output.
  }
  const pre = document.createElement("pre");
  pre.id = "test-report";
  pre.textContent = report;
  document.body.append(pre);
}
