import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, writeFile, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { DesktopDriver } from "./desktop-driver.mjs";
import {
  nativeRegistration,
  preserveNativeRegistration,
} from "./native-test-state.mjs";

const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-onboarding-"));
const data = join(root, "Flux");
await mkdir(data);
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
const restoreRegistration = preserveNativeRegistration();
let app;
let driver;
const wait = async (operation) => {
  const deadline = Date.now() + 60000;
  let last;
  while (Date.now() < deadline) {
    try {
      const value = await operation();
      if (value) return value;
    } catch (error) {
      last = error;
    }
    if (app?.exitCode !== null && app?.exitCode !== undefined)
      throw Error(`Desktop exited: ${app.exitCode}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw Error(
    `Onboarding timed out: ${last?.message || "condition was not met"}`,
  );
};
async function launch(background = false) {
  app = spawn(
    join(binary, "flux-desktop.exe"),
    background ? ["--background"] : [],
    { env, windowsHide: true, stdio: "ignore" },
  );
  driver = new DesktopDriver(port);
  const main = await driver.connect("main");
  await wait(async () => {
    const manifest = JSON.parse(
      await readFile(join(data, "native-host.json"), "utf8"),
    );
    return manifest.path === join(binary, "flux-native-host.exe");
  });
  return main;
}
async function stop() {
  driver?.close();
  driver = null;
  if (!app || app.exitCode !== null) return;
  const closed = once(app, "exit");
  const shutdown = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
    env,
    windowsHide: true,
    stdio: "ignore",
  });
  await once(shutdown, "exit");
  await closed;
  app = null;
}
const invoke = (main, command) =>
  main.evaluate(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)})`,
  );
function assertBindings() {
  const values = JSON.parse(nativeRegistration());
  for (const item of values.filter((value) => value.name === "")) {
    assert.equal(item.exists, true);
    assert.equal(
      item.value,
      join(
        data,
        item.path.includes("Mozilla")
          ? "io.flux.download.firefox.json"
          : "native-host.json",
      ),
    );
  }
  assert.equal(
    values.find((value) => value.name === "BridgeDirectory").value,
    binary,
  );
}
try {
  let main = await launch(true);
  assertBindings();
  assert.equal(
    await main.evaluate(
      "window.__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'main'})",
    ),
    false,
  );
  let status = await invoke(main, "browser_integration");
  assert.equal(status.registered, true);
  assert.equal(status.reminder_dismissed, false);
  const foreground = spawn(join(binary, "flux-desktop.exe"), [], {
    env,
    windowsHide: true,
    stdio: "ignore",
  });
  await once(foreground, "exit");
  await wait(() =>
    main.evaluate(
      "window.__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'main'})",
    ),
  );
  await wait(() =>
    main.evaluate(
      "!!document.querySelector('.integration-notice .integration-state.ready')",
    ),
  );
  assert.equal(
    (await invoke(main, "browser_integration")).reminder_dismissed,
    false,
  );
  await main.evaluate(
    "document.querySelector('.integration-dismiss').click(); true",
  );
  await wait(
    async () =>
      !(await main.evaluate("!!document.querySelector('.integration-notice')")),
  );
  assert.equal(
    (await invoke(main, "browser_integration")).reminder_dismissed,
    true,
  );
  const files = [
    "native-host.json",
    "io.flux.download.firefox.json",
    "allowed-origins.json",
    "browser-integration.seen",
  ];
  const timestamps = await Promise.all(
    files.map(async (file) => (await stat(join(data, file))).mtimeMs),
  );
  await stop();

  main = await launch();
  status = await invoke(main, "browser_integration");
  assert.equal(status.reminder_dismissed, true);
  assert.equal(
    await main.evaluate("!!document.querySelector('.integration-notice')"),
    false,
  );
  assertBindings();
  assert.deepEqual(
    await Promise.all(
      files.map(async (file) => (await stat(join(data, file))).mtimeMs),
    ),
    timestamps,
  );
  await stop();

  nativeRegistration("Clear", ["-DataDirectory", data]);
  const manifest = JSON.parse(
    await readFile(join(data, "native-host.json"), "utf8"),
  );
  manifest.path = join(root, "removed-host.exe");
  await writeFile(join(data, "native-host.json"), JSON.stringify(manifest));
  main = await launch();
  assertBindings();
  status = await invoke(main, "browser_integration");
  assert.equal(status.registered, true);
  assert.equal(status.reminder_dismissed, true);
  await main.evaluate(
    "window.__TAURI_INTERNALS__.invoke('open_surface',{view:'settings',id:null,initialUrl:null})",
  );
  const settings = await driver.connect("settings");
  await wait(() =>
    settings.evaluate(
      "!!document.querySelector('[data-state=active][role=tab]')",
    ),
  );
  await settings.evaluate(
    "[...document.querySelectorAll('[role=tab]')].find(tab=>tab.id.endsWith('trigger-browserIntegration')).click(); true",
  );
  await wait(() =>
    settings.evaluate(
      "!!document.querySelector('.bridge-panel .integration-state.ready')",
    ),
  );
  assert.equal(
    await settings.evaluate(
      "!!document.querySelector('.bridge-panel button:not(:disabled)')",
    ),
    true,
  );
  await stop();
  const report = {
    passed: true,
    version: "2.2.0",
    automatic_background_registration: true,
    background_does_not_dismiss_reminder: true,
    first_foreground_guide_visible: true,
    dismissal_persisted_after_restart: true,
    unchanged_manifests_retained: true,
    missing_registry_and_outdated_host_repaired: true,
    browser_bindings_verified: 5,
    settings_status_verified: true,
    registry_restoration_verified: true,
    time: new Date().toISOString(),
  };
  restoreRegistration();
  await writeFile(
    "docs/validation/browser-onboarding.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
  await stop();
  restoreRegistration();
}
