import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import ts from "typescript";
import React, { act } from "react";

const dom = new JSDOM("<!doctype html><div id='root'></div>", {
  url: "http://localhost/",
});
Object.assign(globalThis, {
  window: dom.window,
  document: dom.window.document,
  IS_REACT_ACT_ENVIRONMENT: true,
});
const { createRoot } = await import("react-dom/client");
const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const reactUrl = import.meta.resolve("react");
const dialogUrl = moduleUrl(`
  import React from ${JSON.stringify(reactUrl)};
  const Context = React.createContext(null);
  export const Root = ({open,onOpenChange,children}) =>
    React.createElement(Context.Provider,{value:{open,onOpenChange}},children);
  export const Portal = ({children}) => children;
  export const Overlay = () => null;
  export const Content = ({children}) => React.useContext(Context).open
    ? React.createElement('section',{role:'dialog'},children) : null;
  export const Title = ({children}) => React.createElement('h2',null,children);
  export const Description = ({children}) => React.createElement('p',null,children);
  export const Close = ({children,disabled}) => {
    const context=React.useContext(Context);
    return React.createElement('button',{disabled,onClick:()=>context.onOpenChange(false)},children);
  };
`);
const dependencies = {
  "@radix-ui/react-dialog": dialogUrl,
  "react-i18next": moduleUrl("export const useTranslation=()=>({t:key=>key});"),
  "@tauri-apps/plugin-updater": moduleUrl(
    "export const check=options=>globalThis.fluxUpdaterFixture.check(options);",
  ),
  "@tauri-apps/api/app": moduleUrl(
    "export const getVersion=async()=>globalThis.fluxUpdaterFixture.version;",
  ),
  "@tauri-apps/plugin-process": moduleUrl("export const relaunch=async()=>{};"),
  "../api": moduleUrl(
    "export const desktop=true; export const api={runtimePlatform:async()=>'windows'};",
  ),
  "../update-version.js": new URL("../src/update-version.js", import.meta.url)
    .href,
  "../update-resource": new URL("../src/update-resource.ts", import.meta.url)
    .href,
  "lucide-react": moduleUrl(
    "export const ArrowDownToLine=()=>null; export const RefreshCw=()=>null;",
  ),
};
const source = await readFile(
  new URL("../src/components/Updater.tsx", import.meta.url),
  "utf8",
);
const compiled = ts
  .transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      jsx: ts.JsxEmit.ReactJSX,
    },
  })
  .outputText.replace(/from\s+["']([^"']+)["']/g, (_, name) => {
    return `from ${JSON.stringify(dependencies[name] || import.meta.resolve(name))}`;
  });
const { Updater } = await import(moduleUrl(compiled));

async function mount(fixture, verify) {
  globalThis.fluxUpdaterFixture = fixture;
  const root = createRoot(document.getElementById("root"));
  try {
    await act(async () => root.render(React.createElement(Updater)));
    await verify();
  } finally {
    await act(async () => root.unmount());
    delete globalThis.fluxUpdaterFixture;
  }
}
async function click(label) {
  const button = [...document.querySelectorAll("button")].find(
    (item) => item.textContent === label,
  );
  assert.ok(button && !button.disabled, `Enabled ${label} button is missing`);
  await act(async () => button.click());
}
const dialog = () => document.querySelector('[role="dialog"]');

test("checking again after Later keeps the replacement available even if old resource cleanup fails", async () => {
  let checks = 0;
  let closed = 0;
  const fixture = {
    version: "2.4.2",
    check: async () => ({
      version: ++checks === 1 ? "2.4.3" : "2.4.4",
      close: async () => {
        closed++;
        if (closed === 1)
          throw Error("Command plugin:resources|close not allowed by ACL");
      },
    }),
  };
  const warnings = [];
  const previousWarn = console.warn;
  console.warn = (...args) => warnings.push(args);
  try {
    await mount(fixture, async () => {
      await click("check_updates");
      assert.match(dialog().textContent, /2\.4\.3/);
      await click("later");
      assert.equal(dialog(), null);
      await click("check_updates");
      assert.match(dialog().textContent, /2\.4\.4/);
      assert.equal(
        document.body.textContent.includes("not allowed by ACL"),
        false,
      );
      assert.equal(checks, 2);
      assert.equal(closed, 1);
    });
    assert.equal(closed, 2, "Unmount must release the replacement resource");
    assert.equal(warnings.length, 1);
  } finally {
    console.warn = previousWarn;
  }
});

test("no update clears a dismissed offer and same-version offers stay hidden", async () => {
  let checks = 0;
  let closed = 0;
  await mount(
    {
      version: "2.4.4",
      check: async () => {
        checks++;
        return checks === 2
          ? null
          : {
              version: checks === 1 ? "2.4.5" : "2.4.4",
              close: async () => closed++,
            };
      },
    },
    async () => {
      await click("check_updates");
      assert.match(dialog().textContent, /2\.4\.5/);
      await click("later");
      await click("check_updates");
      assert.equal(dialog(), null);
      assert.match(document.body.textContent, /up_to_date/);
      await click("check_updates");
      assert.equal(dialog(), null);
      assert.match(document.body.textContent, /up_to_date/);
      assert.equal(closed, 2);
    },
  );
});
