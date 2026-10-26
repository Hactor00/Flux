import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { once } from "node:events";
import { join } from "node:path";
import { DesktopDriver } from "./desktop-driver.mjs";

if (process.env.GITHUB_ACTIONS !== "true")
  throw Error("Published upgrade UI verification is CI-only");
const [installed, expected, profile] = process.argv.slice(2);
const reserve = createServer();
await new Promise((r) => reserve.listen(0, "127.0.0.1", r));
const port = reserve.address().port;
await new Promise((r) => reserve.close(r));
const app = spawn(join(installed, "flux-desktop.exe"), ["--background"], {
  windowsHide: true,
  stdio: "ignore",
  env: {
    ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: join(profile, "webview"),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  },
});
const driver = new DesktopDriver(port);
async function until(check, timeout = 60000) {
  const end = Date.now() + timeout;
  do {
    if (await check()) return;
    await new Promise((r) => setTimeout(r, 150));
  } while (Date.now() < end);
  throw Error("Published updater UI condition timed out");
}
try {
  const main = await driver.connect("main");
  // Open an existing tray instance without changing its original launch arguments.
  spawn(join(installed, "flux-desktop.exe"), [], {
    windowsHide: true,
    stdio: "ignore",
  });
  await until(() =>
    main.evaluate(
      `document.querySelector('.updater-modal h2')?.textContent.includes(${JSON.stringify(expected)})`,
    ),
  );
  await main.evaluate(
    "document.querySelector('.updater-modal .secondary').click(); true",
  );
  await main.evaluate(
    "window.__TAURI_INTERNALS__.invoke('open_surface',{view:'settings',id:null,initialUrl:null})",
  );
  const settings = await driver.connect("settings");
  await settings.evaluate(
    "[...document.querySelectorAll('[role=tab]')].find(tab=>tab.id.endsWith('trigger-updates')).focus(); true",
  );
  await until(() =>
    settings.evaluate("!!document.querySelector('.updater-panel button')"),
  );
  for (let index = 0; index < 2; index++) {
    await settings.evaluate(
      "document.querySelector('.updater-panel button').click(); true",
    );
    await until(() =>
      settings.evaluate(
        `document.querySelector('.updater-modal h2')?.textContent.includes(${JSON.stringify(expected)})`,
      ),
    );
    if (index === 0) {
      await settings.evaluate(
        "document.querySelector('.updater-modal .secondary').click(); true",
      );
      await until(() =>
        settings.evaluate("!document.querySelector('.updater-modal')"),
      );
    }
  }
  const exited = once(app, "exit");
  await settings.evaluate(
    "document.querySelector('.updater-modal .primary').click(); true",
  );
  let timeout;
  try {
    await Promise.race([
      exited,
      new Promise((_, reject) => {
        timeout = setTimeout(
          () => reject(Error("Signed update installation did not finish")),
          180000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timeout);
  }
  console.log(
    "Startup notice, repeated manual checks and signed in-app download/install completed.",
  );
} finally {
  driver.close();
  if (app.exitCode === null) app.kill();
}
