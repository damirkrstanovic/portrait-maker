#!/usr/bin/env bash
# Build the portable Linux deliverables from the pinned static libarchive.
#
# Run through `npm run package:linux`; this file deliberately keeps the
# toolchain setup visible instead of asking contributors to set PKG_CONFIG_PATH
# by hand.  The result is a .deb for Debian/Ubuntu and an AppImage for direct
# download on other mainstream Linux distributions.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
build_root="$project_root/target/packaging/libarchive"
prefix="$build_root/prefix"

command -v cmake >/dev/null || {
  echo "cmake is required to build the pinned libarchive." >&2
  exit 1
}

cmake -S "$project_root/packaging/libarchive" -B "$build_root/build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$prefix"
cmake --build "$build_root/build" --parallel
cmake --install "$build_root/build"

export PKG_CONFIG_PATH="$prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LIBARCHIVE_STATIC=1

cd "$project_root"
if [[ "${1:-}" == "--dev" ]]; then
  shift
  exec npm run tauri -- dev "$@"
fi
# linuxdeploy ships an older strip that cannot read modern .relr.dyn sections.
# Keep host libraries intact; this also works on the Ubuntu release runner.
export NO_STRIP=1
npm run tauri -- build --bundles "${1:-deb,appimage}" --ci

echo
echo "Packages created under target/release/bundle/:"
find target/release/bundle -maxdepth 2 -type f \( -name '*.deb' -o -name '*.AppImage' \) -print
