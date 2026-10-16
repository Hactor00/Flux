import { preserveNativeRegistration } from "./native-test-state.mjs";
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, mkdir, writeFile, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { once } from "node:events";
import { NativeBridge } from "../extension/bridge.js";
const binary = resolve(process.argv[2] || "target/debug");
const desktopMode = process.argv.includes("--desktop");
if (desktopMode && process.platform === "win32") {
  const running = execFileSync(
    join(
      process.env.SystemRoot || "C:/Windows",
      "System32",
      "WindowsPowerShell",
      "v1.0",
      "powershell.exe",
    ),
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      "if(Get-Process -Name flux-desktop -ErrorAction SilentlyContinue | Where-Object {-not $_.HasExited}){'running'}",
    ],
    { encoding: "utf8", windowsHide: true },
  ).trim();
  assert.equal(
    running,
    "",
    "Close the running Flux desktop before the isolated cold-start test; its application-wide single-instance lock would block the test profile.",
  );
}
const root = await mkdtemp(join(tmpdir(), "flux-native-test-"));
const data = join(root, "Flux");
const output = join(root, "output");
const stop = join(root, "shutdown");
await mkdir(data);
await mkdir(output);
const id = (await readFile("extension/extension-id.txt", "utf8")).trim();
const origin = "chrome-extension://" + id + "/";
await writeFile(join(data, "allowed-origins.json"), JSON.stringify([origin]));
const environment = {
  ...process.env,
  LOCALAPPDATA: root,
  WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
};
const payload = Buffer.alloc(256 * 1024, 73);
let authenticated = 0;
const fixture = createServer((request, response) => {
  if (request.headers.cookie !== "session=native-fixture") {
    response.writeHead(401);
    response.end();
    return;
  }
  authenticated++;
  const range = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range || "");
  const start = range ? Number(range[1]) : 0;
  const end = range && range[2] ? Number(range[2]) : payload.length - 1;
  response.writeHead(range ? 206 : 200, {
    "Content-Length": end - start + 1,
    ETag: '"native-v1"',
    ...(range
      ? { "Content-Range": "bytes " + start + "-" + end + "/" + payload.length }
      : {}),
  });
  response.end(payload.subarray(start, end + 1));
});
await new Promise((resolve) => fixture.listen(0, "127.0.0.1", resolve));
const url = "http://127.0.0.1:" + fixture.address().port + "/file";
const restoreRegistration = desktopMode
  ? preserveNativeRegistration()
  : () => {};
