# Firefox integration and security

Flux 1.7 adds Gecko support without changing the version-1 IPC schema, little-endian four-byte framing, 256-KiB frame limit, eight-request host semaphore or loopback authentication token.

## Caller authorization

Chromium launches the host with a `chrome-extension://.../` origin and an optional window argument. The existing origin allowlist remains unchanged. Firefox launches it with the absolute registered manifest path followed by the Gecko add-on ID. The host validates that the manifest is registered, the ID is exactly `flux@flux-download.local`, `name` and `type` match, the allowlist contains only that ID, and the absolute host path resolves to the running executable. A copied manifest, wrong extension, missing argument or changed executable is rejected before stdin is read.

Windows registration uses `HKCU\Software\Mozilla\NativeMessagingHosts\io.flux.download` and a dedicated manifest. Chrome, Edge and Chromium keep their own `allowed_origins` manifest. Linux and macOS registration also include Mozilla's user-scoped native-host directory. Only Windows integration has been exercised end to end in this release.

Uninstall checks installation ownership before removing browser registration. A different Flux installation's registration is retained. User downloads and the task database are preserved. Registration is idempotent and repairs both manifests and all four browser keys.

## Extension execution

`getBrowserApi()` selects Gecko's `browser` namespace or Chromium's `chrome` namespace. Gecko uses a module event background. Chrome keeps its existing module service worker. Firefox does not implement `downloads.onDeterminingFilename`, so its interception begins at `downloads.onCreated`. Chromium keeps its filename completion gate. Shared recovery and durable proposal ownership prevent independent transfers after desktop acceptance.

Header observation uses `requestHeaders` on Gecko and also `extraHeaders` on Chromium. There is no blocking web-request permission. Gecko retries durable proposals through a one-minute alarm; Chromium uses its existing thirty-second alarm. Session storage contains transient credentials, while native protection encrypts them before persistent outbox writes.

Local native transfer is disabled on first Firefox installation until the user consents. Enabling it requests the declared site permissions through a user gesture, which is needed by older MV3 Firefox versions. Each native request checks the persisted consent flag to avoid a race with delayed storage events. Withdrawal disconnects the production port, rejects pending requests, clears session capture and releases unaccepted browser transfers. Stale disconnect callbacks cannot invalidate a newer port.

The modern manifest declares browsing activity, authentication information and website content because selected URLs, required headers and media titles are forwarded to the local application. No analytics endpoint is used. Firefox 120–139 need a separate manifest because they reject Mozilla's newer data-collection declaration; both packages include the consent settings.

## UI isolation and performance

HUDs use closed shadow roots and constructed stylesheets, with a style-element fallback. No page scripting, `eval`, remote executable code or relaxed extension CSP is added. Firefox versions without the popover API use a fixed layer and fullscreen reparenting. Layout reads remain batched through requestAnimationFrame. Existing limits of 24 overlays, 500 outbox entries and bounded media candidates remain in force. No general performance equivalence benchmark is claimed by the functional tests.

## Reproducible checks

Run `npm test`, Rust tests and strict Clippy. Build release resources, then run:

```powershell
./scripts/prepare-firefox-tests.ps1
node scripts/test-firefox.mjs target/release .tools/firefox/120.0/core/firefox.exe .tools/firefox/geckodriver.exe
node scripts/test-firefox.mjs target/release .tools/firefox/157.0.1/core/firefox.exe .tools/firefox/geckodriver.exe --modern
npx --yes web-ext@10.7.0 lint --source-dir extension-firefox-modern --output json
```

The harness copies production source into a temporary test add-on, adds inspection hooks only to that copy and drives real browsers through WebDriver. It verifies the real site-permission prompt where required. Native downloads run against an isolated CLI engine and authenticated local server; the normal NSIS/desktop regression suite separately verifies native approval windows. Firefox is never installed into the user's system or personal profile by the harness.

## Distribution

Release export produces both Firefox ZIPs, includes both extension directories in portable and NSIS packages, and verifies their SHA-256 checksums. The modern package can be submitted to Mozilla after review. An AMO signature, listing and account credentials are external distribution steps; no store upload occurs during local build or tests.

Primary compatibility references: [Mozilla Native Messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging), [background manifests](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background), [host permissions](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/host_permissions), and [Firefox data consent](https://extensionworkshop.com/documentation/develop/firefox-builtin-data-consent/).
