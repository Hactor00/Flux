import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

const executable = resolve(
  process.argv[2] || "C:/Program Files/Google/Chrome/Application/chrome.exe",
);
const root = await mkdtemp(join(tmpdir(), "flux-chrome-hud-"));
const source = await readFile("extension/content.js", "utf8");
const words = JSON.parse(
  await readFile("extension/locales/en-US.json", "utf8"),
);
const fixture = createServer((request, response) => {
  response.setHeader("Content-Type", "text/html");
  response.setHeader(
    "Content-Security-Policy",
    "default-src 'none'; style-src 'none'; script-src 'none'",
  );
  response.end(
    "<!doctype html><title>HUD layout fixture</title><video width='420' height='720'></video>",
  );
});
await new Promise((r) => fixture.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${fixture.address().port}`;
const processHandle = spawn(
  executable,
  [
    "--headless=new",
    "--no-first-run",
    "--no-default-browser-check",
    `--user-data-dir=${join(root, "profile")}`,
    "--remote-debugging-port=0",
    "--remote-debugging-address=127.0.0.1",
    "--window-size=1430,924",
    base,
  ],
  { windowsHide: true, stdio: ["ignore", "ignore", "pipe"] },
);
let socket,
  serial = 0;
let browserErrors = "";
processHandle.stderr.on("data", (chunk) => {
  browserErrors = (browserErrors + chunk.toString()).slice(-8192);
});
const pending = new Map();
async function until(predicate, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      const result = await predicate();
      if (result) return result;
    } catch {}
    await new Promise((r) => setTimeout(r, 50));
  }
  throw Error(`Chrome HUD condition timed out (exit=${processHandle.exitCode}): ${browserErrors}`);
}
function call(method, params = {}) {
  return new Promise((resolveResult, reject) => {
    const id = ++serial;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(Error(`${method} timed out`));
    }, 15000);
    pending.set(id, (message) => {
      clearTimeout(timer);
      pending.delete(id);
      message.error
        ? reject(Error(message.error.message))
        : resolveResult(message.result);
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
const setup = `window.__roots=[];window.__calls=[];window.__saved={};window.__errors=[];
addEventListener('error',event=>window.__errors.push(event.message));
const original=Element.prototype.attachShadow;Element.prototype.attachShadow=function(options){const root=original.call(this,options);window.__roots.push(root);return root;};
const words=${JSON.stringify(words)};
const video=document.querySelector('video');video.src='http://127.0.0.1/video.mp4';video.style.cssText='position:fixed;left:500px;top:80px;width:420px;height:720px';
window.chrome={storage:{local:{get:async key=>({[key]:window.__saved[key]}),set:async value=>Object.assign(window.__saved,value)}},i18n:{getMessage:key=>words[key]||key},runtime:{onMessage:{addListener(){}},sendMessage:async message=>{window.__calls.push(message);let result;if(message.type==='dictionary')result=words;if(message.type==='candidates')result=[];if(message.type==='inspect')result={streams:[{codec_type:'video',height:1080,index:0,codec_name:'h264'}],subtitles:[],chapters:[]};if(message.type==='enqueue')result={id:'layout-proposal'};return {ok:true,result};}}};`;
const geometry = `(()=>{const root=window.__roots[0],host=root.host,panel=root.querySelector('.panel'),bar=root.querySelector('.bar'),main=root.querySelector('.main'),button=root.querySelector('.download');const rect=el=>{const r=el.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};return {host:rect(host),panel:rect(panel),bar:rect(bar),main:rect(main),download:rect(button),grip:rect(root.querySelector('.move')),panelHidden:panel.hidden,ready:!button.disabled,hostOverflow:getComputedStyle(host).overflow,panelOverflow:getComputedStyle(panel).overflowY,hostScroll:host.scrollHeight,hostClient:host.clientHeight,status:root.querySelector('[role=status]').textContent,width:innerWidth,height:innerHeight,saved:window.__saved,errors:window.__errors};})()`;
function validatePanel(value) {
  assert.equal(value.panelHidden, false);
  assert.equal(value.ready, true);
  assert.equal(value.hostOverflow, "visible");
  assert.ok(value.panel.height > 170, JSON.stringify(value));
  assert.ok(value.panel.left >= 7 && value.panel.right <= value.width - 7);
  assert.ok(value.panel.top >= 7 && value.panel.bottom <= value.height - 7);
  assert.ok(
    value.panel.top >= value.bar.bottom || value.panel.bottom <= value.bar.top,
  );
  assert.ok(
    value.host.height < 60 &&
      Math.abs(value.host.height - value.bar.height) < 1,
    JSON.stringify(value),
  );
  assert.equal(value.status, words.mediaReady);
  assert.deepEqual(value.errors, []);
}
async function clickSelector(selector) {
  await evaluate(
    `window.__roots[0].querySelector(${JSON.stringify(selector)}).click()`,
  );
}
async function dragTo(x, y) {
  const before = await evaluate(geometry);
  const startX = before.grip.left + before.grip.width / 2,
    startY = before.grip.top + before.grip.height / 2;
  await call("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: startX,
    y: startY,
  });
  await call("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: startX,
    y: startY,
    button: "left",
    clickCount: 1,
  });
  await call("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x,
    y,
    button: "left",
    buttons: 1,
  });
  await call("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x,
    y,
    button: "left",
    clickCount: 1,
  });
  await new Promise((r) => setTimeout(r, 120));
  return evaluate(geometry);
}
try {
  const active = await until(async () =>
    (await readFile(join(root, "profile", "DevToolsActivePort"), "utf8")).split(
      "\n",
    ),
    60000,
  );
  const targets = await (
    await fetch(`http://127.0.0.1:${active[0]}/json/list`)
  ).json();
  const target = targets.find((target) => target.type === "page");
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await once(socket, "open");
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id) pending.get(message.id)?.(message);
  });
  await until(() => evaluate("!!document.querySelector('video')"));
  const version = await call("Browser.getVersion");
  await evaluate(setup);
  await evaluate(source);
  await until(() =>
    evaluate(
      "window.__roots.length===1 && window.__roots[0].host.matches(':popover-open')",
    ),
  );
  await clickSelector(".main");
  await until(() =>
    evaluate("!window.__roots[0].querySelector('.download').disabled"),
  );
  await new Promise((r) => setTimeout(r, 100));
  const initial = await evaluate(geometry);
  validatePanel(initial);
  const left = await dragTo(20, 20);
  assert.ok(left.host.left < 30 && left.host.top < 30, JSON.stringify(left));
  assert.equal(left.saved["hud-position:127.0.0.1"].space, "viewport");
  await clickSelector(".main");
  await new Promise((r) => setTimeout(r, 100));
  validatePanel(await evaluate(geometry));
  const right = await dragTo(initial.width - 10, initial.height - 10);
  assert.ok(
    right.host.right >= initial.width - 30 &&
      right.host.bottom >= initial.height - 30,
  );
  await clickSelector(".main");
  await new Promise((r) => setTimeout(r, 100));
  const bottomPanel = await evaluate(geometry);
  validatePanel(bottomPanel);
  assert.ok(bottomPanel.panel.bottom < bottomPanel.bar.top);
  await call("Page.captureScreenshot", { format: "png" }).then(
    async ({ data }) =>
      writeFile(
        join(root, "chrome-hud-bottom.png"),
        Buffer.from(data, "base64"),
      ),
  );
  await evaluate(
    "document.querySelector('video').style.left='10px';document.querySelector('video').style.width='240px'",
  );
  await call("Emulation.setDeviceMetricsOverride", {
    width: 360,
    height: 640,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await new Promise((r) => setTimeout(r, 150));
  validatePanel(await evaluate(geometry));
  await evaluate("document.body.click()");
  assert.equal((await evaluate(geometry)).panelHidden, true);
  await clickSelector(".main");
  await new Promise((r) => setTimeout(r, 100));
  await clickSelector(".download");
  assert.ok(
    await evaluate(
      "window.__calls.some(call=>call.type==='enqueue'&&call.options.filename==='HUD layout fixture.mp4')",
    ),
  );
  await clickSelector(".close");
  assert.equal(await evaluate("window.__roots[0].host.style.display"), "none");
  const report = {
    browser: version.product,
    passed: true,
    test_scope:
      "Production HUD source and real native browser layout with an isolated bridge fixture",
    free_viewport_drag: true,
    all_viewport_edges: true,
    popover_unclipped: true,
    toolbar_not_scrolled: true,
    upward_panel: true,
    narrow_viewport: true,
    closed_shadow_dom: true,
    strict_csp: true,
    outside_click: true,
    enqueue_selection: true,
    close_button: true,
    screenshot: join(root, "chrome-hud-bottom.png"),
    initial,
    bottomPanel,
  };
  await mkdir(".tools/hud-validation", { recursive: true });
  await writeFile(
    ".tools/hud-validation/chrome.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(
    JSON.stringify({
      browser: report.browser,
      passed: true,
      screenshot: report.screenshot,
    }),
  );
} finally {
  socket?.close();
  processHandle.kill();
  fixture.closeAllConnections();
  await new Promise((r) => fixture.close(r));
}
