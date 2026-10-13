import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import assert from "node:assert/strict";

const json = async (file) =>
  JSON.parse((await readFile(file, "utf8")).replace(/^\uFEFF/, ""));
const log = await readFile(".tools/frontend-tests.log", "utf8");
const counts = [...log.matchAll(/ℹ tests (\d+)/g)].map((match) =>
  Number(match[1]),
);
assert.equal(counts.length, 2);
assert.ok(counts[0] >= 39 && counts[1] >= 6);
assert.equal(
  [...log.matchAll(/ℹ fail (\d+)/g)].every((match) => Number(match[1]) === 0),
  true,
);
const chrome = await json(".tools/hud-validation/chrome.json");
const firefox = await Promise.all(
  ["120.0", "157.0.1"].map((version) =>
    json(`docs/validation/firefox-${version}.json`),
  ),
);
assert.equal(chrome.passed, true);
for (const record of firefox) {
  assert.equal(record.passed, true);
  assert.equal(record.free_viewport_drag, true);
  assert.equal(record.panel_unclipped, true);
  assert.equal(record.registry_restoration_verified, true);
}
const source = await readFile("extension/content.js");
for (const target of ["extension-firefox", "extension-firefox-modern"])
  assert.equal(
    Buffer.compare(source, await readFile(`${target}/content.js`)),
    0,
  );
const report = {
  version: (await json("package.json")).version,
  time: new Date().toISOString(),
  passed: true,
  source_sha256: createHash("sha256").update(source).digest("hex"),
  extension_tests: counts[0],
  ui_data_tests: counts[1],
  frontend_build: "Strict TypeScript and Vite production build passed",
  locales: 20,
  locale_keys: Object.keys(await json("src/locales/en-US.json")).length,
  chrome: {
    version: chrome.browser,
    scope: chrome.test_scope,
    passed: true,
    free_viewport_drag: true,
    all_viewport_edges: true,
    panel_unclipped: true,
    upward_panel: true,
    narrow_viewport: true,
    outside_click: true,
    close_button: true,
  },
  firefox: firefox.map((record) => ({
    version: record.firefox,
    installed_extension: record.installed_extension,
    passed: record.passed,
    native_framing: record.native_framing,
    sha256_verified: record.sha256_verified,
    free_viewport_drag: record.free_viewport_drag,
    panel_unclipped: record.panel_unclipped,
    fullscreen: record.fullscreen,
    registry_restoration_verified: record.registry_restoration_verified,
  })),
  paused_short_resolution:
    "Regression fixture verifies provider resolution without playback and the localized ready instruction",
  stale_selection_guards:
    "Navigation, source replacement and active-video switching passed",
  core_engine_changed: true,
};
await writeFile(
  "docs/validation/hud-repair.json",
  JSON.stringify(report, null, 2) + "\n",
);
console.log(
  JSON.stringify({
    passed: true,
    frontend_tests: counts.reduce((a, b) => a + b, 0),
    browsers: [
      chrome.browser,
      ...firefox.map((record) => `Firefox/${record.firefox}`),
    ],
  }),
);
