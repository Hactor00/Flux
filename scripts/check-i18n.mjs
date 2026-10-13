import { readFileSync, readdirSync } from "node:fs";
import assert from "node:assert/strict";
const files = readdirSync("src/locales").filter((name) =>
  name.endsWith(".json"),
);
assert.equal(files.length, 20);
const baseline = JSON.parse(readFileSync("src/locales/en-US.json", "utf8"));
for (const file of files) {
  const data = JSON.parse(readFileSync(`src/locales/${file}`, "utf8"));
  assert.deepEqual(
    Object.keys(data).sort(),
    Object.keys(baseline).sort(),
    file,
  );
  for (const value of Object.values(data))
    assert.equal(
      typeof value === "string" && value.trim().length > 0,
      true,
      file,
    );
  // Pidgin shares many short technical terms with English; sentences must still be localized.
  if (file !== "en-US.json")
    assert.ok(
      Object.keys(data).filter((key) => data[key] !== baseline[key]).length >
        (file === "pcm-NG.json" ? 20 : 45),
      `${file}: untranslated catalog`,
    );
}
const source = [
  "src/App.tsx",
  ...readdirSync("src/components")
    .filter((file) => file.endsWith(".tsx"))
    .map((file) => `src/components/${file}`),
]
  .map((path) => readFileSync(path, "utf8"))
  .join("\n");
for (const match of source.matchAll(/\bt\(["']([^"']+)["']\)/g))
  assert.ok(match[1] in baseline, match[1]);
for (const status of [
  "queued",
  "running",
  "paused",
  "verifying",
  "complete",
  "failed",
])
  assert.ok(status in baseline);
console.log(
  `Verified ${files.length} locales, ${Object.keys(baseline).length} keys each.`,
);
