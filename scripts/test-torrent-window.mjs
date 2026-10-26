import { preserveNativeRegistration } from "./native-test-state.mjs";
import assert from "node:assert/strict";
import { createServer as netServer } from "node:net";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { DatabaseSync } from "node:sqlite";
import { DesktopDriver } from "./desktop-driver.mjs";

const binary = resolve(process.argv[2] || "target/debug");
const labels = JSON.parse(await readFile("src/locales/en-US.json"));
const root = await mkdtemp(join(tmpdir(), "flux-torrent-window-"));
const output = join(root, "output");
await mkdir(join(root, "Flux"));
await mkdir(output);
const db = new DatabaseSync(join(root, "Flux/tasks.sqlite"));
db.exec(
  "CREATE TABLE settings(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)",
);
db.prepare("INSERT INTO settings VALUES(1,?)").run(
  JSON.stringify({
    locale: "en-US",
    notifications: false,
    minimize_to_tray: false,
    download_directory: output,
  }),
);
db.close();
const payload = Buffer.from(
  Array.from({ length: 512 * 1024 }, (_, index) => index % 251),
);
const expected = createHash("sha256").update(payload).digest("hex");
function encode(value) {
  if (Buffer.isBuffer(value))
    return Buffer.concat([Buffer.from(`${value.length}:`), value]);
  if (typeof value === "string") return encode(Buffer.from(value));
  if (typeof value === "number") return Buffer.from(`i${value}e`);
  if (Array.isArray(value))
    return Buffer.concat([
      Buffer.from("l"),
      ...value.map(encode),
      Buffer.from("e"),
    ]);
  return Buffer.concat([
    Buffer.from("d"),
    ...Object.keys(value)
      .sort()
      .flatMap((key) => [encode(key), encode(value[key])]),
    Buffer.from("e"),
  ]);
}
const pieces = [];
for (let offset = 0; offset < payload.length; offset += 16384)
  pieces.push(
    createHash("sha1")
      .update(payload.subarray(offset, offset + 16384))
      .digest(),
  );
