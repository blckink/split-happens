#!/bin/bash

# Automatically hand off to the Steam Runtime toolchain when the host environment
# does not provide a `cc` shim so Steam Deck users do not have to install a
# separate compiler suite. The guard variable prevents an infinite re-exec loop
# if the runtime still lacks a usable linker.
# Automatically fall back to clang/gcc shims when the runtime exposes
# versioned toolchains without the generic `cc` symlink so we still provide a
# linker to Cargo in SteamOS environments.

# Capture host-side runtime paths before re-executing inside steam-run so we can
# still discover libgcc/glibc once the sandbox hides the system ld cache.
collect_host_rust_lld_assets() {
  # Skip collection when ldconfig is unavailable (e.g., stripped-down images).
  if ! command -v ldconfig >/dev/null 2>&1; then
    return
  fi

  local libpath libdir host_dirs="" host_libgcc="" shim_dir="target/rust-lld-shims"

  while IFS= read -r libpath; do
    libdir=$(dirname "$libpath")
    case ":${host_dirs}:" in
      *":${libdir}:"*)
        continue
        ;;
    esac

    host_dirs="${host_dirs:+${host_dirs}:}${libdir}"

    if [ -z "$host_libgcc" ] && [ "$(basename "$libpath")" = "libgcc_s.so.1" ]; then
      # Copy libgcc into the repository so the steam-run sandbox can always
      # access it, then expose that local path to the follow-up linker setup.
      if mkdir -p "$shim_dir" && cp -f "$libpath" "$shim_dir/libgcc_s.so.1" 2>/dev/null; then
        host_libgcc="$shim_dir/libgcc_s.so.1"
      else
        host_libgcc="$libpath"
      fi
    fi
  done < <(ldconfig -p | awk -F'=> ' 'NF==2 {gsub(/^ +| +$/, "", $2); print $2}')

  if [ -n "$host_dirs" ]; then
    export PARTYDECK_HOST_RUST_LLD_DIRS="$host_dirs"
  fi

  if [ -n "$host_libgcc" ]; then
    export PARTYDECK_HOST_LIBGCC="$host_libgcc"
  fi
}

add_rust_lld_search_paths() {
  # Gather shared library directories from ldconfig so rust-lld can discover
  # glibc when no system compiler wrapper is present. While scanning, capture
  # the first libgcc path so we can fabricate the unversioned SONAMEs that
  # rust-lld expects when cc is unavailable.
  local libpath libdir added_dirs="" libgcc_source="" shim_dir="target/rust-lld-shims" shim_copy="$shim_dir/libgcc_s.so.1"
  local -a host_dir_array=()

  if ! command -v ldconfig >/dev/null 2>&1; then
    return
  fi

  while IFS= read -r libpath; do
    libdir=$(dirname "$libpath")
    case " ${added_dirs} " in
      *" ${libdir} "*)
        continue
        ;;
    esac

    added_dirs+=" ${libdir}"
    RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=${libdir}"

    # Remember where libgcc_s lives so we can expose libgcc aliases later on.
    if [ -z "$libgcc_source" ] && [ "$(basename "$libpath")" = "libgcc_s.so.1" ]; then
      libgcc_source="$libpath"
    fi
  done < <(ldconfig -p | awk -F'=> ' 'NF==2 {gsub(/^ +| +$/, "", $2); print $2}')

  # Merge host directories captured before the steam-run re-exec so we still
  # see glibc/libgcc when the runtime does not expose them.
  if [ -n "${PARTYDECK_HOST_RUST_LLD_DIRS:-}" ]; then
    IFS=':' read -r -a host_dir_array <<<"${PARTYDECK_HOST_RUST_LLD_DIRS}"
    for libdir in "${host_dir_array[@]}"; do
      [ -n "$libdir" ] || continue
      case " ${added_dirs} " in
        *" ${libdir} "*)
          continue
          ;;
      esac

      added_dirs+=" ${libdir}"
      RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=${libdir}"
    done
  fi

  if [ -z "$libgcc_source" ] && [ -n "${PARTYDECK_HOST_LIBGCC:-}" ]; then
    libgcc_source="$PARTYDECK_HOST_LIBGCC"
  fi

  if [ -n "$libgcc_source" ] && [ -r "$libgcc_source" ]; then
    # Mirror libgcc into the shim directory so the runtime never depends on
    # host-only library paths, then surface the expected SONAME aliases.
    if mkdir -p "$shim_dir"; then
      if [ "$libgcc_source" != "$shim_copy" ]; then
        cp -f "$libgcc_source" "$shim_copy" 2>/dev/null && libgcc_source="$shim_copy"
      fi

      ln -sf "libgcc_s.so.1" "$shim_dir/libgcc.so"
      ln -sf "libgcc_s.so.1" "$shim_dir/libgcc_s.so"
    fi

    case " ${added_dirs} " in
      *" ${shim_dir} "*)
        ;;
      *)
        RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=${shim_dir}"
        ;;
    esac
  fi

  export RUSTFLAGS
}

if ! command -v cc >/dev/null 2>&1; then
  if [ -z "${PARTYDECK_STEAMRUN_REEXEC:-}" ] && command -v steam-run >/dev/null 2>&1; then
    # Record host linker search paths before entering steam-run so rust-lld can
    # still reach libgcc after the sandbox hides the system ld cache.
    collect_host_rust_lld_assets
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

  if ! command -v "${CC:-cc}" >/dev/null 2>&1; then
    # Fall back to rust-lld as the linker when no system compiler shims are
    # present so Deck users can still build without installing toolchains.
    export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C linker=rust-lld"
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER:-rust-lld}
    add_rust_lld_search_paths
  fi
fi

# Tighten the release profile when building on SteamOS/Steam Deck hardware so
# the binaries benefit from the platform's Zen 2 CPU and lean linker settings
# without requiring manual cargo configuration tweaks.
if [ -r /etc/os-release ] && grep -qi 'steamos' /etc/os-release; then
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-cpu=znver2 -C link-arg=-O1"
  export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-thin}"
fi

cargo build --release && {
  # Remove legacy PartyDeck artifacts before staging the new Split Happens binary.
  rm -rf "build/Split Happens"
  rm -rf build/partydeck
} && \
mkdir -p build/ build/res build/bin && \
cp target/release/split-happens "build/Split Happens" && \
command -v strip >/dev/null 2>&1 && strip "build/Split Happens" || true && \
cp LICENSE build/ && cp COPYING.md build/thirdparty.txt && \
# Bundle the Big Picture helper so Steam users can add Split Happens quickly.
cp split_happens_big_picture.sh build/ && chmod +x build/split_happens_big_picture.sh && \
cp res/splitscreen_kwin.js res/splitscreen_kwin_vertical.js build/res && \
gsc=$(command -v gamescope || true) && \
[ -n "$gsc" ] && cp "$gsc" build/bin/gamescope-kbm || true
