#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

# Full local check. Run before every push.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

step "SPDX headers"
scripts/check-spdx.sh
step "Dependency licenses"
cargo deny --log-level error check
node scripts/check-npm-licenses.mjs
step "Format"
cargo fmt --all --check
step "Clippy"
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown -- -D warnings
step "Native tests"
cargo test --workspace
step "Wasm tests (headless Chrome)"
# No -q: cargo forwards it to wasm-bindgen-test-runner, which rejects it.
CHROMEDRIVER="$(scripts/chromedriver.sh)" cargo test --workspace --target wasm32-unknown-unknown
step "Web build"
scripts/build-web.sh
step "Browser tests (desktop Chrome, iPhone)"
npx playwright test --config tests/e2e/playwright.config.mjs
step "All checks passed"
