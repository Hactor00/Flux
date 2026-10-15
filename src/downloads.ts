import type {
  CollectionEntry,
  Input,
  Task,
  ResolvedMedia,
  ResolvedFormat,
} from "./api";

export function mediaFormatKey(format: ResolvedFormat) {
  return format.height
    ? `video-${format.height}`
    : `format-${format.id || "auto"}`;
}

export function capturedMedia(task?: Task): ResolvedMedia | null {
  const media = task?.request.media;
  if (!task || !media?.provider) return null;
  return {
    title: task.request.filename.replace(/\.(mp4|mkv|mp3|mka)$/i, ""),
    duration: media.duration_seconds || null,
    formats: [
      {
        id: media.provider.format_id || "",
        url: task.request.url,
        audio_url: media.audio_url || null,
        height: media.provider.height,
        codec: "",
        container: media.container,
        audio_only: false,
        size: task.total,
      },
    ],
    audio_tracks: [],
    subtitles: media.subtitles || [],
    chapters: media.chapters || [],
  };
}

export function approvalSource(
  task: Task | undefined,
  selectedUrl: string | undefined,
  url: string,
) {
  return task?.request.url || selectedUrl || url;
}

export function providerSource(value: string) {
  try {
    const url = new URL(value);
    if (!/^https?:$/.test(url.protocol) || url.username || url.password)
      return false;
    return [
      "youtube.com",
      "youtube-nocookie.com",
      "youtu.be",
      "vimeo.com",
      "dailymotion.com",
      "dai.ly",
      "twitch.tv",
      "soundcloud.com",
      "bandcamp.com",
      "facebook.com",
      "fb.watch",
      "instagram.com",
      "tiktok.com",
      "reddit.com",
      "redd.it",
      "twitter.com",
      "x.com",
      "bilibili.com",
      "ted.com",
      "archive.org",
    ].some(
      (domain) =>
        url.hostname === domain || url.hostname.endsWith(`.${domain}`),
    );
  } catch {
    return false;
  }
}

export function sanitizeFilename(value: string) {
  let name =
    value.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").replace(/[. ]+$/g, "") ||
    "download";
  while (new TextEncoder().encode(name).length > 175)
    name = [...name].slice(0, -1).join("");
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name))
    name = `_${name}`;
  return name;
}

export function torrentSource(url: string) {
  if (/^magnet:\?/i.test(url.trim())) return true;
  try {
    return new URL(url).pathname.toLowerCase().endsWith(".torrent");
  } catch {
    return false;
  }
}

export function suggestedFilename(url: string, media: boolean) {
  let name = "download";
  try {
    const source = new URL(url);
    name =
      source.protocol === "magnet:"
        ? source.searchParams.get("dn") || "torrent"
        : decodeURIComponent(source.pathname.split("/").pop() || name);
  } catch {}
  name = sanitizeFilename(name);
  if (torrentSource(url)) return name.replace(/\.torrent$/i, "");
  return media ? name.replace(/\.(m3u8|mpd|mp4|mkv)$/i, "") + ".mp4" : name;
}

export function collectionInputs(
  entries: CollectionEntry[],
  base: Input,
): Input[] {
  const names = new Set<string>();
  const urls = new Set<string>();
  const inputs: Input[] = [];
  for (const entry of entries.slice(0, 500)) {
    const source = new URL(entry.url);
    if (!["http:", "https:"].includes(source.protocol))
      throw new Error("Collection entries must use HTTP or HTTPS.");
    if (urls.has(entry.url)) continue;
    urls.add(entry.url);
    const stem = sanitizeFilename(
      entry.title.replace(/\.(mp4|mkv|webm)$/i, ""),
    );
    let filename = `${stem}.mp4`;
    let ordinal = 2;
    while (names.has(filename.toLocaleLowerCase("en-US")))
      filename = `${stem}-${ordinal++}.mp4`;
    names.add(filename.toLocaleLowerCase("en-US"));
    inputs.push({
      ...base,
      url: entry.url,
      filename,
      expected_sha256: null,
      torrent: null,
      media: {
        container: "mp4",
        provider: { url: entry.url, format_id: null, height: null },
        duration_seconds: null,
      },
    });
  }
  return inputs;
}

export const isTorrentTask = (task: Task) =>
  !!task.torrent ||
  !!task.request.torrent ||
  torrentSource(task.request.url) ||
  task.request.content_type?.split(";")[0].trim().toLowerCase() ===
    "application/x-bittorrent";
