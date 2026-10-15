import { preserveNativeRegistration } from "./native-test-state.mjs";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { once } from "node:events";
import { createServer } from "node:net";
import { DesktopDriver } from "./desktop-driver.mjs";
import { makeInput } from "../extension/shared.js";

const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-provider-approval-"));
const data = join(root, "Flux"),
  output = join(root, "output");
await mkdir(data);
await mkdir(output);
const origin = `chrome-extension://${(await readFile("extension/extension-id.txt", "utf8")).trim()}/`;
await writeFile(join(data, "allowed-origins.json"), JSON.stringify([origin]));
const listener = createServer();
await new Promise((resolve) => listener.listen(0, "127.0.0.1", resolve));
const port = listener.address().port;
await new Promise((resolve) => listener.close(resolve));
const env = {
  ...process.env,
  LOCALAPPDATA: root,
  WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
};
const restoreRegistration = preserveNativeRegistration();
const app = spawn(join(binary, "flux-desktop.exe"), ["--background"], {
  env,
  windowsHide: true,
  stdio: ["ignore", "ignore", "pipe"],
});
app.stderr.on("data", (bytes) => process.stderr.write(bytes));
const host = spawn(join(binary, "flux-native-host.exe"), [origin], {
  env,
  windowsHide: true,
  stdio: ["pipe", "pipe", "pipe"],
});
let bytes = Buffer.alloc(0),
  diagnostics = "";
const pending = new Map();
host.stderr.on(
  "data",
  (bytes) => (diagnostics = (diagnostics + bytes).slice(-8192)),
);
host.stdout.on("data", (chunk) => {
  bytes = Buffer.concat([bytes, chunk]);
  while (bytes.length >= 4 && bytes.length >= bytes.readUInt32LE(0) + 4) {
    const size = bytes.readUInt32LE(0),
      reply = JSON.parse(bytes.subarray(4, size + 4));
    bytes = bytes.subarray(size + 4);
    pending.get(reply.request_id)?.(reply);
    pending.delete(reply.request_id);
  }
});
const call = (command, payload) =>
  new Promise((accept, reject) => {
    const request_id = randomUUID();
    const timer = setTimeout(
      () => {
        pending.delete(request_id);
        reject(Error(`${command} timed out: ${diagnostics}`));
      },
      command === "resolve" ? 125000 : 15000,
    );
    pending.set(request_id, (reply) => {
      clearTimeout(timer);
      reply.ok ? accept(reply.result) : reject(Error(reply.error));
    });
    const body = Buffer.from(
        JSON.stringify({ version: 1, request_id, command, payload }),
      ),
      prefix = Buffer.alloc(4);
    prefix.writeUInt32LE(body.length);
    host.stdin.write(Buffer.concat([prefix, body]));
  });
const driver = new DesktopDriver(port);
async function until(predicate, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await predicate();
    if (value) return value;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw Error("Provider approval condition timed out");
}
async function probe(path) {
  const process = spawn(
    join(binary, "ffprobe.exe"),
    ["-v", "error", "-show_streams", "-show_format", "-of", "json", path],
    { windowsHide: true },
  );
  let text = "";
  process.stdout.on("data", (bytes) => (text += bytes));
  assert.equal((await once(process, "exit"))[0], 0);
  return JSON.parse(text);
}
try {
  await until(async () => {
    try {
      await readFile(join(data, "broker.json"));
      return true;
    } catch {
      assert.equal(app.exitCode, null);
      return false;
    }
  }, 60000);
  const provider =
    process.env.FLUX_TEST_YOUTUBE_URL ||
    "https://www.youtube.com/watch?v=aqz-KE-bpKQ";
  const metadata = await call("resolve", { url: provider });
  const format = metadata.formats.find(
    (format) => format.height === Number(process.env.FLUX_TEST_HEIGHT || 720),
  );
  assert.ok(format, "Requested quality is unavailable");
  const results = [];
  for (const container of ["mp4", "mp3"]) {
    const input = makeInput(format.url, {
      filename: `Provider approval.${container}`,
      referer: provider,
      connections: 32,
      media: {
        kind: "file",
        provider: {
          url: provider,
          format_id: format.id,
          height: format.height,
          audio_format_id: null,
        },
        container,
        audio_url: format.audio_url,
        duration_seconds: 8,
        chapters: [],
        subtitles: [],
      },
    });
    const task = await call("propose", input);
    assert.equal(task.request.media.container, container);
    const window = await driver.connect("download");
    await until(() =>
      window.evaluate("!!document.querySelector('form button[type=submit]')"),
    );
    const initial = await window.evaluate(
      "({enabled:!document.querySelector('form button[type=submit]').disabled,inspections:[...document.querySelectorAll('button')].filter(button=>/Inspect streams|Akışları incele/.test(button.textContent)).length})",
    );
    assert.equal(
      initial.enabled,
      true,
      "Captured media must be ready without a second inspection",
    );
    assert.equal(
      initial.inspections,
      1,
      "Provider approval must have a single optional inspection action",
    );
    if (container === "mp3") {
      await window.evaluate(
        "[...document.querySelectorAll('button')].find(button=>/Inspect streams|Akışları incele/.test(button.textContent)).click(); true",
      );
      await until(
        () =>
          window.evaluate(
            "!![...document.querySelectorAll('button')].find(button=>/Inspect streams|Akışları incele/.test(button.textContent)&&!button.disabled)",
          ),
        120000,
      );
    }
    const chosenOutput = await window.evaluate(
      "({name:document.querySelector('input[name=filename]').value,choices:[...document.querySelectorAll('[role=combobox]')].map(item=>item.textContent)})",
    );
    assert.equal(chosenOutput.name, `Provider approval.${container}`);
    assert.ok(
      chosenOutput.choices.includes(container.toUpperCase()),
      JSON.stringify(chosenOutput),
    );
    await window.evaluate(
      "document.querySelector('form').requestSubmit(); true",
    );
    await until(async () => {
      const current = await call("task", { id: task.id });
      if (current.status !== "awaiting_approval") return true;
      const failure = await window
        .evaluate("document.querySelector('[role=alert]')?.textContent||''")
        .catch(() => "");
      assert.equal(failure, "");
      return current.status !== "awaiting_approval";
    });
    let current;
    await until(async () => {
      current = await call("task", { id: task.id });
      assert.notEqual(current.status, "failed", current.error);
      return current.status === "complete";
    }, 180000);
    const info = await probe(current.destination);
    const video = info.streams.find((stream) => stream.codec_type === "video");
    const audio = info.streams.find((stream) => stream.codec_type === "audio");
    assert.ok(audio);
    if (container === "mp4") assert.equal(video.height, format.height);
    else {
      assert.equal(video, undefined);
      assert.equal(audio.codec_name, "mp3");
    }
    assert.ok(Number(info.format.duration) >= 7);
    assert.equal(current.request.media.provider.height, format.height);
    const hash = createHash("sha256")
      .update(await readFile(current.destination))
      .digest("hex");
    assert.equal(current.sha256, hash);
    results.push({
      container,
      immediate_start: true,
      optional_inspection: container === "mp3",
      height: video?.height || null,
      audio_codec: audio.codec_name,
      duration: Number(info.format.duration),
      sha256: hash,
    });
  }
  const report = {
    version: JSON.parse(await readFile("package.json", "utf8")).version,
    passed: true,
    provider: "YouTube",
    production_native_window: true,
    captured_quality_preserved: true,
    single_optional_inspection: true,
    refreshed_source_approved: true,
    results,
    time: new Date().toISOString(),
  };
  await writeFile(
    "docs/validation/provider-approval.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
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
  restoreRegistration();
}
