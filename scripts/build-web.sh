#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

# Builds the web app into dist/. Usage: scripts/build-web.sh [--dev]
set -euo pipefail
cd "$(dirname "$0")/.."

profile=release
cargo_flags=(--release)
if [[ "${1:-}" == "--dev" ]]; then
  profile=debug
  cargo_flags=()
fi

cargo build -p volya-app --target wasm32-unknown-unknown "${cargo_flags[@]}"
rm -rf dist
mkdir -p dist
cp -R web/. dist/
wasm-bindgen --target web --no-typescript --out-dir dist/pkg \
  "target/wasm32-unknown-unknown/$profile/volya_app.wasm"
