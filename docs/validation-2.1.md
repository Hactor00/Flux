# Flux 2.1 validation

This historical validation record describes the 2.1 development tree. Published 1.7.0 and 2.0.0 artifacts are preserved.

## Commands

```powershell
. ./scripts/use-local-rust.ps1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm test
./scripts/build-torrent-runtime.ps1 -SkipDownload
```

## Windows results

| Check | Result |
| --- | --- |
| Rust workspace, all features | 111 passed, zero failures |
| Chromium/Gecko extension regression tests | 55 passed |
| Frontend data tests | 12 passed |
| Strict TypeScript, Vite, Rust format and Clippy | Passed; warnings denied |
| Twenty desktop locales | 219 matching keys |
| Production npm dependency audit | Zero reported vulnerabilities |
| Firefox production manifest lint | Zero errors, warnings or notices |
| Chrome 155 HUD | Viewport dragging, clipping, dismissal and responsive placement passed |
| Firefox 120 and 157 installed integration | Native framing, cookies, consent, interception, HUD and CSP passed |
| Native desktop | Approval/settings/detail windows, repeated destruction/reopening, automatic connections, timestamps and new settings panels passed |
| Media windows | Misleading MIME HLS, estimates, ETA, pause/resume, subtitles, chapters, normalization and in-flight renewal passed |

The Windows fixture suite exercises actual network transfers, disk writes and restart checkpoints. Native torrent tests compare complete payload bytes and SHA-256 for v2, hybrid and v2 magnet transfers, sharing pause, offline resume, corrupted-piece repair, and restart with the download queue disabled. Repeated tests exposed and fixed the completion-before-disk-flush race. Final verification waits for libtorrent's cache-flushed event; moving the destination triggers piece rechecking before network transfer resumes. The installed runtime also imports actual BEP 52 metadata without a development toolchain on PATH.

Service tests inspect encrypted SQLite records and verify that completed browser torrents with implicit options can change sharing rules without changing their file selection. Provider contract tests use local HTTP endpoints and verify authorization, caching, refresh and secret boundaries. The stalled HTTP fixture requires a new range beginning at byte 65536 within the watchdog deadline. Native Messaging tests exchange 20 concurrent requests, verify nine downloads and check active-shutdown resume. Installer tests compare all 16 runtime binaries, validate all five browser registrations, exercise repeated repair/removal, and verify data preservation and registry restoration after uninstall.

Updater tests verify an actual owner-key signature, reject modified bytes and reject a mismatched announced version. Durable update checkpoints restore queue activity after restart or aborted installation. Final installers receive version-bound update signatures and SHA-256 checksums. Update signatures and Windows Authenticode publisher signatures are separate checks.

Linux and macOS jobs are defined in `.github/workflows/ci.yml`; they have not executed for this local branch. Live Debrid accounts, EV/OV certificate signing, SignPath submission and store publication require the publisher's corresponding credentials. A live GitHub update/install round trip has not been run; version-bound payload verification, queue recovery and the actual NSIS installer are tested separately. The 2.1 branch and packages have not been published. Native sleep/hibernate/shutdown and physical drops into unrelated applications are not performed on the user's machine.
