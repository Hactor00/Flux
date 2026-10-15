import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { createServer } from "node:http";
import { mkdir, mkdtemp, writeFile, readdir, readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

const executable =
  process.argv[2] || "C:/Program Files/Google/Chrome/Application/chrome.exe";
const extension = resolve(process.argv[3] || "extension");
const root = await mkdtemp(join(tmpdir(), "flux-chrome-capture-"));
const profile = join(root, "profile");
const downloads = join(root, "browser-downloads");
await mkdir(join(profile, "Default"), { recursive: true });
await mkdir(downloads);
await writeFile(
  join(profile, "Default", "Preferences"),
  JSON.stringify({
    download: { prompt_for_download: true, default_directory: downloads },
    browser: { check_default_browser: false },
  }),
);
const payload = Buffer.alloc(1024 * 1024, 87);
const server = createServer(async (request, response) => {
  if (request.url === "/page") {
    response.setHeader("Content-Type", "text/html");
    return response.end(
      '<!doctype html><title>Download interception test</title><a id="file" href="/fixture.bin">Download</a><video controls width="640" height="360" src="/media.mp4"></video>',
    );
  }
  if (request.url === "/media.mp4") {
    const bytes = await readFile(".tools/firefox/media.mp4");
    response.writeHead(200, {
      "Content-Type": "video/mp4",
      "Content-Length": bytes.length,
    });
    return response.end(bytes);
  }
  if (request.url !== "/fixture.bin") {
    response.writeHead(404);
    return response.end();
  }
  response.writeHead(200, {
    "Content-Type": "application/octet-stream",
    "Content-Disposition": 'attachment; filename="fixture.bin"',
    "Content-Length": payload.length,
  });
  const timer = setTimeout(() => response.end(payload), 2000);
  response.on("close", () => clearTimeout(timer));
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const url = `http://127.0.0.1:${server.address().port}`;
const child = spawn(
  executable,
  [
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-background-networking",
    `--user-data-dir=${profile}`,
    "--remote-debugging-pipe",
    "--enable-unsafe-extension-debugging",
    "--window-position=-32000,-32000",
    "about:blank",
  ],
  { windowsHide: true, stdio: ["ignore", "ignore", "pipe", "pipe", "pipe"] },
);
let serial = 0,
  incoming = Buffer.alloc(0),
  stderr = "";
const pending = new Map();
child.stderr.on("data", (bytes) => {
  stderr = (stderr + bytes).slice(-8192);
});
child.stdio[4].on("data", (bytes) => {
  incoming = Buffer.concat([incoming, bytes]);
  for (;;) {
    const end = incoming.indexOf(0);
    if (end < 0) break;
    const message = JSON.parse(incoming.subarray(0, end));
    incoming = incoming.subarray(end + 1);
    if (message.id) pending.get(message.id)?.(message);
  }
});
function call(method, params = {}, sessionId) {
  return new Promise((done, reject) => {
    const id = ++serial;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(Error(`${method} timed out: ${stderr}`));
    }, 20000);
    pending.set(id, (message) => {
      clearTimeout(timer);
      pending.delete(id);
      message.error
        ? reject(Error(JSON.stringify(message.error)))
        : done(message.result);
    });
    child.stdio[3].write(
      JSON.stringify({
        id,
        method,
        params,
        ...(sessionId ? { sessionId } : {}),
      }) + "\0",
    );
  });
}
async function evaluate(expression, sessionId) {
  const result = await call(
    "Runtime.evaluate",
    { expression, awaitPromise: true, returnByValue: true },
    sessionId,
  );
  if (result.exceptionDetails)
    throw Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
async function until(callback, milliseconds = 20000) {
  const deadline = Date.now() + milliseconds;
  while (Date.now() < deadline) {
    const value = await callback();
    if (value) return value;
    await new Promise((done) => setTimeout(done, 50));
  }
  throw Error("Chrome interception did not finish");
}
try {
  const version = await call("Browser.getVersion");
  const { id } = await call("Extensions.loadUnpacked", { path: extension });
  const inventory = await call("Extensions.getExtensions");
  assert.deepEqual(
    inventory.extensions.filter((item) => item.enabled).map((item) => item.id),
    [id],
  );
  const { targetId: extensionPage } = await call("Target.createTarget", {
    url: `chrome-extension://${id}/options.html`,
  });
  const { sessionId: background } = await call("Target.attachToTarget", {
    targetId: extensionPage,
    flatten: true,
  });
  await call("Runtime.enable", {}, background);
  await until(() => evaluate("!!globalThis.chrome?.storage", background));
  await evaluate(
    "chrome.storage.local.set({interceptDownloads:true})",
    background,
  );
  await evaluate(
    "(async()=>{const {NativeBridge}=await import(chrome.runtime.getURL('bridge.js'));const bridge=new NativeBridge();try{return await bridge.request('list_page',{offset:0,limit:1});}finally{bridge.disconnect();}})()",
    background,
  );
  const { targetId } = await call("Target.createTarget", {
    url: url + "/page",
  });
  const { sessionId: page } = await call("Target.attachToTarget", {
    targetId,
    flatten: true,
  });
  await until(() => evaluate("!!document.querySelector('#file')", page));
  const original = await evaluate(
    "chrome.storage.local.get('lastTransfer').then(x=>x.lastTransfer?.id||null)",
    background,
  );
  await evaluate("document.querySelector('#file').click()", page);
  const result = await until(() =>
    evaluate(
      `chrome.storage.local.get('lastTransfer').then(x=>x.lastTransfer?.id!==${JSON.stringify(original)}&&x.lastTransfer?.ok?x.lastTransfer:null)`,
      background,
    ),
  ).catch(async (error) => {
    const state = await evaluate(
      "Promise.all([chrome.storage.local.get(['lastTransfer','outbox','browserDrafts']),chrome.downloads.search({})])",
      background,
    );
    throw Error(error.message + ": " + JSON.stringify(state));
  });
  const browserItems = await evaluate(
    `chrome.downloads.search({url:${JSON.stringify(url + "/fixture.bin")}})`,
    background,
  );
  assert.ok(
    !browserItems.some(
      (item) => item.state === "in_progress" || item.state === "complete",
    ),
  );
  assert.deepEqual(await readdir(downloads), []);
  const task = await evaluate(
    `(async()=>{const {NativeBridge}=await import(chrome.runtime.getURL('bridge.js'));const bridge=new NativeBridge();try{return await bridge.request('task',{id:${JSON.stringify(result.id)}});}finally{bridge.disconnect();}})()`,
    background,
  );
  assert.equal(task.status, "awaiting_approval");
  await new Promise((done) => setTimeout(done, 1600));
  const windows = JSON.parse(
    (
      await promisify(execFile)(
        "powershell.exe",
        [
          "-NoProfile",
          "-NonInteractive",
          "-File",
          resolve("scripts/browser-dialogs.ps1"),
          "-BrowserProcessId",
          String(child.pid),
        ],
        { windowsHide: true },
      )
    ).stdout,
  );
  assert.deepEqual(
    windows.dialogs,
    [],
    "Chrome Save As must not remain open after handoff",
  );
  assert.ok(
    windows.foreground.startsWith("Flux · "),
    "Flux approval must appear in the foreground: " + JSON.stringify(windows),
  );
  const mediaRoot = await until(async () => {
    const tree = await call(
      "DOM.getDocument",
      { depth: -1, pierce: true },
      page,
    );
    function find(node) {
      if (
        node.attributes?.includes("data-flux-hud") &&
        node.shadowRoots?.length
      )
        return node.shadowRoots[0].nodeId;
      for (const child of [
        ...(node.children || []),
        ...(node.shadowRoots || []),
      ]) {
        const found = find(child);
        if (found) return found;
      }
    }
    return find(tree.root);
  });
  const shadow = await call("DOM.resolveNode", { nodeId: mediaRoot }, page);
  async function hud(functionDeclaration) {
    const result = await call(
      "Runtime.callFunctionOn",
      {
        objectId: shadow.object.objectId,
        functionDeclaration,
        awaitPromise: true,
        returnByValue: true,
      },
      page,
    );
    if (result.exceptionDetails)
      throw Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
  await hud("function(){this.querySelector('.main').click();return true;}");
  await until(
    () => hud("function(){return !this.querySelector('.download').disabled;}"),
    60000,
  );
  await hud("function(){this.querySelector('.download').click();return true;}");
  const mediaResult = await until(() =>
    evaluate(
      `chrome.storage.local.get('lastTransfer').then(x=>x.lastTransfer?.id!==${JSON.stringify(result.id)}&&x.lastTransfer?.ok?x.lastTransfer:null)`,
      background,
    ),
  );
  await until(() =>
    hud("function(){return this.querySelector('.panel').hidden;}"),
  );
  const mediaTask = await evaluate(
    `(async()=>{const {NativeBridge}=await import(chrome.runtime.getURL('bridge.js'));const bridge=new NativeBridge();try{return await bridge.request('task',{id:${JSON.stringify(mediaResult.id)}});}finally{bridge.disconnect();}})()`,
    background,
  );
  assert.equal(mediaTask.status, "awaiting_approval");
  assert.equal(mediaTask.request.url, url + "/media.mp4");
  assert.equal(mediaTask.request.media.container, "mp4");
  const report = {
    version: JSON.parse(await readFile("package.json")).version,
    passed: true,
    browser: version.product,
    production_extension: extension,
    browser_prompt_for_download: true,
    attachment_navigation: true,
    browser_transfer_cancelled: true,
    browser_directory_empty: true,
    native_approval_created: true,
    no_browser_file_chooser: true,
    native_approval_foreground: true,
    hud_media_native_acknowledged: true,
    external_download_manager_isolated: true,
    task_id: task.id,
  };
  await mkdir("docs/validation", { recursive: true });
  await writeFile(
    "docs/validation/chrome-interception.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
  await call("Browser.close").catch(() => child.kill());
  server.closeAllConnections();
  await new Promise((done) => server.close(done));
  if (process.platform === "win32" && process.argv[4]) {
    await promisify(execFile)(
      join(process.argv[4], "flux-desktop.exe"),
      ["--shutdown"],
      { windowsHide: true },
    ).catch(() => {});
  }
}
