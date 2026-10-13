# Flux 1.5 validation

Windows x64, 12 October 2026. The distributed FFmpeg/FFprobe 9.0.2 binaries include the new subtitle, metadata and loudness processing components; a separate full FFmpeg is used only to generate test fixtures.

## Automated checks

| Check | Result |
| --- | --- |
| Rust unit and integration tests | 68 passed |
| Extension/API/DOM tests | 26 passed |
| UI progress-data tests | 6 passed |
| Localization | 20 catalogs, 136 matching keys |
| TypeScript and frontend production build | Passed |
| Workspace Clippy, all targets/features, warnings denied | Passed |
| Rust formatting | Passed |
| npm production dependency audit | No reported vulnerabilities |

The added tests exercise actual TLS/QUIC traffic with sixteen concurrent Range streams, exact byte comparisons and TCP fallback. They also verify in-flight signed-link renewal, resource-substitution/replay rejection, HLS/DASH subtitle discovery, chapter metadata, MP4 and MKV multiple-subtitle output, segmented VTT timing, separate SRT files, original audio packet hashes and measured R128 loudness. Unknown-size media and pause/resume regression tests remain enabled.

## Windows integration

`validation/media-suite-window.json` records a real desktop download started through Native Messaging and the native approval window. The test selects the subtitle language, embedding and normalization controls, renews an expired source twice across pause/resume, then uses the packaged FFprobe to verify the video/audio/subtitle streams and two chapters. Live ETA, estimated size and progress are checked in both the list and detail window.

The installer test launches the installed desktop with development tools removed from PATH, tests twenty native requests and nine authenticated downloads, repeats detail-window close/reopen, checks automatic defaults and timestamps, verifies all bundled runtime binaries, uninstalls, and confirms user-data preservation and registry restoration. `validation/installer.json` identifies the exact tested installer by SHA-256. `scripts/verify-release.ps1` verifies exported archive bytes against the tested binaries and requires the HTTP/3 build configuration in the source archive.

The Windows startup regression was detected in the actual executable and fixed by boxing the combined transport future, moving initialization out of the UI event-loop stack and reserving eight MiB of virtual Windows stack. Native media controls use functional state updates so simultaneous selections do not overwrite one another.

## Scope of the evidence

Local HTTP/QUIC fixtures provide reproducible byte, timing and output checks. Browser service-worker renewal is covered by API/DOM tests and real Native Messaging on Windows; this is not an end-to-end claim for every authenticated website in the user's Chrome profile. Existing external YouTube verification is preserved as historical evidence. DRM/challenge solving and opaque provider refresh APIs are outside the implemented renewal mechanism. Reqwest's HTTP/3 API remains upstream experimental and pinned to 0.12.28.

The main JavaScript bundle triggers Vite's size advisory; the production build succeeds. Remote CI configuration is included but remote CI execution is not claimed.

Detailed machine-readable results: [release record](validation/release.json), [native media window](validation/media-suite-window.json), [installer](validation/installer.json).
