#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

# Full test suite incl. end-to-end browser tests. On demand only.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

step "Native tests"
cargo test --workspace
step "Wasm tests (headless Chrome)"
# No -q: cargo forwards it to wasm-bindgen-test-runner, which rejects it.
CHROMEDRIVER="$(scripts/chromedriver.sh)" cargo test --workspace --exclude volya-devserver --target wasm32-unknown-unknown
step "Web build"
scripts/build-web.sh
step "Browser tests (desktop Chrome, iPhone)"
npx playwright test --config tests/e2e/playwright.config.mjs
step "All tests passed"
