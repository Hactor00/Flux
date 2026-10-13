import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { getBrowserApi, isGecko, nativeAllowed } from "../browser-api.js";
import { NativeBridge } from "../bridge.js";

const events = [];
let enabled = false;
globalThis.browser = {
  runtime: {
    getBrowserInfo() {},
    connectNative() {
      throw Error("Unexpected native connection");
    },
  },
  storage: {
    local: {
      async get() {
        return { nativeConsent: enabled };
      },
    },
    onChanged: {
      addListener(listener) {
        events.push(listener);
      },
    },
  },
};
globalThis.chrome = { runtime: { id: "unused-chromium" } };

test("Gecko namespace takes priority and local native access requires explicit consent", async () => {
  assert.equal(getBrowserApi(), globalThis.browser);
  assert.equal(isGecko(), true);
  assert.equal(await nativeAllowed(), false);
  await assert.rejects(new NativeBridge().request("list"), /Enable local Flux/);
  enabled = true;
  for (const listener of events)
    listener({ nativeConsent: { newValue: true } }, "local");
  assert.equal(await nativeAllowed(), true);
  enabled = false;
  for (const listener of events)
    listener({ nativeConsent: { newValue: false } }, "local");
  assert.equal(await nativeAllowed(), false);
});

test("Firefox package has an explicit identity, event background and transparent native data declaration", async () => {
  const manifest = JSON.parse(
    await readFile(
      new URL("../../extension-firefox/manifest.json", import.meta.url),
    ),
  );
  assert.equal(
    manifest.browser_specific_settings.gecko.id,
    "flux@flux-download.local",
  );
  assert.equal(
    manifest.browser_specific_settings.gecko.strict_min_version,
    "120.0",
  );
  assert.deepEqual(manifest.background, {
    scripts: ["background.js"],
    type: "module",
  });
  assert.equal(manifest.background.service_worker, undefined);
  assert.equal(manifest.permissions.includes("webRequestBlocking"), false);
  assert.equal(
    manifest.browser_specific_settings.gecko.data_collection_permissions,
    undefined,
  );
  const modern = JSON.parse(
    await readFile(
      new URL("../../extension-firefox-modern/manifest.json", import.meta.url),
    ),
  );
  assert.equal(
    modern.browser_specific_settings.gecko.strict_min_version,
    "140.0",
  );
  assert.deepEqual(
    modern.browser_specific_settings.gecko.data_collection_permissions.required,
    ["browsingActivity", "authenticationInfo", "websiteContent"],
  );
  for (const name of [
    "background.js",
    "browser-api.js",
    "bridge.js",
    "content.js",
    "healing.js",
  ])
    assert.equal(
      await readFile(
        new URL(`../../extension-firefox/${name}`, import.meta.url),
        "utf8",
      ),
      await readFile(new URL(`../${name}`, import.meta.url), "utf8"),
    );
});
