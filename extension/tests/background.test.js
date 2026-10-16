import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const event = () => ({
  listeners: [],
  addListener(fn) {
    this.listeners.push(fn);
  },
});
const session = {};
const local = { outbox: [], locale: "tr-TR" };
const menus = [];
const deliveries = [];
const requests = [];
const browserCalls = [],
  tasks = new Map();
let nativeOffline = false;
let supportsPresence = false;
function storage(data) {
  return {
    async get(key) {
      if (Array.isArray(key))
        return Object.fromEntries(key.map((key) => [key, data[key]]));
      return { [key]: data[key] };
    },
    async set(value) {
      Object.assign(data, value);
    },
    async remove(keys) {
      for (const key of [].concat(keys)) delete data[key];
    },
  };
}
globalThis.chrome = {
  runtime: {
    id: "test-extension",
    getManifest: () => ({ version: "2.4.2" }),
    onInstalled: event(),
    onStartup: event(),
    onMessage: event(),
    getURL: (path) => new URL("../" + path, import.meta.url).href,
    connectNative() {
      const port = {
        disconnect() {
          for (const listener of this.onDisconnect.listeners) listener();
        },
        onMessage: event(),
        onDisconnect: event(),
        postMessage(request) {
          requests.push(request);
          const result =
            request.command === "list_page"
              ? {
                  tasks: [],
                  ...(supportsPresence ? { desktop_version: "2.4.2" } : {}),
                }
              : request.command === "browser_connect"
                ? { desktop_version: "2.4.2" }
                : request.command === "protect"
                  ? "sealed-auth"
                  : request.command === "inspect"
                    ? { streams: [], format: "hls" }
                    : request.command === "propose"
                      ? tasks.get(request.request_id) || {
                          id: request.request_id,
                          status: "awaiting_approval",
                          source_validated: false,
                        }
                      : { tasks: [], total: 0 };
          queueMicrotask(() =>
            port.onMessage.listeners.forEach((fn) =>
              fn({
                version: 1,
                request_id: request.request_id,
                ok: !nativeOffline,
                error: nativeOffline ? "Desktop unavailable" : undefined,
                result,
              }),
            ),
          );
        },
      };
      return port;
    },
  },
  storage: {
    session: storage(session),
    local: storage(local),
    onChanged: event(),
  },
  webRequest: { onBeforeSendHeaders: event(), onHeadersReceived: event() },
  tabs: {
    onRemoved: event(),
    onUpdated: event(),
    async sendMessage(tab, message, target) {
      deliveries.push({ tab, message, target });
    },
    async query() {
      return [];
    },
  },
  downloads: {
    onDeterminingFilename: event(),
    onCreated: event(),
    async pause(id) {
      browserCalls.push(["pause", id]);
    },
    async resume(id) {
      browserCalls.push(["resume", id]);
    },
    async cancel(id) {
      browserCalls.push(["cancel", id]);
    },
    async erase(value) {
      browserCalls.push(["erase", value.id]);
    },
  },
  contextMenus: {
    onClicked: event(),
    async removeAll() {
      menus.length = 0;
    },
    create(value) {
      menus.push(value);
    },
  },
  alarms: { onAlarm: event(), async create() {} },
  action: {
    async setBadgeText() {},
    async setBadgeBackgroundColor() {},
    async openPopup() {},
  },
};
globalThis.fetch = async (url) => {
  try {
    return {
      ok: true,
      json: async () => JSON.parse(await readFile(new URL(url), "utf8")),
    };
  } catch {
    return { ok: false };
  }
};
await import("../background.js");
const flush = async () => {
  for (let count = 0; count < 8; count++)
    await new Promise((resolve) => setImmediate(resolve));
};
const message = (
  value,
  sender = {
    id: "test-extension",
    tab: { id: 7, url: "https://page.test/" },
    url: "https://frame.test/player",
    frameId: 2,
  },
) =>
  new Promise((resolve) =>
    chrome.runtime.onMessage.listeners[0](value, sender, resolve),
  );
