#!/usr/bin/env bash
# Launch helper for running Split Happens directly from Steam Big Picture Mode.
# The script tries to locate the launcher binary next to itself first and then
# falls back to the system PATH before forwarding recommended arguments.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_BINARY="${SCRIPT_DIR}/Split Happens"
ALT_BINARY="${SCRIPT_DIR}/split-happens"

# If the packaged binary isn't present, prefer one available on the PATH.
if [[ ! -x "${APP_BINARY}" ]]; then
  if [[ -x "${ALT_BINARY}" ]]; then
    APP_BINARY="${ALT_BINARY}"
  else
    APP_BINARY="$(command -v split-happens || true)"
  fi
fi

if [[ -z "${APP_BINARY}" || ! -x "${APP_BINARY}" ]]; then
  echo "Split Happens binary not found. Place it next to this script or install it system-wide." >&2
  exit 1
fi

exec "${APP_BINARY}" --kwin --fullscreen "$@"
