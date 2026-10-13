# Local IPC protocol

Flux uses a native-endian four-byte length prefix followed by UTF-8 JSON. Windows x64 is little-endian. Both directions enforce a 256 KiB frame limit. Standard output contains protocol frames only; standard error is reserved for diagnostics. The loopback broker uses the same bounded framing as the Native Messaging host.

A request contains version 1, a UUID request_id, a command and its payload. A response repeats the version and request ID and contains ok, result or error. Concurrent responses can arrive out of order; the request ID correlates them.

| Command | Input and result |
| --- | --- |
| add | DownloadRequest to a durable queued task |
| propose | DownloadRequest to an awaiting-approval task |
| resolve | Provider URL and optional protected credentials to titles, formats, fps, audio tracks and subtitles |
| resolve_collection | URL, offset, limit of 1–50 and optional protected credentials to a collection page; at most 500 entries |
| task | ID to a current compact task summary |
| torrent_files | ID, offset and limit of 1–50 to a bounded file page |
| pause | ID to a checkpointed task summary |
| resume | ID to a queued task summary |
| list | The first bounded task page |
| list_page | Offset and limit of 1–50 to tasks, total, next_offset and has_more |
| inspect | URL, referer and optional protected credentials to media information |
| protect | Credentials to an encrypted envelope; handled only by the native host |
| refresh_poll | Outstanding page-bound, short-lived source renewal tickets |
| refresh_submit | Ticket, original page, matching resource URL and optional protected credentials |

DownloadRequest retains HTTP/media fields and adds optional torrent options: upload_limit, peer_limit, only_files and initial_peers. A magnet URL or torrent MIME/extension selects the torrent engine. Imported local metadata uses an opaque task-owned torrent:UUID source. The extension cannot ask the host to read arbitrary local file paths. Local import and approval remain Tauri commands.

Provider selections preserve format_id, audio_format_id and height. Media output supports MP4, MKV, MP3 and MKA, subtitle embedding or external output, chapters and normalization. Request size, URL schemes, credentials, source counts and output names are validated before insertion. Public task serialization removes credentials and their encrypted envelopes.

Task summaries include timestamps, status, byte totals, estimated totals, progress and optional torrent counters. Torrent summaries expose file_count without carrying the full file list. The torrent_files command pages metadata without rejecting otherwise supported large multi-file torrents. Desktop detail commands can read the full selected task and virtualize its file controls. Both list page types may return fewer entries than requested to satisfy the byte budget; use next_offset rather than assuming a full page.

The native host checks exact Chromium launcher origins against allowed-origins.json and checks Firefox's extension ID, supplied manifest path and executable identity. Broker discovery is stored in the user's Flux data directory with user-only ACLs or permissions. A random token authenticates TCP envelopes; it is not returned to browser content scripts or the frontend. The host can start the companion desktop in the background when necessary.

Durable transfer UUIDs are persisted before native requests. An individual captured browser download uses propose and a separate desktop approval window. A user-initiated extension batch uses add per persisted UUID and feeds the queue directly. Replaying a task ID returns the same task; changing the initial data under an existing ID is rejected. The outbox holds at most 500 entries and eight MiB of metadata. Authenticated preparations persist their encrypted envelope rather than rereading changing credentials during replay.

Browser transfer ownership changes only after the desktop proposal is durably accepted. Chromium's filename gate waits for the early pause; the browser transfer is then cancelled and erased. Persisted browserDrafts recover a pause that never reached the outbox after an MV3 worker restart. An inaccessible initial bridge returns ownership to the browser. Later desktop cancellation does not silently restart the browser download.

Refresh tickets bind a task, original page/frame and content identity, expire after a bounded interval and reject replay. The extension reads fresh same-origin session cookies and DOM/network sources without reloading, seeking or focusing the player. Renewed requests retain byte ranges and validators. Access failures, changed content or closed pages degrade to an explicit error rather than substituting another resource.

Native host binding is installed for Chrome, Edge, Brave, Chromium and Firefox. Chromium uses allowed_origins, Firefox uses allowed_extensions. See [browser distribution](browser-stores.md), [Chrome Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [Chrome downloads](https://developer.chrome.com/docs/extensions/reference/api/downloads) and [Chrome webRequest](https://developer.chrome.com/docs/extensions/reference/api/webRequest).
