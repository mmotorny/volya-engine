// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Fails if any npm package in package-lock.json is not permissively licensed.

import { readFileSync } from "node:fs";

const allowed = new Set([
  "0BSD", "Apache-2.0", "BlueOak-1.0.0", "BSD-2-Clause", "BSD-3-Clause",
  "CC0-1.0", "ISC", "MIT", "Python-2.0", "Unlicense", "Zlib",
]);
const lock = JSON.parse(readFileSync(new URL("../package-lock.json", import.meta.url)));
let failed = false;
for (const [path, pkg] of Object.entries(lock.packages)) {
  if (path === "") continue;
  const ids = (pkg.license ?? "UNKNOWN").replace(/[()]/g, "").split(/\s+OR\s+/);
  if (!ids.some((id) => allowed.has(id))) {
    console.log(`disallowed license ${pkg.license ?? "(none)"}: ${path}`);
    failed = true;
  }
}
process.exit(failed ? 1 : 0);
