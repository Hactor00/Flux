# Flux 2.0 validation

This record describes local Windows x64 validation of the 2.0 upgrade. The public 1.7 tag and its release assets are unchanged. Exact test counts, timestamps and binary identities are recorded in [release.json](validation/release.json).

| Automated suite | Passed |
| --- | ---: |
| Rust core, native host, CLI and desktop registration | 100 |
| Shared browser extension | 51 |
| Frontend data and progress | 12 |
| Total | 163 |

Native GUI, packaged Firefox, installer/uninstaller, daily-use and Chromium layout scenarios run separately from these counts. Production npm audit reported zero advisories; strict workspace Clippy, Rust formatting, TypeScript build and the modern Firefox manifest lint passed.

## Core transfer coverage

HTTP tests verify byte-for-byte downloads, 32 disjoint workers, NTFS sparse staging, no chunk files, slow-worker splitting, range negotiation, strong content validators, bounded retries and rate limiting. Pause/reopen validates saved prefixes before reuse. Corrupted checkpoints and changed ETags cause safe re-downloads. Legacy chunk checkpoints migrate into the direct staging file without losing verified prefixes. Publication preserves existing files and waits for the combined range/final SHA-256 verification.

Two session recovery scenarios exercise initial and mid-transfer expiry. The mid-transfer fixture interrupts a response after bytes were written, then returns 403 and 410. Browser renewal supplies a new URL; range requests resume from nonzero offsets and the final SHA-256 matches the fixture.

Seven embedded torrent scenarios cover real seeder/leecher and magnet traffic, HTTP metainfo bounds, multi-file selection, path traversal rejection, destination ownership, pause/reopen with corrupt-piece repair and the shared HTTP/torrent bandwidth budget. Selected output files are checked against exact SHA-256 values.

Real FFmpeg/FFprobe fixtures cover HLS, DASH, mismatched MIME, extensionless manifests, segments named .jpg, audio/video muxing, selected MP3 audio, normalization, chapters and subtitles. Probe results include the fixture's actual 12 fps video rate. Remote manifests cannot read local files.

## Native desktop and installation

The NSIS isolation harness installs the exact package under a temporary directory. It checks all bundled binaries and extension source hashes, five Windows browser registrations, repeated repair/removal, cold-start Native Messaging, concurrent requests, actual file downloads and uninstall behavior. Tests restore the previous registry and startup values; uninstall preserves user data.

Native window tests exercise automatic defaults, 32 connections, creation/start/completion timestamps, multiple details-window close/reopen cycles and active download pause/reopen. Unknown-size finite HLS shows duration-based progress and estimated remaining time, and finishes as a valid audio/video file. Its [screenshot](images/media-progress-2.0.png) replaces the temporary profile path with a neutral label after assertions.

The [torrent window test](validation/torrent-window.json) imports real metainfo through the installed desktop, transfers 32 pieces from a local TCP peer, changes/preserves pause state, reopens the details window and verifies final file bytes. It also checks that the virtualized row visibly displays the filename, size and integrity indicator. Its animation-frame samples measure responsiveness on the test machine; they do not establish a universal 60 or 120 fps rendering guarantee.

## Browser coverage

Firefox 120 and 157 tests temporarily install the actual packaged addon, connect its Native Messaging bridge, capture cookies for the fixture origin, intercept browser downloads and verify output hashes. They test consent withdrawal, Shadow DOM isolation, strict page CSP, fullscreen/resize, viewport-wide HUD movement, outside-click closing and registry restoration. Firefox 120 uses the DOM/network fallback for MAIN-world hints.

The Chromium layout fixture loads the actual HUD into a controlled browser page. It validates viewport edges, clipping, responsive placement and dismissal. It is not an automated store-installed Chrome integration test. Shared extension unit tests exercise Chromium event/API contracts, durable batch admission, restart/replay, origin-scoped credential renewal, stale source guards, audio/subtitle choices and magnet proposals.

The [store package check](validation/browser-stores.json) verifies 57 runtime files in each Chrome, Edge and Firefox ZIP against source hashes. Tests and developer-only files are excluded; modern Firefox manifest lint reports zero errors and warnings. This validation does not request a store review or Mozilla signature.

## Reproduction

Use the pinned toolchain described in CONTRIBUTING.md. Set FLUX_REQUIRE_MEDIA_TESTS=1 with the bundled FFmpeg/FFprobe and a full fixture encoder, then run:

```powershell
cargo fmt --all -- --check
cargo test -p flux-core -p flux-cli -p flux-native-host --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
npm test
./scripts/test-installer.ps1
node scripts/test-daily-use.mjs target/release
node scripts/test-chrome-hud.mjs
node scripts/record-hud-validation.mjs
node scripts/record-release-validation.mjs
./scripts/export-release.ps1
./scripts/verify-release.ps1
```

The installer harness requires the ordinary desktop instance to be closed because production builds share a single-instance lock. It uses a separate profile and never tests against the user's download database. The Firefox fixture requires the prepared browser runtimes and geckodriver. The native torrent scenario is included in the installer harness and can also run through scripts/test-torrent-window.mjs.

## Boundaries

The package is unsigned until external Authenticode/AMO signing is configured. Store schemas and package contents are checked; store review is a separate process. BitTorrent v2-only metadata is explicitly rejected. Direct HTTP writes use bounded network buffers and sparse offset writes; they are not kernel-level network-to-disk zero-copy. Final SHA-256 verification still reads the completed staging file.

Linux/macOS execution, physical OLE drops into third-party programs, real sleep/hibernate/shutdown, multilingual native-speaker review, long-running WAN stress and comparative IDM/FDM/JDownloader/Motrix benchmarks are outside this local run. DRM, CAPTCHA and provider authentication challenges are not bypassed. The WebView2 runtime may emit a class-unregistration warning during test teardown; the torrent report records it explicitly and rejects unexpected stderr.
