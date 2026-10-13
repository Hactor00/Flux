# Flux 1.5: media assets, browser healing and QUIC

This release implements roadmap Module 1 and the HTTP/3 portion of Module 2. Multi-NIC bonding, remote archive extraction, automation hooks and remote management are separate future work; they are not enabled by this release.

## Media assets

Inspection returns `subtitles` and `chapters` alongside audio/video streams. HLS `EXT-X-MEDIA` subtitle renditions, standalone WebVTT/SRT DASH representations, FFprobe embedded text streams and provider subtitles are discovered. Explicit HLS chapter date ranges, DASH chapter events, container chapters and provider chapter metadata are collected. Formats that do not publish chapter information do not receive invented chapters.

The download window offers per-language selection, MP4/MKV embedding and separate SRT output. MP4 uses `mov_text`; Matroska uses SubRip text. Prepared subtitle inputs are bounded task-owned files. HLS WebVTT playlists must be finite, contain at most 2,048 segments, and remain within an 8 MiB text budget. Timestamp maps and duplicate cues are processed before FFmpeg. Subtitle renditions are removed from FFmpeg's audio/video HLS playlist because its HLS demuxer cannot safely treat WebVTT segments as regular video segments. Discovery retains the original manifest.

External SRT outputs are preflighted, flushed, hashed and recorded before publication of the main file. Existing files with different bytes are never overwritten. Recovery verifies recorded subtitle hashes in addition to the main output. MP3 and MKA cannot embed subtitles; they may export separate SRT files. Bitmap subtitles are not silently presented as text.

Audio normalization measures loudness before encoding, then applies the measured EBU R128 filter with a −23 LUFS target, −1 dBTP ceiling and 7 LU target range. Audio encoding uses AAC for MP4/MKV and LAME for MP3; the video stream is copied. Silent audio uses an identity filter when the measurements are non-finite. MKA extraction copies the original audio packets without decoding or re-encoding and is mutually exclusive with normalization.

The bundled FFmpeg build now includes WebVTT/SubRip demuxers and encoders, MP4 timed-text encoding, FFmetadata, the null muxer and the loudness filter. Tests execute these exact bundled binaries. No shell command is constructed from remote content.

## Silent browser renewal

Tasks store their approved browser page and optional iframe URL. On an HTTP 401, 403 or 410, the transfer can issue a 45-second refresh capability over the authenticated Native Messaging broker. Identical concurrent waits share one capability. `refresh_poll` is bounded by the protocol frame size; `refresh_submit` is single-use and must match both the original page and resource identity.

The extension finds an already open source tab, checks current captured requests and may fetch the original page or iframe in the background with its browser session. It does not create, focus, reload or navigate a visible tab. URLs embedded in a refreshed page are compared using the original origin, path and content-identifying query parameters; only known signature/token/expiry parameters may change. Newly captured authentication headers are sealed by the native host before being returned to the desktop. A successful replacement is used for the current GET and subsequent requests in that task, while Range/ETag checks continue to guard the bytes.

This mechanism requires an accessible original browser session and a source that exposes a renewed URL or usable session credentials. It does not solve login challenges, CAPTCHA, DRM, or opaque provider-specific refresh APIs automatically. A closed source page or changed content identity is rejected instead of downloading unrelated content. Existing provider-specific YouTube resolution remains available.

## HTTP/3 transport

HTTP/2 is enabled explicitly. Automatic mode learns HTTP/3 from a same-authority, same-port HTTPS `Alt-Svc: h3` advertisement; cross-host advertisements are ignored. A task can also prefer HTTP/3 or choose TCP-only mode in its advanced options. QUIC uses Quinn/rustls through pinned reqwest 0.12.28. Reqwest currently requires `reqwest_unstable`; `.cargo/config.toml` makes this build requirement explicit and reproducible.

QUIC connection establishment is bounded to three seconds. A failed attempt falls back to the same GET over TCP and blocks further QUIC attempts for that origin for sixty seconds. The circuit breaker and advertisement cache are bounded. HTTP/3 body failures continue through the existing checkpoint/retry machinery. HTTPS downgrade redirects are rejected, redirects are bounded, and Cookie/Authorization headers are removed when the origin changes. Both file workers and the private FFmpeg resource gateway use this transport.

The combined TCP/QUIC future is boxed so it does not expand every native IPC command's stack frame. Engine and crypto initialization run outside the native event-loop stack. Windows builds reserve eight MiB of virtual main-thread stack with pages committed on demand; the startup and window tests exercise the distributed executable rather than only the Rust test runtime.

## Validation

The regression suite includes a real local TLS/QUIC server, sixteen concurrent HTTP/3 ranges with byte comparisons, TCP fallback, signature renewal of a nine MiB file with SHA-256 validation, HLS and DASH asset parsing, multi-language MP4 embedding, chapter preservation, external SRT timing, original audio packet hashes and measured normalized output loudness. Windows window/Native Messaging integration is recorded separately in `validation/media-suite-window.json`.

Build and run:

```powershell
. ./scripts/use-local-rust.ps1
cargo test --locked -p flux-core -p flux-cli -p flux-native-host
npm test
./scripts/build-release.ps1
node scripts/test-media-window.mjs target/release --suite
./scripts/test-installer.ps1
./scripts/export-release.ps1
./scripts/verify-release.ps1
```

Reference documentation: [reqwest HTTP/3](https://docs.rs/reqwest/0.12.28/reqwest/), [FFmpeg loudness normalization](https://ffmpeg.org/ffmpeg-filters.html#loudnorm), [FFmpeg subtitle encoding](https://ffmpeg.org/ffmpeg-codecs.html).
