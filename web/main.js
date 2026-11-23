// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import init, { start, test_status } from "./pkg/volya_app.js";

// `?test=<suite>` runs a browser test suite. The harness is served by the dev
// server only; it never ships in dist/.
const params = new URLSearchParams(location.search);
const suite = params.get("test");
const harness = suite ? await import("./__test/harness.js") : null;
harness?.install(params.get("run"));

// Engine test interface for tests and debugging (see docs/testing.md).
globalThis.volya = {
  status: () => JSON.parse(test_status()),
};

let startupError = null;
try {
  await init();
  start();
} catch (error) {
  startupError = error;
  const status = document.getElementById("status");
  status.textContent = `Failed to start: ${error}`;
  status.dataset.state = "error";
}

if (harness) {
  await harness.run(suite, startupError);
} else if (startupError) {
  throw startupError;
}
