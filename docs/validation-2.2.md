# Flux 2.2 validation

The current local validation targets Windows x64. Machine-readable evidence is stored in `docs/validation/release.json` and the browser/window records in the same directory.

| Suite | Passing tests |
| --- | ---: |
| Rust workspace, all features | 116 |
| Shared browser extension | 63 |
| Frontend data and locale logic | 15 |
| Total | 194 |

The total excludes separate browser, installer and desktop-window regressions. Formatting, Clippy with warnings rejected, strict TypeScript and the production frontend build are checked separately. The production dependency audit and modern Firefox manifest lint report zero findings.

Hosted checks for Windows, Linux and macOS core code and the Windows native desktop are identified in the [CI record](validation/github-ci-2.2.json). This does not establish a Linux/macOS desktop release.

## Media handoff repair, 14 October

The 2.2.0 package refresh addresses two separate failures. A newer extension sent an empty `audio_format_id` to a registered 1.7 host, whose strict request decoder closed the connection. The HUD had acknowledged only local queue storage and concealed that failure. Empty optional fields are now omitted; interactive transfers wait for native acknowledgment, retain retryable durable entries and reuse their request IDs on retry. Chromium and Gecko HUD regressions cover delayed confirmation and visible failures.

The desktop approval form duplicated its inspection action and required provider metadata even though the browser had already chosen a valid stream. Reinspection then replaced the captured signed URL, causing `Approval cannot change the captured source`. Approval now starts with captured metadata, retains its original source capability and preserves name/output/quality across optional reinspection. The engine still refreshes expiring provider URLs before transferring. The live [provider approval regression](validation/provider-approval.json) exercises the production native window, YouTube MP4 and MP3 output, optional reinspection and final stream/SHA-256 checks.

The production Rust logic and yt-dlp/FFmpeg versions are unchanged by this repair. The stateful native torrent regression now runs in a bounded child process, isolating its native runtime from unrelated parallel fixtures. A Windows MSVC CI process crash prompted that test isolation; the distributed Windows binaries use the separately validated GNU toolchain. Browser and desktop packages are refreshed together without a version increment; installed 2.2.0 clients require manual package refresh because version comparison does not identify a same-version rebuild.

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
