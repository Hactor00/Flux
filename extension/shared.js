export const HOST = "io.flux.download";
export const MAX_CANDIDATES = 80;
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
  if (
    !/^https?:$/.test(parsed.protocol) ||
    parsed.username ||
    parsed.password ||
    url.length > 16384
  )
    throw new Error("Invalid HTTP(S) URL");
  const kind = mediaKind(url);
  const media =
    options.media ||
    (kind === "hls" || kind === "dash" ? { container: "mp4" } : null);
  let referer = options.referer || null;
  if (referer && !/^https?:\/\//i.test(referer)) referer = null;
  return {
    url,
    filename: safeFilename(url, options.filename, media?.container || false),
    connections: options.connections ?? 0,
    speed_limit: 0,
    expected_sha256: null,
    priority: 0,
    start_at: null,
    stop_at: null,
    media,
    referer,
    auto_rename: true,
    browser_source: referer ? { page_url: referer } : null,
    content_type: options.content_type || null,
  };
}
