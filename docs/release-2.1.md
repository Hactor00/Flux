# Flux 2.1.0

- Embedded BEP 52 v2 and hybrid transfers through libtorrent 2.1.2; the existing v1 engine remains available.
- Global and per-task seeding rules: ratio, duration, either condition, unlimited sharing, or stop after verification. Seeding resumes after restart and does not occupy download queue slots.
- Sparse, aligned unbuffered writes on Windows for aligned task ranges; existing unaligned checkpoints retain buffered positional writes. Linux uses fallocate and macOS uses F_PREALLOCATE. No part concatenation is required.
- TCP keepalive, nodelay, 256 KiB send and 1 MiB receive buffer requests, pooled connections, adaptive slow-transfer detection, and a three-second network idle watchdog. Reconnection retains verified offsets.
- Encrypted Real-Debrid, AllDebrid, and Premiumize account settings, supported host resolution, and expired CDN link renewal. Browser session credentials remain scoped to their origins.
- Bounded fetch/XHR manifest observation and native HLS/DASH extraction when provider metadata is unavailable. DASH representation filtering preserves the selected video and audio tracks.
- Signed in-app updates, release version binding, progress and release notes, silent per-user installation, and durable queue checkpoints across update installation.
- Reproducible native runtime preparation, Chrome and Firefox store packages, SignPath automation, and Windows certificate signing.

Validation commands and platform results are recorded in `validation-2.1.md`.
Debrid accounts use the account owner's API key and service entitlements. Windows certificate signing and store distribution use the publisher's configured credentials.
