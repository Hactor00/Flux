import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
export interface Input {
  url: string;
  filename: string;
  connections: number;
  speed_limit: number;
  expected_sha256: string | null;
  priority?: number;
  start_at?: number | null;
  stop_at?: number | null;
  media?: MediaOptions | null;
  referer?: string | null;
  auto_rename?: boolean;
  browser_source?: { page_url: string; frame_url?: string | null } | null;
  transport?: "auto" | "tcp" | "http3";
  content_type?: string | null;
  torrent?: TorrentOptions | null;
}
export interface TorrentOptions {
  seeding?: SeedingRule | null;
  upload_limit: number;
  peer_limit: number;
  only_files: number[];
  initial_peers: string[];
}
export type HostProvider = "real_debrid" | "all_debrid" | "premiumize";
export type SeedingRule =
  | { mode: "disabled" | "infinite" }
  | { mode: "ratio"; ratio_milli: number }
  | { mode: "duration"; minutes: number }
  | { mode: "either"; ratio_milli: number; minutes: number };
export interface HostAccount {
  provider: HostProvider;
  enabled: boolean;
  configured: boolean;
}
export interface TorrentFile {
  index: number;
  path: string;
  length: number;
  selected: boolean;
  sha256: string | null;
}
export interface TorrentSnapshot {
  seeded_seconds?: number;
  file_count?: number;
  info_hash: string;
  state: string;
  peers: number;
  uploaded: number;
  upload_bps: number;
  files: TorrentFile[];
}
export interface MediaOptions {
  manifest_snapshot?: { url: string; text: string } | null;
  manifest_video_id?: string | null;
  manifest_audio_id?: string | null;
  kind?: "file" | "hls" | "dash" | null;
  provider?: {
    url: string;
    format_id: string | null;
    height: number | null;
    audio_format_id?: string | null;
  } | null;
  segment_urls?: string[];
  container: "mp4" | "mkv" | "mp3" | "mka";
  audio_url?: string | null;
  video_stream?: number | null;
  audio_stream?: number | null;
  duration_seconds?: number | null;
  subtitles?: SubtitleTrack[];
  subtitle_mode?: "none" | "embed" | "external";
  chapters?: MediaChapter[];
  normalize_audio?: boolean;
}
export interface SubtitleTrack {
  url: string | null;
  index: number | null;
  language: string;
  title: string;
}
export interface MediaChapter {
  start_ms: number;
  end_ms: number;
  title: string;
}
export interface MediaStream {
  fps?: number | null;
  index: number;
  codec_type: string;
  codec_name: string;
  width: number | null;
  height: number | null;
  language: string | null;
  title?: string | null;
}
export interface MediaInfo {
  streams: MediaStream[];
  duration_seconds: number | null;
  format: string;
  subtitles?: SubtitleTrack[];
  chapters?: MediaChapter[];
}
export interface Settings {
  seeding: SeedingRule;
  torrent_upload_limit: number;
  // Application preferences are shared by every native window.
  concurrent_tasks: number;
  connections: number;
  speed_limit: number;
  queue_enabled: boolean;
  clipboard_monitor: boolean;
  minimize_to_tray: boolean;
  notifications: boolean;
  locale: string;
  theme: "dark" | "light";
  download_directory: string | null;
  completion_action: ShutdownAction;
  auto_categorize: boolean;
  category_folders: Record<
    "compressed" | "video" | "music" | "programs" | "documents",
    string | null
  >;
  clipboard_mode: "approval" | "silent_queue";
  launch_at_startup: boolean;
  torrent_watch: {
    enabled: boolean;
    folders: string[];
    destination: string | null;
    mode: "approval" | "automatic";
  };
}
export type ShutdownAction =
  "none" | "exit_app" | "sleep" | "hibernate" | "shutdown_computer";
