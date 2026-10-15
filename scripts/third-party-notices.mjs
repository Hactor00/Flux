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
for (const pkg of packages.filter(
  (pkg) => pkg.source || pkg.manifest_path.includes("vendor"),
))
  collect(
    "cargo",
    pkg.name,
    pkg.version,
    pkg.license,
    dirname(pkg.manifest_path),
    pkg.source || "Locally patched upstream source; see vendor/FLUX-PATCH.md",
  );
const nativeLock = JSON.parse(
  readFileSync("native/torrent/windows-runtime.lock.json"),
);
const nativePackages = {
  "libtorrent-rasterbar": "BSD-3-Clause",
  openssl: "Apache-2.0",
  "libstdc++": "GPL-3.0-or-later WITH GCC-exception-3.1",
  libgcc: "GPL-3.0-or-later WITH GCC-exception-3.1",
  libwinpthread: "MIT",
};
for (const [name, license] of Object.entries(nativePackages)) {
  const pkg = nativeLock.find(
    (pkg) => pkg.name === `mingw-w64-ucrt-x86_64-${name}`,
  );
  if (!pkg) throw Error(`Native dependency is absent from the lock: ${name}`);
  const directory = join(stage, "native-licenses", name);
  if (!existsSync(directory))
    throw Error(`Native dependency license is missing: ${name}`);
  collect(
    "native",
    name,
    pkg.version,
    license,
    directory,
    `https://repo.msys2.org/mingw/ucrt64/${pkg.file}`,
  );
}
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
  "# Flux third-party notices\n\nFlux source is MIT licensed. FFmpeg is a separate executable built under LGPL-2.1-or-later; matching complete source archives and the build recipe are in `media-source/`. libxml2 is MIT licensed. The embedded libtorrent runtime is BSD-3-Clause licensed and OpenSSL is Apache-2.0 licensed. GCC runtime libraries carry the GCC Runtime Library Exception; they do not apply GPL requirements to Flux. Full native package notices are preserved in `native-licenses/`. License texts for Rust, npm and shipped native dependencies are in `licenses/`. Patched reqwest and librqbit retain their upstream licenses. The inventory includes development dependencies for reproducibility. Microsoft WebView2 is an operating-system web runtime installed separately by its official bootstrapper.\n\n" +
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
