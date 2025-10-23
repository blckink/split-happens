#!/bin/bash

# Automatically hand off to the Steam Runtime toolchain when the host environment
# does not provide a `cc` shim so Steam Deck users do not have to install a
# separate compiler suite. The guard variable prevents an infinite re-exec loop
# if the runtime still lacks a usable linker.
# Automatically fall back to clang/gcc shims when the runtime exposes
# versioned toolchains without the generic `cc` symlink so we still provide a
# linker to Cargo in SteamOS environments.
if ! command -v cc >/dev/null 2>&1; then
  if [ -z "${PARTYDECK_STEAMRUN_REEXEC:-}" ] && command -v steam-run >/dev/null 2>&1; then
    export PARTYDECK_STEAMRUN_REEXEC=1
    exec steam-run "$0" "$@"
  fi

  # Surface the first available compiler shim for Rust's build scripts.
  if command -v clang >/dev/null 2>&1; then
    export CC=${CC:-clang}
    export CXX=${CXX:-clang++}
  elif command -v gcc >/dev/null 2>&1; then
    export CC=${CC:-gcc}
    export CXX=${CXX:-g++}
  fi
fi

# Tighten the release profile when building on SteamOS/Steam Deck hardware so
# the binaries benefit from the platform's Zen 2 CPU and lean linker settings
# without requiring manual cargo configuration tweaks.
if [ -r /etc/os-release ] && grep -qi 'steamos' /etc/os-release; then
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-cpu=znver2 -C link-arg=-Wl,-O1"
  export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-thin}"
fi

cargo build --release && \
rm -rf build/partydeck
mkdir -p build/ build/res build/bin && \
cp target/release/partydeck build/ && \
command -v strip >/dev/null 2>&1 && strip build/partydeck || true && \
cp LICENSE build/ && cp COPYING.md build/thirdparty.txt && \
cp res/splitscreen_kwin.js res/splitscreen_kwin_vertical.js build/res && \
gsc=$(command -v gamescope || true) && \
[ -n "$gsc" ] && cp "$gsc" build/bin/gamescope-kbm || true