export interface PendingPower {
  id: string;
  action: ShutdownAction;
  deadline: number;
}
export interface ResolvedFormat {
  alternative_ids?: string[];
  kind?: "hls" | "dash" | null;
  manifest_video_id?: string | null;
  manifest_audio_id?: string | null;
  id: string;
  url: string;
  audio_url: string | null;
  height: number | null;
  codec: string;
  size: number | null;
  container: "mp4" | "mkv" | "mp3" | "mka";
  fps?: number | null;
  audio_only?: boolean;
  audio_codec?: string | null;
}
export interface ResolvedMedia {
  title: string;
  duration: number | null;
  formats: ResolvedFormat[];
  subtitles?: SubtitleTrack[];
  chapters?: MediaChapter[];
  audio_tracks?: ResolvedAudioTrack[];
}
export interface ResolvedAudioTrack {
  id: string;
  url: string;
  codec: string;
  language: string | null;
  bitrate: number | null;
  size: number | null;
}
export interface CollectionEntry {
  url: string;
  title: string;
  duration: number | null;
}
export interface ResolvedCollection {
  title: string;
  entries: CollectionEntry[];
  next_offset: number;
  has_more: boolean;
  total: number | null;
}
export const defaultSettings: Settings = {
  seeding: { mode: "ratio", ratio_milli: 1000 },
  torrent_upload_limit: 0,
  concurrent_tasks: 3,
  connections: 0,
  speed_limit: 0,
  queue_enabled: true,
  clipboard_monitor: false,
  minimize_to_tray: true,
  notifications: true,
  locale: "en-US",
  theme: "dark",
  download_directory: null,
  completion_action: "none",
  auto_categorize: false,
  category_folders: {
    compressed: null,
    video: null,
    music: null,
    programs: null,
    documents: null,
  },
  clipboard_mode: "approval",
  launch_at_startup: false,
  torrent_watch: {
    enabled: false,
    folders: [],
    destination: null,
    mode: "approval",
  },
};
export type Status =
  | "awaiting_approval"
  | "cancelled"
  | "queued"
  | "running"
  | "paused"
  | "verifying"
  | "complete"
  | "failed";
export interface Task {
  id: string;
  created_at?: number | null;
  started_at?: number | null;
  finished_at?: number | null;
  timestamp_estimated?: boolean;
  request: Input;
  status: Status;
  completed: number;
  total: number | null;
  total_estimated?: boolean;
  media_progress?: MediaProgress | null;
  sha256: string | null;
  error: string | null;
  destination: string;
  torrent?: TorrentSnapshot | null;
}
export const taskFilename = (task: Task) =>
  task.destination.split(/[\\/]/).pop() || task.request.filename;
export const taskTime = (task: Task) =>
  task.started_at || task.finished_at || task.created_at;
export interface Progress {
  id: string;
  completed: number;
  total: number | null;
  bytes_per_second: number;
  total_estimated?: boolean;
  media?: MediaProgress | null;
}
export interface MediaProgress {
  duration_ms: number | null;
  processed_ms: number;
  segments_total: number | null;
  segments_completed: number;
  percent: number | null;
  eta_seconds: number | null;
  phase: "downloading" | "processing" | "live";
}
import { listen } from "@tauri-apps/api/event";
const harness = import.meta.env.DEV && import.meta.env.MODE === "e2e";
export const desktop = isTauri() || harness;
async function invoke<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!harness) return tauriInvoke<T>(command, args);
  const response = await fetch("/__flux_test/" + command, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args),
  });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}
