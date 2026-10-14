import test from "node:test";
import assert from "node:assert/strict";
import {
  collectionInputs,
  sanitizeFilename,
  suggestedFilename,
  torrentSource,
  isTorrentTask,
  providerSource,
  capturedMedia,
  approvalSource,
} from "../src/downloads.ts";

const base = {
  url: "",
  filename: "",
  connections: 0,
  speed_limit: 1024,
  expected_sha256: null,
  torrent: null,
};
test("captured provider media is immediately selectable and approval retains its original signed URL", () => {
  const task = {
    request: {
      url: "https://cdn.example/video?signature=captured",
      filename: "Chosen title.mp4",
      media: {
        provider: {
          url: "https://youtube.com/watch?v=1",
          format_id: "136",
          height: 720,
        },
        audio_url: "https://cdn.example/audio",
        container: "mp4",
        chapters: [],
        subtitles: [],
      },
    },
    total: null,
  };
  const captured = capturedMedia(task);
  assert.equal(captured.formats[0].height, 720);
  assert.equal(captured.formats[0].id, "136");
  assert.equal(captured.formats[0].audio_url, task.request.media.audio_url);
  assert.equal(
    approvalSource(task, "https://cdn.example/video?signature=renewed", ""),
    task.request.url,
  );
  assert.equal(
    approvalSource(
      undefined,
      "https://cdn.example/new",
      "https://youtube.com/watch?v=1",
    ),
    "https://cdn.example/new",
  );
  assert.equal(capturedMedia({ request: { media: null } }), null);
});
test("desktop provider recognition matches browser hosts without accepting spoofed origins", () => {
  for (const value of [
    "https://vimeo.com/123",
    "https://soundcloud.com/artist/track",
    "https://www.youtube.com/watch?v=1",
  ])
    assert.equal(providerSource(value), true);
  for (const value of [
    "https://youtube.com.attacker.test/watch",
    "https://user:password@vimeo.com/123",
    "file:///youtube.com/watch",
  ])
    assert.equal(providerSource(value), false);
});
test("magnet display names become safe output folders without a file suffix", () => {
  assert.equal(torrentSource("magnet:?xt=urn:btih:abcd&dn=Example"), true);
  assert.equal(
    suggestedFilename("magnet:?xt=urn:btih:abcd&dn=CON", false),
    "_CON",
  );
  assert.equal(
    suggestedFilename("https://example.org/Example.torrent?token=1", false),
    "Example",
  );
  assert.equal(
    torrentSource("https://example.org/video.mp4?torrent=true"),
    false,
  );
  assert.equal(
    isTorrentTask({
      request: {
        url: "https://example.org/stream",
        torrent: { peer_limit: 80 },
      },
    }),
    true,
  );
  assert.equal(
    isTorrentTask({
      request: {
        url: "https://example.org/download?id=7",
        content_type: "Application/X-BitTorrent; charset=binary",
      },
    }),
    true,
  );
  assert.equal(
    isTorrentTask({
      request: {
        url: "https://example.org/download",
        content_type: "application/octet-stream",
      },
    }),
    false,
  );
});
test("collection queue requests keep provider pages and disambiguate case-insensitive names", () => {
  const requests = collectionInputs(
    [
      { url: "https://example.org/watch/1", title: "Film", duration: 123 },
      { url: "https://example.org/watch/2", title: "film", duration: null },
      { url: "https://example.org/watch/3", title: "Film-2", duration: null },
      {
        url: "https://example.org/watch/1",
        title: "Duplicate",
        duration: null,
      },
    ],
    base,
  );
  assert.deepEqual(
    requests.map((input) => input.filename),
    ["Film.mp4", "film-2.mp4", "Film-2-2.mp4"],
  );
  assert.equal(requests[0].media.provider.url, requests[0].url);
  assert.equal(requests[0].media.provider.format_id, null);
  assert.equal(requests[0].media.duration_seconds, null);
  assert.equal(requests[0].speed_limit, 1024);
  assert.equal(requests[0].torrent, null);
});
test("collection input rejects non-network entries and caps queue growth", () => {
  assert.throws(
    () =>
      collectionInputs([{ url: "file:///secret.txt", title: "File" }], base),
    /HTTP/,
  );
  assert.equal(
    collectionInputs(
      Array.from({ length: 600 }, (_, index) => ({
        url: `https://example.org/${index}`,
        title: `${index}`,
        duration: null,
      })),
      base,
    ).length,
    500,
  );
});
test("filenames retain complete Unicode code points within the UTF-8 limit", () => {
  const value = sanitizeFilename("🍇".repeat(100));
  assert.ok(new TextEncoder().encode(value).length <= 175);
  assert.equal(value.includes("\ufffd"), false);
  assert.equal(sanitizeFilename("report?.mp4. "), "report_.mp4");
});
