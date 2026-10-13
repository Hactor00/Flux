import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { once } from "node:events";
const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-benchmark-"));
const bytes = Buffer.alloc(64 * 1024 * 1024);
for (let index = 0; index < bytes.length; index++) bytes[index] = index % 251;
const sha256 = createHash("sha256").update(bytes).digest("hex");
const server = createServer((request, response) => {
  const match = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range || "");
  const start = match ? Number(match[1]) : 0;
  const end = match?.[2] ? Number(match[2]) : bytes.length - 1;
  response.writeHead(match ? 206 : 200, {
    ETag: '"benchmark-v1"',
    "Content-Length": end - start + 1,
    ...(match
      ? { "Content-Range": `bytes ${start}-${end}/${bytes.length}` }
      : {}),
  });
  if (end === start) {
    response.end(bytes.subarray(start, end + 1));
    return;
  }
  let offset = start;
  const interval = setInterval(() => {
    const next = Math.min(offset + 65536, end + 1);
    response.write(bytes.subarray(offset, next));
    offset = next;
    if (offset > end) {
      clearInterval(interval);
      response.end();
    }
  }, 5);
  response.on("close", () => clearInterval(interval));
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const results = [];
try {
  for (const connections of [1, 8, 32]) {
    const data = join(root, "data-" + connections);
    const output = join(root, "output-" + connections);
    await mkdir(output);
    const start = performance.now();
    const child = spawn(
      join(binary, "flux-cli.exe"),
      [
        "--data",
        data,
        "--output",
        output,
        "download",
        `http://127.0.0.1:${server.address().port}/file`,
        "benchmark.bin",
        "--connections",
        String(connections),
        "--sha256",
        sha256,
      ],
      { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] },
    );
    let errors = "";
    child.stderr.on("data", (chunk) => (errors += chunk));
    child.stdout.resume();
    const [code] = await once(child, "exit");
    assert.equal(code, 0, errors);
    const seconds = (performance.now() - start) / 1000;
    assert.equal(
      createHash("sha256")
        .update(await readFile(join(output, "benchmark.bin")))
        .digest("hex"),
      sha256,
    );
    results.push({
      connections,
      seconds,
      mebibytes_per_second: 64 / seconds,
      sha256,
    });
  }
  const result = {
    time: new Date().toISOString(),
    bytes: bytes.length,
    scenario:
      "Controlled local HTTP server, 64 KiB per connection every 5 ms; includes startup, merging and SHA-256",
    comparison: "Flux 1/8/32 connections; IDM was not measured",
    results,
    speedup_8: results[0].seconds / results[1].seconds,
    speedup_32: results[0].seconds / results[2].seconds,
    root,
  };
  await mkdir("docs/validation", { recursive: true });
  await writeFile(
    "docs/validation/benchmark.json",
    JSON.stringify(result, null, 2) + "\n",
  );
  console.log(JSON.stringify(result));
} finally {
  server.close();
}
