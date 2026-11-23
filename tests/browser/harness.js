// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// In-page browser test harness. Runs a suite and POSTs a plain-text report to
// the dev server:
//
//   run <id>
//   suite <name>
//   ok <test>[\t<note>]
//   skip <test>\t<reason>
//   fail <test>\t<message>
//
// A report with no `fail` lines and at least one `ok` line passes.
//
// `?env=<name>` tells tests where they run (e.g. `ios-simulator`), for
// skipping checks a known environment can't perform.

const errors = [];
let runId = "manual";

/** Starts capturing page errors. Call before the engine starts. */
export function install(id) {
  runId = id ?? runId;
  addEventListener("error", (e) => errors.push(`error: ${e.message}`));
  addEventListener("unhandledrejection", (e) => errors.push(`unhandled rejection: ${e.reason}`));
  const consoleError = console.error;
  console.error = (...args) => {
    errors.push(`console.error: ${args.join(" ")}`);
    consoleError(...args);
  };
}

/** Where the suite runs, from `?env=`; `null` if unspecified. */
export const env = new URLSearchParams(location.search).get("env");

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

/** Resources that failed to load, where the browser reports status codes. */
function failedResources() {
  return performance
    .getEntriesByType("resource")
    .filter((r) => r.responseStatus >= 400)
    .map((r) => `HTTP ${r.responseStatus}: ${r.name}`);
}

export async function run(suite, startupError) {
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
    await fetch("./__test/results", { method: "POST", body: report });
  } catch {
    // No dev server (e.g. GitHub Pages): the on-page report is the output.
  }
  const pre = document.createElement("pre");
  pre.id = "test-report";
  pre.textContent = report;
  document.body.append(pre);
}
