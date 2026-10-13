import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { createReadStream } from "node:fs";
import { createHash, randomUUID } from "node:crypto";
import { once } from "node:events";
import { resolve, join } from "node:path";
const binary = resolve(process.argv[2]),
  id = process.argv[3];
assert.match(id, /^[0-9a-f-]{36}$/i);
const origin = `chrome-extension://${(await readFile("extension/extension-id.txt", "utf8")).trim()}/`;
const host = spawn(join(binary, "flux-native-host.exe"), [origin], {
  windowsHide: true,
  stdio: ["pipe", "pipe", "pipe"],
});
let buffer = Buffer.alloc(0);
const pending = new Map();
host.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (buffer.length >= 4) {
    const length = buffer.readUInt32LE(0);
    if (buffer.length < length + 4) break;
    const value = JSON.parse(buffer.subarray(4, length + 4));
    buffer = buffer.subarray(length + 4);
    pending.get(value.request_id)?.(value);
    pending.delete(value.request_id);
  }
});
const call = (command, payload) =>
  new Promise((resolve, reject) => {
    const request_id = randomUUID(),
      timer = setTimeout(
        () => reject(Error("Native response timed out")),
        15000,
      );
    pending.set(request_id, (value) => {
      clearTimeout(timer);
      value.ok ? resolve(value.result) : reject(Error(value.error));
    });
    const data = Buffer.from(
        JSON.stringify({ version: 1, request_id, command, payload }),
      ),
      prefix = Buffer.alloc(4);
    prefix.writeUInt32LE(data.length);
    host.stdin.write(Buffer.concat([prefix, data]));
  });
try {
  const before = await call("task", { id });
  assert.ok(["failed", "paused"].includes(before.status));
  assert.ok(before.request.media);
  assert.match(
    before.request.referer || before.request.media.provider?.url || "",
    /^https:\/\/(www\.)?youtube\.com\/watch\?/,
  );
  await call("resume", { id });
  const deadline = Date.now() + 300000;
  let current;
  do {
    current = await call("task", { id });
    if (current.status === "failed")
      throw Error((current.error || "").split(" for url")[0]);
    if (current.status === "complete") break;
    assert.ok(Date.now() < deadline, "Existing media recovery timed out");
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
  const metadata = JSON.parse(output);
  const video = metadata.streams.find((s) => s.codec_type === "video"),
    audio = metadata.streams.find((s) => s.codec_type === "audio");
  assert.equal(video.height, 1080);
  assert.ok(audio);
  assert.ok(Number(metadata.format.duration) > 1290);
  const digest = createHash("sha256");
  for await (const bytes of createReadStream(current.destination))
    digest.update(bytes);
  assert.equal(digest.digest("hex"), current.sha256);
  const report = {
    passed: true,
    legacy_task_recovered: true,
    filename: current.request.filename,
    height: video.height,
    video_codec: video.codec_name,
    audio_codec: audio.codec_name,
    duration: Number(metadata.format.duration),
    bytes: current.completed,
    sha256: current.sha256,
    time: new Date().toISOString(),
  };
  await writeFile(
    "docs/validation/recovered-task.json",
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report));
} finally {
  host.stdin.end();
  if (host.exitCode === null) await once(host, "exit");
}
