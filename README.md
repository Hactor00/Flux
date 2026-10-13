# Flux

![Flux — downloads, media and your browser](docs/images/flux-banner.svg)

**English** · [Turkish v1.7 guide](README.tr.md)

[![Rust](https://img.shields.io/badge/Rust-1.99-DEA584?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri)](https://v2.tauri.app/)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react)](https://react.dev/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

Flux is an open-source Windows download manager with a Rust engine, separate desktop windows, an embedded BitTorrent engine and shared Chromium/Firefox companion extensions.

**Download files. Capture accessible media. Keep your queue under control.**

Flux brings segmented transfers, browser handoff, media processing and a persistent local history into one desktop application. Its name refers to flow: files and media moving from the browser to your download queue.

## Download packages

Version **2.0.0** targets **Windows x64**. Get the verified packages from [Flux 2.0.0](https://github.com/Hactor00/Flux/releases/tag/v2.0.0). The previous [1.7.0 release](https://github.com/Hactor00/Flux/releases/tag/v1.7.0) remains available.

| Package                      | Use                                                  |
| ---------------------------- | ---------------------------------------------------- |
| [Windows installer](https://github.com/Hactor00/Flux/releases/download/v2.0.0/Flux_2.0.0_x64-setup.exe) | Per-user installation with the browser bridge |
| [Portable ZIP](https://github.com/Hactor00/Flux/releases/download/v2.0.0/Flux-2.0.0-windows-x64.zip) | Portable application and bundled media tools |
| [Chromium extension](https://github.com/Hactor00/Flux/releases/download/v2.0.0/Flux-2.0.0-chrome.zip) | Chrome, Edge, Brave and other compatible Chromium browsers |
| [Firefox 140+](https://github.com/Hactor00/Flux/releases/download/v2.0.0/Flux-2.0.0-firefox.zip) | Modern Firefox extension |
| [Firefox 120–139](https://github.com/Hactor00/Flux/releases/download/v2.0.0/Flux-2.0.0-firefox-120.zip) | Legacy Gecko extension |
| [Source ZIP](https://github.com/Hactor00/Flux/releases/download/v2.0.0/Flux-2.0.0-source.zip) | Source distribution, including media license notices |

The release packages are unsigned. Windows may show a publisher or SmartScreen warning. The signing pipeline is configured separately; see [release and signing](docs/release.md). Firefox packages require Mozilla signing for permanent installation in standard Firefox. Check package identity against [SHA256SUMS.txt](https://github.com/Hactor00/Flux/releases/download/v2.0.0/SHA256SUMS.txt).

## At a glance

| Downloads                              | Media                                           | Desktop and browser                      |
| -------------------------------------- | ----------------------------------------------- | ---------------------------------------- |
| Automatic connection count, up to 32   | HLS, DASH and ordered TS                        | Separate approval and progress windows   |
| Pause, resume and SHA-256 verification | MP4, MKV, MP3 and MKA                           | Chrome and Firefox companion extensions  |
| Queues, schedule and speed limits      | Subtitles, chapters and normalization           | Searchable history with download times   |
| Batch and wildcard URL import          | Quality selection and accessible source renewal | Twenty languages and optional RTL layout |

## Features

- Automatic or manual 1–32 connections per file, dynamic HTTP Range splitting, bounded retries, SHA-256 verification and persistent pause/resume.
- SQLite history with creation, first-start and completion times. Priority queues, scheduling, shared and per-file rate limits, and batches of up to 500 URLs.
- HLS, DASH and ordered TS processing. MP4/MKV video, MP3 audio, lossless MKA extraction, text subtitle embedding or SRT export, chapter metadata and two-pass audio normalization.
- DOM/network capture for video, audio, frame players and accessible blob/MediaSource inputs. Supported providers resolve titles, fps, audio tracks and subtitles through asynchronous yt-dlp metadata. Playlists, channel feeds and captured page links feed the durable queue in batches of up to 500.
- Embedded librqbit 9.0.1: BitTorrent v1/hybrid magnets, HTTP and local .torrent import, DHT, trackers, peer limits, file selection, standard piece verification and per-file SHA-256. HTTP/media and torrent downloads share the configured global bandwidth budget.
- Sparse destination-volume staging, disjoint worker offsets, verified resume ranges and migration of legacy chunk checkpoints. Completion performs a single integrity read and exclusive publication without chunk concatenation.
- HTTP/3 with TCP fallback and conditional signed-source renewal through the original browser session.
- Separate approval, settings and progress windows. Virtualized table, search, sorting, multi-selection and completed-file native drag-out on Windows.
- Optional category folders, silent clipboard queueing, Windows startup and one-shot queue-completion power actions with a cancellable 30-second countdown.
- Twenty languages, 193 desktop translation keys, searchable language selection and Arabic/Urdu right-to-left layout.
- Firefox Gecko module backgrounds, authenticated native-host registration, explicit local transfer consent and media HUD compatibility from Firefox 120 onward.

## Install

Run the Windows x64 installer from a release. Installation is per-user and registers the Native Messaging bridge for Chrome, Edge, Brave, Chromium and Firefox. FFmpeg, FFprobe, yt-dlp and Node are bundled. WebView2 is installed through Microsoft's bootstrapper when missing, which requires internet access.

In `chrome://extensions` or `edge://extensions`, enable Developer mode and load the installed `extension` directory as an unpacked extension. Its ID is `iheejmamcoeigoallciifjminjnnnlbj`. Store publication is separate. Reload the extension and refresh video tabs after an update.

The portable ZIP also runs directly. Register its browser bridge through Settings. Application data is stored in the current user's local application-data directory under `Flux`; the normal download folder is `Downloads/Flux`.

For Firefox, use the modern `firefox.zip` package on Firefox 140+, or `firefox-120.zip` on Firefox 120–139. Unsigned local builds are loaded temporarily through `about:debugging`; permanent installation needs Mozilla signing. Enable local Flux data transfer in the extension settings, accepting site access if requested. See [Firefox setup](extension-firefox/README.md) and [implementation and validation](docs/firefox.md).

## Daily use

Browser interception pauses the browser transfer and opens a separate Flux approval window. Choose the name, folder and whether to start immediately. Once Flux accepts the durable proposal, the browser transfer is cancelled. If the initial bridge transfer fails, the browser keeps ownership.

Category routing is opt-in. Defaults are `Downloads/Compressed`, `Downloads/Video`, `Downloads/Music`, `Downloads/Programs` and `Downloads/Documents`. Each category folder is configurable. An explicitly selected destination takes precedence.

Clipboard monitoring is opt-in. Select an approval window or silent addition to the persistent queue. Silent mode follows the queue's enabled/disabled state and does not open a window.

Completion actions are disabled by default and require observed queue activity. Paused, pending-approval or unsuccessful tracked downloads prevent completion. The action runs once. A separate 30-second countdown supports cancellation; a changed task list also cancels it. Sleep, hibernate and shutdown use Windows APIs and do not force other applications to close.

The video button has a drag grip. Drag it anywhere in the tab's visible viewport, including outside the video, or use arrow keys while the grip is focused. Home restores its default video corner. Position persists per domain and adapts to window resizing. The options panel opens above or below the button, stays within the viewport and scrolls independently when space is limited. Clicking outside closes the options panel; the cross hides the whole button for that page session. Navigation and video-source changes invalidate old selections.

## Development and releases

Use Rust 1.99.0, Node 24.18.0 and [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run tauri dev
```

`npm run dev` is a browser preview. Native operations require the desktop application. The development test harness is excluded from production builds.

See [contribution and testing instructions](CONTRIBUTING.md) and the [clean build and SignPath signing pipeline](docs/release.md). Signing requires a reviewed SignPath project, configured repository variables and a submitter token. A prepared workflow does not indicate that a certificate has already been issued.

## Source and runtime limits

The extension observes HTTP(S) requests with `webRequest`. Blob URLs require their underlying network resource. DRM, CAPTCHA, login challenges and opaque provider renewal APIs are not automatically solved. Source access, codec compatibility and server policy still apply.

Unknown-size finite media uses duration/segment estimates marked with ≈. Live streams or resources without sufficient metadata cannot provide a definite remaining time. HTTP/3 uses reqwest's pinned experimental API and falls back to TCP. The distributed platform is Windows x64. BitTorrent v2-only metadata is rejected explicitly. Per-torrent limits require 0 or at least 16 KiB/s, and the shared global limit requires 0 or at least 32 KiB/s while torrents run. Completion stops sharing the torrent.

Test records cover defined local, native and historical external scenarios. They do not establish success on every site or comparative superiority over other download managers. OS power-off/sleep actions are not executed on the developer's machine by automated tests.

Flux code is MIT. FFmpeg and LAME use LGPL, libxml2 uses MIT, yt-dlp uses Unlicense, and Node and other dependencies retain their own notices. Release packages include license inventories and media sources. SHA-256 checks establish file identity and do not replace Authenticode signatures or malware scanning.

[2.0 implementation](docs/releases/v2.0.0.md) · [Browser store packaging](docs/browser-stores.md) · [Architecture](docs/architecture.md) · [IPC](docs/protocol.md) · [Security policy](SECURITY.md) · [Validation](docs/validation.md)

Found a reproducible problem? Use the repository's bug-report template. Include Flux and browser versions and remove private URLs and session credentials from diagnostics. See [Contributing](CONTRIBUTING.md) for development and test commands.