const host = spawn(join(binary, "flux-native-host.exe"), [origin], {
  env: environment,
  windowsHide: true,
  stdio: ["pipe", "pipe", "pipe"],
});
let hostErrors = "";
host.stderr.on("data", (bytes) => (hostErrors += bytes));
const messages = [];
const disconnects = [];
let buffer = Buffer.alloc(0);
host.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (buffer.length >= 4) {
    const size = buffer.readUInt32LE(0);
    assert.ok(size > 0 && size <= 256 * 1024);
    if (buffer.length < size + 4) break;
    const message = JSON.parse(buffer.subarray(4, size + 4));
    buffer = buffer.subarray(size + 4);
    for (const callback of messages) callback(message);
  }
});
host.on("exit", () => {
  for (const callback of disconnects) callback();
});
globalThis.chrome = {
  runtime: {
    connectNative() {
      return {
        onMessage: {
          addListener(fn) {
            messages.push(fn);
          },
        },
        onDisconnect: {
          addListener(fn) {
            disconnects.push(fn);
          },
        },
        postMessage(value) {
          const bytes = Buffer.from(JSON.stringify(value));
          const header = Buffer.alloc(4);
          header.writeUInt32LE(bytes.length);
          host.stdin.write(Buffer.concat([header, bytes]));
        },
      };
    },
  },
};
const bridge = new NativeBridge();
let cli;
try {
  await assert.rejects(
    bridge.request("browser_connect", { extension_version: "2.4.2" }),
    /not running|broker unavailable/i,
  );
  await assert.rejects(
    bridge.request("refresh_poll"),
    /not running|broker unavailable/i,
  );
  await assert.rejects(
    bridge.request("list_page", { offset: 0, limit: 1, probe: true }),
    /not running|broker unavailable/i,
  );
  await assert.rejects(readFile(join(data, "broker.json")), { code: "ENOENT" });
  const protected_credentials = await bridge.request("protect", {
    credentials: [
      {
        origin: new URL(url).origin,
        cookie: "session=native-fixture",
        authorization: null,
        user_agent: null,
      },
    ],
  });
  assert.ok(
    protected_credentials.length > 32 &&
      !protected_credentials.includes("native-fixture"),
  );
  if (desktopMode) {
    const settingsFile = join(root, "settings.json");
    await writeFile(
      settingsFile,
      JSON.stringify({
        concurrent_tasks: 3,
        connections: 8,
        speed_limit: 0,
        queue_enabled: true,
        clipboard_monitor: false,
        minimize_to_tray: true,
        notifications: false,
        locale: "en-US",
        download_directory: output,
      }),
    );
    const setup = spawn(
      join(binary, "flux-cli.exe"),
      ["--data", data, "--output", output, "settings", "--input", settingsFile],
      { env: environment, windowsHide: true, stdio: "ignore" },
    );
    const [code] = await once(setup, "exit");
    assert.equal(code, 0);
    await bridge.request("list_page", { offset: 0, limit: 1 });
  } else {
    cli = spawn(
      join(binary, "flux-cli.exe"),
      ["--data", data, "--output", output, "serve", "--shutdown-file", stop],
      {
        env: environment,
        windowsHide: true,
        stdio: ["ignore", "ignore", "pipe"],
      },
    );
    let cliErrors = "";
    cli.stderr.on("data", (bytes) => (cliErrors += bytes));
    const readyDeadline = Date.now() + 15000;
    while (true) {
      try {
        JSON.parse(await readFile(join(data, "broker.json")));
        break;
      } catch {
        assert.ok(
          Date.now() < readyDeadline,
          "Broker startup timed out: " + cliErrors,
        );
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
    }
  }
  const concurrent = await Promise.all(
    Array.from({ length: 20 }, (_, index) =>
      bridge.request("list_page", { offset: index, limit: 1 }),
    ),
  );
  assert.ok(concurrent.every((value) => value.total === 0));
  const hash = createHash("sha256").update(payload).digest("hex");
  for (let index = 0; index < 8; index++) {
    const request_id = randomUUID();
    const input = {
      url,
      filename: "native-file.bin",
      connections: 8,
      auto_rename: true,
      protected_credentials,
      expected_sha256: hash,
    };
    const first = await bridge.request("add", input, request_id);
    const replay = await bridge.request("add", input, request_id);
    assert.equal(first.id, replay.id);
    assert.ok(!JSON.stringify(first).includes("native-fixture"));
  }
  const hello = await bridge.request("browser_connect", {
    extension_version: JSON.parse(await readFile("package.json", "utf8"))
      .version,
  });
  assert.equal(
    hello.desktop_version,
    JSON.parse(await readFile("package.json", "utf8")).version,
  );
  const deadline = Date.now() + 30000;
  let tasks;
  while (true) {
    tasks = (await bridge.request("list_page", { offset: 0, limit: 50 })).tasks;
    if (tasks.every((task) => task.status === "complete")) break;
    assert.ok(
      Date.now() < deadline,
      JSON.stringify(
        tasks.map((task) => ({ status: task.status, error: task.error })),
      ),
    );
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.equal(tasks.length, 8);
  assert.equal(
    new Set(tasks.map((task) => task.destination.toLowerCase())).size,
    8,
  );
  for (const task of tasks) {
    assert.equal(task.sha256, hash);
    assert.deepEqual(await readFile(task.destination), payload);
    assert.ok(task.source_validated);
  }
  assert.ok(authenticated >= 16);
  let shutdownResume = false;
  if (desktopMode) {
    const input = {
      url,
      filename: "shutdown-resume.bin",
      connections: 8,
      speed_limit: 65536,
      protected_credentials,
      expected_sha256: hash,
    };
    const started = await bridge.request("add", input);
    const deadline = Date.now() + 10000;
    let active;
    do {
      active = await bridge.request("task", { id: started.id });
      assert.ok(Date.now() < deadline, "Active transfer did not advance");
      await new Promise((resolve) => setTimeout(resolve, 50));
    } while (!active.source_validated);
    assert.equal(active.status, "running");
    const stopProcess = spawn(
      join(binary, "flux-desktop.exe"),
      ["--shutdown"],
      { env: environment, windowsHide: true, stdio: "ignore" },
    );
    const [code] = await once(stopProcess, "exit");
    assert.equal(code, 0);
    const restored = await bridge.request("task", { id: started.id });
    assert.equal(restored.status, "paused");
    assert.ok(restored.completed > 0 && restored.completed < payload.length);
    await bridge.request("resume", { id: started.id });
    let finished;
    const finishDeadline = Date.now() + 10000;
    do {
      finished = await bridge.request("task", { id: started.id });
      assert.ok(
        Date.now() < finishDeadline,
        finished.error || "Resume timed out",
      );
      await new Promise((resolve) => setTimeout(resolve, 50));
    } while (finished.status !== "complete");
    assert.equal(finished.sha256, hash);
    assert.deepEqual(await readFile(finished.destination), payload);
    shutdownResume = true;
  }
  if (desktopMode) {
    const shutdown = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
      env: environment,
      windowsHide: true,
      stdio: "ignore",
    });
    await once(shutdown, "exit");
    const deadline = Date.now() + 10000;
    while (true) {
      try {
        await readFile(join(data, "broker.json"));
      } catch (error) {
        if (error.code === "ENOENT") break;
        throw error;
      }
      assert.ok(Date.now() < deadline, "Desktop shutdown timed out");
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  } else {
    await writeFile(stop, "stop");
    await once(cli, "exit");
    cli = undefined;
  }
  for (const name of ["tasks.sqlite", "tasks.sqlite-wal"]) {
    try {
      const bytes = await readFile(join(data, name));
      assert.ok(!bytes.includes(Buffer.from("native-fixture")));
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  const result = {
    passed: true,
    transport: "Real Native Messaging stdio + authenticated TCP broker",
    desktop_cold_start: desktopMode,
    binary_directory: binary,
    concurrent_requests: 20,
    downloads: desktopMode ? 9 : 8,
    active_shutdown_resume: shutdownResume,
    authenticated_requests: authenticated,
    sha256: hash,
    output,
    root,
    time: new Date().toISOString(),
  };
  await mkdir("docs/validation", { recursive: true });
  const publicResult = {
    ...result,
    version: JSON.parse(await readFile("package.json", "utf8")).version,
    local_paths_and_credentials_removed: true,
    passive_checks_do_not_start_desktop: true,
  };
  delete publicResult.binary_directory;
  delete publicResult.output;
  delete publicResult.root;
  await writeFile(
    desktopMode
      ? "docs/validation/desktop-integration.json"
      : "docs/validation/native-integration.json",
    JSON.stringify(publicResult, null, 2) + "\n",
  );
  console.log(JSON.stringify(result));
  assert.equal(hostErrors, "");
} finally {
  host.stdin.end();
  if (desktopMode) {
    const shutdown = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
      env: environment,
      windowsHide: true,
      stdio: "ignore",
    });
    await once(shutdown, "exit");
  }
  if (cli) {
    await writeFile(stop, "stop");
  }
  fixture.close();
  restoreRegistration();
}
