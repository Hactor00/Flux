# Flux 2.3 engineering and packaging

## Watched torrent folders

Settings persist an opt-in `torrent_watch` object with `folders`, `destination` and `mode` (`approval` or `automatic`). Older preference documents deserialize with monitoring disabled. A dedicated asynchronous scan loop runs alongside the transfer scheduler; queue admission, concurrency limits, bandwidth policy and seeding remain owned by the engine.

Only directly contained regular `.torrent` files are eligible. Symbolic-link entries are excluded. Files must have an unchanged length and modification timestamp in consecutive observations, and are checked again after bounded reading. Metadata uses the existing guarded v1/v2 validation. Scans cap the folder count, entry count, individual file size, candidate count and buffered bytes.

The SHA-256 of the complete metadata identifies byte-identical imports. Task insertion and the duplicate ledger commit in one SQLite transaction. Ledger rows survive task removal. Renamed copies and restart rescans do not create another task. Manual import is unaffected. Files with equivalent info hashes but different complete metadata are not considered byte-identical by the watcher.

Failures are isolated per file/folder, exposed through `torrent_watch_report` and retried with a thirty-second backoff. Changing a failed file clears its backoff; a missing directory is retried on later scans. Source files are not moved or deleted. Monitoring does not run after Flux exits; files created while closed are discovered on the next start.

## Linux runtime layout

The RPM installs `flux-desktop`, `flux-cli` and `flux-native-host` under `/usr/bin`. Private torrent libraries and the pinned yt-dlp zipapp live under `/usr/lib/flux`. License notices live under `/usr/share/licenses/flux`. The native bridge has an `$ORIGIN` runtime search path so it resolves its adjacent pinned libtorrent library without a global library-path override.

The package declares WebKitGTK 4.1, GTK, AppIndicator, OpenSSL, FFmpeg, Python, Node and xdg-utils dependencies. The supported Fedora baseline supplies OpenSSL 3, Python 3.10+ and Node 20+. System FFmpeg codec availability follows the distribution's build. The Windows distribution continues to bundle its existing pinned media runtime. The RPM includes the original libtorrent source archive and collected dependency notices alongside its license files.

Browser-host manifests are written for the current user on first launch. Chromium-family directories use XDG configuration paths; Firefox uses its native host directory. The extension packages are shared with Windows. Browser sandbox confinement is outside the RPM's permissions. XDG autostart is opt-in and supports settings-save rollback. Power operations use systemctl without interactive password prompting; authorization failures return errors.

RPM upgrades use `dnf`. The Linux Updates control opens the public release page rather than attempting to install a Windows update artifact. Windows updater signatures and behavior remain unchanged.

## Building Linux

Build on Ubuntu 24.04 or a compatible baseline with the Tauri prerequisites, Rust and Node installed. The resulting RPM targets Fedora 43 x86_64 or newer with glibc 2.39 or newer:

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libboost-dev libboost-json-dev cmake ninja-build pkg-config patchelf rpm ffmpeg
bash scripts/build-linux-release.sh
```

The script verifies pinned libtorrent sources, validates the yt-dlp archive against its release checksums, builds the native bridge, runs frontend/core/desktop tests and Clippy, and generates the RPM. `.github/workflows/linux-release.yml` then installs it into Fedora and exercises the installed application as an unprivileged user under Xvfb.

The release contains Windows installer/portable packages, the Linux RPM, shared Chromium/Firefox extension ZIPs, a source archive, checksums and the Windows updater manifest/signature. Existing release tags and assets remain intact when main's intermediate work is consolidated into release commits.

## Assembling validated packages

Record successful Windows, Linux, platform-matrix and native lifecycle run identities in `docs/validation/release-2.3.json`. Import the Windows package's current validation records before creating the final release commit. Run the `Assemble verified platform release` workflow against that commit with the Windows and Linux workflow IDs.

Assembly verifies build provenance and checks that runtime/build sources match the tested commits. It checks the original package checksums, installed installer identity, every portable binary and the tested RPM digest. It refreshes portable documentation, exports the final Git tree as the source archive and retains the installer/update signature unchanged. Uploads target an unpublished draft and are checked against GitHub's SHA-256 asset digests. Previously published assets are preserved. Publishing the verified draft is a separate maintainer action.
