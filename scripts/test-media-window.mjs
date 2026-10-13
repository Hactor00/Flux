import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:http";
import { mkdtemp, mkdir, readFile, writeFile, readdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { DesktopDriver } from "./desktop-driver.mjs";

const binary = resolve(process.argv[2] || "target/release");
const suite = process.argv.includes("--suite");
const root = await mkdtemp(join(tmpdir(), "flux-media-window-"));
const data = join(root, "Flux"),
  output = join(root, "output");
await mkdir(data);
await mkdir(output);
const origin = `chrome-extension://${(await readFile("extension/extension-id.txt", "utf8")).trim()}/`;
await writeFile(join(data, "allowed-origins.json"), JSON.stringify([origin]));
const db = new DatabaseSync(join(data, "tasks.sqlite"));
db.exec(
  "CREATE TABLE settings(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)",
);
db.prepare("INSERT INTO settings VALUES(1,?)").run(
  JSON.stringify({
    locale: "tr-TR",
    download_directory: output,
    connections: 0,
    notifications: false,
    minimize_to_tray: false,
  }),
);
db.close();
const files = new Map();
for (const name of await readdir("tests/fixtures/media"))
  files.set(name, await readFile(join("tests/fixtures/media", name)));
files.set(
  "english.srt",
  Buffer.from("1\n00:00:00,200 --> 00:00:02,000\nNative subtitle test\n"),
);
const fixture = createServer((request, response) => {
  if (
    suite &&
    new URL(request.url, "http://fixture").searchParams.get("token") ===
      "expired"
  ) {
    response.writeHead(403);
    response.end();
    return;
  }
  const name = new URL(request.url, "http://fixture").pathname.slice(1);
  const bytes = files.get(name === "playlist" ? "film.m3u8" : name);
  if (!bytes) {
    response.writeHead(404);
    response.end();
    return;
  }
  // Deliberately omit Content-Length, validators and a helpful playlist suffix/MIME.
  response.writeHead(200, { "Content-Type": "application/octet-stream" });
  let offset = 0;
  const send = () => {
    if (response.destroyed) return;
    const next = Math.min(offset + 4096, bytes.length);
    response.write(bytes.subarray(offset, next));
    offset = next;
    if (offset === bytes.length) response.end();
    else setTimeout(send, 5);
  };
  send();
});
fixture.listen(0, "127.0.0.1");
await once(fixture, "listening");
const reservation = createServer();
reservation.listen(0, "127.0.0.1");
await once(reservation, "listening");
const port = reservation.address().port;
await new Promise((r) => reservation.close(r));
const driver = new DesktopDriver(port);
const env = {
  ...process.env,
  LOCALAPPDATA: root,
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
};
const app = spawn(join(binary, "flux-desktop.exe"), [], {
  env,
  windowsHide: true,
  stdio: ["ignore", "pipe", "pipe"],
});
app.stdout.resume();
app.stderr.on("data", (chunk) => process.stderr.write(chunk));
const host = spawn(join(binary, "flux-native-host.exe"), [origin], {
  env,
  windowsHide: true,
  stdio: ["pipe", "pipe", "pipe"],
});
let buffer = Buffer.alloc(0);
const pending = new Map();
host.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (buffer.length >= 4 && buffer.length >= buffer.readUInt32LE(0) + 4) {
    const size = buffer.readUInt32LE(0),
      value = JSON.parse(buffer.subarray(4, size + 4));
    buffer = buffer.subarray(size + 4);
    pending.get(value.request_id)?.(value);
    pending.delete(value.request_id);
  }
});
const call = (command, payload) =>
  new Promise((accept, reject) => {
    const request_id = randomUUID(),
      timer = setTimeout(() => {
        pending.delete(request_id);
        reject(Error("Native media test timed out"));
      }, 15000);
    pending.set(request_id, (value) => {
      clearTimeout(timer);
      value.ok ? accept(value.result) : reject(Error(value.error));
    });
    const body = Buffer.from(
        JSON.stringify({ version: 1, request_id, command, payload }),
      ),
      prefix = Buffer.alloc(4);
    prefix.writeUInt32LE(body.length);
    host.stdin.write(Buffer.concat([prefix, body]));
  });
