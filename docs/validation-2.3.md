# Flux 2.3 validation

## Verified automated checks

- Windows: 124 Rust tests passed across the core, CLI, native host and desktop packages, with required FFmpeg tests enabled. The fixture encoder is separate from the shipped LGPL media runtime.
- Browser extension: 63 tests passed against the shared Chromium/Gecko sources.
- Frontend: 15 tests passed; all twenty locale catalogs, strict TypeScript and the production build passed.
- Workspace formatting and Clippy with all targets/features and warnings denied passed.
- Watch-folder regression coverage includes stable/partial files, malformed metadata, case-insensitive extensions, approval/automatic modes, custom destinations, disabled monitoring, unavailable folders, restart persistence, renamed copies, task removal and preference compatibility.

## Release checks

The release is backed by these successful GitHub Actions runs:

- [Windows installer, portable edition and browser integration](https://github.com/Hactor00/Flux/actions/runs/38742148145): Chromium, Firefox 120 and Firefox 157, native approval windows, installed binary identities, HTTP/media/torrent transfers, watched-folder settings, paused queue admission and restart duplicate prevention.
- [Linux RPM build and installed Fedora 43 validation](https://github.com/Hactor00/Flux/actions/runs/38736528656).
- [Windows, Ubuntu and macOS regression matrix](https://github.com/Hactor00/Flux/actions/runs/38736528552): 124 Rust, 63 extension and 15 frontend tests, with twenty locale catalogs.
- [Native torrent lifecycle and repeated transfer regressions](https://github.com/Hactor00/Flux/actions/runs/38736534312): both Windows runner images passed.

Exact run identities, build provenance and package hashes are recorded in [the platform evidence](validation/release-2.3.json). The Windows recovery run reuses the passed core/desktop test and Clippy log from [its original binary build](https://github.com/Hactor00/Flux/actions/runs/38736529145), after verifying unchanged runtime/build sources. That original run later failed Firefox process startup; it is not reported as an overall success. Frontend, browser, installation and native window checks were rerun in the successful recovery. [The Windows evidence](validation/release.json) records the core log's origin separately.

The Linux installed-package test covers dependency loading, the native BEP 52 ABI, yt-dlp/FFmpeg/Node startup, an HTTP transfer with expected SHA-256, desktop window startup, per-user browser manifests, authenticated Native Messaging, watched-torrent admission, duplicate suppression, graceful shutdown and SQLite integrity.

Windows installer/portable identities and browser integration are checked separately. Historical 2.2 evidence remains under the earlier validation documents and [its preserved JSON](validation/release-2.2.json); it is not presented as a new 2.3 run.

## Boundaries

Tests do not prove every site is downloadable, bypass DRM, certify all RPM distributions or benchmark Flux against other applications. Physical sleep/hibernate/power-off, live paid Debrid accounts, an end-user Windows updater round trip and sandboxed Linux browser integration are not performed. Linux drag-out is not implemented. macOS remains a source-level development target without a published application package.

Earlier MSVC test runs intermittently terminated with an access violation in the isolated native torrent fixture. A standalone native stress probe reproduced the fault while fixture metadata was being generated. That helper now uses libtorrent's synchronous POSIX disk hasher instead of creating an asynchronous disk context; actual download sessions retain their asynchronous backend. The metadata regression interleaves one hundred generation rounds with three hundred session lifecycles. Repeated transfer checks additionally run five child processes per Windows host, covering v2, hybrid and v2 magnets, corruption repair and restart recovery. A separate cold seeder readiness timeout has explicit diagnostics. These checks do not establish that every possible native runtime fault has been eliminated.
