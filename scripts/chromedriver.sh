#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

# Prints the path to a chromedriver matching the installed Google Chrome,
# downloading it into .cache/ on first use.
set -euo pipefail
cd "$(dirname "$0")/.."

chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
version="$("$chrome" --version | awk '{print $3}')"
npx --no-install @puppeteer/browsers install "chromedriver@$version" \
  --path "$PWD/.cache/browsers" --format '{{path}}' 2>/dev/null | tail -1
