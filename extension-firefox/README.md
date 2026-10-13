# Flux for Firefox

The shared JavaScript, styles, icons and twenty language catalogs are generated with `npm run build:extensions`. Edit `extension/` for common behavior; do not edit generated runtime files.

## Packages

- `Flux-<version>-firefox.zip` targets Firefox 140 and later. Its generated manifest includes Mozilla's native data-transfer declarations.
- `Flux-<version>-firefox-120.zip` targets Firefox 120–139. These versions reject the newer manifest field, so this package uses the explicit local data-transfer consent in the extension settings.
- Both packages have the same add-on ID, `flux@flux-download.local`, and share the Native Messaging host `io.flux.download`.

Local packages are unsigned. For development, open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on**, and select the extracted package's `manifest.json`. Firefox removes temporary add-ons after restarting. Permanent distribution requires a Mozilla-signed XPI; ZIP packaging and local validation do not provide that signature.

Install Flux, or run **Install browser bridge** in its desktop settings. The current-user Mozilla registry key points to `%LOCALAPPDATA%/Flux/io.flux.download.firefox.json`, whose executable path is absolute. Installation, repair and removal are handled by the desktop executable, including when invoked by NSIS.

Enable local Flux data transfer in the extension settings and accept Firefox's site-access prompt if shown. This permits selected URLs, media titles and necessary session headers to reach the local Flux application. Consent can be withdrawn in the same settings. Credentials stay in session storage until sealed by the native host; the durable outbox stores encrypted envelopes.

## Runtime behavior

Firefox uses a module event background rather than a Chromium service worker. Gecko download creation triggers a pause before proposing a transfer to Flux. A saved desktop proposal owns the transfer; Firefox's transfer is cancelled and erased. If pause or initial native contact fails, the browser retains its transfer. A small transfer can finish before Firefox exposes its creation event; it is then left with Firefox instead of creating a duplicate Flux proposal.

The HUD uses a closed Shadow DOM and constructed stylesheets. Firefox 120 uses a fixed overlay, including reparenting into fullscreen containers. Firefox 125 and later use the native popover top layer. Position is stored per hostname using normalized, bounded coordinates.

## Validation

`scripts/prepare-firefox-tests.ps1` prepares checksum-pinned, isolated Firefox and geckodriver runtimes. `scripts/test-firefox.mjs` tests native framing, protected cookies, SHA-256 downloads, browser ownership, explicit consent, consent withdrawal, strict page CSP, closed shadow roots, outside clicks, real pointer dragging, resizing, fullscreen and dismissal. No personal Firefox profile is used. The Mozilla registry entry is restored after each test.
