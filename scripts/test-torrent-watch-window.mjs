import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:net";
import { mkdtemp, mkdir, writeFile, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { createHash } from "node:crypto";
import { DesktopDriver } from "./desktop-driver.mjs";
import { preserveNativeRegistration } from "./native-test-state.mjs";

const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-watch-window-"));
const data = join(root, "Flux");
const watch = join(root, "watch");
await mkdir(data);
await mkdir(watch);
const db = new DatabaseSync(join(data, "tasks.sqlite"));
db.exec(
  "CREATE TABLE settings(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)",
);
db.prepare("INSERT INTO settings VALUES(1,?)").run(
  JSON.stringify({
    locale: "en-US",
    queue_enabled: false,
    notifications: false,
    torrent_watch: { enabled: true, folders: [watch], mode: "automatic" },
  }),
);
db.close();
const reservation = createServer();
reservation.listen(0, "127.0.0.1");
await once(reservation, "listening");
const port = reservation.address().port;
await new Promise((resolve) => reservation.close(resolve));
const env = {
  ...process.env,
  LOCALAPPDATA: root,
  WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
};
const restore = preserveNativeRegistration();
let app, driver;
const wait = async (operation) => {
  const deadline = Date.now() + 40000;
  while (Date.now() < deadline) {
    if (await operation()) return;
    if (app?.exitCode !== null) throw Error("Desktop exited");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw Error("Watch-folder window test timed out");
};
async function launch() {
  app = spawn(join(binary, "flux-desktop.exe"), [], {
    env,
    windowsHide: true,
    stdio: "ignore",
  });
  driver = new DesktopDriver(port);
  return driver.connect("main");
}
async function stop() {
  driver?.close();
  if (app?.exitCode === null) {
    const closed = once(app, "exit");
    const shutdown = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
      env,
      windowsHide: true,
      stdio: "ignore",
    });
    await once(shutdown, "exit");
    await closed;
  }
}
try {
  let main = await launch();
  await main.evaluate(
    "window.__TAURI_INTERNALS__.invoke('open_surface',{view:'settings',id:null,initialUrl:null})",
  );
  const settings = await driver.connect("settings");
  await settings.evaluate(
    "[...document.querySelectorAll('[role=tab]')].find(el=>el.textContent==='Torrent watch folders').focus(); true",
  );
  await wait(() =>
    settings.evaluate("!!document.querySelector('.torrent-watch-settings')"),
  );
  assert.ok(
    await settings.evaluate(
      "document.querySelector('.torrent-watch-settings').textContent.includes('Ask for confirmation') || document.querySelector('.torrent-watch-settings').textContent.includes('Add to the download queue automatically')",
    ),
  );
  await settings.evaluate("document.activeElement?.blur(); true");
  const screenshot = await settings.call("Page.captureScreenshot", { format: "png" });
  await writeFile("docs/images/torrent-watch-2.3.png", Buffer.from(screenshot.data, "base64"));
  const payload = Buffer.from("fixture");
  const metainfo = Buffer.concat([
    Buffer.from(
      "d4:infod6:lengthi7e4:name10:sample.bin12:piece lengthi16384e6:pieces20:",
    ),
    createHash("sha1").update(payload).digest(),
    Buffer.from("ee"),
  ]);
  await writeFile(join(watch, "sample.torrent"), metainfo);
  await wait(
    async () =>
      (await main.evaluate("window.__TAURI_INTERNALS__.invoke('list_tasks')"))
        .length === 1,
  );
  const tasks = await main.evaluate(
    "window.__TAURI_INTERNALS__.invoke('list_tasks')",
  );
  assert.equal(tasks[0].status, "queued");
  const report = await main.evaluate(
    "window.__TAURI_INTERNALS__.invoke('torrent_watch_report')",
  );
  assert.equal(report.imported, 1);
  assert.equal(report.error, null);
  await stop();
  main = await launch();
  await new Promise((resolve) => setTimeout(resolve, 6500));
  assert.equal(
    (await main.evaluate("window.__TAURI_INTERNALS__.invoke('list_tasks')"))
      .length,
    1,
  );
  assert.equal(
    (await main.evaluate("window.__TAURI_INTERNALS__.invoke('get_settings')"))
      .torrent_watch.enabled,
    true,
  );
  await writeFile(
    "docs/validation/torrent-watch-window.json",
    JSON.stringify(
      {
        version: JSON.parse(await readFile("package.json", "utf8")).version,
        passed: true,
        settings_visible: true,
        background_import: true,
        queue_pause_respected: true,
        restart_duplicate_prevention: true,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    "Torrent watch settings, background import and restart persistence passed",
  );
} finally {
  await stop();
  restore();
}
