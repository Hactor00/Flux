import {
  readFile,
  readdir,
  writeFile,
  mkdir,
  copyFile,
} from "node:fs/promises";
import { join } from "node:path";
const directory = "docs/validation";
const backup = `.tools/private-validation/${Date.now()}`;
await mkdir(backup, { recursive: true });
function clean(value, key = "") {
  if (/^(cookie|authorization|protected_credentials|credentials)$/i.test(key))
    return undefined;
  if (
    typeof value === "string" &&
    (/\\Users\\[^\\]+\\/i.test(value) || /\/Users\//i.test(value))
  )
    return undefined;
  if (
    typeof value === "string" &&
    /googlevideo\.com\/videoplayback/i.test(value)
  )
    return undefined;
  if (Array.isArray(value))
    return value
      .map((item) => clean(item))
      .filter((item) => item !== undefined);
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value)
        .map(([name, item]) => [name, clean(item, name)])
        .filter(([, item]) => item !== undefined),
    );
  return value;
}
for (const name of await readdir(directory)) {
  if (!name.endsWith(".json")) continue;
  const file = join(directory, name);
  await copyFile(file, join(backup, name));
  const data = clean(JSON.parse(await readFile(file, "utf8")));
  data.local_paths_and_credentials_removed = true;
  await writeFile(file, JSON.stringify(data, null, 2) + "\n");
}
