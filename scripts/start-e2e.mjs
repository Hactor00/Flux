import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdir, writeFile, mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { randomBytes, createHash } from "node:crypto";
const root = await mkdtemp(join(tmpdir(), "flux-ui-test-"));
const output = join(root, "downloads");
const shutdown = join(root, "shutdown");
await mkdir(output);
const bytes = Buffer.alloc(8 * 1024 * 1024);
for (let index = 0; index < bytes.length; index++) bytes[index] = index % 251;
const fixture = createServer((request, response) => {
  const range = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range || "");
  const start = range ? Number(range[1]) : 0;
  const end = range && range[2] ? Number(range[2]) : bytes.length - 1;
  response.writeHead(range ? 206 : 200, {
    ETag: '"ui-fixture-v1"',
    "Content-Length": end - start + 1,
    ...(range
      ? { "Content-Range": `bytes ${start}-${end}/${bytes.length}` }
      : {}),
  });
  let offset = start;
  const timer = setInterval(() => {
    if (offset > end) {
      clearInterval(timer);
      response.end();
      return;
    }
    const next = Math.min(offset + 65536, end + 1);
    response.write(bytes.subarray(offset, next));
    offset = next;
  }, 5);
  response.on("close", () => clearInterval(timer));
});
await new Promise((resolve) => fixture.listen(0, "127.0.0.1", resolve));
const token = randomBytes(32).toString("hex");
const environment = { ...process.env, FLUX_TEST_TOKEN: token };
const cli = spawn(
  resolve("target/debug/flux-cli.exe"),
  [
    "--data",
    join(root, "data"),
    "--output",
    output,
    "harness",
    "--frontend-origin",
    "http://127.0.0.1:1422",
    "--shutdown-file",
    shutdown,
  ],
  {
    env: environment,
    windowsHide: true,
    stdio: ["ignore", "inherit", "inherit"],
  },
);
const vite = spawn(
  process.execPath,
  ["node_modules/vite/bin/vite.js", "--mode", "e2e"],
  {
    env: environment,
    windowsHide: true,
    stdio: ["ignore", "inherit", "inherit"],
  },
);
await mkdir(".tools/e2e", { recursive: true });
await writeFile(
  ".tools/e2e/session.json",
  JSON.stringify(
    {
      url: `http://127.0.0.1:${fixture.address().port}/payload.bin`,
      output,
      root,
      shutdown,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    },
    null,
    2,
  ),
);
console.log(
  "UI test: http://127.0.0.1:1422; fixture metadata: .tools/e2e/session.json",
);
let closing = false;
async function stop() {
  if (closing) return;
  closing = true;
  await writeFile(shutdown, "stop");
  vite.kill();
  fixture.close();
}
process.on("SIGINT", () => void stop());
process.on("SIGTERM", () => void stop());
cli.on("exit", () => {
  if (!closing) void stop();
});
vite.on("exit", () => {
  if (!closing) void stop();
});
const watcher = setInterval(async () => {
  try {
    await readFile(shutdown);
    await stop();
    clearInterval(watcher);
  } catch {}
}, 250);
