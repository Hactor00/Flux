import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { DurableOutbox, NativeBridge } from "../bridge.js";
import { makeInput, mediaKind, safeFilename } from "../shared.js";

test("authentication is sealed before durable storage and frozen across retries", async () => {
  const data = storage();
  let prepares = 0;
  let calls = 0;
  const queue = new DurableOutbox(
    {
      async request(command, input) {
        calls++;
        assert.equal(input.protected_credentials, "encrypted-fixture");
        assert.equal(input.credentials, undefined);
        throw new Error("Desktop offline");
      },
    },
    async () => {},
    async (entry) => {
      prepares++;
      return { ...entry.input, protected_credentials: "encrypted-fixture" };
    },
  );
  await queue.enqueue(
    {
      ...makeInput("https://example.com/private.zip"),
      credentials: [{ cookie: "plaintext-fixture" }],
    },
    undefined,
    { tabId: 7 },
  );
  await until(() => data.outbox?.[0]?.attempts === 1);
  assert.ok(!JSON.stringify(data).includes("plaintext-fixture"));
  assert.equal(data.outbox[0].prepared, true);
  const replay = new DurableOutbox(
    {
      async request(command, input) {
        assert.equal(input.protected_credentials, "encrypted-fixture");
        return { id: "task" };
      },
    },
    async () => {},
    async () => {
      throw new Error("Must not hydrate a persisted request again");
    },
  );
  await replay.mutate((entries) => {
    entries[0].nextAt = 0;
  });
  await replay.pump();
  assert.equal(prepares, 1);
  assert.equal(calls, 1);
  assert.equal(data.outbox.length, 0);
});
test("browser ownership remains durable until source validation finishes", async () => {
  const data = storage();
  let validated = false;
  const queue = new DurableOutbox(
    {
      async request() {
        return { id: "task", source_validated: false };
      },
    },
    async (entry, task) => {
      if (task && !validated) throw new Error("Waiting for source validation");
    },
  );
  await queue.enqueue(makeInput("https://example.com/file.zip"), 73);
  await until(() => data.outbox?.[0]?.attempts === 1);
  assert.equal(data.outbox[0].downloadId, 73);
  validated = true;
  await queue.mutate((entries) => {
    entries[0].nextAt = 0;
  });
  await queue.pump();
  assert.equal(data.outbox.length, 0);
});
test("approval waits survive a worker restart without blocking other proposals", async () => {
  const data = storage(),
    calls = [];
  const queue = new DurableOutbox(
    {
      async request(command, input, id) {
        calls.push({ command, id });
        return { id, status: "awaiting_approval" };
      },
    },
    async () => false,
    async (entry) => entry.input,
    "propose",
  );
  const first = await queue.enqueue(makeInput("https://example.com/one.zip"));
  const second = await queue.enqueue(makeInput("https://example.com/two.zip"));
  await until(
    () => calls.length === 2 && data.outbox.every((entry) => entry.nextAt > 0),
  );
  assert.deepEqual(
    calls.map((call) => call.id),
    [first, second],
  );
  assert.ok(calls.every((call) => call.command === "propose"));
  const replay = new DurableOutbox(
    {
      async request(command, input, id) {
        calls.push({ command, id });
        return { id, status: "cancelled" };
      },
    },
    async () => true,
    async () => {
      throw Error("Prepared proposals must survive restart");
    },
    "propose",
  );
  await replay.mutate((entries) => {
    for (const entry of entries) entry.nextAt = 0;
  });
  await replay.pump();
  assert.deepEqual(
    calls.slice(2).map((call) => call.id),
    [first, second],
  );
  assert.equal(data.outbox.length, 0);
});
test("media filenames retain the selected MP3 or MKV output container", () => {
  assert.equal(
    safeFilename(
      "https://example.com/video.mp4",
      "Real video title.mp4",
      "mp3",
    ),
    "Real video title.mp3",
  );
  assert.equal(
    makeInput("https://example.com/video.m3u8", {
      filename: "Film",
      media: { container: "mkv" },
    }).filename,
    "Film.mkv",
  );
});
function storage() {
  const data = {};
  globalThis.chrome = {
    storage: {
      local: {
        async get(key) {
          return structuredClone(
            typeof key === "string" ? { [key]: data[key] } : data,
          );
        },
        async set(values) {
          Object.assign(data, structuredClone(values));
        },
      },
    },
  };
  return data;
}
async function until(condition) {
  const deadline = Date.now() + 2000;
  while (!condition()) {
    assert.ok(Date.now() < deadline, "Timed out");
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}
test("magnet proposals preserve display names and torrent options", () => {
  const input = makeInput(
    "magnet:?xt=urn:btih:cab507494d02ebb1178b38f2e9d7be299c86b862&dn=CON.zip",
    { referer: "https://example.com/releases" },
  );
  assert.equal(input.filename, "_CON.zip");
  assert.equal(input.media, null);
  assert.equal(input.torrent.peer_limit, 80);
  assert.deepEqual(input.torrent.only_files, []);
  assert.equal(input.browser_source.page_url, "https://example.com/releases");
  assert.ok(
    makeInput("magnet:?xt=urn:btih:ABCDEFGHIJKLMNOPQRSTUVWXYZ234567").torrent,
  );
});

test("magnet proposals reject malformed hashes, network authorities and v2-only sources", () => {
  for (const url of [
    "magnet:?xt=urn:btih:abcd",
    "magnet://attacker.test/?xt=urn:btih:cab507494d02ebb1178b38f2e9d7be299c86b862",
    "magnet:?xt=urn:btmh:1220abcd",
    "file:///private.torrent",
  ])
    assert.throws(() => makeInput(url));
});

test("detects adaptive streams by MIME and rejects non-HTTP blob sources", () => {
  assert.equal(
    mediaKind(
      "https://example.com/play?token=abc",
      "application/vnd.apple.mpegurl",
    ),
    "hls",
  );
  assert.equal(mediaKind("https://example.com/manifest.mpd"), "dash");
  assert.equal(mediaKind("blob:https://example.com/id", "video/mp4"), null);
  assert.equal(mediaKind("https://example.com/segment.ts"), null);
  assert.equal(
    makeInput("https://example.com/master.m3u8").filename,
    "master.mp4",
  );
  assert.throws(() => makeInput("https://name:secret@example.com/file.zip"));
  assert.equal(
    safeFilename("https://example.com/a", "C:\\fakepath\\CON.zip"),
    "_CON.zip",
  );
  assert.ok(
    new TextEncoder().encode(
      safeFilename("https://example.com/a", "ğ".repeat(300)),
    ).length <= 180,
  );
});
test("durable queue preserves UUID across service-worker restart and serializes writes", async () => {
  const data = storage();
  const sent = [];
  const offline = new DurableOutbox({
    async request(command, input, id) {
      sent.push({ command, input, id });
      throw new Error("Desktop broker unavailable");
    },
  });
  const ids = await Promise.all(
    Array.from({ length: 12 }, (_, index) =>
      offline.enqueue(makeInput(`https://example.com/${index}.zip`)),
    ),
  );
  await until(
    () => data.outbox?.length === 12 && data.outbox[0].attempts === 1,
  );
  assert.equal(new Set(data.outbox.map((item) => item.id)).size, 12);
  const online = new DurableOutbox({
    async request(command, input, id) {
      sent.push({ command, input, id });
      return { id };
    },
  });
  await online.mutate((entries) => {
    for (const item of entries) item.nextAt = 0;
  });
  await online.pump();
  assert.equal(data.outbox.length, 0);
  assert.deepEqual(
    sent.slice(-12).map((item) => item.id),
    ids,
  );
  assert.equal(sent[0].id, sent[1].id);
});
test("acknowledgement precedes browser ownership change and terminal failures leave no retry", async () => {
  const data = storage();
  const sequence = [];
  const queue = new DurableOutbox(
    {
      async request() {
        sequence.push("ack");
        return { id: "task" };
      },
    },
    async () => {
      sequence.push("browser-cancel");
      assert.equal(data.outbox.length, 1);
    },
  );
  await queue.enqueue(makeInput("https://example.com/file.zip"), 42);
  await until(() => data.outbox?.length === 0);
  assert.deepEqual(sequence, ["ack", "browser-cancel"]);
  const failures = new DurableOutbox({
    async request() {
      throw new Error("Destination already exists");
    },
  });
  await failures.enqueue(makeInput("https://example.com/file.zip"));
  await until(() => data.outbox?.length === 0);
});
test("native bridge correlates responses and rejects all pending messages on disconnect", async () => {
  let receive, disconnect;
  const messages = [];
  globalThis.chrome = {
    runtime: {
      connectNative() {
        return {
          onMessage: {
            addListener(fn) {
              receive = fn;
            },
          },
          onDisconnect: {
            addListener(fn) {
              disconnect = fn;
            },
          },
          postMessage(message) {
            messages.push(message);
          },
        };
      },
      lastError: { message: "Connection lost" },
    },
  };
  const bridge = new NativeBridge();
  const first = bridge.request("list_page", { offset: 0, limit: 1 });
  const second = bridge.request("list_page", { offset: 1, limit: 1 });
  receive({
    version: 1,
    request_id: messages[1].request_id,
    ok: true,
    result: { tasks: ["second"] },
  });
  assert.deepEqual(await second, { tasks: ["second"] });
  disconnect();
  await assert.rejects(first, /Connection lost/);
});
test("manifest identity and all 20 custom language catalogs are consistent", () => {
  const manifest = JSON.parse(
    readFileSync(new URL("../manifest.json", import.meta.url)),
  );
  assert.equal(manifest.manifest_version, 3);
  assert.ok(!manifest.permissions.includes("webRequestBlocking"));
  const id = [
    ...createHash("sha256")
      .update(Buffer.from(manifest.key, "base64"))
      .digest()
      .subarray(0, 16),
  ]
    .map((byte) => String.fromCharCode(97 + (byte >> 4), 97 + (byte & 15)))
    .join("");
  assert.equal(
    readFileSync(
      new URL("../extension-id.txt", import.meta.url),
      "utf8",
    ).trim(),
    id,
  );
  const files = readdirSync(new URL("../locales", import.meta.url));
  assert.equal(files.length, 20);
  const baseline = Object.keys(
    JSON.parse(readFileSync(new URL("../locales/en-US.json", import.meta.url))),
  ).sort();
  for (const file of files)
    assert.deepEqual(
      Object.keys(
        JSON.parse(
          readFileSync(new URL(`../locales/${file}`, import.meta.url)),
        ),
      ).sort(),
      baseline,
    );
});

test("local disconnect rejects pending requests and late old-port events cannot close a new connection", async () => {
  const ports = [];
  globalThis.chrome = {
    runtime: {
      connectNative() {
        const port = {
          onMessage: {
            addListener(fn) {
              port.receive = fn;
            },
          },
          onDisconnect: {
            addListener(fn) {
              port.closed = fn;
            },
          },
          postMessage(message) {
            port.message = message;
          },
          disconnect() {},
        };
        ports.push(port);
        return port;
      },
    },
  };
  const bridge = new NativeBridge();
  const first = bridge.request("list");
  bridge.disconnect();
  await assert.rejects(first, /connection closed/);
  assert.equal(bridge.connected, false);
  const second = bridge.request("list");
  ports[0].closed();
  assert.equal(bridge.connected, true);
  ports[1].receive({
    version: 1,
    request_id: ports[1].message.request_id,
    ok: true,
    result: [],
  });
  assert.deepEqual(await second, []);
  bridge.disconnect();
});
