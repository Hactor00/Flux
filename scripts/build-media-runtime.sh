#!/usr/bin/env bash
set -euo pipefail
: "$FLUX_MEDIA_WORK" "$FLUX_LLVM_BIN"
if command -v cygpath >/dev/null; then
  work="$(cygpath -u "$FLUX_MEDIA_WORK")"
  llvm="$(cygpath -u "$FLUX_LLVM_BIN")"
  pkgconf="$(cygpath -u "$FLUX_PKGCONF_BIN")"
  make=mingw32-make
  shell="$(cygpath -m /usr/bin/sh.exe)"
  export PKG_CONFIG=x86_64-w64-mingw32-pkg-config
else
  work="$FLUX_MEDIA_WORK"
  llvm="$FLUX_LLVM_BIN"
  pkgconf="/usr/bin"
  make=make
  shell=/bin/sh
  export PKG_CONFIG=pkg-config
fi
export PATH="$llvm:$pkgconf:/usr/bin:$PATH"
export MAKE="$make"
export CC=x86_64-w64-mingw32-clang
export AR=x86_64-w64-mingw32-ar
export RANLIB=x86_64-w64-mingw32-ranlib
export NM=x86_64-w64-mingw32-nm
prefix="$work/runtime"
trap 'tail -n 50 "$work"/*configure.log "$work"/*build.log 2>/dev/null || true' ERR
mkdir -p "$prefix"
echo "Configuring pinned libxml2 for Windows x64."
cd "$work/libxml2-2.15.4"
./configure --host=x86_64-w64-mingw32 --prefix="$prefix" \
  --disable-shared --enable-static --without-python --without-iconv \
  --without-zlib --without-http >"$work/xml-configure.log" 2>&1
echo "Building libxml2."
"$make" -j8 SHELL="$shell" >"$work/xml-build.log" 2>&1
"$make" SHELL="$shell" install >"$work/xml-install.log" 2>&1
# Libtool names LLVM's COFF archive .lib; GNU-mode LLD searches for .a.
if [ -f "$prefix/lib/libxml2.lib" ]; then cp "$prefix/lib/libxml2.lib" "$prefix/lib/libxml2.a"; fi
cd "$work/ffmpeg-9.0.2"
if [ -d "$work/lame-3.100" ]; then
  cd "$work/lame-3.100"
  echo "Configuring and building pinned LAME."
  ./configure --host=x86_64-w64-mingw32 --prefix="$prefix" --disable-shared --enable-static --disable-frontend --disable-decoder --disable-nasm CFLAGS="-O2 -Wno-int-conversion" >"$work/lame-configure.log" 2>&1
  "$make" -j8 SHELL="$shell" >"$work/lame-build.log" 2>&1
  "$make" SHELL="$shell" install >"$work/lame-install.log" 2>&1
  if [ -f "$prefix/lib/libmp3lame.lib" ]; then cp "$prefix/lib/libmp3lame.lib" "$prefix/lib/libmp3lame.a"; fi
  cd "$work/ffmpeg-9.0.2"
fi
if command -v cygpath >/dev/null; then export PKG_CONFIG_LIBDIR="$(cygpath -m "$prefix/lib/pkgconfig")"; else export PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig"; fi
echo "Configuring pinned FFmpeg for Windows x64."
./configure --target-os=mingw32 --arch=x86_64 --enable-cross-compile \
  --cc="$CC" --ar="$AR" --nm="$NM" --ranlib="$RANLIB" --strip=x86_64-w64-mingw32-strip \
  --pkg-config="$PKG_CONFIG" --pkg-config-flags=--static \
  --prefix="$prefix" --disable-autodetect --disable-debug --disable-doc --disable-x86asm \
  --disable-avdevice --disable-everything --enable-ffmpeg --enable-ffprobe \
  --enable-network --enable-schannel --enable-libxml2 --enable-libmp3lame --disable-pthreads --enable-w32threads \
  --enable-protocol=file,pipe,http,https,tcp,tls,crypto \
  --enable-demuxer=mov,matroska,mpegts,hls,dash,concat,mp3,aac,flac,ogg,avi,asf,flv,wav,webm_dash_manifest,webvtt,srt,ffmetadata \
  --enable-muxer=mp4,matroska,mp3,srt,webvtt,null,image2pipe --enable-encoder=libmp3lame,aac,movtext,srt,webvtt,pcm_s16le,mjpeg --enable-decoders --enable-parsers \
  --enable-bsf=aac_adtstoasc,h264_mp4toannexb,hevc_mp4toannexb,extract_extradata,av1_metadata,vp9_superframe \
  --enable-filter=null,anull,format,aformat,aresample,loudnorm,scale \
  --extra-cflags="-I$prefix/include -I$prefix/include/libxml2 -DLIBXML_STATIC" \
  --extra-ldflags="-L$prefix/lib" >"$work/ffmpeg-configure.log" 2>&1
echo "Building FFmpeg and FFprobe."
"$make" -j8 SHELL="$shell" ffmpeg.exe ffprobe.exe >"$work/ffmpeg-build.log" 2>&1
mkdir -p "$prefix/bin"
cp ffmpeg.exe ffprobe.exe "$prefix/bin/"
if command -v cygpath >/dev/null; then "$prefix/bin/ffmpeg.exe" -version >"$work/ffmpeg-version.txt" 2>&1; fi
echo "Media runtime built from pinned FFmpeg and libxml2 sources."
