# Flux Chromium extension

The free Flux desktop app is required. Download it from [GitHub Releases](https://github.com/Hactor00/Flux/releases/latest), install/enable the extension, start Flux and open the extension popup. In **Settings → Browser integration**, Flux distinguishes a registered local host from a live extension connection. Repairing the host does not install an extension.

Since 2.4.2 the popup checks GitHub release metadata with a six-hour cache and offline backoff. It links to the official release when the extension or desktop is outdated. ZIP installations require manual replacement; the extension does not install downloaded code or bypass browser signing requirements.

See the [browser distribution guide](../docs/browser-stores.md) for developer-mode installation, store publication and native host registration.
