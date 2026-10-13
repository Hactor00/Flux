# Architecture

## Ownership and persistence

The React 19 interface uses Radix primitives and calls Tauri commands. Rust owns task state, credentials, downloads and output publication. Chrome MV3 and Firefox Gecko extensions share sources and communicate through a Native Messaging host and a user-scoped loopback broker. See [Firefox integration](firefox.md) for manifest, caller authorization and event-background differences. FFmpeg and the provider extractor run as cancellable hidden processes.

A process lock prevents two engines from owning one profile. SQLite uses WAL, FULL synchronization and transactions. Database work runs on blocking workers behind a short-lived mutex; network requests never hold that mutex. Cancellation waits for workers and durable checkpoints before shutdown.

The desktop has a virtualized task table, menu, toolbar and status bar. Approval, settings, task details and completion countdowns use separate OS windows. Window creation runs outside synchronous GUI callbacks to avoid WebView2 deadlocks. Closing a proposal cancels that proposal; closing a completion countdown cancels its action.

## HTTP transport and recovery

Files use up to 32 workers, with 64 shared network permits and at most eight active tasks. Automatic connection selection adapts to source capabilities. Initial chunks are 4 MiB, writes are bounded, and chunk count is limited to 65,536. Idle workers can split a slow worker's remaining range after its prefix has been checkpointed.

Range negotiation validates a real 206 response, Content-Range, identity encoding and validators. Workers check offsets and body bounds. Sensitive headers remain scoped to their canonical origin; HTTPS downgrades are rejected. HTTP/3 is optional, with TCP fallback. The upstream reqwest HTTP/3 interface is experimental and pinned.

Transient disconnects, truncated bodies, 429 and server errors use bounded retries. Retry-After supports seconds and dates. Disk, protocol and validator failures are not treated as transient network errors. Separate per-task and global token buckets enforce speed limits and wake waiting workers when settings change.

Each durable prefix records its byte count and SHA-256 after fsync. Recovery verifies recorded ranges and overwrites uncommitted bytes rather than trusting them. Changed validators invalidate stale plans; unverifiable streams restart safely. HTTP staging stays on the destination volume, checks hashes in a single final read and publishes exclusively. Media publication retains its cross-volume fallback and recovers from a crash between publication and the database commit.

## Media pipeline

A random-port loopback gateway rewrites HLS and DASH resource references into registered HTTP(S) sources. File and data schemes are rejected. Manifests and resource counts are bounded. Cached resources retain validators, prefix hashes and journals across pause/resume. Fixed media resources can use parallel range workers; finite segment lists preserve playback order.

FFmpeg assembles selected video/audio tracks, containers, subtitles and chapters. Audio extraction supports lossless output and measured loudness normalization. Demuxer options are detected from the installed runtime, including support for segments whose URL suffix does not match their actual media content. Permanent resource failures prevent publication of partial output.

Provider resolution uses bundled yt-dlp and Node with bounded output and a timeout. DRM and irrelevant audio-only candidates are excluded from video choices. The selected video and audio come from the same provider metadata. Browser-assisted renewal can replace expired URLs when the original session exposes a fresh compatible resource; login challenges and opaque refresh APIs are not automatically solved.

Media progress tracks distinct resources, segment duration and FFmpeg output time. Failed range attempts roll back provisional progress, and repeated reads do not inflate totals. Finite HLS/DASH durations provide percentage and ETA when Content-Length is absent. Estimated size is replaced by exact size when known. Live or unbounded sources remain indeterminate. Progress checkpoints persist, and late events cannot regress completed tasks.

## Queue and daily use

The scheduler polls ready task IDs every 300 ms, respecting priority, creation order, schedules and active-task limits. Paused tasks do not start automatically. Batch insertion is transactional and bounded. Added, first-started and completed timestamps persist; legacy completion times may use a marked estimate, while unknown start times remain unknown.

Completion tracking arms only after observed work. It requires successful completion and no pending, paused or active tasks. Historical completed rows cannot trigger a power action at startup. The desktop displays a cancellable 30-second countdown, uses a monotonic execution deadline and revalidates the task snapshot. New work, changed task state or cancellation disarms it. Windows actions use shutdown privileges and native APIs; application exit checkpoints the engine.

Category routing prefers a recognized extension, then MIME. Explicit destinations bypass automatic routing. Unknown names may be rerouted on HTTP metadata before transfer bytes are written. Category directories, clipboard mode and completion preferences persist in SQLite. Silent clipboard resolution runs independently of GUI polling and inserts into the persistent queue without an approval window.

Completed-file drag-out validates task IDs and existing regular files or torrent directories, then uses native OLE CF_HDROP with copy semantics. Windows startup writes only the current-user Flux Run value with a quoted executable and `--background`. Failed preference writes restore the exact previous registry value. Uninstallation removes only the startup entry owned by that executable.

## Browser integration, security and localization

The extension persists interception drafts across MV3 worker restarts. Accepted transfers open desktop approval windows; interception and browser recovery are coordinated to avoid duplicate downloads. The HUD closes its panel on outside clicks, supports dismissal, and stores movable normalized positions per domain. Native Messaging framing, request IDs and broker tokens validate requests.

Broker discovery uses Windows user SID ACLs or Unix private permissions. Credentials use Windows DPAPI or a user-private encrypted store; public task serialization omits secrets, and listing tasks does not decrypt credentials. The same local user and administrators are outside this isolation boundary.

Twenty independent JSON catalogs provide all desktop labels, an English fallback and Arabic/Urdu RTL layout. The extension maintains its own localized catalog and language preference. Live speeds emit at 250 ms; task refreshes run at one second. Fixed row heights and bounded chart history keep rendering costs predictable.

Flux 2.0 HTTP workers open independent handles to a task-owned destination-volume staging file. Strong validators and checked Content-Range bounds gate reuse; persisted byte ranges are verified before resume. Windows sparse allocation and Linux fallocate avoid concatenation. Bounded buffered I/O writes network bytes to disjoint offsets; no kernel-level zero-copy claim is made. A single sequential verification pass checks range hashes and calculates the final SHA-256 before exclusive publication. Legacy chunk tasks migrate their verified prefixes.

Torrent tasks lazily initialize an embedded Tokio librqbit session. SQLite remains the queue authority; bounded metainfo and destination ownership records permit restart without a daemon. The engine rechecks native pieces, applies per-task upload/download quotas and peer limits, and hashes selected outputs. HTTP and torrent budgets partition the shared cap. Task list responses omit torrent file arrays; native torrent_files pages stay below the frame limit and desktop detail views virtualize selection.

Provider resolution uses a two-process semaphore, bounded output and explicit cancellation cleanup. Exact-origin cookies enter a private Netscape jar removed after the process stops. Authentication failures can request a page-bound refresh capability and retry with current origin credentials. HTTP refresh retains range validators and offsets; changed content does not silently replace a downloaded prefix.

## Release boundaries

The release workflow provisions pinned build dependencies, runs core, extension, UI and real Windows integration tests, and packages an NSIS installer, portable application, extension and source archive. ZIP entries use sorted paths and fixed timestamps. Source export follows Git ignore rules, and validation records remove local paths and credentials.

SignPath submission uses immutable GitHub artifact IDs. Signed finalization validates Authenticode and regenerates checksums. Certificate issuance and production signing require external project approval and repository credentials; local package generation does not imply that signing occurred.
