#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

# Fails if a source file lacks SPDX headers in its first five lines.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
while IFS= read -r file; do
  head=$(head -n 5 "$file")
  if ! grep -q 'SPDX-FileCopyrightText: ' <<<"$head" ||
    ! grep -q 'SPDX-License-Identifier: BSD-3-Clause' <<<"$head"; then
    echo "missing SPDX headers: $file"
    status=1
  fi
done < <(git ls-files --cached --others --exclude-standard \
  '*.rs' '*.sh' '*.js' '*.mjs' '*.html' '*.css' '*.svg' '*.toml')
exit $status
