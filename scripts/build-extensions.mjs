import { cp, readFile, readdir, mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("../", import.meta.url));
const source = join(root, "extension");
const target = join(root, "extension-firefox");
const chrome = JSON.parse(await readFile(join(source, "manifest.json")));
const firefox = JSON.parse(await readFile(join(target, "manifest.json")));
const id = (await readFile(join(target, "extension-id.txt"), "utf8")).trim();
assert.equal(firefox.version, chrome.version);
assert.equal(firefox.browser_specific_settings.gecko.id, id);
assert.equal(
  firefox.browser_specific_settings.gecko.strict_min_version,
  "120.0",
);
assert.equal(firefox.manifest_version, 3);
assert.equal(firefox.background.type, "module");
assert.deepEqual(firefox.background.scripts, ["background.js"]);
assert.equal(firefox.background.service_worker, undefined);
assert.equal(firefox.key, undefined);
assert.deepEqual(
  firefox.permissions.filter(
    (permission) => permission !== "webRequestBlocking",
  ),
  chrome.permissions,
);
assert.ok(firefox.permissions.includes("webRequestBlocking"));
assert.deepEqual(
  firefox.content_scripts,
  chrome.content_scripts.filter((script) => script.world !== "MAIN"),
);
assert.deepEqual(
  firefox.content_security_policy,
  chrome.content_security_policy,
);
const files = (await readdir(source, { withFileTypes: true })).filter(
  (entry) =>
    entry.isDirectory()
      ? ["icons", "_locales", "locales"].includes(entry.name)
      : /\.(?:js|html|css|json)$/.test(entry.name) &&
        entry.name !== "manifest.json",
);
await mkdir(target, { recursive: true });
for (const file of files) {
  await cp(join(source, file.name), join(target, file.name), {
    recursive: true,
  });
}
const modernTarget = join(root, "extension-firefox-modern");
await mkdir(modernTarget, { recursive: true });
for (const file of files) {
  await cp(join(source, file.name), join(modernTarget, file.name), {
    recursive: true,
  });
}
const modern = structuredClone(firefox);
modern.content_scripts = structuredClone(chrome.content_scripts);
modern.browser_specific_settings.gecko.strict_min_version = "140.0";
modern.browser_specific_settings.gecko_android = {
  strict_min_version: "142.0",
};
modern.browser_specific_settings.gecko.data_collection_permissions = {
  required: ["browsingActivity", "authenticationInfo", "websiteContent"],
};
await writeFile(
  join(modernTarget, "manifest.json"),
  JSON.stringify(modern, null, 2) + "\n",
);
await cp(
  join(target, "extension-id.txt"),
  join(modernTarget, "extension-id.txt"),
);
for (const directory of [target, modernTarget])
  await cp(join(root, "LICENSE"), join(directory, "LICENSE"));
console.log(
  `Firefox ${firefox.version}: ${files.length} shared source entries generated`,
);