const info = {
  length: payload.length,
  name: "torrent-window.bin",
  "piece length": 16384,
  pieces: Buffer.concat(pieces),
  private: 1,
};
const infoHash = createHash("sha1").update(encode(info)).digest();
const metainfo = join(root, "fixture.torrent");
await writeFile(metainfo, encode({ info }));
const v2Metainfo = join(root, "fixture-v2.torrent");
await writeFile(
  v2Metainfo,
  encode({
    info: {
      name: "native-v2.bin",
      "meta version": 2,
      "piece length": 16384,
      "file tree": {
        "native-v2.bin": {
          "": {
            length: 128,
            "pieces root": createHash("sha256")
              .update(payload.subarray(0, 128))
              .digest(),
          },
        },
      },
    },
  }),
);
const sockets = new Set();
let requests = 0;
const peer = netServer((socket) => {
  sockets.add(socket);
  socket.on("close", () => sockets.delete(socket));
  socket.on("error", () => {});
  let buffer = Buffer.alloc(0),
    handshake = false;
  function message(id, body = Buffer.alloc(0)) {
    const prefix = Buffer.alloc(5);
    prefix.writeUInt32BE(body.length + 1);
    prefix[4] = id;
    socket.write(Buffer.concat([prefix, body]));
  }
  socket.on("data", (bytes) => {
    buffer = Buffer.concat([buffer, bytes]);
    if (!handshake) {
      if (buffer.length < 68) return;
      assert.deepEqual(buffer.subarray(28, 48), infoHash);
      buffer = buffer.subarray(68);
      handshake = true;
      socket.write(
        Buffer.concat([
          Buffer.from([19]),
          Buffer.from("BitTorrent protocol"),
          Buffer.alloc(8),
          infoHash,
          Buffer.from("-FX2000-123456789012"),
        ]),
      );
      message(5, Buffer.alloc(Math.ceil(pieces.length / 8), 255));
      message(1);
    }
    while (buffer.length >= 4) {
      const length = buffer.readUInt32BE();
      if (buffer.length < length + 4) break;
      const packet = buffer.subarray(4, 4 + length);
      buffer = buffer.subarray(4 + length);
      if (packet[0] !== 6) continue;
      const piece = packet.readUInt32BE(1),
        offset = packet.readUInt32BE(5),
        count = packet.readUInt32BE(9);
      assert.ok(count <= 16384);
      assert.ok(piece * 16384 + offset + count <= payload.length);
      requests++;
      const header = Buffer.alloc(8);
      header.writeUInt32BE(piece);
      header.writeUInt32BE(offset, 4);
      message(
        7,
        Buffer.concat([
          header,
          payload.subarray(
            piece * 16384 + offset,
            piece * 16384 + offset + count,
          ),
        ]),
      );
    }
  });
});
await new Promise((resolve) => peer.listen(0, "127.0.0.1", resolve));
const reservation = createServer();
await new Promise((resolve) => reservation.listen(0, "127.0.0.1", resolve));
const port = reservation.address().port;
await new Promise((resolve) => reservation.close(resolve));
const driver = new DesktopDriver(port);
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
  stdio: ["ignore", "ignore", "pipe"],
});
let errors = "";
app.stderr.on("data", (bytes) => (errors = (errors + bytes).slice(-16384)));
async function until(action, timeout = 45000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const value = await action();
    if (value) return value;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw Error("Torrent native window condition timed out");
}
const invoke = (page, command, args = {}) =>
  page.evaluate(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`,
  );
let report;
try {
  const main = await driver.connect("main");
  const v2 = await invoke(main, "import_torrent", {
    path: v2Metainfo,
    filename: "Native v2 Runtime Test",
    directory: output,
    start: false,
    options: {
      upload_limit: 0,
      peer_limit: 8,
      only_files: [],
      initial_peers: [],
    },
    transfer: { speed_limit: 0 },
  });
  assert.equal(v2.status, "paused");
  assert.match(v2.request.url, /^torrent:/);
  await invoke(main, "remove_task", { id: v2.id });
  await invoke(main, "open_surface", { view: "download" });
  const download = await driver.connect("download");
  await until(() =>
    download.evaluate(
      "[...document.querySelectorAll('[role=tab]')].some(tab=>tab.textContent.includes('Torrent'))",
    ),
  );
  const tabPoint = await download.evaluate(
    "(()=>{const tab=[...document.querySelectorAll('[role=tab]')].find(tab=>tab.textContent.includes('Torrent'));const box=tab.getBoundingClientRect();return {x:box.left+box.width/2,y:box.top+box.height/2};})()",
  );
  await download.call("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...tabPoint,
    button: "left",
    clickCount: 1,
  });
  await download.call("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...tabPoint,
    button: "left",
    clickCount: 1,
  });
  await until(() =>
    download.evaluate("!!document.querySelector('input[name=peer_limit]')"),
  );
  const task = await invoke(download, "import_torrent", {
    path: metainfo,
    filename: "Native Torrent Test",
    directory: output,
    start: false,
    options: {
      upload_limit: 0,
      peer_limit: 8,
      only_files: [],
      initial_peers: [`127.0.0.1:${peer.address().port}`],
    },
    transfer: { speed_limit: 65536, expected_sha256: expected },
  });
  assert.equal(task.status, "paused");
  await download.evaluate(
    "void window.__TAURI_INTERNALS__.invoke('close_surface');true",
  );
  await driver.absent("download");
  await invoke(main, "resume_download", { id: task.id });
  await invoke(main, "open_surface", { view: "details", id: task.id });
  let details = await driver.connect("details");
  await details.evaluate(
    "document.querySelector('.transfer-details-button').click(); true",
  );
  await until(async () => {
    const value = await invoke(details, "get_task", { id: task.id });
    if (value.status === "failed") throw Error(value.error);
    return value.completed > 0;
  });
  await until(() =>
    details.evaluate("!!document.querySelector('.torrent-details')"),
  );
  const frames = await details.evaluate(
    "new Promise(resolve=>{const times=[];let last=performance.now();const tick=now=>{times.push(now-last);last=now;if(times.length===90)resolve({frames:times.length,duration_ms:times.reduce((sum,time)=>sum+time,0),maximum_interval_ms:Math.max(...times)});else requestAnimationFrame(tick);};requestAnimationFrame(tick);})",
  );
  const paused = await invoke(details, "pause_download", { id: task.id });
  assert.equal(paused.status, "paused");
  assert.ok(paused.completed > 0 && paused.completed < payload.length);
  await details.evaluate(
    "void window.__TAURI_INTERNALS__.invoke('close_surface');true",
  );
  await driver.absent("details");
  await invoke(main, "open_surface", { view: "details", id: task.id });
  details = await driver.connect("details");
  await details.evaluate(
    "document.querySelector('.transfer-details-button').click(); true",
  );
  await until(() =>
    details.evaluate("!!document.querySelector('.torrent-details')"),
  );
  await invoke(details, "resume_download", { id: task.id });
  const complete = await until(async () => {
    const value = await invoke(details, "get_task", { id: task.id });
    if (value.status === "failed") throw Error(value.error);
    return value.status === "complete" && value;
  });
  assert.equal(complete.sha256, expected);
  assert.equal(complete.torrent.files[0].sha256, expected);
  assert.equal(
    createHash("sha256")
      .update(await readFile(join(complete.destination, "torrent-window.bin")))
      .digest("hex"),
    expected,
  );
  await until(() =>
    details.evaluate(
      `[...document.querySelectorAll('.torrent-details dd')].some(value=>value.textContent.trim()===${JSON.stringify(labels.seeding)})`,
    ),
  );
  await details.evaluate(
    "document.querySelector('.torrent-details').scrollIntoView({block:'start'});true",
  );
  await details.evaluate(
    "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))",
  );
  const fileRow = await details.evaluate(
    `(()=>{const row=document.querySelector('.torrent-details .collection-entry');const title=row?.querySelector(':scope > span');const detail=row?.querySelector('small');const a=row?.getBoundingClientRect(),b=title?.getBoundingClientRect(),c=detail?.getBoundingClientRect();const rules=[];function inspect(entries){for(const rule of entries){if(rule.selectorText&&row.matches(rule.selectorText))rules.push(rule.cssText);if(rule.cssRules)inspect(rule.cssRules);}}for(const sheet of document.styleSheets){try{inspect(sheet.cssRules);}catch{}}return {name:title?.textContent,direction:row&&getComputedStyle(row).flexDirection,visible:!!(a&&b&&c&&b.width>30&&b.top>=a.top&&b.bottom<=a.bottom&&c.left>=b.right-1),hash:!!row?.querySelector('.selection-integrity'),rules,stylesheets:[...document.styleSheets].map(sheet=>sheet.href)};})()`,
  );
  assert.equal(fileRow.name, "torrent-window.bin");
  assert.equal(fileRow.direction, "row", JSON.stringify(fileRow));
  assert.equal(fileRow.visible, true);
  assert.equal(fileRow.hash, true);
  const screenshot = await details.call("Page.captureScreenshot", {
    format: "png",
  });
  await writeFile(
    `docs/images/torrent-${JSON.parse(await readFile("package.json"))
      .version.split(".")
      .slice(0, 2)
      .join(".")}.png`,
    Buffer.from(screenshot.data, "base64"),
  );
  report = {
    version: JSON.parse(await readFile("package.json")).version,
    passed: true,
    native_local_import: true,
    native_v2_runtime_import: true,
    real_peer_wire: true,
    graceful_pause_resume: true,
    details_close_reopen: true,
    torrent_file_row_visible: true,
    sha256: expected,
    piece_requests: requests,
    render_sample: frames,
  };
  await writeFile(
    "docs/validation/torrent-window.json",
    JSON.stringify(report, null, 2) + "\n",
  );
} finally {
  driver.close();
  if (app.exitCode === null) {
    const shutdown = spawn(join(binary, "flux-desktop.exe"), ["--shutdown"], {
      env,
      windowsHide: true,
      stdio: "ignore",
    });
    await Promise.race([
      once(shutdown, "exit"),
      new Promise((resolve) => setTimeout(resolve, 10000)),
    ]);
  }
  if (app.exitCode === null)
    await Promise.race([
      once(app, "exit"),
      new Promise((resolve) => setTimeout(resolve, 10000)),
    ]);
  if (app.exitCode === null) app.kill();
  for (const socket of sockets) socket.destroy();
  await new Promise((resolve) => peer.close(resolve));
  restoreRegistration();
}
const cleanupWarnings = errors
  .split(/\r?\n/)
  .filter((line) =>
    line.includes(
      "Failed to unregister class Chrome_WidgetWin_0. Error = 1412",
    ),
  );
const unexpected = errors
  .split(/\r?\n/)
  .filter((line) => line && !cleanupWarnings.includes(line));
assert.deepEqual(unexpected, []);
assert.equal(app.exitCode, 0);
report.webview_cleanup_warning = cleanupWarnings.length > 0;
await writeFile(
  "docs/validation/torrent-window.json",
  JSON.stringify(report, null, 2) + "\n",
);
console.log(JSON.stringify(report));