let healingTimer;
try {
  const main = await driver.connect("main");
  let renewalCount = 0;
  let healingBusy = false;
  healingTimer = suite
    ? setInterval(async () => {
        if (healingBusy) return;
        healingBusy = true;
        try {
          for (const request of await call("refresh_poll")) {
            await call("refresh_submit", {
              ticket: request.ticket,
              page_url: request.page_url,
              url: request.url.replace("token=expired", "token=fresh"),
              protected_credentials: null,
            });
            renewalCount++;
          }
        } catch {
          /* Shutdown may close the broker while a poll is pending. */
        } finally {
          healingBusy = false;
        }
      }, 200)
    : null;
  if (healingTimer) healingTimer.unref();
  const source = `http://127.0.0.1:${fixture.address().port}`;
  const task = await call("propose", {
    url: `${source}/playlist${suite ? "?token=expired" : ""}`,
    filename: "Test filmi.mp4",
    connections: 0,
    speed_limit: 16384,
    browser_source: suite
      ? { page_url: `${source}/watch`, frame_url: null }
      : null,
    media: {
      kind: "hls",
      container: "mp4",
      ...(suite
        ? {
            subtitles: [
              {
                url: `${source}/english.srt`,
                index: null,
                language: "eng",
                title: "English",
              },
            ],
            chapters: [
              { start_ms: 0, end_ms: 4000, title: "First chapter" },
              { start_ms: 4000, end_ms: 8000, title: "Second chapter" },
            ],
          }
        : {}),
    },
  });
  const download = await driver.connect("download");
  const ready = Date.now() + 10000;
  while (
    !(await download.evaluate(
      "!!document.querySelector('input[name=filename]')",
    ))
  ) {
    assert.ok(Date.now() < ready);
    await new Promise((r) => setTimeout(r, 100));
  }
  if (suite) {
    await download.evaluate(
      "document.querySelector('.media-extras').open=true; document.querySelector('.media-extras [role=combobox]').click(); true",
    );
    await download.evaluate(
      "[...document.querySelectorAll('[role=option]')].find(item=>item.textContent.includes('Videoya göm')).click(); true",
    );
    await download.evaluate(
      "const switches=document.querySelectorAll('.media-extras [role=switch]'); switches[0].click(); switches[1].click(); true",
    );
    await new Promise((resolve) => setTimeout(resolve, 100));
    assert.equal(
      await download.evaluate(
        "document.querySelectorAll('.media-extras [role=switch][data-state=checked]').length",
      ),
      3,
    );
    const screenshot = await download.call("Page.captureScreenshot", {
      format: "png",
    });
    await writeFile(
      "docs/images/media-suite-1.5.png",
      Buffer.from(screenshot.data, "base64"),
    );
  }
  await download.evaluate(
    "document.querySelector('form').requestSubmit(); true",
  );
  const details = await driver.connect("details");
  let current;
  const deadline = Date.now() + 45000;
  do {
    current = await call("task", { id: task.id });
    assert.notEqual(current.status, "failed", current.error);
    assert.ok(Date.now() < deadline, "Movie progress unavailable");
    if (
      current.media_progress?.percent > 0 &&
      current.media_progress.percent < 99 &&
      current.media_progress.eta_seconds > 0
    )
      break;
    await new Promise((r) => setTimeout(r, 150));
  } while (true);
  assert.equal(current.total_estimated, true);
  assert.equal(current.media_progress.duration_ms, 8000);
  const ui = Date.now() + 7000;
  let popup, list;
  do {
    popup = await details.evaluate(
      "({percent:document.querySelector('.transfer-summary strong')?.textContent,timing:document.querySelector('.transfer-timing')?.textContent,total:document.querySelector('.transfer-summary > div:first-child span')?.textContent})",
    );
    list = await main.evaluate(
      "({percent:document.querySelector('.desktop-progress small')?.textContent,total:document.querySelector('.desktop-row [role=cell]:nth-child(2)')?.textContent,remaining:document.querySelector('.desktop-row [role=cell]:nth-child(6)')?.textContent})",
    );
    if (
      popup?.percent?.startsWith("≈ ") &&
      popup.timing.includes("Kalan süre: ≈ ") &&
      list?.remaining?.startsWith("≈ ")
    )
      break;
    assert.ok(Date.now() < ui, "Native ETA did not reach both views");
    await new Promise((r) => setTimeout(r, 100));
  } while (true);
  assert.ok(popup.total.includes("≈ "));
  assert.ok(list.total.includes("≈ "));
  assert.ok(popup.timing.includes("Parçalar:"));
  assert.ok(popup.timing.includes("İndirilen medya süresi:"));
  const screenshot = await details.call("Page.captureScreenshot", {
    format: "png",
  });
  await writeFile(
    suite
      ? "docs/images/media-suite-progress-1.5.png"
      : "docs/images/media-progress-1.4.png",
    Buffer.from(screenshot.data, "base64"),
  );
  const paused = await call("pause", { id: task.id });
  assert.ok(paused.completed > 0 && paused.media_progress.percent > 0);
  await call("resume", { id: task.id });
  do {
    current = await call("task", { id: task.id });
    assert.notEqual(current.status, "failed", current.error);
    assert.ok(Date.now() < deadline, "Movie completion timed out");
    if (current.status === "complete") break;
    await new Promise((r) => setTimeout(r, 200));
  } while (true);
  assert.equal(current.total_estimated, false);
  assert.equal(current.total, current.completed);
  const probe = spawn(
    join(binary, "ffprobe.exe"),
    [
      "-v",
      "error",
      "-show_streams",
      "-show_format",
      "-show_chapters",
      "-of",
      "json",
      current.destination,
    ],
    { windowsHide: true },
  );
  let text = "";
  probe.stdout.on("data", (chunk) => (text += chunk));
  assert.equal((await once(probe, "exit"))[0], 0);
  const video = JSON.parse(text);
  assert.ok(
    video.streams.some((stream) => stream.codec_type === "video") &&
      video.streams.some((stream) => stream.codec_type === "audio"),
  );
  const report = {
    passed: true,
    native_ipc: true,
    chunked_transfer: true,
    extensionless_wrong_mime_hls: true,
    estimated_total_visible: true,
    nonzero_progress_visible: true,
    eta_visible_in_popup_and_list: true,
    media_time_and_segments_visible: true,
    pause_resume_snapshot_preserved: true,
    final_size_exact: true,
    duration: Number(video.format.duration),
    sha256: current.sha256,
    time: new Date().toISOString(),
  };
  if (suite) {
    assert.ok(
      video.streams.some(
        (stream) =>
          stream.codec_name === "mov_text" && stream.tags.language === "eng",
      ),
    );
    assert.equal(video.chapters.length, 2);
    assert.ok(current.request.media.normalize_audio);
    assert.equal(current.request.media.subtitle_mode, "embed");
    assert.ok(renewalCount > 0);
    Object.assign(report, {
      native_subtitle_selection: true,
      embedded_subtitle_verified: true,
      chapter_output_verified: true,
      normalization_enabled: true,
      in_flight_native_renewal: true,
      renewal_count: renewalCount,
    });
    clearInterval(healingTimer);
  }
  await writeFile(
    suite
      ? "docs/validation/media-suite-window.json"
      : "docs/validation/media-window.json",
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report));
} finally {
  clearInterval(healingTimer);
  driver.close();
  const shutdown = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
    env,
    windowsHide: true,
    stdio: "ignore",
  });
  await once(shutdown, "exit");
  if (app.exitCode === null) await once(app, "exit");
  host.stdin.end();
  if (host.exitCode === null) await once(host, "exit");
  await new Promise((r) => fixture.close(r));
}
