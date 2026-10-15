#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "$0")/.." && pwd)"
cd "$workspace"
stage="$workspace/.tools/linux-package"
mkdir -p "$stage" release
bash scripts/bootstrap-torrent-runtime.sh
patchelf --set-rpath '$ORIGIN' target/release/libflux_torrent.so
cp -L .tools/torrent-system/lib/libtorrent-rasterbar.so.2.1 "$stage/"
cp .tools/torrent-source/libtorrent-rasterbar-2.1.2/LICENSE "$stage/libtorrent-LICENSE"
python3 - <<'PY'
from pathlib import Path
root = Path('.tools/torrent-source/libtorrent-rasterbar-2.1.2')
notices = []
for path in sorted(root.rglob('*')):
    if path.is_file() and any(term in path.name.lower() for term in ('license', 'copying', 'copyright', 'notice')) and path.stat().st_size < 1024 * 1024:
        notices.append(f'\n## {path.relative_to(root)}\n\n' + path.read_text(errors='replace'))
Path('.tools/linux-package/TORRENT-THIRD-PARTY-NOTICES.txt').write_text('Flux native torrent dependencies\n' + '\n'.join(notices))
PY
curl --fail --location --retry 3 https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp -o "$stage/yt-dlp"
curl --fail --location --retry 3 https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/SHA2-256SUMS -o "$stage/SHA2-256SUMS"
(cd "$stage" && grep -E '  yt-dlp$' SHA2-256SUMS | sha256sum --check --strict)
curl --fail --location --retry 3 https://raw.githubusercontent.com/yt-dlp/yt-dlp/2026.08.19/LICENSE -o "$stage/yt-dlp-LICENSE"
curl --fail --location --retry 3 https://raw.githubusercontent.com/yt-dlp/yt-dlp/2026.08.19/THIRD_PARTY_LICENSES.txt -o "$stage/yt-dlp-THIRD-PARTY-LICENSES"
chmod 755 "$stage/yt-dlp"
npm ci
npm test
export FLUX_REQUIRE_MEDIA_TESTS=1
export LD_LIBRARY_PATH="$workspace/.tools/torrent-system/lib:${LD_LIBRARY_PATH:-}"
cp "$stage/yt-dlp" target/debug/yt-dlp
cp "$stage/yt-dlp" target/release/yt-dlp
cargo test --locked -p flux-core -p flux-cli -p flux-native-host -p flux-desktop
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo build --locked --release -p flux-cli -p flux-native-host
npm run tauri build -- --config src-tauri/tauri.linux.conf.json --bundles rpm --no-binary-patching
version="$(node -p 'JSON.parse(require("fs").readFileSync("package.json")).version')"
cp target/release/bundle/rpm/*.rpm "release/Flux-${version}-linux-x86_64.rpm"
sha256sum "release/Flux-${version}-linux-x86_64.rpm" > release/SHA256SUMS-linux.txt
rpm -qip "release/Flux-${version}-linux-x86_64.rpm"
rpm -qlp "release/Flux-${version}-linux-x86_64.rpm"