export async function subscribe<T>(
  name: string,
  callback: (event: { payload: T }) => void,
): Promise<() => void> {
  if (isTauri()) return listen<T>(name, callback);
  if (!harness || name !== "download-progress") return () => {};
  let busy = false;
  const timer = setInterval(async () => {
    if (busy) return;
    busy = true;
    try {
      for (const event of await invoke<T[]>("progress"))
        callback({ payload: event });
    } finally {
      busy = false;
    }
  }, 250);
  return () => clearInterval(timer);
}
export interface BrowserIntegrationStatus {
  registered: boolean;
  extension_id: string | null;
  error: string | null;
  reminder_dismissed: boolean;
}
export const api = {
  surface: (
    view: "download" | "settings" | "details" | "power",
    id: string | null = null,
    initialUrl: string | null = null,
  ) => invoke<void>("open_surface", { view, id, initialUrl }),
  closeSurface: () => invoke<void>("close_surface"),
  task: (id: string) => invoke<Task>("get_task", { id }),
  approve: (id: string, input: Input, directory: string, start = true) =>
    invoke<Task>("approve_download", { id, input, directory, start }),
  reject: (id: string) => invoke<Task>("reject_download", { id }),
  addTo: (input: Input, directory: string | null, start = true) =>
    invoke<Task>("add_download_to", { input, directory, start }),
  list: () => (desktop ? invoke<Task[]>("list_tasks") : Promise.resolve([])),
  add: (input: Input) => invoke<Task>("add_download", { input }),
  addBatch: (inputs: Input[], directory?: string, start = true) =>
    invoke<Task[]>("add_batch", { inputs, directory, start }),
  pause: (id: string) => invoke<Task>("pause_download", { id }),
  resume: (id: string) => invoke<Task>("resume_download", { id }),
  settings: () =>
    desktop
      ? invoke<Settings>("get_settings")
      : Promise.resolve(defaultSettings),
  saveSettings: (input: Settings) =>
    invoke<Settings>("save_settings", { input }),
  hostAccounts: () =>
    desktop ? invoke<HostAccount[]>("host_accounts") : Promise.resolve([]),
  saveHostAccount: (
    provider: HostProvider,
    enabled: boolean,
    key: string | null,
  ) => invoke<HostAccount[]>("save_host_account", { provider, enabled, key }),
  remove: (id: string) => invoke<void>("remove_task", { id }),
  priority: (id: string, priority: number) =>
    invoke<Task>("set_priority", { id, priority }),
  pauseAll: () => invoke<void>("pause_all"),
  prepareUpdate: () => invoke<void>("prepare_update"),
  abortUpdate: () => invoke<void>("abort_update"),
  startQueue: () => invoke<void>("start_queue"),
  open: (id: string, reveal = false) =>
    invoke<void>("open_task", { id, reveal }),
  inspect: (
    url: string,
    referer: string | null = null,
    taskId: string | null = null,
  ) => invoke<MediaInfo>("inspect_media", { url, referer, taskId }),
  resolve: (url: string, taskId?: string) =>
    invoke<ResolvedMedia>("resolve_media", { url, taskId }),
  inspectCollection: (url: string, offset = 0, limit = 50) =>
    invoke<ResolvedCollection>("inspect_collection", { url, offset, limit }),
  importTorrent: (
    path: string,
    filename: string,
    directory: string | null,
    start: boolean,
    options: TorrentOptions,
    transfer: Pick<
      Input,
      "speed_limit" | "priority" | "start_at" | "stop_at" | "expected_sha256"
    >,
  ) =>
    invoke<Task>("import_torrent", {
      path,
      filename,
      directory,
      start,
      options,
      transfer,
    }),
  torrentOptions: (id: string, options: TorrentOptions, speedLimit: number) =>
    invoke<Task>("set_torrent_options", { id, options, speedLimit }),
  torrentWatchReport: () =>
    invoke<{ imported: number; error: string | null }>("torrent_watch_report"),
  runtimePlatform: () => invoke<string>("runtime_platform"),
  openReleasePage: () => invoke<void>("open_release_page"),
  expand: (input: string) => invoke<string[]>("expand_batch", { input }),
  registerBrowser: () => invoke<string>("register_browser"),
  browserIntegration: () =>
    invoke<BrowserIntegrationStatus>("browser_integration"),
  dismissBrowserReminder: () => invoke<void>("dismiss_browser_reminder"),
  suggestDirectory: (filename: string, contentType: string | null = null) =>
    invoke<string>("suggest_directory", { filename, contentType }),
  prepareDownloadPrefix: (
    prefix: string,
    filename: string,
    directory: string | null,
  ) =>
    invoke<{ filename: string; directory: string }>("prepare_download_prefix", {
      prefix,
      filename,
      directory,
    }),
  pendingPower: () => invoke<PendingPower | null>("pending_power"),
  cancelPower: () => invoke<void>("cancel_power"),
  dragCompleted: (ids: string[]) => invoke<void>("drag_completed", { ids }),
};
