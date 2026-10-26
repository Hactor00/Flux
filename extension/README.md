# Flux Chromium extension

**[Install Flux from the Chrome Web Store](https://chromewebstore.google.com/detail/flux-download-manager/okimieponfbegjhmfdnnefnddofooipk)** for Chrome, Edge or Brave. Store-installed copies receive approved updates through the browser. As of 25 October 2026, version 2.5.0 is public and the 2.6.0 update is awaiting review; both connect to Flux desktop 2.6.0.

For manual installation, download the Chromium ZIP from [GitHub Releases](https://github.com/Hactor00/Flux/releases/latest), extract it, enable developer mode in `chrome://extensions` or `edge://extensions`, and choose **Load unpacked**. ZIP installations are updated manually. For Firefox, see [the Firefox installation guide](../extension-firefox/README.md).

The free Flux desktop app is required. Download it from [GitHub Releases](https://github.com/Hactor00/Flux/releases/latest), install/enable the extension, start Flux and open the extension popup. In **Settings â†’ Browser integration**, Flux distinguishes a registered local host from a live extension connection. Repairing the host does not install an extension.

Since 2.4.2 the popup checks GitHub release metadata with a six-hour cache and offline backoff. It links to the official release when the extension or desktop is outdated. ZIP installations require manual replacement; the extension does not install downloaded code or bypass browser signing requirements.

See the [browser distribution guide](../docs/browser-stores.md) for developer-mode installation, store publication and native host registration.

## Version 2.6.0

Chrome store and GitHub packages share runtime sources. Store packages omit the development key and remain publisher artifacts. Firefox store ID: `firefox@fluxdownload.org`; GitHub Firefox ID: `flux@flux-download.local`. Desktop 2.6.0 accepts both. A six-hour release check and popup check notify about newer releases; the browser installs approved store updates, while unpacked ZIPs require manual replacement.

Version 2.6.5 refreshes the popup and settings layout while preserving both desktop bridge identities and all 20 UI languages. Extension release checks use a six-hour cache. Optional desktop usage statistics are separate and are never sent by this extension.
