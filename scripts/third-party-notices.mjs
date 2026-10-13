import { execFileSync } from "node:child_process";
import {
  readFileSync,
  readdirSync,
  mkdirSync,
  copyFileSync,
  writeFileSync,
  existsSync,
} from "node:fs";
import { join, dirname, resolve } from "node:path";
const stage = resolve(process.argv[2]);
const licenses = join(stage, "licenses");
mkdirSync(licenses, { recursive: true });
const packages = JSON.parse(
  execFileSync("cargo", ["metadata", "--locked", "--format-version", "1"], {
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  }),
).packages;
const records = [];
function collect(kind, name, version, license, directory, source) {
  const target = join(licenses, kind, name.replaceAll("/", "_"), version);
  const files = existsSync(directory)
    ? readdirSync(directory).filter((name) =>
        /^(licen[cs]e|copying|notice|copyright)([.-]|$)/i.test(name),
      )
    : [];
  mkdirSync(target, { recursive: true });
  for (const file of files) {
    try {
      copyFileSync(join(directory, file), join(target, file));
    } catch (error) {
      if (error.code !== "EISDIR") throw error;
    }
  }
  records.push({
    kind,
    name,
    version,
    license: license || "See license file",
    source,
    license_files: files,
  });
}
for (const pkg of packages.filter((pkg) => pkg.source))
  collect(
    "cargo",
    pkg.name,
    pkg.version,
    pkg.license,
    dirname(pkg.manifest_path),
    pkg.source,
  );
const lock = JSON.parse(readFileSync("package-lock.json"));
for (const [path, pkg] of Object.entries(lock.packages))
  if (path.startsWith("node_modules/"))
    collect(
      "npm",
      pkg.name || path.slice(path.lastIndexOf("node_modules/") + 13),
      pkg.version,
      pkg.license,
      resolve(path),
      pkg.resolved,
    );
records.sort(
  (a, b) => a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name),
);
writeFileSync(
  join(stage, "dependency-inventory.json"),
  JSON.stringify(records, null, 2) + "\n",
);
writeFileSync(
  join(stage, "THIRD-PARTY-NOTICES.md"),
  "# Flux third-party notices\n\nFlux source is MIT licensed. FFmpeg is a separate executable built under LGPL-2.1-or-later; matching complete source archives and the build recipe are in `media-source/`. libxml2 is MIT licensed. License texts for Rust and npm dependencies are in `licenses/`. The inventory includes development dependencies for reproducibility. Microsoft WebView2 is an operating-system web runtime installed separately by its official bootstrapper.\n\n" +
    records
      .map(
        (pkg) => `- ${pkg.kind}: ${pkg.name}@${pkg.version} — ${pkg.license}`,
      )
      .join("\n") +
    "\n",
);
console.log(
  `${records.length} dependencies inventoried with available license texts.`,
);
