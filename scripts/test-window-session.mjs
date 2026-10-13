import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, mkdir, writeFile, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { once } from "node:events";
import { createHash, randomUUID } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import assert from "node:assert/strict";
import { DesktopDriver } from "./desktop-driver.mjs";
const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-window-test-"));
const data = join(root, "Flux"),
  output = join(root, "output"),
  stop = join(root, "stop");
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
    connections: 0,
    notifications: false,
    download_directory: output,
    minimize_to_tray: false,
  }),
);
db.close();
const payload = Buffer.alloc(96 * 1024 * 1024, 71);
const digest = createHash("sha256").update(payload).digest("hex");
const fixture = createServer((request, response) => {
  const range = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range || "");
  let offset = range ? Number(range[1]) : 0;
  const end = range?.[2] ? Number(range[2]) : payload.length - 1;
  response.writeHead(range ? 206 : 200, {
    "Content-Length": end - offset + 1,
    ETag: '"window-fixture"',
    "Content-Type": "application/octet-stream",
    ...(range
      ? { "Content-Range": `bytes ${offset}-${end}/${payload.length}` }
      : {}),
  });
  const tick = () => {
    if (response.destroyed) return;
    const next = Math.min(offset + 65536, end + 1);
    response.write(payload.subarray(offset, next));
    offset = next;
    if (offset > end) response.end();
    else setTimeout(tick, 120);
  };
  tick();
});
fixture.listen(0, "127.0.0.1");
await once(fixture, "listening");
const automatic = process.argv.includes("--automatic");
let driver;
const env = { ...process.env, LOCALAPPDATA: root };
if (automatic) {
  const reservation = createServer();
  reservation.listen(0, "127.0.0.1");
  await once(reservation, "listening");
  const port = reservation.address().port;
  await new Promise((r) => reservation.close(r));
  env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`;
  driver = new DesktopDriver(port);
}
const app = spawn(join(binary, "flux-desktop.exe"), [], {
  env,
  windowsHide: true,
  stdio: "ignore",
});
const host = spawn(join(binary, "flux-native-host.exe"), [origin], {
  env,
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
const call = (command, input = {}) =>
  new Promise((accept, reject) => {
    const request_id = randomUUID();
    const timeout = setTimeout(() => {
      pending.delete(request_id);
      reject(Error("Native response timed out"));
    }, 15000);
    pending.set(request_id, (value) => {
      clearTimeout(timeout);
      value.ok ? accept(value.result) : reject(Error(value.error));
    });
    const body = Buffer.from(
        JSON.stringify({ version: 1, request_id, command, payload: input }),
      ),
      prefix = Buffer.alloc(4);
    prefix.writeUInt32LE(body.length);
    host.stdin.write(Buffer.concat([prefix, body]));
  });
try {
  const deadline = Date.now() + 20000;
  while (true) {
    try {
      JSON.parse(await readFile(join(data, "broker.json"), "utf8"));
      break;
    } catch {
      if (app.exitCode !== null)
        throw Error("Another desktop instance is already running");
      if (Date.now() > deadline) throw Error("Desktop unavailable");
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  const task = await call("propose", {
    url: `http://127.0.0.1:${fixture.address().port}/window-test.bin`,
    filename: "Pencere testi.bin",
    connections: 32,
    expected_sha256: digest,
  });
  await writeFile(
    ".tools/window-session.json",
    JSON.stringify({
      root,
      stop,
      pid: app.pid,
      id: task.id,
      expected_sha256: digest,
      output,
    }),
  );
  console.log("Window test session ready.");
  let windowChecks;
  if (automatic) {
    const main = await driver.connect("main"),
      download = await driver.connect("download");
    const uiDeadline = Date.now() + 10000;
    while (
      !(await download.evaluate(
        "!!document.querySelector('input[name=filename]')",
      ))
    ) {
      if (Date.now() > uiDeadline) throw Error("Download form unavailable");
      await new Promise((r) => setTimeout(r, 100));
    }
    assert.equal(
      await download.evaluate(
        "document.querySelector('[aria-label=\"Bağlantı sayısı\"]').textContent",
      ),
      "32",
    );
    assert.equal(
      await download.evaluate(
        "document.querySelector('input[name=filename]').value",
      ),
      "Pencere testi.bin",
    );
    await download.evaluate(
      "document.querySelector('form').requestSubmit(); true",
    );
    let details = await driver.connect("details");
    const detailsInstances = new Set([details.id]);
    await driver.absent("download");
    let startedTask = await details.evaluate(
      "window.__TAURI_INTERNALS__.invoke('get_task'," +
        JSON.stringify({ id: task.id }) +
        ")",
    );
    const timestampDeadline = Date.now() + 10000;
    while (!startedTask.started_at) {
      assert.ok(Date.now() < timestampDeadline, "Task did not start");
      await new Promise((r) => setTimeout(r, 100));
      startedTask = await call("task", { id: task.id });
    }
    assert.ok(startedTask.created_at && startedTask.started_at);
    while (
      !(await details.evaluate(
        "[...document.querySelectorAll('dt')].some(el => el.textContent === 'İndirme başlangıcı' && el.nextElementSibling.textContent !== '—')",
      ))
    ) {
      assert.ok(
        Date.now() < timestampDeadline,
        "Start timestamp did not appear in the native details window",
      );
      await new Promise((r) => setTimeout(r, 100));
    }
    await details.evaluate(
      "void window.__TAURI_INTERNALS__.invoke('close_surface'); true",
    );
    await driver.absent("details");
    for (let index = 0; index < 3; index++) {
      await main.evaluate(
        "window.__TAURI_INTERNALS__.invoke('open_surface'," +
          JSON.stringify({ view: "details", id: task.id }) +
          ")",
      );
      details = await driver.connect("details");
      detailsInstances.add(details.id);
      assert.equal(
        (
          await details.evaluate(
            "window.__TAURI_INTERNALS__.invoke('get_task'," +
              JSON.stringify({ id: task.id }) +
              ")",
          )
        ).id,
        task.id,
      );
      await details.evaluate(
        "void window.__TAURI_INTERNALS__.invoke('close_surface'); true",
      );
      await driver.absent("details");
    }
    await main.evaluate(
      "window.__TAURI_INTERNALS__.invoke('open_surface',{view:'download'})",
    );
    const fresh = await driver.connect("download");
    let autoValue;
    for (let index = 0; index < 40; index++) {
      autoValue = await fresh.evaluate(
        "document.querySelector('[aria-label=\"Bağlantı sayısı\"]')?.textContent",
      );
      if (autoValue === "Otomatik") break;
      await new Promise((r) => setTimeout(r, 100));
    }
    assert.equal(autoValue, "Otomatik");
    await fresh.evaluate(
      "void window.__TAURI_INTERNALS__.invoke('close_surface'); true",
    );
    await driver.absent("download");
    await main.evaluate(
      "window.__TAURI_INTERNALS__.invoke('open_surface',{view:'settings'})",
    );
    const settings = await driver.connect("settings");
    assert.ok(
      (await settings.evaluate("document.body.textContent")).includes(
        "Ayarlar",
      ),
    );
    await settings.evaluate(
      "void window.__TAURI_INTERNALS__.invoke('close_surface'); true",
    );
    await driver.absent("settings");
    assert.equal(detailsInstances.size, 4);
    windowChecks = {
      native_ipc: true,
      start_with_32_connections: true,
      details_open_close_cycles: 4,
      distinct_details_windows: detailsInstances.size,
      window_destruction_verified: true,
      automatic_default_verified: true,
      settings_window_verified: true,
      start_timestamp_verified: true,
    };
    console.log(
      "Native window creation, repeated close/reopen and automatic defaults passed.",
    );
  }
  let validated = false;
  while (true) {
    try {
      await readFile(stop);
      break;
    } catch {}
    const current = await call("task", { id: task.id });
    if (current.status === "failed") throw Error(current.error);
    if (current.status === "complete" && !validated) {
      if (current.sha256 !== digest) throw Error("Incorrect bytes");
      validated = true;
      assert.ok(
        current.finished_at >= current.started_at &&
          current.started_at >= current.created_at,
      );
      if (automatic) {
        const main = await driver.connect("main");
        const timestampDeadline = Date.now() + 10000;
        while (
          !(await main.evaluate(
            "!!document.querySelector('.desktop-date time')",
          ))
        ) {
          assert.ok(
            Date.now() < timestampDeadline,
            "History timestamp did not appear",
          );
          await new Promise((r) => setTimeout(r, 100));
        }
        assert.equal(
          Date.parse(
            await main.evaluate(
              "document.querySelector('.desktop-date time').dateTime",
            ),
          ) / 1000,
          current.started_at,
        );
        windowChecks.history_timestamp_verified = true;
        windowChecks.completion_timestamp_persisted = true;
      }
      console.log("Window fixture completed with the expected SHA-256.");
      if (automatic) break;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  await writeFile(
    "docs/validation/windows.json",
    JSON.stringify(
      {
        passed: validated,
        ...windowChecks,
        task: task.id,
        sha256: digest,
        time: new Date().toISOString(),
      },
      null,
      2,
    ),
  );
} finally {
  driver?.close();
  if (app.exitCode === null) {
    const shutdown = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
      env,
      windowsHide: true,
      stdio: "ignore",
    });
    if (shutdown.exitCode === null) await once(shutdown, "exit");
  }
  host.stdin.end();
  fixture.closeAllConnections();
  fixture.close();
}
