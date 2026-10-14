import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:http";
import { mkdtemp, mkdir, writeFile, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { DesktopDriver } from "./desktop-driver.mjs";
import { preserveNativeRegistration } from "./native-test-state.mjs";

assert.equal(process.platform, "win32");
const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-daily-use-"));
const data = join(root, "Flux"),
  output = join(root, "output");
await mkdir(data);
await mkdir(output);
const snapshot = JSON.parse(
  execFileSync(
    "pwsh",
    ["-NoProfile", "-File", "scripts/startup-test-state.ps1", "-Mode", "Read"],
    { encoding: "utf8", windowsHide: true },
  ),
);
const snapshotFile = join(root, "startup.json");
await writeFile(snapshotFile, JSON.stringify(snapshot));
const database = new DatabaseSync(join(data, "tasks.sqlite"));
database.exec(
  "CREATE TABLE settings(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL)",
);
const folders = Object.fromEntries(
  ["compressed", "video", "music", "programs", "documents"].map((name) => [
    name,
    join(root, name),
  ]),
);
database.prepare("INSERT INTO settings VALUES(1,?)").run(
  JSON.stringify({
    locale: "en-US",
    download_directory: output,
    notifications: false,
    minimize_to_tray: false,
    category_folders: folders,
  }),
);
database.close();
const fixture = createServer((_request, response) => {
  response.writeHead(200, {
    "Content-Type": "application/pdf",
    "Content-Length": 17,
  });
  response.end("verified document");
});
fixture.listen(0, "127.0.0.1");
await once(fixture, "listening");
const source = `http://127.0.0.1:${fixture.address().port}/resource`;
const reservation = createServer();
reservation.listen(0, "127.0.0.1");
await once(reservation, "listening");
const port = reservation.address().port;
await new Promise((r) => reservation.close(r));
const env = {
  ...process.env,
  LOCALAPPDATA: root,
  WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
};
const restoreRegistration = preserveNativeRegistration();
const app = spawn(join(binary, "flux-desktop.exe"), [], {
  env,
  windowsHide: true,
  stdio: ["ignore", "pipe", "pipe"],
});
const driver = new DesktopDriver(port);
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(callback, timeout = 15000) {
  const end = Date.now() + timeout;
  while (!(await callback())) {
    assert.ok(Date.now() < end, "Native daily-use check timed out");
    await delay(100);
  }
}
let main;
const invoke = (name, args = {}) =>
  main.evaluate(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)})`,
  );
const input = (name) => ({
  url: source,
  filename: name,
  connections: 0,
  speed_limit: 0,
  expected_sha256: null,
  auto_rename: true,
});
const report = {
  version: JSON.parse(await readFile("package.json", "utf8")).version,
  power_os_actions_executed: false,
  passed: false,
};
try {
  main = await driver.connect("main");
  let settings = await invoke("get_settings");
  settings = {
    ...settings,
    auto_categorize: true,
    clipboard_mode: "silent_queue",
    launch_at_startup: true,
  };
  await invoke("save_settings", { input: settings });
  assert.equal((await invoke("get_settings")).launch_at_startup, true);
  const current = JSON.parse(
    execFileSync(
      "pwsh",
      [
        "-NoProfile",
        "-File",
        "scripts/startup-test-state.ps1",
        "-Mode",
        "Read",
      ],
      { encoding: "utf8", windowsHide: true },
    ),
  );
  assert.equal(
    current.value,
    `"${join(binary, "flux-desktop.exe")}" --background`,
  );
  report.startup_registry_verified = true;
  settings = { ...settings, launch_at_startup: false };
  await invoke("save_settings", { input: settings });
  assert.equal((await invoke("get_settings")).launch_at_startup, false);
  report.startup_disable_verified = true;
  await invoke("open_surface", { view: "settings" });
  const preferences = await driver.connect("settings");
  const tab = await preferences.evaluate(
    `(()=>{const tab=[...document.querySelectorAll('[role=tab]')].find(tab=>tab.textContent==='Daily use');const rect=tab.getBoundingClientRect();return {x:rect.x+rect.width/2,y:rect.y+rect.height/2}})()`,
  );
  await preferences.call("Input.dispatchMouseEvent", {
    type: "mousePressed",
    button: "left",
    clickCount: 1,
    ...tab,
  });
  await preferences.call("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    button: "left",
    clickCount: 1,
    ...tab,
  });
  await until(() =>
    preferences.evaluate(
      `!!document.querySelector('[role=switch][aria-label="Open on Windows startup"]')`,
    ),
  );
  assert.ok(
    (await preferences.evaluate("document.body.textContent")).includes(
      "Add to queue silently",
    ),
  );
  report.daily_settings_visible = true;
  const image = await preferences.call("Page.captureScreenshot", {
    format: "png",
  });
  await writeFile(
    join(root, "daily-settings.png"),
    Buffer.from(image.data, "base64"),
  );
  await preferences.evaluate(
    `void window.__TAURI_INTERNALS__.invoke('close_surface');true`,
  );
  await driver.absent("settings");
  const task = await invoke("add_download_to", {
    input: input("native-routing"),
    directory: null,
    start: true,
  });
  await until(
    async () =>
      (await invoke("get_task", { id: task.id })).status === "complete",
  );
  const finished = await invoke("get_task", { id: task.id });
  assert.ok(
    finished.destination.replace(/^\\\\\?\\/, "").startsWith(folders.documents),
  );
  report.native_mime_routing_verified = true;
  await until(() =>
    main.evaluate(`!!document.querySelector('[role=row][draggable=true]')`),
  );
  report.completed_row_draggable = true;
  const later = await invoke("add_download_to", {
    input: input("not-finished.pdf"),
    directory: output,
    start: false,
  });
  await assert.rejects(
    invoke("drag_completed", { ids: [later.id] }),
    /Only completed/,
  );
  await invoke("remove_task", { id: later.id });
  report.incomplete_drag_rejected = true;
  settings = {
    ...(await invoke("get_settings")),
    completion_action: "exit_app",
  };
  await invoke("save_settings", { input: settings });
  const second = await invoke("add_download_to", {
    input: input("countdown.pdf"),
    directory: output,
    start: true,
  });
  await until(
    async () =>
      (await invoke("get_task", { id: second.id })).status === "complete",
  );
  const countdown = await driver.connect("power");
  await countdown.evaluate(`(async()=>{
    const handler=window.__TAURI_INTERNALS__.transformCallback(()=>{});
    const event='power-pending';
    const eventId=await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event,target:{kind:'Any'},handler});
    await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten',{event,eventId});
    return true;
  })()`);
  report.countdown_event_permission_verified = true;
  const pending = await invoke("pending_power");
  assert.equal(pending.action, "exit_app");
  assert.ok(
    pending.deadline - Date.now() > 20_000 &&
      pending.deadline - Date.now() <= 30_000,
  );
  report.countdown_deadline_verified = true;
  await until(() =>
    countdown.evaluate(
      `document.body.textContent.includes('Downloads finished')`,
    ),
  );
  const powerImage = await countdown.call("Page.captureScreenshot", {
    format: "png",
  });
  await writeFile(
    join(root, "power-countdown.png"),
    Buffer.from(powerImage.data, "base64"),
  );
  await countdown.evaluate(
    `[...document.querySelectorAll('button')].find(button=>button.textContent==='Cancel').click();true`,
  );
  await driver.absent("power");
  assert.equal(await invoke("pending_power"), null);
  assert.equal((await invoke("get_settings")).completion_action, "none");
  report.native_cancel_verified = true;
  settings = {
    ...(await invoke("get_settings")),
    completion_action: "exit_app",
  };
  await invoke("save_settings", { input: settings });
  const third = await invoke("add_download_to", {
    input: input("changed-queue.pdf"),
    directory: output,
    start: true,
  });
  await until(
    async () =>
      (await invoke("get_task", { id: third.id })).status === "complete",
  );
  await driver.target("power");
  const added = await invoke("add_download_to", {
    input: input("added-during-countdown.pdf"),
    directory: output,
    start: false,
  });
  await driver.absent("power");
  assert.equal(await invoke("pending_power"), null);
  report.new_work_cancels_countdown = true;
  await invoke("remove_task", { id: added.id });
  settings = {
    ...(await invoke("get_settings")),
    completion_action: "exit_app",
  };
  await invoke("save_settings", { input: settings });
  const closeTask = await invoke("add_download_to", {
    input: input("close-countdown.pdf"),
    directory: output,
    start: true,
  });
  await until(
    async () =>
      (await invoke("get_task", { id: closeTask.id })).status === "complete",
  );
  const closingCountdown = await driver.connect("power");
  await closingCountdown.evaluate(
    "void window.__TAURI_INTERNALS__.invoke('close_surface');true",
  );
  await driver.absent("power");
  assert.equal(await invoke("pending_power"), null);
  assert.equal((await invoke("get_settings")).completion_action, "none");
  report.window_close_cancels_countdown = true;
  await invoke("save_settings", { input: settings });
  const exitTask = await invoke("add_download_to", {
    input: input("exit-after-completion.pdf"),
    directory: output,
    start: true,
  });
  await until(
    async () =>
      (await invoke("get_task", { id: exitTask.id })).status === "complete",
  );
  await driver.target("power");
  await until(() => app.exitCode !== null, 45000);
  assert.equal(app.exitCode, 0);
  report.exit_app_after_countdown_verified = true;
  const restored = execFileSync(
    "pwsh",
    [
      "-NoProfile",
      "-File",
      "scripts/startup-test-state.ps1",
      "-Mode",
      "Restore",
      "-Snapshot",
      snapshotFile,
    ],
    { windowsHide: true },
  );
  void restored;
  report.startup_registry_restored = true;
  report.passed = true;
  report.time = new Date().toISOString();
  await mkdir("docs/validation", { recursive: true });
  await writeFile(
    "docs/validation/daily-use.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
  execFileSync(
    "pwsh",
    [
      "-NoProfile",
      "-File",
      "scripts/startup-test-state.ps1",
      "-Mode",
      "Restore",
      "-Snapshot",
      snapshotFile,
    ],
    { windowsHide: true },
  );
  driver.close();
  if (app.exitCode === null) {
    const stop = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
      env,
      windowsHide: true,
      stdio: "ignore",
    });
    if (stop.exitCode === null) await once(stop, "exit");
  }
  fixture.closeAllConnections();
  await new Promise((r) => fixture.close(r));
  restoreRegistration();
}
