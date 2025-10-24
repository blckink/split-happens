#!/usr/bin/env bash
# Launch helper for running Split Happens directly from Steam Big Picture Mode.
# The script tries to locate the launcher binary next to itself first and then
# falls back to the system PATH before forwarding recommended arguments.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_BINARY="${SCRIPT_DIR}/Split Happens"
ALT_BINARY="${SCRIPT_DIR}/split-happens"

# Seed Steam's Big Picture artwork with the bundled Split Happens icon so the
# library entry looks polished without any manual configuration.
apply_big_picture_art() {
  local app_id=""

  # Prefer explicit Steam identifiers when the client forwards them, falling
  # back to the overlay value which embeds the non-Steam game hash.
  if [[ -n "${SteamGameId:-}" ]]; then
    app_id="${SteamGameId}"
  elif [[ -n "${SteamGameID:-}" ]]; then
    app_id="${SteamGameID}"
  elif [[ -n "${SteamAppId:-}" && "${SteamAppId}" != "0" ]]; then
    app_id="${SteamAppId}"
  elif [[ -n "${SteamAppID:-}" && "${SteamAppID}" != "0" ]]; then
    app_id="${SteamAppID}"
  elif [[ -n "${SteamOverlayGameId:-}" ]]; then
    app_id="${SteamOverlayGameId#NonSteamGame }"
  elif [[ -n "${SteamOverlayGameID:-}" ]]; then
    app_id="${SteamOverlayGameID#NonSteamGame }"
  fi

  # Strip any lingering prefixes (e.g. "NonSteamGame ") so we end up with the
  # raw numeric app identifier used for grid artwork filenames.
  app_id="${app_id//[^0-9]/}"
  if [[ -z "${app_id}" ]]; then
    return
  fi

  local steam_root="${XDG_DATA_HOME:-$HOME/.local/share}/Steam"
  if [[ ! -d "${steam_root}" && -d "$HOME/.steam/steam" ]]; then
    steam_root="$HOME/.steam/steam"
  fi
  if [[ ! -d "${steam_root}" ]]; then
    return
  fi

  local art_source="${SCRIPT_DIR}/res/executable_icon.png"
  if [[ ! -f "${art_source}" ]]; then
    return
  fi

  # Propagate the hero and logo artwork to every local Steam profile so the
  # launcher looks consistent regardless of the signed-in account.
  shopt -s nullglob
  for userdir in "${steam_root}"/userdata/*; do
    [[ -d "${userdir}" ]] || continue
    local grid_dir="${userdir}/config/grid"
    mkdir -p "${grid_dir}"
    cp "${art_source}" "${grid_dir}/${app_id}_hero.png"
    cp "${art_source}" "${grid_dir}/${app_id}_logo.png"
  done
  shopt -u nullglob
}

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

# Refresh the Big Picture artwork before handing control over to the launcher.
apply_big_picture_art

exec "${APP_BINARY}" --kwin --fullscreen "$@"
