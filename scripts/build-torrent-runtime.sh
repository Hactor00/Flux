#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "$0")/.." && pwd)"
cd "$workspace"
if ! pkg-config --atleast-version=2.1.2 libtorrent-rasterbar; then
    printf '%s\n' 'libtorrent-rasterbar >= 2.1.2 and pkg-config are required.' >&2
    exit 1
fi
case "$(uname -s)" in
    Darwin) library=libflux_torrent.dylib; linker=(-dynamiclib) ;;
    Linux) library=libflux_torrent.so; linker=(-shared -Wl,-rpath,'$ORIGIN') ;;
    *) printf '%s\n' 'Unsupported native torrent build platform.' >&2; exit 1 ;;
esac
mkdir -p target/debug target/release
read -r -a flags <<< "$(pkg-config --cflags libtorrent-rasterbar)"
read -r -a libraries <<< "$(pkg-config --libs libtorrent-rasterbar)"
"${CXX:-c++}" -std=c++17 -O2 -fPIC -fvisibility=hidden "${linker[@]}" "${flags[@]}" native/torrent/bridge.cpp "${libraries[@]}" -o "target/debug/$library"
cp "target/debug/$library" "target/release/$library"
printf '%s\n' 'Native BEP 52 torrent bridge compiled and staged.'