test("presence negotiation keeps legacy hosts usable and announces the installed extension to current hosts", async () => {
  await flush();
  const start = requests.length;
  const legacy = await message({ type: "status" }, { id: "test-extension" });
  assert.equal(legacy.result.connected, true);
  assert.equal(legacy.result.legacy_desktop, true);
  assert.equal(
    requests
      .slice(start)
      .some((request) => request.command === "browser_connect"),
    false,
  );
  supportsPresence = true;
  const current = await message({ type: "status" }, { id: "test-extension" });
  assert.equal(current.result.desktop_version, "2.4.2");
  assert.equal(current.result.legacy_desktop, false);
  assert.equal(
    requests.filter((request) => request.command === "browser_connect").at(-1)
      .payload.extension_version,
    "2.4.2",
  );
});
test("interactive media handoff reports native failure and retries the same persisted proposal", async () => {
  const input = {
    type: "enqueue",
    url: "https://media.test/confirmed",
    options: { filename: "Confirmed.mp4", media: { container: "mp4" } },
  };
  nativeOffline = true;
  try {
    const failed = await message(structuredClone(input));
    assert.equal(failed.ok, false);
    assert.match(failed.error, /Desktop unavailable/);
    const entry = local.outbox.find((entry) => entry.input.url === input.url);
    assert.ok(entry);
    nativeOffline = false;
    const confirmed = await message(structuredClone(input));
    assert.equal(confirmed.ok, true);
    assert.equal(confirmed.result.id, entry.id);
    assert.equal(confirmed.result.task.status, "awaiting_approval");
    assert.equal(
      local.outbox.some((candidate) => candidate.id === entry.id),
      false,
    );
  } finally {
    nativeOffline = false;
  }
});
test("background scopes media to frames, keeps auth session-only and refreshes localized menus", async () => {
  chrome.webRequest.onHeadersReceived.listeners[0]({
    tabId: 7,
    frameId: 2,
    url: "https://media.test/stream",
    initiator: "https://frame.test",
    responseHeaders: [
      { name: "Content-Type", value: "application/vnd.apple.mpegurl" },
    ],
  });
  chrome.webRequest.onHeadersReceived.listeners[0]({
    tabId: 7,
    frameId: 3,
    url: "https://other.test/file.mp4",
    responseHeaders: [],
  });
  await flush();
  const captured = await message({ type: "candidates" });
  assert.equal(captured.result.length, 1);
  assert.equal(captured.result[0].kind, "hls");
  assert.equal(deliveries[0].target.frameId, 2);
  assert.equal(deliveries[0].message.candidates.length, 1);
  const all = await message(
    { type: "candidates", tabId: 7 },
    {
      id: "test-extension",
      url: "chrome-extension://test-extension/popup.html",
    },
  );
  assert.equal(all.result.length, 2);
  chrome.webRequest.onBeforeSendHeaders.listeners[0]({
    tabId: 7,
    url: "https://media.test/stream",
    requestHeaders: [{ name: "Cookie", value: "private-session" }],
  });
  await flush();
  await message({
    type: "inspect",
    url: "https://media.test/stream",
    referer: "https://frame.test/player",
  });
  assert.equal(requests.at(-1).payload.referer, "https://frame.test/player");
  assert.equal(requests.at(-1).payload.protected_credentials, "sealed-auth");
  assert.ok(!JSON.stringify(local).includes("private-session"));
  chrome.webRequest.onBeforeSendHeaders.listeners[0]({
    tabId: 7,
    url: "https://media.test/stream",
    requestHeaders: [],
  });
  await flush();
  assert.equal(session["auth:7"][0].cookie, null);
  assert.equal(session["auth:7"][0].authorization, null);
  await chrome.runtime.onInstalled.listeners[0]();
  assert.equal(menus[0].title, "Flux ile indir");
  local.locale = "pcm-NG";
  chrome.storage.onChanged.listeners[0](
    { locale: { newValue: "pcm-NG" } },
    "local",
  );
  await flush();
  assert.ok(menus[0].title.includes("Flux"));
  assert.equal(
    (
      await message(
        { type: "status" },
        { id: "foreign-extension", url: "https://page.test/" },
      )
    ).ok,
    false,
  );
  assert.equal(
    (
      await message(
        { type: "status" },
        { id: "test-extension", tab: { id: 7 }, url: "file:///private" },
      )
    ).ok,
    false,
  );
});
async function until(condition) {
  const deadline = Date.now() + 2500;
  while (!condition()) {
    assert.ok(Date.now() < deadline, "Timed out");
    await flush();
  }
}
async function captureDownload(id, url = "https://media.test/archive.zip") {
  const requestStart = requests.length;
  let suggestions = 0;
  chrome.downloads.onDeterminingFilename.listeners[0](
    {
      id,
      url,
      finalUrl: url,
      filename: "C:\\Downloads\\archive.zip",
      referrer: "https://page.test/",
    },
    () => {
      browserCalls.push(["suggest", id]);
      suggestions++;
    },
  );
  await until(
    () =>
      suggestions === 1 &&
      requests.slice(requestStart).some((r) => r.command === "propose") &&
      local.outbox.length === 0,
  );
  const request = requests
    .slice(requestStart)
    .find((r) => r.command === "propose");
  assert.equal(suggestions, 1);
  assert.ok(
    browserCalls.findIndex((call) => call[0] === "pause" && call[1] === id) <
      browserCalls.findIndex((call) => call[0] === "suggest" && call[1] === id),
  );
  assert.ok(
    browserCalls.findIndex((call) => call[0] === "cancel" && call[1] === id) <
      browserCalls.findIndex((call) => call[0] === "suggest" && call[1] === id),
    "Browser cancellation must finish before the Save As filename gate is released",
  );
  return { id: request.request_id, input: request.payload, downloadId: id };
}
test("browser transfer is cancelled and erased as soon as the durable desktop proposal is accepted", async () => {
  browserCalls.length = 0;
  await captureDownload(81);
  assert.deepEqual(
    browserCalls.filter((call) => ["cancel", "erase"].includes(call[0])),
    [
      ["cancel", 81],
      ["erase", 81],
    ],
  );
  assert.ok(!browserCalls.some((call) => call[0] === "resume"));
  assert.equal(local.outbox.length, 0);
});
test("onCreated pauses a transfer before filename determination or desktop startup", async () => {
  browserCalls.length = 0;
  chrome.downloads.onCreated.listeners[0]({
    id: 82,
    url: "https://media.test/archive.zip",
  });
  await until(() =>
    browserCalls.some((call) => call[0] === "pause" && call[1] === 82),
  );
  assert.ok(!browserCalls.some((call) => call[0] === "cancel"));
  await captureDownload(82);
  assert.equal(
    browserCalls.filter((call) => call[0] === "pause" && call[1] === 82).length,
    1,
  );
  assert.ok(!browserCalls.some((call) => call[0] === "resume"));
});
test("unavailable desktop safely returns the unclaimed transfer to Chrome", async () => {
  browserCalls.length = 0;
  nativeOffline = true;
  let suggested = 0;
  const initialRequests = requests.length;
  chrome.downloads.onDeterminingFilename.listeners[0](
    { id: 83, url: "https://media.test/archive.zip" },
    () => suggested++,
  );
  await until(() => suggested === 1);
  nativeOffline = false;
  assert.ok(
    browserCalls.some((call) => call[0] === "resume" && call[1] === 83),
  );
  assert.ok(
    !requests.slice(initialRequests).some((r) => r.command === "propose"),
  );
  assert.ok(!browserCalls.some((call) => call[0] === "cancel"));
});
test("download requests without a tab preserve exact session headers privately", async () => {
  const url = "https://download.test/signed.apk";
  chrome.webRequest.onBeforeSendHeaders.listeners[0]({
    tabId: -1,
    url,
    requestHeaders: [
      { name: "Cookie", value: "download-session-fixture" },
      { name: "User-Agent", value: "Browser-UA-fixture" },
      { name: "Referer", value: "https://download.test/share/" },
    ],
  });
  await flush();
  const entry = await captureDownload(84, url);
  const sealed = requests.filter((item) => item.command === "protect").at(-1);
  assert.equal(
    sealed.payload.credentials[0].cookie,
    "download-session-fixture",
  );
  assert.equal(sealed.payload.credentials[0].user_agent, "Browser-UA-fixture");
  assert.equal(entry.input.referer, "https://download.test/share/");
  assert.ok(!JSON.stringify(local).includes("download-session-fixture"));
  tasks.set(entry.id, { id: entry.id, status: "cancelled" });
  await message({ type: "retry" });
  await until(() => local.outbox.length === 0);
});
test("disabled interception leaves Chrome alone and releases its filename gate", async () => {
  local.interceptDownloads = false;
  chrome.storage.onChanged.listeners[0](
    { interceptDownloads: { newValue: false } },
    "local",
  );
  browserCalls.length = 0;
  let suggested = 0;
  chrome.downloads.onDeterminingFilename.listeners[0](
    { id: 85, url: "https://download.test/plain.zip" },
    () => suggested++,
  );
  await until(() => suggested === 1);
  assert.deepEqual(browserCalls, []);
  local.interceptDownloads = true;
  chrome.storage.onChanged.listeners[0](
    { interceptDownloads: { newValue: true } },
    "local",
  );
});
test("startup resumes orphaned browser pauses and preserves durable proposals", async () => {
  browserCalls.length = 0;
  local.browserDrafts = [90, 91];
  local.outbox = [
    {
      id: crypto.randomUUID(),
      downloadId: 91,
      input: { url: "https://example.com/held.zip", filename: "held.zip" },
      prepared: true,
      nextAt: Date.now() + 60000,
    },
  ];
  await chrome.runtime.onStartup.listeners[0]();
  await flush();
  assert.deepEqual(local.browserDrafts, []);
  assert.deepEqual(
    browserCalls.filter((call) => call[0] === "resume"),
    [["resume", 90]],
  );
  assert.equal(local.outbox[0].downloadId, 91);
  local.outbox.push({
    id: crypto.randomUUID(),
    downloadId: 92,
    ready: false,
    input: {
      url: "https://example.com/recovered.zip",
      filename: "recovered.zip",
    },
    context: { browserCancelled: true },
    nextAt: Date.now() + 60000,
  });
  await chrome.runtime.onStartup.listeners[0]();
  assert.ok(
    browserCalls.some(([action, id]) => action === "cancel" && id === 92),
  );
  assert.equal(local.outbox[1].ready, true);
  local.outbox = [];
});
