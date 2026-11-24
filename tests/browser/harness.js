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
// `cargo xtask e2e` match reports to runs; otherwise it is `manual`.

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
    // installer is configured) are not page errors.
    .filter((r) => r.responseStatus >= 400 && !new URL(r.name).pathname.includes("/__test/"))
    .map((r) => `HTTP ${r.responseStatus}: ${r.name}`);
}

/** Runs `suite` against the app. Entry point of every suite page. */
export async function runPage(suite) {
  captureErrors();
  const path = location.pathname;
  const base = path.slice(0, path.indexOf("/__test/") + 1);
  const runId = path.match(/\/__test\/run\/([^/]+)\//)?.[1] ?? "manual";

  // The app's DOM, as in index.html.
  document.head.insertAdjacentHTML(
    "beforeend",
    `<link rel="icon" href="${base}icon.svg" type="image/svg+xml"><link rel="stylesheet" href="${base}style.css">`,
  );
  document.body.insertAdjacentHTML(
    "afterbegin",
    `<canvas id="view"></canvas><main><p id="status" data-state="loading">Loading…</p><section id="game" hidden></section></main>`,
  );

  let startupError = null;
  try {
    await import(`${base}main.js`);
  } catch (error) {
    startupError = error;
  }

  const lines = [`run ${runId}`, `suite ${suite}`];
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
      for (const [name, test] of Object.entries(tests)) {
        try {
          // A test may return a note (e.g. a measured value) for the report.
          const note = await test();
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
