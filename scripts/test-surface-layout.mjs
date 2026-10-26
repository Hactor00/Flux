import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve, extname } from "node:path";
import { tmpdir } from "node:os";

// An isolated browser exercises the production modules with a local bridge fixture.
const directory = resolve("dist");
const output = resolve(".tools/surface-layout-validation");
await mkdir(output, { recursive: true });
const manifest = JSON.parse(await readFile("package.json", "utf8"));
const apiSource = await readFile("src/api.ts", "utf8");
const defaults = Function(
  "return (" +
    apiSource.match(
      /export const defaultSettings: Settings = (\{[\s\S]*?\n\});/,
    )[1] +
    ")",
)();
const languages = JSON.parse(
  await readFile("extension/languages.json", "utf8"),
);
const server = createServer(async (request, response) => {
  const path = resolve(
    directory,
    "." + new URL(request.url, "http://localhost").pathname,
  );
  if (!path.startsWith(directory + "/") && !path.startsWith(directory + "\\")) {
    response.writeHead(403).end();
    return;
  }
  try {
    const body = await readFile(path);
    response.setHeader(
      "Content-Type",
      {
        ".js": "text/javascript",
        ".html": "text/html",
        ".css": "text/css",
        ".json": "application/json",
        ".png": "image/png",
      }[extname(path)] || "application/octet-stream",
    );
    response.setHeader(
      "Content-Security-Policy",
      "default-src 'self'; script-src 'self'; object-src 'none'",
    );
    response.end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;
const root = await mkdtemp(join(tmpdir(), "flux-surface-layout-"));
const browser = spawn(
  process.argv[2] || "C:/Program Files/Google/Chrome/Application/chrome.exe",
  [
    "--headless=new",
    "--no-first-run",
    "--no-default-browser-check",
    `--user-data-dir=${join(root, "profile")}`,
    "--remote-debugging-port=0",
    "--remote-debugging-address=127.0.0.1",
    "about:blank",
  ],
  { windowsHide: true, stdio: "ignore" },
);
let socket,
  serial = 0;
const pending = new Map();
async function until(predicate, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) return value;
    } catch {}
    await new Promise((r) => setTimeout(r, 40));
  }
  if (socket?.readyState === WebSocket.OPEN) {
    console.error(
      await evaluate(
        "({url:location.href,text:document.body.innerText.slice(0,2500),errors:window.__errors})",
      ).catch(String),
    );
  }
  throw Error("Surface layout fixture timed out");
}
function call(method, params = {}) {
  return new Promise((yes, no) => {
    const id = ++serial;
    const timer = setTimeout(() => {
      pending.delete(id);
      no(Error(`${method} timed out`));
    }, 15000);
    pending.set(id, (message) => {
      clearTimeout(timer);
      pending.delete(id);
      message.error ? no(Error(message.error.message)) : yes(message.result);
    });
    socket.send(JSON.stringify({ id, method, params }));
  });
}
async function evaluate(expression) {
  const result = await call("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails)
    throw Error(
      result.exceptionDetails.exception?.description ||
        result.exceptionDetails.text,
    );
  return result.result.value;
}
async function click(selector) {
  await evaluate(
    `document.querySelector(${JSON.stringify(selector)}).scrollIntoView({block:'center',behavior:'instant'})`,
  );
  await evaluate(
    "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))",
  );
  const point = await evaluate(
    `(()=>{const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+r.height/2}})()`,
  );
  await call("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...point,
    button: "left",
    clickCount: 1,
  });
  await call("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...point,
    button: "left",
    clickCount: 1,
  });
}
async function screenshot(name) {
  await evaluate("scrollTo(0,0)");
  const { data } = await call("Page.captureScreenshot", { format: "png" });
  await writeFile(join(output, name), Buffer.from(data, "base64"));
}
const setup = `
window.isTauri=true; window.__fixtureURL=location.href; window.__sizes=[]; window.__errors=[]; let callback=0;
addEventListener('error',e=>__errors.push(e.message));
addEventListener('unhandledrejection',e=>__errors.push(String(e.reason)));
const params=new URLSearchParams(location.search), view=params.get('view');
const settings={...${JSON.stringify(defaults)},locale:params.get('locale')||'tr-TR',theme:'dark',download_directory:'C:\\\\Downloads\\\\Flux'};
const task={id:'11111111-1111-4111-8111-111111111111',status:view==='complete'?'complete':'queued',request:{url:'https://files.test/Flux.zip',filename:'Flux-window-layout-test.zip',connections:0,speed_limit:0,expected_sha256:null,auto_rename:true,priority:0},destination:'C:\\\\Downloads\\\\Flux\\\\Flux-window-layout-test.zip',completed:0,total:179602,sha256:null,error:null,created_at:1793000000};
window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:view?view+'-new':'main'},currentWebview:{label:view?view+'-new':'main'}},transformCallback:()=>++callback,unregisterCallback:()=>{},invoke:async(command,args={})=>{
 if(command==='resize_surface'){__sizes.push(args.height);return null;}
 if(command==='get_settings'||command==='save_settings')return settings;
 if(command==='get_task'){await new Promise(r=>setTimeout(r,100));return task;}
 if(command==='list_tasks'||command==='host_accounts')return [];
 if(command==='inspect_source')return {size:179602,estimated:false,content_type:'application/zip',files:[]};
 if(command==='suggest_directory')return settings.download_directory;
 if(command==='pending_power')return {action:'exit_app',deadline:Date.now()+30000};
 if(command==='runtime_platform')return 'windows';
 if(command==='installation_mode')return 'installed';
 if(command==='plugin:app|version'||command==='latest_release_version')return '${manifest.version}';
 if(command==='plugin:updater|check')return params.has('update')?{rid:7,currentVersion:'${manifest.version}',version:'${manifest.version.split(".").slice(0, 2).join(".")}.999',body:'Window layout verification. '.repeat(100)}:null;
 if(command==='browser_integration')return {registered:true,connected:false,extension_versions:[],extension_id:null,error:null,reminder_dismissed:true};
 if(command==='statistics_status')return {choice:params.has('consent')?'unasked':'never',enabled:false,prompt_due:params.has('consent'),remind_after:0,last_success:null,deletion_pending:false,notice_version:'2026-10-26'};
 if(command==='torrent_watch_report')return {imported:0,error:null};
 if(command.startsWith('plugin:event|'))return ++callback;
 return null;
}};`;
async function viewport(width, height) {
  await call("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  });
}
async function navigate(
  view,
  locale = "tr-TR",
  width = 640,
  height = 320,
  extra = "",
) {
  await viewport(width, height);
  const query = new URLSearchParams({ locale });
  if (view) query.set("view", view);
  if (["download", "details", "complete"].includes(view))
    query.set("id", "11111111-1111-4111-8111-111111111111");
  const url = base + "/index.html?" + query + extra;
  await call("Page.navigate", { url });
  await until(() =>
    evaluate(
      `window.__fixtureURL===${JSON.stringify(url)} && document.querySelector('${view ? "[role=dialog]" : ".desktop-shell"}') && document.documentElement.lang===${JSON.stringify(locale)}`,
    ),
  );
  await evaluate(
    "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))",
  );
}
async function fit(view, width = 640) {
  await until(() => evaluate("__sizes.length>0"));
  const requested = await evaluate("__sizes.at(-1)");
  const height = Math.max(
    view === "complete" ? 300 : 220,
    Math.min(720, Math.ceil(requested)),
  );
  await viewport(width, height);
  return height;
}
async function assertFooter(selector) {
  const box = await evaluate(
    `(()=>{const e=document.querySelector(${JSON.stringify(selector)}),r=e.getBoundingClientRect();return {top:r.top,bottom:r.bottom,left:r.left,right:r.right,width:innerWidth,height:innerHeight}})()`,
  );
  assert.ok(
    box.top >= 0 &&
      box.bottom <= box.height + 2 &&
      box.left >= 0 &&
      box.right <= box.width + 2,
    JSON.stringify(box),
  );
}
try {
  const active = await until(
    async () =>
      (
        await readFile(join(root, "profile", "DevToolsActivePort"), "utf8")
      ).split("\n"),
    60000,
  );
  const targets = await (
    await fetch(`http://127.0.0.1:${active[0]}/json/list`)
  ).json();
  socket = new WebSocket(
    targets.find((t) => t.type === "page").webSocketDebuggerUrl,
  );
  await once(socket, "open");
  socket.addEventListener("message", (event) => {
    const m = JSON.parse(event.data);
    if (m.id) pending.get(m.id)?.(m);
  });
  await call("Page.enable");
  await call("Page.bringToFront");
  await call("Page.addScriptToEvaluateOnNewDocument", { source: setup });
  for (const { code } of languages) {
    await navigate("download", code);
    const height = await fit("download");
    assert.ok(height <= 340, code + ": compact download grew too tall");
    await assertFooter(".download-actions [type=submit]");
    assert.ok(
      await evaluate(
        "document.querySelector('.download-actions .primary').getBoundingClientRect().width<300",
      ),
      code + ": full-width action",
    );
    assert.ok(
      await evaluate(
        "document.querySelector('.modal').scrollHeight<=innerHeight+2",
      ),
      code + ": initial scrollbar",
    );
    assert.deepEqual(await evaluate("__errors"), []);
  }
  await navigate("download");
  await fit("download");
  await screenshot("download-compact.png");
  const compact = await evaluate("innerHeight");
  await click(".download-details-button");
  await until(() => evaluate(`__sizes.at(-1)>${compact + 80}`));
  await fit("download");
  const checkbox = await evaluate(
    "(()=>{const r=document.querySelector('.queue-later input').getBoundingClientRect();return {width:r.width,height:r.height,direction:getComputedStyle(document.querySelector('.queue-later')).flexDirection}})()",
  );
  assert.ok(
    checkbox.width <= 20 &&
      checkbox.height <= 20 &&
      checkbox.direction === "row",
    JSON.stringify(checkbox),
  );
  await evaluate(
    "document.querySelector('.modal').scrollTop=document.querySelector('.modal').scrollHeight",
  );
  await assertFooter(".download-actions [type=submit]");
  await screenshot("download-expanded.png");
  await evaluate("document.querySelector('.download-details-button').click()");
  await until(() => evaluate(`__sizes.at(-1)<=${compact + 2}`));
  await fit("download");
  await assertFooter(".download-actions [type=submit]");
  for (const view of ["details", "complete", "power"]) {
    await navigate(view);
    await fit(view, view === "complete" || view === "power" ? 520 : 640);
    await assertFooter(
      view === "details"
        ? ".transfer-actions"
        : view === "complete"
          ? ".download-complete .details-actions"
          : ".power-countdown .primary",
    );
    assert.deepEqual(await evaluate("__errors"), []);
    await screenshot(view + ".png");
  }
  await navigate("details", "tr-TR", 540, 280);
  await click(".transfer-details-button");
  await until(() =>
    evaluate("!!document.querySelector('.transfer-actions .danger')"),
  );
  await click(".transfer-actions .danger");
  await until(() => evaluate("!!document.querySelector('.confirm-modal')"));
  await assertFooter(".confirm-modal .dialog-actions");
  assert.ok(
    await evaluate(
      "document.querySelector('.confirm-modal').getBoundingClientRect().bottom<=innerHeight+2",
    ),
  );
  await screenshot("confirm.png");
  for (const [width, height] of [
    [800, 480],
    [1200, 800],
    [1024, 560],
  ]) {
    await navigate(null, "tr-TR", width, height);
    await assertFooter(".desktop-status");
    assert.ok(
      await evaluate(
        "document.documentElement.scrollWidth<=innerWidth && document.documentElement.scrollHeight<=innerHeight+2",
      ),
      "Main window overflow",
    );
    assert.deepEqual(await evaluate("__errors"), []);
  }
  await screenshot("main.png");
  await navigate("settings", "tr-TR", 680, 660);
  await until(() =>
    evaluate("!!document.querySelector('.settings-footer button')"),
  );
  const tabs = await evaluate(
    "[...document.querySelectorAll('[role=tab]')].map(e=>e.id)",
  );
  for (const id of tabs) {
    await click(`[id=${JSON.stringify(id)}]`);
    await until(() =>
      evaluate(
        `document.getElementById(${JSON.stringify(id)}).getAttribute('data-state')==='active'`,
      ),
    );
    await assertFooter(".settings-footer");
    assert.ok(
      await evaluate("document.documentElement.scrollWidth<=innerWidth"),
      "Settings horizontal overflow",
    );
    assert.deepEqual(await evaluate("__errors"), []);
  }
  await screenshot("settings.png");
  await navigate("settings", "tr-TR", 680, 660, "&update=1");
  await until(() =>
    evaluate("!!document.querySelector('[role=tab][id$=trigger-updates]')"),
  );
  await click("[role=tab][id$=trigger-updates]");
  await until(() =>
    evaluate("!!document.querySelector('.updater-panel button')"),
  );
  await click(".updater-panel button");
  await until(() => evaluate("!!document.querySelector('.updater-modal')"));
  await viewport(640, 420);
  await assertFooter(".updater-modal .dialog-actions");
  await screenshot("updater.png");
  await navigate(null, "tr-TR", 640, 420, "&consent=1");
  await until(() => evaluate("!!document.querySelector('.usage-modal')"));
  await evaluate(
    "document.querySelector('.usage-actions').scrollIntoView({block:'end'})",
  );
  await assertFooter(".usage-actions");
  assert.deepEqual(await evaluate("__errors"), []);
  await screenshot("consent.png");
  const report = {
    version: manifest.version,
    passed: true,
    locales: languages.length,
    portal_resize_attached: true,
    compact_footer_visible: true,
    expand_collapse_fits: true,
    checkbox_size_verified: true,
    surfaces: [
      "download",
      "details",
      "complete",
      "power",
      "settings",
      "updater",
      "consent",
      "confirm",
      "main",
    ],
    scope:
      "Production UI in isolated Chromium with fixture native IPC; native geometry is checked separately.",
  };
  await writeFile(
    join(output, "report.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  await mkdir("docs/validation", { recursive: true });
  await writeFile(
    "docs/validation/surface-layout.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
  socket?.close();
  browser.kill();
  server.closeAllConnections();
  await new Promise((r) => server.close(r));
}
