# Flux 1.7 HUD repair validation

The 13 October 2026 repair addresses the collapsed media menu and video-bounded dragging demonstrated in a Chrome YouTube Shorts recording.

## Changes

- Manual pointer and keyboard movement uses the tab viewport and the measured toolbar dimensions. Domain-scoped normalized coordinates survive resizing and reloads. Existing coordinates retain their legacy video-relative meaning until the user moves the toolbar; Home restores the default video corner.
- Popover browser defaults are reset explicitly. The toolbar remains at its natural height with visible overflow. The media panel has its own viewport-bounded width, height and scrolling. It opens above the toolbar when there is more space there.
- Successful inspection displays a localized selection instruction instead of the unrelated instruction to play a video. All 20 custom catalogs contain the new key.
- Firefox 120 closes the toolbar without calling an unsupported Popover API method.
- Navigation and video-source changes discard obsolete formats. Late inspection or enqueue replies cannot overwrite a newly opened panel. Lost pointer capture and window blur finish dragging.
- When several eligible videos exist, a manually positioned toolbar follows the active video rather than overlapping identical controls.

## Automated and real-browser checks

The extension suite passed 39 tests and the UI data suite passed six tests. TypeScript validation, the Vite production build and the 20-catalog/162-key consistency check passed.

Chrome 154.0.8037.98 ran the production HUD source in an isolated browser profile with a controlled bridge fixture. Native browser geometry verified dragging outside the video to opposite viewport corners, an unclipped panel, independent toolbar geometry, upward opening, a 360-pixel-wide viewport, outside dismissal, selection forwarding and closing. This test validates actual Chromium rendering; it does not claim to exercise an installed Chrome extension or a live YouTube download.

Firefox 120.0 and 157.0.1 ran the updated bundled extension through the actual Native Messaging host in isolated profiles. Both verified panel geometry and viewport dragging alongside closed Shadow DOM, strict page CSP, resize, full screen, dismissal, browser-download handoff, captured-cookie protection and SHA-256 download integrity. Native registry entries were restored after each run. Firefox 120 used the fixed-layer fallback; 157 used the native popover layer.

The rebuilt NSIS installer passed installation, idempotent native-host registration and removal, desktop cold start, 20 concurrent native requests, nine fixture downloads, active shutdown/resume, repeated detail-window close/reopen, HLS estimated progress and remaining-time display, and bundled runtime checks. Uninstall preserved fixture data and restored the registry. The tested installer identity is recorded in `validation/installer.json`; distribution verification checks that identity and every archived Chromium/Firefox extension file.

The existing HTTP download engine was not changed by this repair. Live provider availability remains separate from the controlled layout and native handoff tests.

## Reproduction commands

```powershell
npm test
node scripts/test-chrome-hud.mjs
node scripts/test-firefox.mjs target/release .tools/firefox/120.0/core/firefox.exe .tools/firefox/geckodriver.exe
node scripts/test-firefox.mjs target/release .tools/firefox/157.0.1/core/firefox.exe .tools/firefox/geckodriver.exe --modern
./scripts/test-installer.ps1 -Installer target/release/bundle/nsis/Flux_1.7.0_x64-setup.exe
```

Prepare the pinned Firefox test binaries with `scripts/prepare-firefox-tests.ps1` when missing. Extension sources must be regenerated and copied into the binary test bundle before testing a bundled installation. The Chrome script accepts an executable path as its first argument. All profiles and downloads are temporary; personal browser profiles are not used.

Installer and desktop cold-start tests require the normal Flux application to be closed. Tauri's application-wide single-instance lock is shared across temporary data directories. These scripts now reject that condition before making installer changes or waiting for an unreachable test broker.
