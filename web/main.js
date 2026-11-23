// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

import init, { start } from "./pkg/volya_app.js";

try {
  await init();
  start();
} catch (error) {
  const status = document.getElementById("status");
  status.textContent = `Failed to start: ${error}`;
  status.dataset.state = "error";
  throw error;
}
