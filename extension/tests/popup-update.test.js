import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";

test("popup shows actionable extension/desktop updates and the desktop installation link", async () => {
  const dom = new JSDOM(
    await readFile(new URL("../popup.html", import.meta.url), "utf8"),
  );
  globalThis.document = dom.window.document;
  const words = JSON.parse(
    await readFile(new URL("../locales/en-US.json", import.meta.url), "utf8"),
  );
  globalThis.chrome = {
    runtime: {
      getManifest: () => ({ version: "2.4.1" }),
      async sendMessage(message) {
        return {
          ok: true,
          result:
            message.type === "dictionary"
              ? words
              : message.type === "release-update"
                ? "2.4.2"
                : message.type === "status"
                  ? { connected: false, desktop_version: "2.4.0", tasks: [] }
                  : {},
        };
      },
    },
    i18n: { getMessage: () => "", getUILanguage: () => "en-US" },
    storage: {
      local: {
        async get() {
          return {};
        },
        async set() {},
      },
    },
    tabs: {
      async query() {
        return [];
      },
    },
  };
  await import("../popup.js");
  assert.equal(document.getElementById("update-notice").hidden, false);
  assert.match(document.getElementById("update-title").textContent, /2\.4\.2/);
  assert.match(document.getElementById("update-help").textContent, /ZIP/);
  assert.match(document.getElementById("update-help").textContent, /Settings/);
  assert.equal(document.getElementById("desktop-install-help").hidden, false);
  const downloadButton = document.querySelector(
    "#desktop-install-help .download-link",
  );
  assert.equal(
    downloadButton.href,
    "https://github.com/Hactor00/Flux/releases/latest",
  );
  assert.match(
    document.getElementById("desktop-install-help").textContent,
    /If installed/,
  );
  assert.equal(
    document.querySelector("#update-notice a").href,
    "https://github.com/Hactor00/Flux/releases/latest",
  );
  assert.equal(
    document.getElementById("dot").classList.contains("online"),
    false,
  );
  dom.window.close();
  delete globalThis.document;
  delete globalThis.chrome;
});
