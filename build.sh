#!/bin/bash

# Automatically hand off to the Steam Runtime toolchain when the host environment
# does not provide a `cc` shim so Steam Deck users do not have to install a
# separate compiler suite. The guard variable prevents an infinite re-exec loop
# if the runtime still lacks a usable linker.
if [ -z "${PARTYDECK_STEAMRUN_REEXEC:-}" ] && ! command -v cc >/dev/null 2>&1; then
  if command -v steam-run >/dev/null 2>&1; then
    export PARTYDECK_STEAMRUN_REEXEC=1
    exec steam-run "$0" "$@"
  fi
fi

cargo build --release && \
rm -rf build/partydeck
mkdir -p build/ build/res build/bin && \
cp target/release/partydeck build/ && \
cp LICENSE build/ && cp COPYING.md build/thirdparty.txt && \
cp res/splitscreen_kwin.js res/splitscreen_kwin_vertical.js build/res && \
gsc=$(command -v gamescope || true) && \
[ -n "$gsc" ] && cp "$gsc" build/bin/gamescope-kbm || true
