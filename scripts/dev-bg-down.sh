#!/usr/bin/env bash
set -euo pipefail

session="catnap-dev"

if command -v screen >/dev/null 2>&1; then
  screen -S "$session" -X quit >/dev/null 2>&1 || true
fi

echo "stopped screen session '$session' (if it existed)."

