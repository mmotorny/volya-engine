#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

# Fast static checks (no tests). Run before every push.
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
cargo clippy --workspace --exclude volya-devserver --all-targets --target wasm32-unknown-unknown -- -D warnings
step "Static checks passed"
