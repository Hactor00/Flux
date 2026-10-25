# Flux for Firefox

The shared JavaScript, styles, icons and twenty language catalogs are generated with `npm run build:extensions`. Edit `extension/` for common behavior; do not edit generated runtime files.

## Packages

- `Flux-<version>-firefox.zip` targets Firefox 140 and later. Its generated manifest includes Mozilla's native data-transfer declarations.
- `Flux-<version>-firefox-120.zip` targets Firefox 120–139. These versions reject the newer manifest field, so this package uses the explicit local data-transfer consent in the extension settings.
- Both packages have the same add-on ID, `flux@flux-download.local`, and share the Native Messaging host `io.flux.download`.

Local packages are unsigned. For development, open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on**, and select the extracted package's `manifest.json`. Firefox removes temporary add-ons after restarting. Permanent distribution requires a Mozilla-signed XPI; ZIP packaging and local validation do not provide that signature.

Launch Flux once to register the browser bridge automatically, or use **Settings → Browser integration → Repair browser bridge**. On Windows, the current-user Mozilla registry key points to `%LOCALAPPDATA%/Flux/io.flux.download.firefox.json`. On Linux, Flux writes `~/.mozilla/native-messaging-hosts/io.flux.download.json`. Host executable paths are absolute. Windows installation, repair and removal are also handled by NSIS. Sandboxed browser packages may need additional host access.

Enable local Flux data transfer in the extension settings and accept Firefox's site-access prompt if shown. This permits selected URLs, media titles and necessary session headers to reach the local Flux application. Consent can be withdrawn in the same settings. Credentials stay in session storage until sealed by the native host; the durable outbox stores encrypted envelopes.

## Connection and update notices

The free Flux desktop app is required. Download it from [GitHub Releases](https://github.com/Hactor00/Flux/releases/latest), start Flux, grant local-transfer consent and open the extension popup. **Settings → Browser integration** shows a live extension connection separately from local host registration. Repair does not install the extension.

Since 2.4.2 the popup checks GitHub release metadata with a six-hour cache and offline backoff. It links to the official release when the extension or desktop is outdated. ZIP installations require manual replacement; permanent Firefox distribution still requires Mozilla signing.

## Runtime behavior

Flux uses a Gecko module event background rather than a Chromium service worker. Its Firefox-only blocking response-header listener intercepts HTTP GET attachment navigation before file selection. Explicit browser API downloads use an early pause, persisted deferred metadata and cancellation before desktop proposal delivery. If Flux is unavailable before ownership changes, the browser keeps its transfer; delivery errors after durable cancellation remain in Flux's outbox.

The toolbar popup controls page media-button visibility across open frames. The initial language follows the browser UI locale, with English for unsupported locales; explicit selections persist.

The HUD uses a closed Shadow DOM and constructed stylesheets. Firefox 120 uses a fixed overlay, including reparenting into fullscreen containers. Firefox 125 and later use the native popover top layer. Position is stored per hostname using normalized, bounded coordinates.

## Validation

`scripts/prepare-firefox-tests.ps1` prepares checksum-pinned, isolated Firefox and geckodriver runtimes. `scripts/test-firefox.mjs` tests native framing, protected cookies, SHA-256 downloads, browser ownership, explicit consent, consent withdrawal, strict page CSP, closed shadow roots, outside clicks, real pointer dragging, resizing, fullscreen and dismissal. No personal Firefox profile is used. The Mozilla registry entry is restored after each test.

## Flux 2.6.0

The GitHub build retains `flux@flux-download.local`; the Mozilla store package uses `firefox@fluxdownload.org`. Desktop 2.6.0 registers and authorizes both IDs. Both distributions contain the shared periodic and popup release checker. AMO delivers approved store updates through Firefox; temporary/unpacked extensions are updated manually.
