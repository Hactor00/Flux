#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
mkdir -p .tools/media-sources .tools/media-work .tools/media-artifact/media-source
fetch() {
  curl --fail --location --retry 3 --connect-timeout 30 --max-time 300 "$1" -o "$2"
  echo "$3  $2" | sha256sum --check --strict
}
toolchain=llvm-mingw-20261006-ucrt-ubuntu-22.04-x86_64
fetch "https://github.com/mstorsjo/llvm-mingw/releases/download/20261006/$toolchain.tar.xz" .tools/llvm-mingw-linux.tar.xz 5f9c6ed95b2d4bdb2869a488c5fd5857fbdabcf288a0aa3eb1da43f6a08d8ab4
tar -xf .tools/llvm-mingw-linux.tar.xz -C .tools
fetch https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz .tools/media-sources/ffmpeg-9.0.2.tar.xz 8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
fetch https://download.gnome.org/sources/libxml2/2.15/libxml2-2.15.4.tar.xz .tools/media-sources/libxml2-2.15.4.tar.xz 98087fd181d9070724f3fbc65c7377db03038eb92bd882374daff44940138821
fetch https://download.videolan.org/pub/contrib/lame/lame-3.100.tar.gz .tools/media-sources/lame-3.100.tar.gz ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e
for source in .tools/media-sources/*; do tar -xf "$source" -C .tools/media-work; done
export FLUX_MEDIA_WORK="$root/.tools/media-work"
export FLUX_LLVM_BIN="$root/.tools/$toolchain/bin"
timeout 30m bash scripts/build-media-runtime.sh
artifact="$root/.tools/media-artifact"
cp .tools/media-work/runtime/bin/{ffmpeg.exe,ffprobe.exe} "$artifact/"
cp ".tools/$toolchain/x86_64-w64-mingw32/bin/libunwind.dll" "$artifact/"
cp .tools/media-sources/* "$artifact/media-source/"
cp .tools/media-work/ffmpeg-9.0.2/COPYING.LGPLv2.1 "$artifact/media-source/FFmpeg-LICENSE.txt"
cp .tools/media-work/libxml2-2.15.4/Copyright "$artifact/media-source/libxml2-LICENSE.txt"
cp .tools/media-work/lame-3.100/COPYING "$artifact/media-source/LAME-LICENSE.txt"
cp ".tools/$toolchain/LICENSE.TXT" "$artifact/media-source/LLVM-runtime-LICENSE.txt"
cp scripts/build-media-runtime.sh scripts/build-windows-media.sh "$artifact/media-source/"
python3 - <<'PY'
import json, os
from pathlib import Path
Path('.tools/media-artifact/provenance.json').write_text(json.dumps({'run_id':os.environ['GITHUB_RUN_ID'],'source_commit':os.environ['GITHUB_SHA'],'toolchain_sha256':'5f9c6ed95b2d4bdb2869a488c5fd5857fbdabcf288a0aa3eb1da43f6a08d8ab4','target':'x86_64-w64-mingw32','ffmpeg':'9.0.2'},indent=2)+'\n')
PY
(cd "$artifact" && find . -type f ! -name SHA256SUMS.txt -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS.txt)
file "$artifact/ffmpeg.exe" "$artifact/ffprobe.exe"
