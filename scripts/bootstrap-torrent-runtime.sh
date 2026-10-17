#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "$0")/.." && pwd)"
cd "$workspace"
version=2.1.2
archive=".tools/libtorrent-$version.tar.gz"
checksum=3362546d9cd71b9e49ee6cac7d3f1f914ce9cdb217c86b63d5b22cbed0334dbc
prefix="$workspace/.tools/torrent-system"
mkdir -p .tools/torrent-source
if [ ! -f "$archive" ]; then
    curl --fail --location --proto '=https' --tlsv1.2 --retry 3 "https://github.com/arvidn/libtorrent/releases/download/v$version/libtorrent-rasterbar-$version.tar.gz" --output "$archive"
fi
if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$archive" | cut -d ' ' -f 1)"
else
    actual="$(shasum -a 256 "$archive" | cut -d ' ' -f 1)"
fi
[ "$actual" = "$checksum" ] || { printf '%s\n' 'Torrent source checksum mismatch.' >&2; exit 1; }
tar -xf "$archive" -C .tools/torrent-source
if [ ! -f "$prefix/lib/pkgconfig/libtorrent-rasterbar.pc" ]; then
    cmake -S ".tools/torrent-source/libtorrent-rasterbar-$version" -B .tools/torrent-native-build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" -DBUILD_SHARED_LIBS=ON -Dbuild_tests=OFF -Dbuild_examples=OFF -Dpython-bindings=OFF
    cmake --build .tools/torrent-native-build --parallel "${FLUX_NATIVE_BUILD_JOBS:-2}"
    cmake --install .tools/torrent-native-build
fi
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
export LD_LIBRARY_PATH="$prefix/lib:${LD_LIBRARY_PATH:-}"
export DYLD_LIBRARY_PATH="$prefix/lib:${DYLD_LIBRARY_PATH:-}"
bash scripts/build-torrent-runtime.sh
cp "$prefix"/lib/libtorrent-rasterbar.* target/debug/
cp "$prefix"/lib/libtorrent-rasterbar.* target/release/
