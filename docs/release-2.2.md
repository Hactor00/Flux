# Flux 2.2

Flux 2.2 fixes browser download ownership, checkpoint resume and first-run language selection while retaining the HTTP, media and BitTorrent engines introduced in earlier releases.

## Browser handoff

Chromium downloads are paused while desktop availability is checked. Their transfer metadata is persisted before browser cancellation. The filename gate is released only after cancellation and the desktop proposal have completed, preventing a browser file chooser from competing with Flux approval. A deferred durable entry survives a service-worker restart without sending the same download twice. If Flux is unavailable before ownership changes, the original browser transfer is resumed.

Firefox intercepts HTTP GET attachment navigation at the response-header stage before its file chooser. Explicit browser API downloads retain the durable cancellation path. Firefox alone requests `webRequestBlocking`; Chromium retains its MV3 permissions.

The extension popup includes a switch for showing media buttons. Changing it updates open frames immediately and closes their panels. Re-enabling it also restores a HUD dismissed through its close button. The extension follows the browser UI language unless the user selects a language explicitly. Unsupported languages use English.

## Resume integrity

A successful `206` probe enables byte ranges even when the origin omits a strong ETag. Before appending to a checkpoint without an ETag, Flux streams and hashes its saved prefix from the origin and compares the SHA-256 digest. Completed ranges receive the same validation. A changed prefix fails without truncating saved data.

For origins that do not support ranges, a new response replays and verifies the saved prefix before appending its suffix. This preserves visible progress and disk data, but retransmitting that prefix consumes bandwidth. Verification respects cancellation and both bandwidth limits. Origins that change their strong validator or total size require a fresh transfer rather than mixing different objects.

## Desktop

New profiles resolve the OS locale to one of the 20 supported languages, falling back to English. Saved language preferences remain in effect. Settings is accessible directly from the top menu. Native messaging passes the desktop process ID to Windows foreground authorization for user-triggered proposals, and approval windows are explicitly shown and focused.

## Distribution

The native torrent runtime remains loaded for the process lifetime. C++ thread-local cleanup may outlive a session or metadata call, so unloading that module after the last short-lived API handle would leave dangling cleanup code. A fresh-process regression exercises metadata failures on repeatedly created worker threads. The native tracker user agent identifies Flux 2.2.0. macOS CI selects FFmpeg's full package so DASH/libxml2 support is available, and matrix failures do not cancel other platform checks.

Version 2.2.0 uses the existing updater verification key. Private signing material stays outside the repository. The installer, portable distribution, three extension targets, source archive and update metadata are produced by the existing packaging scripts. Public upload and browser-store signing are separate publication steps.

Validation records are stored under `docs/validation/`. Real browser regressions use isolated profiles with the browser file chooser enabled and separate test executable names to avoid system-wide interception by another installed download manager. Flux does not modify another manager's integration settings. If another manager captures a request before it reaches the browser extension, that browser needs one active download owner. Core regressions cover missing ETags, absent ranges, unknown response sizes, changed remote prefixes and loss of range support.
