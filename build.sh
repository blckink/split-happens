#!/bin/bash

# Resolve a usable C toolchain before invoking cargo so crates that rely on
# build scripts (like `nix`) can link successfully even on systems without a
# `cc` shim (e.g. SteamOS default images).
if [ -z "${CC:-}" ] || ! command -v "$CC" >/dev/null 2>&1; then
  # Try a list of common compiler front-ends, preferring gcc-style naming to
  # stay compatible with the default GNU triple that Rust uses on Linux.
  for candidate in cc gcc clang clang-17 clang-16 clang-15; do
    if command -v "$candidate" >/dev/null 2>&1; then
      export CC="$candidate"
      break
    fi
  done
fi

# Mirror the detected C compiler for C++ if one was not already provided by the
# caller. Some crates pull in C++ helpers through the `cc` crate and expect CXX
# to follow suit.
if [ -n "${CC:-}" ] && { [ -z "${CXX:-}" ] || ! command -v "$CXX" >/dev/null 2>&1; }; then
  if [[ "$CC" == *clang* ]]; then
    for cxx_candidate in "${CC/clang/clang++}" clang++; do
      if command -v "$cxx_candidate" >/dev/null 2>&1; then
        export CXX="$cxx_candidate"
        break
      fi
    done
  else
    if command -v "${CC}++" >/dev/null 2>&1; then
      export CXX="${CC}++"
    fi
  fi
fi

# Abort early with actionable hints when no compiler was located.
if [ -z "${CC:-}" ] || ! command -v "$CC" >/dev/null 2>&1; then
  cat >&2 <<'EOF'
Error: no usable C compiler detected.

SteamOS / Arch:  sudo pacman -S --needed base-devel
Ubuntu / Debian: sudo apt-get update && sudo apt-get install build-essential

Re-run this script after installing the toolchain so Cargo can finish linking.
EOF
  exit 1
fi

cargo build --release && \
rm -rf build/partydeck
mkdir -p build/ build/res build/bin && \
cp target/release/partydeck build/ && \
cp LICENSE build/ && cp COPYING.md build/thirdparty.txt && \
cp res/splitscreen_kwin.js res/splitscreen_kwin_vertical.js build/res && \
gsc=$(command -v gamescope || true) && \
[ -n "$gsc" ] && cp "$gsc" build/bin/gamescope-kbm || true
