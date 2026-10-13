# Flux 1.6 validation

Validated on Windows x64 on 12 October 2026. This record applies to the 1.6 installer and portable binaries listed in [release.json](validation/release.json). Earlier external-provider and release records are preserved separately.

## Automated checks

| Check | Result |
| --- | --- |
| Rust unit and integration tests | 77 passed, media tests required |
| Chrome MV3, bridge, healing and HUD tests | 29 passed |
| UI progress and remaining-time tests | 6 passed |
| Translation catalogs | 20 languages, 159 shared keys |
| Rust formatting and strict Clippy | Passed |
| Strict TypeScript and production frontend build | Passed |
| Production dependency audit | 0 reported vulnerabilities |

Core checks exercise parallel downloads, retries, integrity, persisted recovery, transport fallback, HLS/DASH, media assembly, subtitles, normalization, queueing and credentials. New checks cover category routing, MIME fallback before transfer, explicit-folder precedence, legacy preferences, silent clipboard queue persistence and immediate completion-action rearming after cancellation.

## Real Windows application checks

[Daily-use validation](validation/daily-use.json) verifies the actual current-user startup value, disabling startup, exact registry restoration, the settings window, MIME-based destination routing, completed-row drag eligibility and rejection of unfinished tasks. It verifies countdown event permissions and the 30-second deadline, cancellation by button, cancellation by window close, cancellation when new work arrives, and successful application exit when the countdown expires.

[Installer validation](validation/installer.json) installs the NSIS package into an isolated directory. It exercises real Native Messaging, concurrent requests, actual downloads, shutdown/resume, repeated details-window creation and destruction, automatic connection defaults, media percentage and ETA, and bundled runtimes with the developer toolchain removed from PATH. Uninstallation preserves user data, removes the owned browser and startup entries, and restores pre-existing registry values.

The matching installer SHA-256 is recorded in the JSON report. Export verification checks that the distributed installer is the tested installer and that all nine portable binaries match their built originals. ZIP paths are sorted and timestamps fixed. Source export excludes caches, personal artifacts and credentials.

## Scope

Sleep, hibernate and computer shutdown use native Windows APIs, but tests do not change the developer machine's power state. Physical native file drops into third-party applications are not automated. Silent clipboard persistence is tested without replacing user clipboard contents. Test success describes the exercised scenarios; it does not establish access to every remote source.

The SignPath workflow is configured. Local packages are unsigned. Certificate approval, repository credentials and a real GitHub-hosted workflow run remain external prerequisites. DRM, CAPTCHA, login challenges and inaccessible renewal APIs remain source restrictions.
