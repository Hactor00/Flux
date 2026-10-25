import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { DesktopDriver } from "./desktop-driver.mjs";
import { preserveNativeRegistration } from "./native-test-state.mjs";
const binary = resolve(process.argv[2] || "target/release");
const root = await mkdtemp(join(tmpdir(), "flux-download-feedback-"));
const output = join(root, "output");
await mkdir(output);
const restore = preserveNativeRegistration();
const payload = Buffer.alloc(1024 * 1024, 73);
const fixture = createServer((req, res) => {
  const match = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range || "");
  const start = match ? Number(match[1]) : 0;
  const end = match?.[2] ? Number(match[2]) : payload.length - 1;
  res.writeHead(match ? 206 : 200, {
    "Content-Length": end - start + 1,
    "Content-Type": "application/octet-stream",
    ETag: '"feedback-v1"',
    ...(match
      ? { "Content-Range": `bytes ${start}-${end}/${payload.length}` }
      : {}),
  });
  res.end(payload.subarray(start, end + 1));
});
await new Promise((r) => fixture.listen(0, "127.0.0.1", r));
const reservation = createServer();
await new Promise((r) => reservation.listen(0, "127.0.0.1", r));
const port = reservation.address().port;
await new Promise((r) => reservation.close(r));
const env = {
  ...process.env,
  LOCALAPPDATA: root,
  WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
};
let app, driver, main;
async function launch() {
  app = spawn(join(binary, "flux-desktop.exe"), ["--background"], {
    env,
    windowsHide: true,
    stdio: ["ignore", "ignore", "pipe"],
  });
  app.stderr.on("data", (b) => process.stderr.write(b));
  driver = new DesktopDriver(port);
  main = await driver.connect("main");
}
const invoke = (command, args = {}) =>
  main.evaluate(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`,
  );
async function until(check) {
  const deadline = Date.now() + 30000;
  do {
    const result = await check();
    if (result) return result;
    await new Promise((r) => setTimeout(r, 100));
  } while (Date.now() < deadline);
  throw Error("Download feedback condition timed out");
}
async function stop() {
  driver?.close();
  if (app?.exitCode === null) {
    const stopping = once(app, "exit");
    const helper = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
      env,
      windowsHide: true,
      stdio: "ignore",
    });
    if (helper.exitCode === null) await once(helper, "exit");
    await stopping;
  }
}
try {
  await launch();
  await invoke("save_settings", {
    input: {
      ...(await invoke("get_settings")),
      locale: "en-US",
      notifications: true,
      minimize_to_tray: false,
      download_directory: output,
    },
  });
  const source = `http://127.0.0.1:${fixture.address().port}/payload.bin`;
  const base = {
    url: source,
    filename: "Completed test.bin",
    connections: 0,
    speed_limit: 0,
    expected_sha256: null,
    auto_rename: true,
  };
  const media = await invoke("add_download_to", {
    input: {
      ...base,
      filename: "Media size test.mp4",
      media: { kind: "file", container: "mp4", audio_url: source + "?audio=1" },
    },
    directory: output,
    start: false,
  });
  await invoke("open_surface", {
    view: "download",
    id: media.id,
    initialUrl: null,
  });
  const approval = await driver.connect("download");
  await until(() =>
    approval.evaluate(
      `document.querySelector('.source-summary strong')?.textContent.includes('2 MB')`,
    ),
  );
  await approval.evaluate(
    `setTimeout(() => document.querySelector('.modal-close').click(), 50); true`,
  );
  await driver.absent("download");
  const task = await invoke("add_download_to", {
    input: base,
    directory: output,
    start: true,
  });
  await until(
    async () =>
      (await invoke("get_task", { id: task.id })).status === "complete",
  );
  const popup = await driver.connect("complete");
  await until(() =>
    popup.evaluate(
      `document.body.textContent.includes('Download complete') && document.body.textContent.includes('Completed test.bin')`,
    ),
  );
  const popupText = await popup.evaluate("document.body.textContent");
  for (const text of ["Open file", "Show in folder", "Close"])
    assert.ok(popupText.includes(text));
  await mkdir("output/260-refresh", { recursive: true });
  const shot = await popup.call("Page.captureScreenshot", { format: "png" });
  await writeFile(
    "output/260-refresh/completion-window.png",
    Buffer.from(shot.data, "base64"),
  );
  await popup.evaluate(
    `setTimeout(() => document.querySelector('.modal-close').click(), 50); true`,
  );
  await driver.absent("complete");
  const select = async (name) => {
    await until(() =>
      main.evaluate(
        `!![...document.querySelectorAll('.desktop-row')].find(row=>row.textContent.includes(${JSON.stringify(name)}))`,
      ),
    );
    await main.evaluate(
      `[...document.querySelectorAll('.desktop-row')].find(row=>row.textContent.includes(${JSON.stringify(name)})).click()`,
    );
  };
  const actions = () =>
    main.evaluate(
      `Object.fromEntries([...document.querySelectorAll('.desktop-toolbar button')].map(b=>[b.textContent.trim(),!b.disabled]))`,
    );
  await select(base.filename);
  let buttons = await actions();
  assert.equal(buttons.Resume, false);
  assert.equal(buttons.Pause, false);
  assert.equal(buttons["Remove task"], true);
  await main.evaluate(
    `document.querySelector('.desktop-main').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true}))`,
  );
  await until(async () => !(await actions())["Remove task"]);
  await select(media.request.filename);
  buttons = await actions();
  assert.equal(buttons.Resume, true);
  assert.equal(buttons.Pause, false);
  await main.evaluate(
    `document.body.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))`,
  );
  await until(async () => !(await actions())["Remove task"]);
  await select(base.filename);
  await main.evaluate(
    `[...document.querySelectorAll('.desktop-sidebar button')].find(b=>b.textContent.includes('Paused')).click()`,
  );
  await until(async () => !(await actions())["Remove task"]);
  assert.equal(
    await main.evaluate(
      `document.querySelectorAll('.desktop-row[aria-selected="true"]').length`,
    ),
    0,
  );
  await stop();
  await launch();
  await new Promise((r) => setTimeout(r, 2500));
  await driver.absent("complete");
  await writeFile(
    "docs/validation/download-feedback.json",
    JSON.stringify(
      {
        version: JSON.parse(await readFile("package.json", "utf8")).version,
        media_source_size_before_approval: true,
        completion_window_once: true,
        completion_file_actions_visible: true,
        history_does_not_reopen: true,
        selection_empty_space: true,
        selection_escape: true,
        selection_filter: true,
        state_appropriate_actions: true,
        passed: true,
      },
      null,
      2,
    ) + "\n",
  );
  console.log("Download size, completion and selection checks passed.");
} finally {
  await stop();
  fixture.closeAllConnections();
  await new Promise((r) => fixture.close(r));
  restore();
}
