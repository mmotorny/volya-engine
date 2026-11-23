// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import { defineConfig, devices } from "@playwright/test";

const port = 8123;

// Targets: desktop Chrome (installed), iOS Safari and Chrome (WebKit).
export default defineConfig({
  testDir: ".",
  outputDir: "../../test-results",
  reporter: [["list"]],
  // Mirrors GitHub Pages: project site under /volya-engine/.
  use: { baseURL: `http://localhost:${port}/volya-engine/` },
  webServer: {
    command: `cargo run -q -p volya-devserver -- --root dist --port ${port}`,
    cwd: "../..",
    url: `http://localhost:${port}/volya-engine/`,
    reuseExistingServer: false,
  },
  projects: [
    { name: "desktop-chrome", use: { ...devices["Desktop Chrome"], channel: "chrome" } },
    { name: "iphone", use: { ...devices["iPhone 17 Pro Max"] } },
  ],
});
