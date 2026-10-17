import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve, extname } from "node:path";
import { tmpdir } from "node:os";

// An isolated browser exercises the production modules with a local bridge fixture.
const directory = resolve("extension");
const output = resolve(".tools/extension-ui-validation");
await mkdir(output, { recursive: true });
const languages = JSON.parse(
  await readFile(join(directory, "languages.json"), "utf8"),
);
const dictionaries = Object.fromEntries(
  await Promise.all(
    languages.map(async ({ code }) => [
      code,
      JSON.parse(
        await readFile(join(directory, "locales", `${code}.json`), "utf8"),
      ),
    ]),
  ),
);
const manifest = JSON.parse(
  await readFile(join(directory, "manifest.json"), "utf8"),
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
const root = await mkdtemp(join(tmpdir(), "flux-extension-ui-"));
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
  throw Error("Extension UI fixture timed out");
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
  const point = await evaluate(
    `(()=>{const el=document.querySelector(${JSON.stringify(selector)});el.scrollIntoView({block:'center'});const r=el.getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+r.height/2}})()`,
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
async function navigate(page, locale = "tr-TR", extra = "") {
  await call("Page.navigate", {
    url: `${base}/${page}?locale=${locale}${extra}`,
  });
  try {
    await until(() =>
      evaluate(
        `document.getElementById('extension-version')?.textContent===${JSON.stringify(manifest.version)} && (document.body.classList.contains('options') ? document.querySelector('#language option')!==null : document.querySelector('#media .item button')!==null)`,
      ),
    );
  } catch (error) {
    console.error(
      await evaluate(
        "({url:location.href,errors:window.__errors,calls:window.__calls,text:document.body.innerText})",
      ),
    );
    throw error;
  }
}
async function screenshot(name) {
  await evaluate("scrollTo(0,0)");
  const { data } = await call("Page.captureScreenshot", { format: "png" });
  await writeFile(join(output, name), Buffer.from(data, "base64"));
}
const setup = `window.__calls=[];window.__errors=[];window.__changes=[];
addEventListener('error',event=>__errors.push(event.message));addEventListener('unhandledrejection',event=>__errors.push(String(event.reason)));
const dictionaries=${JSON.stringify(dictionaries)},manifest=${JSON.stringify(manifest)};
const parameters=new URLSearchParams(location.search);
let values=JSON.parse(localStorage.getItem('fixture-settings')||'{}');values.locale=parameters.get('locale')||'tr-TR';
values.extensionTheme=values.extensionTheme||'dark';
const links=['https://files.test/alpha.zip','https://files.test/readme.pdf','https://files.test/photo.jpg'];
window.chrome={runtime:{id:'fixture-extension',getManifest:()=>manifest,openOptionsPage:async()=>__calls.push({type:'options'}),sendMessage:async message=>{
__calls.push(message);let result={};
if(message.type==='dictionary')result=dictionaries[values.locale];
if(message.type==='status')result={connected:!parameters.has('offline'),desktop_version:'2.5.0',tasks:[{destination:'C:/Downloads/Archive.zip',request:{},status:'running'}],outbox:[]};
if(message.type==='release-update')result='2.4.4';
if(message.type==='candidates')result=[{url:'https://files.test/video.mp4',kind:'file',contentType:'video/mp4',seenAt:1}];
if(message.type==='inspect')result={streams:[{codec_type:'video',height:1080,index:0,codec_name:'h264',fps:30},{codec_type:'video',height:720,index:1,codec_name:'h264',fps:30}]};
return {ok:true,result};}},i18n:{getMessage:key=>dictionaries[values.locale]?.[key]||'',getUILanguage:()=>values.locale},
storage:{onChanged:{addListener:fn=>__changes.push(fn)},local:{get:async key=>typeof key==='string'?{[key]:values[key]}:{...values},set:async update=>{const changes=Object.fromEntries(Object.entries(update).map(([key,newValue])=>[key,{newValue,oldValue:values[key]}]));Object.assign(values,update);localStorage.setItem('fixture-settings',JSON.stringify(values));for(const listener of __changes)listener(changes,'local');}},session:{get:async()=>({'links:7':links})}},
tabs:{query:async()=>[{id:7,url:'https://film.test/watch',title:'A film from the active page'}],sendMessage:async()=>{}},permissions:{request:async()=>true}};
if(parameters.has('gecko'))chrome.runtime.getBrowserInfo=async()=>({name:'Firefox'});`;
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
    targets.find((target) => target.type === "page").webSocketDebuggerUrl,
  );
  await once(socket, "open");
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id) pending.get(message.id)?.(message);
  });
  await call("Page.enable");
  await call("Page.addScriptToEvaluateOnNewDocument", { source: setup });
  await call("Emulation.setDeviceMetricsOverride", {
    width: 410,
    height: 600,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await navigate("popup.html");
  assert.equal(
    await evaluate(
      "document.getElementById('dot').classList.contains('online')",
    ),
    true,
  );
  assert.equal(
    await evaluate("document.getElementById('update-notice').hidden"),
    true,
  );
  await screenshot("popup-dark.png");
  await click("#tab-links");
  assert.equal(
    await evaluate("document.querySelector('#links-section').hidden"),
    false,
  );
  await evaluate(
    "document.getElementById('filter').value='*.zip';document.getElementById('filter').dispatchEvent(new Event('input'))",
  );
  assert.equal(
    await evaluate("document.querySelectorAll('#links input').length"),
    1,
  );
  await click("#download-links");
  assert.deepEqual(
    await evaluate("__calls.find(call=>call.type==='enqueue-batch').urls"),
    ["https://files.test/alpha.zip"],
  );
  await click("#links input");
  assert.equal(
    await evaluate("document.getElementById('download-links').disabled"),
    true,
  );
  await click("#tab-queue");
  assert.match(
    await evaluate("document.getElementById('tasks').textContent"),
    /Archive.zip/,
  );
  await click("#show-media-buttons");
  assert.equal(
    await evaluate(
      "JSON.parse(localStorage.getItem('fixture-settings')).showMediaButtons",
    ),
    false,
  );
  await evaluate(
    "document.getElementById('url').value='https://files.test/new.zip';document.getElementById('add').requestSubmit()",
  );
  await until(() =>
    evaluate(
      "__calls.some(call=>call.type==='enqueue'&&call.url==='https://files.test/new.zip')",
    ),
  );
  await navigate("popup.html", "tr-TR", "&offline=1");
  assert.equal(
    await evaluate("document.getElementById('desktop-install-help').hidden"),
    false,
  );
  assert.match(
    await evaluate("document.querySelector('#desktop-install-help a').href"),
    /github.com\/Hactor00\/Flux/,
  );
  await screenshot("popup-offline.png");
  for (const { code } of languages) {
    await navigate("popup.html", code);
    const layout = await evaluate(
      "({width:innerWidth,scroll:document.documentElement.scrollWidth,errors:__errors,untranslated:[...document.querySelectorAll('[data-i18n]')].filter(el=>el.textContent===el.dataset.i18n).map(el=>el.dataset.i18n)})",
    );
    assert.ok(layout.scroll <= layout.width, `${code}: horizontal overflow`);
    assert.deepEqual(layout.errors, []);
    assert.deepEqual(layout.untranslated, []);
  }
  await call("Emulation.setDeviceMetricsOverride", {
    width: 1020,
    height: 960,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await navigate("options.html");
  await screenshot("settings-dark.png");
  await click('[data-theme-choice="light"]');
  assert.equal(
    await evaluate("document.documentElement.dataset.theme"),
    "light",
  );
  await screenshot("settings-light.png");
  await navigate("popup.html");
  assert.equal(
    await evaluate("document.documentElement.dataset.theme"),
    "light",
  );
  await call("Emulation.setDeviceMetricsOverride", {
    width: 410,
    height: 600,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await screenshot("popup-light.png");
  await navigate("options.html", "ar-SA", "&gecko=1");
  assert.equal(
    await evaluate("document.getElementById('native-consent-panel').hidden"),
    false,
  );
  await click("#native-consent");
  assert.equal(
    await evaluate(
      "JSON.parse(localStorage.getItem('fixture-settings')).nativeConsent",
    ),
    true,
  );
  await evaluate(
    "document.getElementById('language-search').value='Türkçe';document.getElementById('language-search').dispatchEvent(new Event('input'))",
  );
  assert.equal(
    await evaluate("document.getElementById('language').options.length"),
    2,
  );
  await evaluate(
    "document.getElementById('language').value='tr-TR';document.getElementById('language').dispatchEvent(new Event('change'))",
  );
  await until(() =>
    evaluate(
      "document.documentElement.lang==='tr-TR'&&document.documentElement.dir==='ltr'",
    ),
  );
  assert.equal(
    await evaluate("document.documentElement.scrollWidth<=innerWidth"),
    true,
  );
  assert.deepEqual(await evaluate("__errors"), []);
  await screenshot("settings-narrow.png");
  const report = {
    passed: true,
    version: manifest.version,
    locales: languages.length,
    popup_tabs: true,
    batch_selection: true,
    url_handoff: true,
    hud_preference: true,
    offline_install_help: true,
    theme_persistence: true,
    firefox_consent: true,
    language_switch: true,
    responsive_layout: true,
    scope:
      "Production UI modules in an isolated Chromium browser with a local native bridge fixture",
  };
  await writeFile(
    join(output, "report.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
  socket?.close();
  browser.kill();
  server.closeAllConnections();
  await new Promise((r) => server.close(r));
}
