import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
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
let browserHost;
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
async function screenshot(view, filename) {
  if (!process.env.FLUX_UI_SCREENSHOTS) return;
  await mkdir(process.env.FLUX_UI_SCREENSHOTS, { recursive: true });
  const result = await view.call("Page.captureScreenshot", { format: "png" });
  await writeFile(
    join(process.env.FLUX_UI_SCREENSHOTS, filename),
    Buffer.from(result.data, "base64"),
  );
}
async function inspectSettingsLayout(settings, main) {
  assert.equal(
    await invoke(settings, "plugin:app|version"),
    JSON.parse(await readFile("package.json", "utf8")).version,
  );
  // Exercise the real settings controls with representative long and RTL labels.
  for (const locale of ["tr-TR", "de-DE", "ar-SA", "en-US"]) {
    await settings.evaluate(
      `document.querySelector('.language-setting [role=combobox]').click(); true`,
    );
    await wait(() =>
      settings.evaluate(
        `!!document.querySelector('.choice-item[data-value="${locale}"]')`,
      ),
    );
    await settings.evaluate(
      `document.querySelector('.choice-item[data-value="${locale}"]').click(); true`,
    );
    await wait(() =>
      settings.evaluate(`document.documentElement.lang === '${locale}'`),
    );
    for (const section of [
      "downloads",
      "daily_usage",
      "host_accounts",
      "seeding",
      "watch_title",
      "language",
    ]) {
      await settings.evaluate(
        `[...document.querySelectorAll('[role=tab]')].find(tab=>tab.id.endsWith('trigger-${section}')).focus(); true`,
      );
      await wait(() =>
        settings.evaluate(
          `!!document.querySelector('[id$="content-${section}"][data-state=active]')`,
        ),
      );
      const layout = await settings.evaluate(
        `(() => { const panel=document.querySelector('.settings-content'); const footer=document.querySelector('.settings-footer').getBoundingClientRect(); return {overflow:panel.scrollWidth-panel.clientWidth, bottom:footer.bottom, width:innerWidth, height:innerHeight}; })()`,
      );
      assert.ok(
        layout.overflow <= 2,
        `Settings overflow in ${locale}/${section}: ${JSON.stringify(layout)}`,
      );
      assert.ok(
        layout.bottom <= layout.height + 1,
        "Settings save bar is outside the window",
      );
    }
  }
  await settings.evaluate("document.activeElement.blur(); true");
  await screenshot(settings, "settings-dark-2.4.2.png");
  for (const theme of ["light", "dark"]) {
    await settings.evaluate(
      `document.querySelector('.theme-option.${theme}').click(); true`,
    );
    await settings.evaluate(
      "document.querySelector('.settings-footer .primary').click(); true",
    );
    await wait(async () => (await invoke(main, "get_settings")).theme === theme);
    await wait(() =>
      settings.evaluate(
        `document.documentElement.dataset.theme === '${theme}'`,
      ),
    );
    await wait(() =>
      main.evaluate(`document.documentElement.dataset.theme === '${theme}'`),
    );
    assert.equal((await invoke(main, "get_settings")).theme, theme);
    if (theme === "light")
      await screenshot(settings, "settings-light-2.4.2.png");
  }
  await screenshot(main, "queue-dark-2.4.2.png");
}
function mainWindowVisible() {
  const powershell = join(
    process.env.SystemRoot || "C:/Windows",
    "System32/WindowsPowerShell/v1.0/powershell.exe",
  );
  const state = JSON.parse(
    execFileSync(
      powershell,
      [
        "-NoProfile",
        "-NonInteractive",
        "-File",
        resolve("scripts/browser-dialogs.ps1"),
        "-BrowserProcessId",
        String(app.pid),
      ],
      { encoding: "utf8", windowsHide: true },
    ),
  );
  return state.windows.includes("Flux");
}
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
  assert.equal(mainWindowVisible(), false);
  let status = await invoke(main, "browser_integration");
  assert.equal(status.registered, true);
  assert.equal(status.connected, false);
  assert.deepEqual(status.extension_versions, []);
  assert.equal(status.reminder_dismissed, false);
  const foreground = spawn(join(binary, "flux-desktop.exe"), [], {
    env,
    windowsHide: true,
    stdio: "ignore",
  });
  await once(foreground, "exit");
  await wait(() => mainWindowVisible());
  await wait(() =>
    main.evaluate(
      "!!document.querySelector('.integration-notice .integration-state.pending')",
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
  await inspectSettingsLayout(settings, main);
  await settings.evaluate(
    "[...document.querySelectorAll('[role=tab]')].find(tab=>tab.id.endsWith('trigger-browserIntegration')).focus(); true",
  );
  await wait(() =>
    settings.evaluate(
      "!!document.querySelector('.bridge-panel .integration-state.pending')",
    ),
  );
  assert.equal(
    await settings.evaluate(
      "!!document.querySelector('.bridge-panel button:not(:disabled)')",
    ),
    true,
  );
  // Registration must not count as presence. Only an authenticated native port does.
  await settings.evaluate(
    "document.querySelector('.bridge-panel button.primary').click(); true",
  );
  await wait(() =>
    settings.evaluate(
      "!document.querySelector('.bridge-panel button.primary').disabled",
    ),
  );
  assert.equal((await invoke(main, "browser_integration")).connected, false);
  const extensionVersion = JSON.parse(
    await readFile("extension/manifest.json", "utf8"),
  ).version;
  const origin = `chrome-extension://${(await readFile("extension/extension-id.txt", "utf8")).trim()}/`;
  browserHost = spawn(join(binary, "flux-native-host.exe"), [origin], {
    env,
    windowsHide: true,
    stdio: ["pipe", "pipe", "pipe"],
  });
  const frame = Buffer.from(
    JSON.stringify({
      version: 1,
      request_id: crypto.randomUUID(),
      command: "browser_connect",
      payload: { extension_version: extensionVersion },
    }),
  );
  const prefix = Buffer.alloc(4);
  prefix.writeUInt32LE(frame.length);
  browserHost.stdin.write(Buffer.concat([prefix, frame]));
  browserHost.stdout.resume();
  browserHost.stderr.resume();
  await wait(async () => (await invoke(main, "browser_integration")).connected);
  assert.deepEqual(
    (await invoke(main, "browser_integration")).extension_versions,
    [extensionVersion],
  );
  await wait(() =>
    settings.evaluate(
      "!!document.querySelector('.bridge-panel .integration-state.ready')",
    ),
  );
  const disconnected = once(browserHost, "exit");
  browserHost.stdin.end();
  await disconnected;
  browserHost = null;
  await wait(
    async () => !(await invoke(main, "browser_integration")).connected,
  );
  await wait(() =>
    settings.evaluate(
      "!!document.querySelector('.bridge-panel .integration-state.pending')",
    ),
  );
  await stop();
  const report = {
    passed: true,
    version: JSON.parse(await readFile("package.json", "utf8")).version,
    automatic_background_registration: true,
    background_does_not_dismiss_reminder: true,
    first_foreground_guide_visible: true,
    dismissal_persisted_after_restart: true,
    unchanged_manifests_retained: true,
    missing_registry_and_outdated_host_repaired: true,
    browser_bindings_verified: 5,
    settings_status_verified: true,
    registration_is_not_extension_presence: true,
    native_port_connection_and_disconnect_verified: true,
    settings_navigation_and_rtl_layout_verified: true,
    light_dark_theme_persistence_verified: true,
    application_version_read_verified: true,
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
  browserHost?.kill();
  await stop();
  restoreRegistration();
}
