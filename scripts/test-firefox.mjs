import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import {
  cp,
  mkdtemp,
  mkdir,
  readFile,
  writeFile,
  access,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { once } from "node:events";

const binary = resolve(process.argv[2] || "target/release");
const firefox = resolve(process.argv[3]);
const driver = resolve(process.argv[4]);
const root = await mkdtemp(join(tmpdir(), "flux-firefox-"));
const data = join(root, "Flux"),
  addon = join(root, "addon"),
  output = join(root, "output");
await mkdir(data);
await mkdir(output);
const extensionDirectory = process.argv.includes("--modern")
  ? "extension-firefox-modern"
  : "extension-firefox";
let addonSource = join(binary, extensionDirectory);
await access(addonSource).catch(() => {
  addonSource = resolve(extensionDirectory);
});
await cp(addonSource, addon, { recursive: true });
const token = randomUUID(),
  commands = [],
  reports = [];
let controlRequests = 0;
const payload = Buffer.alloc(1024 * 1024, 83);
const sha256 = createHash("sha256").update(payload).digest("hex");
const fixture = createServer(async (request, response) => {
  response.setHeader("Access-Control-Allow-Origin", "*");
  const url = new URL(request.url, "http://fixture");
  if (url.pathname === `/control/${token}`) {
    controlRequests++;
    response.setHeader("Cache-Control", "no-store");
    response.setHeader("Content-Type", "application/json");
    return response.end(JSON.stringify(commands.shift() || null));
  }
  if (url.pathname === `/report/${token}`) {
    const chunks = [];
    for await (const bytes of request) chunks.push(bytes);
    reports.push(JSON.parse(Buffer.concat(chunks)));
    return response.end("OK");
  }
  if (url.pathname === "/page") {
    response.writeHead(200, {
      "Content-Type": "text/html",
      "Set-Cookie": "session=firefox-fixture; HttpOnly; SameSite=Lax",
      "Content-Security-Policy":
        "default-src 'none'; script-src 'self'; media-src 'self'; style-src 'none'; connect-src 'none'",
    });
    return response.end(
      '<!doctype html><title>Firefox media fixture</title><button id="fullscreen">Fullscreen</button><video width="640" height="360" controls src="/media.mp4"></video><script src="/page.js"></script>',
    );
  }
  if (url.pathname === "/page.js") {
    response.setHeader("Content-Type", "application/javascript");
    return response.end(
      'document.querySelector("#fullscreen").onclick=()=>document.querySelector("video").parentElement.requestFullscreen();',
    );
  }
  if (url.pathname === "/media.mp4") {
    const bytes = await readFile(".tools/firefox/media.mp4");
    response.writeHead(200, {
      "Content-Type": "video/mp4",
      "Content-Length": bytes.length,
    });
    return response.end(bytes);
  }
  if (!["/fixture.bin", "/native.bin"].includes(url.pathname)) {
    response.writeHead(404);
    return response.end();
  }
  if (request.headers.cookie !== "session=firefox-fixture") {
    response.writeHead(401);
    return response.end();
  }
  const range = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range || "");
  const start = range ? +range[1] : 0,
    end = range?.[2] ? +range[2] : payload.length - 1;
  response.writeHead(range ? 206 : 200, {
    "Content-Type": "application/octet-stream",
    "Content-Disposition": "attachment; filename=fixture.bin",
    "Content-Length": end - start + 1,
    ETag: '"firefox-v1"',
    ...(range
      ? { "Content-Range": `bytes ${start}-${end}/${payload.length}` }
      : {}),
  });
  let cursor = start;
  const timer = setInterval(() => {
    const next = Math.min(cursor + 32768, end + 1);
    response.write(payload.subarray(cursor, next));
    cursor = next;
    if (cursor > end) {
      clearInterval(timer);
      response.end();
    }
  }, 20);
  response.on("close", () => clearInterval(timer));
});
await new Promise((r) => fixture.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${fixture.address().port}`;
const manifestPath = join(data, "io.flux.download.firefox.json");
await writeFile(
  manifestPath,
  JSON.stringify({
    name: "io.flux.download",
    type: "stdio",
    description: "Flux Native Messaging Bridge for Firefox",
    path: join(binary, "flux-native-host.exe"),
    allowed_extensions: ["flux@flux-download.local"],
  }),
);
function registry(mode, value) {
  const code =
    mode === "read"
      ? '$k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\\Mozilla\\NativeMessagingHosts\\io.flux.download"); if($k){try{ConvertTo-Json -Compress @{exists=$true;value=$k.GetValue("")}}finally{$k.Dispose()}}else{\'{"exists":false}\'}'
      : mode === "write"
        ? '$k=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("Software\\Mozilla\\NativeMessagingHosts\\io.flux.download");try{$k.SetValue("",$env:FLUX_TEST_MANIFEST)}finally{$k.Dispose()}'
        : '[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey("Software\\Mozilla\\NativeMessagingHosts\\io.flux.download",$false)';
  const result = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-Command", code],
    {
      encoding: "utf8",
      windowsHide: true,
      env: { ...process.env, FLUX_TEST_MANIFEST: value || "" },
    },
  );
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}
const previous = JSON.parse(registry("read"));
const environment = { ...process.env, LOCALAPPDATA: root };
let gecko, cli, session, driverPort, validation;
async function until(predicate, timeout = 20000) {
  const deadline = Date.now() + timeout;
  do {
    const result = await predicate();
    if (result) return result;
    await new Promise((r) => setTimeout(r, 100));
  } while (Date.now() < deadline);
  throw Error(
    "Firefox assertion timed out: " +
      JSON.stringify({
        reports: reports.slice(-12),
        controlRequests,
        pending: commands,
      }),
  );
}
async function wd(method, path, body) {
  const response = await fetch(`http://127.0.0.1:${driverPort}${path}`, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(30000),
  });
  const result = await response.json();
  if (result.value?.error) throw Error(JSON.stringify(result.value));
  return result.value;
}
function command(action, payload = {}) {
  const id = randomUUID();
  commands.push({ id, action, ...payload });
  return until(() => reports.find((r) => r.id === id)).then((result) => {
    assert.equal(result.ok, true, result.error);
    return result.result;
  });
}
try {
  registry("write", manifestPath);
  cli = spawn(
    join(binary, "flux-cli.exe"),
    [
      "--data",
      data,
      "--output",
      output,
      "serve",
      "--shutdown-file",
      join(root, "stop"),
    ],
    {
      env: environment,
      windowsHide: true,
      stdio: ["ignore", "ignore", "pipe"],
    },
  );
  let cliError = "";
  cli.stderr.on("data", (b) => (cliError += b));
  await until(async () => {
    try {
      return JSON.parse(await readFile(join(data, "broker.json")));
    } catch {
      return false;
    }
  });
  const portServer = createServer();
  await new Promise((r) => portServer.listen(0, "127.0.0.1", r));
  driverPort = portServer.address().port;
  await new Promise((r) => portServer.close(r));
  gecko = spawn(driver, ["--port", String(driverPort)], {
    env: environment,
    windowsHide: true,
    stdio: ["ignore", "ignore", "pipe"],
  });
  let driverErrors = "";
  gecko.stderr.on("data", (b) => (driverErrors += b));
  await until(async () => {
    try {
      return (await wd("GET", "/status")).ready;
    } catch {
      return false;
    }
  });
  session = (
    await wd("POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "firefox",
          "moz:firefoxOptions": {
            binary: firefox,
            args: ["-headless", "-remote-allow-system-access"],
            prefs: {
              "browser.shell.checkDefaultBrowser": false,
              "browser.startup.page": 0,
              "browser.download.useDownloadDir": false,
              "browser.download.folderList": 2,
              "browser.download.dir": join(root, "browser-downloads"),
              "datareporting.policy.dataSubmissionEnabled": false,
              "toolkit.telemetry.enabled": false,
            },
          },
        },
      },
    })
  ).sessionId;
  const prefix = `/session/${session}`;
  const helper = `import './background.js';import { NativeBridge } from './bridge.js';import { makeInput } from './shared.js';
const bridge=new NativeBridge();const base=${JSON.stringify(base)}, token=${JSON.stringify(token)};
async function report(data){await fetch(base+'/report/'+token,{method:'POST',body:JSON.stringify(data)});}
void browser.runtime.getBrowserInfo().then(info=>report({kind:'ready',info,extensionUrl:browser.runtime.getURL('options.html')}));
let busy=false;setInterval(async()=>{if(busy)return;busy=true;try{const c=await(await fetch(base+'/control/'+token)).json();if(!c)return;await report({kind:'received',command:c});try{let result;
if(c.action==='denied'){try{await bridge.request('list');throw Error('Native access preceded consent');}catch(e){if(!e.message.includes('Enable local'))throw e;result=true;}}
if(c.action==='consent'){const deadline=Date.now()+10000;while(Date.now()<deadline){if((await browser.storage.local.get('nativeConsent')).nativeConsent){result=await browser.permissions.contains({origins:['http://*/*','https://*/*']});break;}await new Promise(r=>setTimeout(r,100));}if(!result)throw Error('Site-access consent was not granted');}
if(c.action==='download'){await browser.storage.local.remove('lastTransfer');const id=await browser.downloads.download({url:base+'/fixture.bin',filename:'firefox.bin',saveAs:false});const deadline=Date.now()+20000;while(Date.now()<deadline){const {lastTransfer}=await browser.storage.local.get('lastTransfer');if(lastTransfer?.ok){const page=await bridge.request('list_page',{offset:0,limit:50});const task=page.tasks.find(t=>t.id===lastTransfer.id);const downloads=await browser.downloads.search({id});result={task,downloads,credentials:await browser.storage.session.get('auth:downloads')};break;}await new Promise(r=>setTimeout(r,100));}if(!result)throw Error('Firefox interception did not finish');}
if(c.action==='attachment'){const tabs=await browser.tabs.query({});const tab=tabs.find(t=>t.url===base+'/page');const previous=(await browser.storage.local.get('lastTransfer')).lastTransfer?.id;void browser.tabs.update(tab.id,{url:base+'/fixture.bin?navigation=1'}).catch(()=>{});const deadline=Date.now()+20000;while(Date.now()<deadline){const {lastTransfer}=await browser.storage.local.get('lastTransfer');if(lastTransfer?.ok&&lastTransfer.id!==previous){const page=await bridge.request('list_page',{offset:0,limit:50});result={task:page.tasks.find(t=>t.id===lastTransfer.id),downloads:await browser.downloads.search({url:base+'/fixture.bin?navigation=1'})};break;}await new Promise(r=>setTimeout(r,100));}if(!result)throw Error('Attachment navigation was not intercepted before the browser file chooser: '+JSON.stringify(await browser.storage.local.get(['lastTransfer','outbox'])));}
if(c.action==='hud-toggle'){await browser.storage.local.set({showMediaButtons:c.enabled});const tabs=await browser.tabs.query({});const tab=tabs.find(t=>t.url===base+'/page');await new Promise(r=>setTimeout(r,120));result=await browser.tabs.sendMessage(tab.id,{type:'flux-test-hud',action:'visible'});}
if(c.action==='native'){const session=(await browser.storage.session.get('auth:downloads'))['auth:downloads']||[];const captured=session.find(entry=>entry.credentials?.origin===base&&entry.credentials.cookie==='session=firefox-fixture');if(!captured?.credentials?.cookie)throw Error('Captured browser credentials are missing');const protected_credentials=await bridge.request('protect',{credentials:[captured.credentials]});const task=await bridge.request('add',{...makeInput(base+'/native.bin',{filename:'verified.bin'}),expected_sha256:c.sha256,protected_credentials});const deadline=Date.now()+20000;while(Date.now()<deadline){const current=await bridge.request('task',{id:task.id});if(current.status==='complete'){result=current;break;}if(current.status==='failed')throw Error(current.error);await new Promise(r=>setTimeout(r,100));}if(!result)throw Error('Native download did not finish');}
if(c.action==='hud'){const tabs=await browser.tabs.query({});const tab=tabs.find(t=>t.url===base+'/page');result=await browser.tabs.sendMessage(tab.id,{type:'flux-test-hud',action:c.hudAction});}
if(c.action==='hud-media'){const tabs=await browser.tabs.query({});const tab=tabs.find(t=>t.url===base+'/page');const confirmed=await browser.tabs.sendMessage(tab.id,{type:'flux-test-hud',action:'download'});const transfer=(await browser.storage.local.get('lastTransfer')).lastTransfer;if(!transfer?.ok)throw Error(transfer?.error||'HUD media was not acknowledged');const task=await bridge.request('task',{id:transfer.id});result={confirmed,task};}
if(c.action==='withdraw'){await browser.storage.local.set({nativeConsent:false});try{await bridge.request('list');throw Error('Consent withdrawal failed');}catch(e){if(!e.message.includes('Enable local'))throw e;}result=true;}
await report({id:c.id,ok:true,result});}catch(e){await report({id:c.id,ok:false,error:e.message||String(e)});}}catch(e){void report({kind:'helper-error',error:e.message||String(e)});}finally{busy=false;}},100);
`;
  const probe = `(()=>{const roots=[];const original=Element.prototype.attachShadow;Element.prototype.attachShadow=function(options){const root=original.call(this,options);if(this.hasAttribute('data-flux-hud'))roots.push(root);return root;};
browser.runtime.onMessage.addListener(async(message)=>{if(message.type!=='flux-test-hud')return;const deadline=Date.now()+10000;while(!roots.length&&Date.now()<deadline)await new Promise(r=>setTimeout(r,50));const root=roots[0];if(!root)throw Error('HUD was not attached');const host=root.host;const panel=root.querySelector('.panel');const main=root.querySelector('.main');
if(message.action==='panel'){main.click();const deadline=Date.now()+15000;while(root.querySelector('.download').disabled&&Date.now()<deadline)await new Promise(r=>setTimeout(r,100));await new Promise(r=>setTimeout(r,100));const opened=!panel.hidden,panelBox=panel.getBoundingClientRect(),barBox=root.querySelector('.bar').getBoundingClientRect();const geometry={height:panelBox.height,left:panelBox.left,right:panelBox.right,top:panelBox.top,bottom:panelBox.bottom,barHeight:barBox.height,hostHeight:host.getBoundingClientRect().height,overflow:getComputedStyle(host).overflow,ready:!root.querySelector('.download').disabled};document.body.click();return {opened,closed:panel.hidden,buttonVisible:host.style.display==='block',shadowClosed:host.shadowRoot===null,padding:getComputedStyle(main).paddingTop,position:getComputedStyle(host).position,bodyPadding:getComputedStyle(document.body).paddingTop,popover:typeof host.showPopover==='function',geometry};}
if(message.action==='visible'){return {visible:host.style.display==='block'};}
if(message.action==='download'){if(panel.hidden)main.click();const ready=Date.now()+15000;while(root.querySelector('.download').disabled&&Date.now()<ready)await new Promise(r=>setTimeout(r,100));if(root.querySelector('.download').disabled)throw Error(root.querySelector('[role=status]').textContent);root.querySelector('.download').click();const deadline=Date.now()+20000;while(!panel.hidden&&Date.now()<deadline)await new Promise(r=>setTimeout(r,100));if(!panel.hidden)throw Error(root.querySelector('[role=status]').textContent);return {closed_after_confirmation:true,button_visible:host.style.display==='block'};}
if(message.action==='move'){const grip=root.querySelector('.move');const before=host.style.transform;grip.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true}));await new Promise(r=>setTimeout(r,100));return {before,after:host.style.transform,stored:await browser.storage.local.get('hud-position:'+location.hostname)};}
if(message.action==='reset'){root.querySelector('.move').dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true}));await new Promise(r=>setTimeout(r,100));return true;}
if(message.action==='geometry'){const box=root.querySelector('.move').getBoundingClientRect(),hud=host.getBoundingClientRect();return {x:Math.round(box.left+box.width/2),y:Math.round(box.top+box.height/2),left:hud.left,top:hud.top,right:hud.right,width:innerWidth,transform:host.style.transform,stored:await browser.storage.local.get('hud-position:'+location.hostname)};}
if(message.action==='fullscreen'){return {full:!!document.fullscreenElement,inFullscreen:document.fullscreenElement?.contains(host)||host.matches(':popover-open'),visible:host.style.display==='block'};}
if(message.action==='close'){root.querySelector('.close').click();dispatchEvent(new Event('resize'));await new Promise(r=>setTimeout(r,100));return {hidden:host.style.display==='none',dismissed:sessionStorage.getItem('flux-dismissed:'+location.pathname)};}
});})();`;
  await writeFile(join(addon, "test-background.js"), helper);
  await writeFile(join(addon, "test-content.js"), probe);
  const manifest = JSON.parse(await readFile(join(addon, "manifest.json")));
  manifest.background.scripts = ["test-background.js"];
  const hudScript = manifest.content_scripts.find(
    (script) => script.world !== "MAIN" && script.js.includes("content.js"),
  );
  assert.ok(hudScript, "The isolated HUD content script is missing");
  hudScript.js = ["test-content.js", "content.js"];
  await writeFile(join(addon, "manifest.json"), JSON.stringify(manifest));
  assert.equal(
    await wd("POST", prefix + "/moz/addon/install", {
      path: addon,
      temporary: true,
    }),
    "flux@flux-download.local",
  );
  const ready = await until(
    () => reports.find((r) => r.kind === "ready"),
    30000,
  );
  await command("denied");
  await wd("POST", prefix + "/url", { url: ready.extensionUrl });
  const checkbox = await wd("POST", prefix + "/element", {
    using: "css selector",
    value: "#native-consent",
  });
  await wd(
    "POST",
    prefix +
      `/element/${checkbox["element-6066-11e4-a52e-4f735466cecf"]}/click`,
    {},
  );
  const consentId = randomUUID();
  commands.push({ id: consentId, action: "consent" });
  // Accept the real Firefox site-access prompt in the isolated test profile.
  await wd("POST", prefix + "/moz/context", { context: "chrome" });
  await until(async () => {
    const confirmed = reports.find((report) => report.id === consentId);
    if (confirmed) {
      assert.equal(confirmed.ok, true, confirmed.error);
      return confirmed.result;
    }
    await wd("POST", prefix + "/execute/sync", {
      script:
        "const n=document.querySelector('#addon-webext-permissions-notification');if(n && !n.hidden){n.button.click();return true;}return false;",
      args: [],
    });
    return false;
  }, 10000);
  await wd("POST", prefix + "/moz/context", { context: "content" });
  await wd("POST", prefix + "/url", { url: base + "/page" });
  const panel = await command("hud", { hudAction: "panel" });
  assert.equal(panel.opened, true);
  assert.equal(panel.closed, true);
  assert.equal(panel.buttonVisible, true);
  assert.equal(panel.shadowClosed, true);
  assert.equal(panel.padding, "9px");
  assert.equal(panel.position, "fixed");
  assert.equal(panel.bodyPadding, "0px");
  assert.equal(panel.geometry.ready, true);
  assert.equal(panel.geometry.overflow, "visible");
  assert.ok(panel.geometry.height > 170, JSON.stringify(panel.geometry));
  assert.ok(Math.abs(panel.geometry.hostHeight - panel.geometry.barHeight) < 1);
  const move = await command("hud", { hudAction: "move" });
  assert.notEqual(move.before, move.after);
  assert.ok(move.stored["hud-position:127.0.0.1"].x < 1);
  const beforeDrag = await command("hud", { hudAction: "geometry" });
  await wd("POST", prefix + "/actions", {
    actions: [
      {
        type: "pointer",
        id: "hud-mouse",
        parameters: { pointerType: "mouse" },
        actions: [
          {
            type: "pointerMove",
            duration: 0,
            x: beforeDrag.x,
            y: beforeDrag.y,
            origin: "viewport",
          },
          { type: "pointerDown", button: 0 },
          {
            type: "pointerMove",
            duration: 150,
            x: 20,
            y: 20,
            origin: "viewport",
          },
          { type: "pointerUp", button: 0 },
        ],
      },
    ],
  });
  const afterDrag = await command("hud", { hudAction: "geometry" });
  assert.notEqual(afterDrag.transform, beforeDrag.transform);
  assert.ok(
    afterDrag.left < 30 && afterDrag.top < 30,
    JSON.stringify(afterDrag),
  );
  assert.equal(afterDrag.stored["hud-position:127.0.0.1"].space, "viewport");
  assert.ok(
    afterDrag.stored["hud-position:127.0.0.1"].x <
      move.stored["hud-position:127.0.0.1"].x,
  );
  await wd("POST", prefix + "/window/rect", { width: 700, height: 700 });
  const resized = await command("hud", { hudAction: "geometry" });
  assert.ok(resized.left >= 0 && resized.right <= resized.width + 1);
  await command("hud", { hudAction: "reset" });
  const fullButton = await wd("POST", prefix + "/element", {
    using: "css selector",
    value: "#fullscreen",
  });
  await wd(
    "POST",
    prefix +
      `/element/${fullButton["element-6066-11e4-a52e-4f735466cecf"]}/click`,
    {},
  );
  await new Promise((r) => setTimeout(r, 500));
  const fullscreen = await command("hud", { hudAction: "fullscreen" });
  assert.equal(fullscreen.full, true);
  assert.equal(fullscreen.inFullscreen, true);
  assert.equal(fullscreen.visible, true);
  const close = await command("hud", { hudAction: "close" });
  assert.equal(close.hidden, true);
  assert.equal(close.dismissed, "1");
  const hiddenHud = await command("hud-toggle", { enabled: false });
  assert.equal(hiddenHud.visible, false);
  const restoredHud = await command("hud-toggle", { enabled: true });
  assert.equal(restoredHud.visible, true);
  const mediaProposal = await command("hud-media");
  assert.equal(mediaProposal.confirmed.closed_after_confirmation, true);
  assert.equal(mediaProposal.confirmed.button_visible, true);
  assert.equal(mediaProposal.task.status, "awaiting_approval");
  assert.equal(mediaProposal.task.request.url, base + "/media.mp4");
  assert.equal(mediaProposal.task.request.media.container, "mp4");
  const intercepted = await command("download");
  assert.equal(intercepted.task.status, "awaiting_approval");
  assert.equal(
    intercepted.downloads.some(
      (d) => d.state === "complete" || d.state === "in_progress",
    ),
    false,
  );
  assert.ok(
    intercepted.credentials["auth:downloads"].some(
      (i) => i.credentials.cookie === "session=firefox-fixture",
    ),
  );
  const completed = await command("native", { sha256 });
  assert.equal(completed.sha256, sha256);
  assert.equal(
    createHash("sha256")
      .update(await readFile(completed.destination))
      .digest("hex"),
    sha256,
  );
  const navigation = await command("attachment");
  assert.equal(navigation.task.status, "awaiting_approval");
  assert.equal(navigation.downloads.length, 0);
  await command("withdraw");
  const version = JSON.parse(await readFile("package.json")).version;
  validation = {
    version,
    firefox: ready.info.version,
    passed: true,
    installed_extension: addonSource.startsWith(binary),
    native_framing: true,
    hud_media_native_acknowledged: true,
    cookie_forwarding: true,
    sha256_verified: true,
    interception_approval: true,
    browser_transfer_cancelled: true,
    attachment_intercepted_before_file_chooser: true,
    toolbar_hud_toggle: true,
    external_download_manager_isolated: firefox
      .toLowerCase()
      .includes("flux-gecko-test"),
    consent_before_native: true,
    consent_withdrawal: true,
    closed_shadow_dom: true,
    strict_page_csp: true,
    outside_click: true,
    domain_placement: true,
    native_pointer_drag: true,
    free_viewport_drag: true,
    panel_unclipped: true,
    responsive_placement: true,
    fullscreen: true,
    close_button: true,
    popover_api: panel.popover,
  };
  assert.equal(cliError, "");
} finally {
  if (session) await wd("DELETE", `/session/${session}`).catch(() => {});
  gecko?.kill();
  await writeFile(join(root, "stop"), "stop");
  if (cli && cli.exitCode === null)
    await Promise.race([
      once(cli, "exit"),
      new Promise((r) => setTimeout(r, 5000)),
    ]);
  if (cli?.exitCode === null) cli.kill();
  if (previous.exists) registry("write", previous.value);
  else registry("delete");
  assert.deepEqual(JSON.parse(registry("read")), previous);
  fixture.closeAllConnections();
  await new Promise((r) => fixture.close(r));
}
validation.registry_restoration_verified = true;
await mkdir("docs/validation", { recursive: true });
await writeFile(
  `docs/validation/firefox-${validation.firefox}.json`,
  JSON.stringify(validation, null, 2) + "\n",
);
console.log(JSON.stringify(validation));
