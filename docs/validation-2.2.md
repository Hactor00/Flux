# Flux 2.2 validation

The current local validation targets Windows x64. Machine-readable evidence is stored in `docs/validation/release.json` and the browser/window records in the same directory.

| Suite | Passing tests |
| --- | ---: |
| Rust workspace, all features | 116 |
| Shared browser extension | 59 |
| Frontend data and locale logic | 14 |
| Total | 189 |

The total excludes separate browser, installer and desktop-window regressions. Formatting, Clippy with warnings rejected, strict TypeScript and the production frontend build are checked separately. The production dependency audit and modern Firefox manifest lint report zero findings.

## Resume regressions

The native runtime regression runs in a fresh child process, repeatedly creates worker threads and calls malformed metadata through the real bridge. This checks error propagation and worker-exit cleanup with no torrent session keeping the library alive. CI uses DASH-capable `ffmpeg-full` on macOS and retains all platform results when one matrix job fails.

The HTTP fixture suite checks successful ranged resume without an ETag, prefix replay for an origin without ranges, unknown response sizes, local corruption, changed remote prefixes, loss of range support, transient failures, rate limits, persisted checkpoints and exclusive destination publication. Saved progress is retained while a validator-free prefix is compared. Changed bytes are never published as a completed mixed file.

## Browser regressions

Chrome 155 loads the production extension into a new profile with download location prompting enabled. An attachment link creates a durable native approval task, cancels the browser transfer, leaves the browser download directory empty and opens Flux approval in the foreground. The test checks that the browser has no visible file chooser.

Firefox 120 and 157 exercise attachment interception before filename selection, native framing, scoped cookies, file SHA-256, consent withdrawal, HUD visibility preferences, drag positioning, closed Shadow DOM, strict CSP, outside-click dismissal and fullscreen behavior.

An installed IDM instance intercepted ordinary Chrome and Firefox test navigations before Flux received them. Browser tests therefore use separate executable names in addition to isolated profiles. No IDM files or integration settings are modified. Normal use requires one active download owner for each browser.

## Packaging and desktop

The installer regression compares installed binaries and extension assets against the current build, exercises idempotent registration/repair/removal, runs browser and desktop tests from the temporary installation and checks uninstall data retention and registry restoration. Desktop regressions open Settings through its top menu button and repeatedly create and close task detail windows. Media and torrent windows verify real local payloads and assembled outputs.

`scripts/verify-release.ps1` checks package hashes, the tested installer identity, archived binaries, extension manifests and required source files. Update signatures are verified against the application's existing public key and expected version. The private signing key is excluded from source and package exports.

Browser-store approval, Windows Authenticode signing, live paid-host accounts, cross-platform runtime validation and comparative competitor benchmarks are independent of these local regression results.

A nonfatal upstream WebView2 class-unregistration warning (1412) was observed during application cleanup and recorded in `torrent-window.json`. Window destruction, process shutdown, pause/resume and output SHA-256 checks passed. A live GitHub updater installation round trip was not run; signature verification, durable preparation and NSIS installation were tested separately.
