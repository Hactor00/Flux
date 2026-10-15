export const HOST = "io.flux.download";
export const MAX_CANDIDATES = 80;
export function isProviderUrl(value) {
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
export function isCollectionUrl(value) {
  if (!isProviderUrl(value)) return false;
  const url = new URL(value);
  return (
    url.searchParams.has("list") ||
    /^\/(?:playlist|channel\/|c\/|user\/|@[^/]+(?:\/(?:videos|shorts|streams))?\/?$)/.test(
      url.pathname,
    ) ||
    /\/(?:sets|albums?|showcase|videos|collection|playlists)(?:\/|$)/.test(
      url.pathname,
    )
  );
}
export function mediaKind(url, contentType = "") {
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (!/^https?:$/.test(parsed.protocol)) return null;
  const path = parsed.pathname.toLowerCase();
  if (/\.m3u8$/.test(path) || /mpegurl/i.test(contentType)) return "hls";
  if (/\.mpd$/.test(path) || /dash\+xml/i.test(contentType)) return "dash";
  if (
    /\.(mp4|webm|mkv|mov|m4v|mp3|m4a|aac|ogg|wav|flac)$/.test(path) ||
    /^(video|audio)\//i.test(contentType)
  )
    return "file";
  return null;
}
export function safeFilename(url, suggested, media = false) {
  let name = suggested;
  if (!name) {
    try {
      name = decodeURIComponent(new URL(url).pathname.split("/").pop());
    } catch {
      name = "download";
    }
  }
  name = (name || "download")
    .split(/[\\/]/)
    .pop()
    .replace(/[<>:"/\\|?*\x00-\x1f]/g, "_")
    .replace(/[. ]+$/, "");
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name))
    name = `_${name}`;
  // Bound UTF-8 length, matching the engine's filename constraint.
  while (new TextEncoder().encode(name).length > 180) name = name.slice(0, -1);
  if (media)
    name =
      name.replace(/\.(m3u8|mpd|mp4|mkv|mp3)$/i, "") +
      "." +
      (typeof media === "string" ? media : "mp4");
  return name || "download";
}
export function makeInput(url, options = {}) {
  const parsed = new URL(url);
  const magnet = parsed.protocol === "magnet:";
  if (
    (!/^https?:$/.test(parsed.protocol) && !magnet) ||
    parsed.username ||
    parsed.password ||
    url.length > 16384
  )
    throw new Error("Invalid download URL");
  if (
    magnet &&
    (parsed.host ||
      parsed.pathname ||
      !parsed.searchParams
        .getAll("xt")
        .some((value) =>
          /^urn:btih:(?:[a-f\d]{40}|[a-z2-7]{32})$/i.test(value),
        ))
  )
    throw new Error("Invalid BitTorrent v1 magnet");
  const kind = mediaKind(url);
  let media = magnet
    ? null
    : options.media ||
      (kind === "hls" || kind === "dash" ? { container: "mp4" } : null);
  if (media) {
    media = structuredClone(media);
    for (const key of [
      "manifest_snapshot",
      "manifest_video_id",
      "manifest_audio_id",
    ])
      if (media[key] == null) delete media[key];
    if (media.provider && media.provider.audio_format_id == null)
      delete media.provider.audio_format_id;
  }
  let referer = options.referer || null;
  if (referer && !/^https?:\/\//i.test(referer)) referer = null;
  return {
    url,
    filename: safeFilename(
      url,
      options.filename || (magnet ? parsed.searchParams.get("dn") : null),
      media?.container || false,
    ),
    connections: options.connections ?? 0,
    speed_limit: 0,
    expected_sha256: null,
    priority: 0,
    start_at: null,
    stop_at: null,
    media,
    ...(magnet
      ? {
          torrent: {
            upload_limit: 0,
            peer_limit: 80,
            only_files: [],
            initial_peers: [],
          },
        }
      : {}),
    referer,
    auto_rename: true,
    browser_source: referer ? { page_url: referer } : null,
    content_type: options.content_type || null,
  };
}
