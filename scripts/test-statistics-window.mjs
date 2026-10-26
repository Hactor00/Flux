import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { DesktopDriver } from "./desktop-driver.mjs";
import { preserveNativeRegistration } from "./native-test-state.mjs";

const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-consent-test-"));
const reservation = createServer();
await new Promise((r) => reservation.listen(0, "127.0.0.1", r));
const port = reservation.address().port;
await new Promise((r) => reservation.close(r));
const restore = preserveNativeRegistration();
const env = {
  ...process.env,
  LOCALAPPDATA: root,
  WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
};
const app = spawn(join(binary, "flux-desktop.exe"), [], {
  env,
  windowsHide: true,
  stdio: "ignore",
});
const driver = new DesktopDriver(port, { preserveStatisticsPrompt: true });
async function until(check) {
  const deadline = Date.now() + 30000;
  do {
    const value = await check();
    if (value) return value;
    await new Promise((r) => setTimeout(r, 100));
  } while (Date.now() < deadline);
  throw Error("Consent UI condition timed out");
}
try {
  const main = await driver.connect("main");
  const invoke = (command, args = {}) =>
    main.evaluate(
      `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`,
    );
  const initial = await invoke("statistics_status");
  assert.equal(initial.enabled, false);
  assert.equal(initial.prompt_due, true);
  assert.equal("installation_id" in initial, false);
  await until(() => main.evaluate("!!document.querySelector('.usage-modal')"));
  assert.equal(
    await main.evaluate(
      "document.querySelector('.usage-modal .primary').textContent.includes('Yes')",
    ),
    true,
  );
  await main.evaluate(
    "document.querySelectorAll('.usage-actions button')[1].click(); true",
  );
  await until(async () => !(await invoke("statistics_status")).prompt_due);
  const later = await invoke("statistics_status");
  assert.ok(
    Math.abs(later.remind_after - Math.floor(Date.now() / 1000) - 7 * 86400) <
      20,
  );
  const preferences = JSON.parse(
    await readFile(join(root, "Flux/usage-preferences.json"), "utf8"),
  );
  assert.equal(preferences.installation_id, null);
  assert.equal(preferences.last_success, null);
  await invoke("statistics_choice", { choice: "never" });
  assert.equal((await invoke("statistics_status")).prompt_due, false);
  for (const [width, height] of [
    [1180, 720],
    [760, 500],
    [640, 420],
  ]) {
    await main.call("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    const layout = await main.evaluate(
      "(() => {const footer=document.querySelector('.desktop-status').getBoundingClientRect();return {overflow:document.documentElement.scrollWidth-innerWidth,bottom:footer.bottom,height:innerHeight,inspector:!!document.querySelector('.desktop-inspector')};})()",
    );
    assert.ok(layout.overflow <= 1, JSON.stringify(layout));
    assert.ok(layout.bottom <= layout.height + 1, JSON.stringify(layout));
    assert.equal(layout.inspector, false);
  }
  await main.call("Emulation.clearDeviceMetricsOverride");
  await writeFile(
    "docs/validation/statistics-window.json",
    JSON.stringify(
      {
        version: JSON.parse(await readFile("package.json")).version,
        passed: true,
        default_off: true,
        no_identity_before_acceptance: true,
        later_seven_days: true,
        permanent_suppression: true,
        responsive_sizes: [
          [1180, 720],
          [760, 500],
          [640, 420],
        ],
      },
      null,
      2,
    ),
  );
  console.log(
    "Native consent and responsive desktop checks passed without transmitting usage events.",
  );
} finally {
  driver.close();
  const stopping = app.exitCode === null ? once(app, "exit") : null;
  const helper = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
    env,
    windowsHide: true,
    stdio: "ignore",
  });
  if (helper.exitCode === null) await once(helper, "exit");
  if (stopping) await stopping;
  restore();
}
