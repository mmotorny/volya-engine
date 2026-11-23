// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import { defineConfig, devices } from "@playwright/test";

const port = 8123;

// Targets: desktop Chrome (installed), iOS/iPadOS Safari and Chrome (WebKit).
export default defineConfig({
  testDir: ".",
  outputDir: "../../test-results",
  reporter: [["list"]],
  use: { baseURL: `http://localhost:${port}` },
  webServer: {
    command: `node scripts/serve.mjs dist`,
    cwd: "../..",
    env: { PORT: String(port) },
    url: `http://localhost:${port}/`,
    reuseExistingServer: false,
  },
  projects: [
    { name: "desktop-chrome", use: { ...devices["Desktop Chrome"], channel: "chrome" } },
    { name: "iphone", use: { ...devices["iPhone 17"] } },
    { name: "ipad", use: { ...devices["iPad Pro 11"] } },
  ],
});
