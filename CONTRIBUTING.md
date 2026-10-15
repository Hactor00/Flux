# Contributing to Flux

Use Rust 1.99.0, Node.js 24.18.0 and the platform prerequisites documented by Tauri 2. Keep `Cargo.lock` and `package-lock.json` under version control. Do not include local toolchains, databases, credentials, download caches or generated release packages in a pull request.

## Development

```powershell
npm ci
./scripts/build-torrent-runtime.ps1
npm run tauri dev
```

The frontend browser preview does not run the native downloader. Use the Tauri application for native features. The isolated LLVM/Rust toolchain helper is optional and only applies to workspaces where that toolchain has been installed explicitly.

## Validation

```powershell
npm test
cargo fmt --all -- --check
cargo test --locked --workspace --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Set `FLUX_REQUIRE_MEDIA_TESTS=1` to reject skipped media tests. `FLUX_FFMPEG` and `FLUX_FFPROBE` select the runtime under test. `FLUX_FIXTURE_FFMPEG` selects the full test-only encoder used to create HLS/DASH fixtures. On Windows, `scripts/prepare-test-runtime.ps1` downloads the pinned encoder and verifies its SHA-256. The full fixture encoder is never bundled with Flux.

The HTTP test harness is compiled only with the `test-harness` Cargo feature. Frontend access requires development mode and the explicit `e2e` Vite mode. Production builds expose neither interface.

See [Flux 2.3 validation](docs/validation-2.3.md) for the release evidence and [Linux packaging](docs/release-2.3.md) for the RPM build. Source code, identifiers and technical comments use English. Keep the English and Turkish README files aligned when user-visible behavior changes. Preserve historical validation records under their versioned names; do not present an old test run as current evidence.

## Changes

Keep OS side effects in desktop adapters. Test queue completion and cancellation without powering off the machine running the test suite. Use temporary download/data directories and restore registry changes made by integration tests. Add localized user text to all twenty catalogs. Preserve manual destination choices and never overwrite an existing download.

Use the issue templates for reproducible bugs and feature requests. Security reports follow `SECURITY.md`.
