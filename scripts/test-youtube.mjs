import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import {
  mkdtemp,
  mkdir,
  writeFile,
  readFile,
  unlink,
  readdir,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { once } from "node:events";
import { NativeBridge } from "../extension/bridge.js";
const binary = resolve(process.argv[2] || "target/release"),
  root = await mkdtemp(join(tmpdir(), "flux-youtube-test-")),
  data = join(root, "Flux"),
  out = join(root, "out"),
  stop = join(root, "stop");
await mkdir(data);
await mkdir(out);
const origin =
  "chrome-extension://" +
  (await readFile("extension/extension-id.txt", "utf8")).trim() +
  "/";
await writeFile(join(data, "allowed-origins.json"), JSON.stringify([origin]));
const env = { ...process.env, LOCALAPPDATA: root };
const startCli = () =>
  spawn(
    join(binary, "flux-cli.exe"),
    ["--data", data, "--output", out, "serve", "--shutdown-file", stop],
    { env, windowsHide: true, stdio: ["ignore", "ignore", "pipe"] },
  );
let cli = startCli();
const host = spawn(join(binary, "flux-native-host.exe"), [origin], {
  env,
  windowsHide: true,
  stdio: ["pipe", "pipe", "pipe"],
});
let bytes = Buffer.alloc(0),
  listeners = [],
  disconnects = [];
host.stdout.on("data", (chunk) => {
  bytes = Buffer.concat([bytes, chunk]);
  while (bytes.length >= 4) {
    const size = bytes.readUInt32LE(0);
    if (bytes.length < size + 4) return;
    const value = JSON.parse(bytes.subarray(4, 4 + size));
    bytes = bytes.subarray(4 + size);
    for (const listener of listeners) listener(value);
  }
});
host.on("exit", () => disconnects.forEach((fn) => fn()));
globalThis.chrome = {
  runtime: {
    connectNative: () => ({
      onMessage: { addListener: (fn) => listeners.push(fn) },
      onDisconnect: { addListener: (fn) => disconnects.push(fn) },
      postMessage: (value) => {
        const buffer = Buffer.from(JSON.stringify(value)),
          header = Buffer.alloc(4);
        header.writeUInt32LE(buffer.length);
        host.stdin.write(Buffer.concat([header, buffer]));
      },
    }),
  },
};
const bridge = new NativeBridge();
try {
  const deadline = Date.now() + 15000;
  while (true) {
    try {
      JSON.parse(await readFile(join(data, "broker.json"), "utf8"));
      break;
    } catch {
      if (Date.now() > deadline) throw Error("Broker unavailable");
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  const providerUrl =
    process.env.FLUX_TEST_YOUTUBE_URL ||
    "https://www.youtube.com/watch?v=aqz-KE-bpKQ";
  const metadata = await bridge.request("resolve", {
    url: providerUrl,
  });
  assert.ok(metadata.formats.length > 0);
  const height = Number(process.env.FLUX_TEST_HEIGHT || 144);
  assert.equal(
    new Set(metadata.formats.map((item) => item.height)).size,
    metadata.formats.length,
  );
  const format = metadata.formats
    .filter((item) => item.height && item.height === height)
    .sort((a, b) => a.height - b.height)[0];
  assert.ok(format);
  const task = await bridge.request(
    "add",
    {
      // An unusable signed URL must be replaced using the persisted provider.
      url: "https://expired.invalid/source?expire=1",
      filename: "Big-Buck-Bunny-test." + format.container,
      connections: 32,
      media: {
        provider: {
          url: providerUrl,
          format_id: format.id,
          height: format.height,
        },
        container: format.container,
        audio_url: format.audio_url,
        duration_seconds: 8,
      },
    },
    randomUUID(),
  );
  let cachedParts = 0;
  if (height === 1080) {
    const cache = join(data, "parts", task.id, "media-cache");
    const cacheDeadline = Date.now() + 60000;
    while (!cachedParts) {
      const directories = await readdir(cache, { withFileTypes: true }).catch(
        () => [],
      );
      for (const directory of directories.filter((item) => item.isDirectory()))
        cachedParts += (await readdir(join(cache, directory.name))).filter(
          (name) => name.endsWith(".sha256"),
        ).length;
      assert.ok(
        Date.now() < cacheDeadline,
        "No verified media range was persisted",
      );
      if (!cachedParts)
        await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  await bridge.request("pause", { id: task.id });
  const paused = await bridge.request("task", { id: task.id });
  assert.equal(paused.status, "paused");
  await writeFile(stop, "stop");
  if (cli.exitCode === null) await once(cli, "exit");
  await unlink(stop);
  await unlink(join(data, "broker.json")).catch(() => {});
  cli = startCli();
  const reopenDeadline = Date.now() + 15000;
  while (true) {
    try {
      JSON.parse(await readFile(join(data, "broker.json"), "utf8"));
      break;
    } catch {
      assert.ok(Date.now() < reopenDeadline);
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  await bridge.request("resume", { id: task.id });
  const end = Date.now() + 300000;
  let current;
  do {
    current = await bridge.request("task", { id: task.id });
    if (current.status === "failed") throw Error(current.error);
    if (current.status === "complete") break;
    if (Date.now() > end) throw Error("Media download timed out");
    await new Promise((r) => setTimeout(r, 500));
  } while (true);
  const probe = spawn(
    join(binary, "ffprobe.exe"),
    [
      "-v",
      "error",
      "-show_streams",
      "-show_format",
      "-of",
      "json",
      current.destination,
    ],
    { windowsHide: true },
  );
  let output = "";
  probe.stdout.on("data", (bytes) => (output += bytes));
  const [code] = await once(probe, "exit");
  assert.equal(code, 0);
  const info = JSON.parse(output);
  assert.ok(info.streams.some((stream) => stream.codec_type === "video"));
  assert.ok(info.streams.some((stream) => stream.codec_type === "audio"));
  assert.equal(
    info.streams.find((stream) => stream.codec_type === "video").height,
    height,
  );
  assert.ok(Number(info.format.duration) >= 7);
  const audioTask = await bridge.request(
    "add",
    {
      url: format.url,
      filename: "Big-Buck-Bunny-soundtrack.mp3",
      connections: 4,
      media: {
        provider: {
          url: providerUrl,
          format_id: format.id,
          height: format.height,
        },
        container: "mp3",
        audio_url: format.audio_url,
        duration_seconds: 8,
      },
    },
    randomUUID(),
  );
  const audioEnd = Date.now() + 120000;
  let audioCurrent;
  do {
    audioCurrent = await bridge.request("task", { id: audioTask.id });
    if (audioCurrent.status === "failed") throw Error(audioCurrent.error);
    if (audioCurrent.status === "complete") break;
    assert.ok(Date.now() < audioEnd, "Soundtrack download timed out");
    await new Promise((resolve) => setTimeout(resolve, 500));
  } while (true);
  const audioProbe = spawn(
    join(binary, "ffprobe.exe"),
    [
      "-v",
      "error",
      "-show_streams",
      "-show_format",
      "-of",
      "json",
      audioCurrent.destination,
    ],
    { windowsHide: true },
  );
  let audioOutput = "";
  audioProbe.stdout.on("data", (bytes) => (audioOutput += bytes));
  const [audioCode] = await once(audioProbe, "exit");
  assert.equal(audioCode, 0);
  const audioInfo = JSON.parse(audioOutput);
  assert.equal(audioInfo.streams.length, 1);
  assert.equal(audioInfo.streams[0].codec_type, "audio");
  assert.equal(audioInfo.streams[0].codec_name, "mp3");
  assert.ok(Number(audioInfo.format.duration) >= 7);
  const report = {
    passed: true,
    provider: "YouTube",
    title: metadata.title,
    formats: metadata.formats.length,
    height: format.height,
    connections: 32,
    stale_source_renewed_after_restart: true,
    verified_cached_parts_before_restart: cachedParts,
    signed_source_changed_on_resume: paused.request.url !== current.request.url,
    selected_format: format.id,
    duration: Number(info.format.duration),
    video_codec: info.streams.find((s) => s.codec_type === "video").codec_name,
    audio_codec: info.streams.find((s) => s.codec_type === "audio").codec_name,
    sha256: current.sha256,
    output: current.destination,
    mp3: {
      codec: "mp3",
      streams: 1,
      duration: Number(audioInfo.format.duration),
      sha256: audioCurrent.sha256,
      output: audioCurrent.destination,
    },
    time: new Date().toISOString(),
  };
  await writeFile(
    height === 1080
      ? "docs/validation/youtube-1080p.json"
      : "docs/validation/youtube.json",
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report));
} finally {
  await writeFile(stop, "stop");
  host.stdin.end();
  if (cli.exitCode === null) await once(cli, "exit");
  if (host.exitCode === null) await once(host, "exit");
}
