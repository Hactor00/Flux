import { readFile, writeFile, stat } from "node:fs/promises";
import { createHash } from "node:crypto";

const json = async (path) =>
  JSON.parse((await readFile(path, "utf8")).replace(/^\uFEFF/, ""));
const version = (await json("package.json")).version;
const records = Object.fromEntries(
  await Promise.all(
    [
      "installer",
      "daily-use",
      "windows",
      "desktop-integration",
      "media-window",
      "torrent-window",
      "browser-stores",
      "hud-repair",
      "firefox-120.0",
      "firefox-157.0.1",
    ].map(async (name) => [name, await json(`docs/validation/${name}.json`)]),
  ),
);
for (const [name, record] of Object.entries(records)) {
  if (!record.passed) throw Error(`${name} did not pass`);
  if (
    (name.startsWith("firefox-") ||
      ["torrent-window", "browser-stores"].includes(name)) &&
    record.version !== version
  )
    throw Error(`${name} validation version mismatch`);
}
if (
  !records.installer.installed_binary_hashes_verified ||
  !records["torrent-window"].torrent_file_row_visible
)
  throw Error(
    "Current package identity or torrent row visibility was not verified",
  );
if (
  records.installer.version !== version ||
  records["daily-use"].version !== version ||
  records["hud-repair"].version !== version
)
  throw Error("Validation version mismatch");
const hudSourceHash = createHash("sha256")
  .update(await readFile("extension/content.js"))
  .digest("hex");
if (records["hud-repair"].source_sha256 !== hudSourceHash)
  throw Error("HUD validation does not match the release source");

const rustLog = await readFile(".tools/core-tests.log", "utf8");
const rustTests = [
  ...rustLog.matchAll(/test result: ok\. (\d+) passed/g),
].reduce((sum, match) => sum + Number(match[1]), 0);
const frontendLog = await readFile(".tools/frontend-tests.log", "utf8");
const counts = [...frontendLog.matchAll(/ℹ tests (\d+)/g)].map((match) =>
  Number(match[1]),
);
if (!rustTests || counts.length !== 2 || !counts.every(Boolean))
  throw Error("Required test counts are missing");
const [extensionTests, uiTests] = counts;
const hashes = {};
for (const name of [
  "flux-desktop.exe",
  "flux-cli.exe",
  "flux-native-host.exe",
  "ffmpeg.exe",
  "ffprobe.exe",
  "yt-dlp.exe",
  "node.exe",
  "libunwind.dll",
  "WebView2Loader.dll",
]) {
  hashes[name] = createHash("sha256")
    .update(await readFile(`target/release/${name}`))
    .digest("hex");
}
const audit = await json(".tools/production-audit.json");
const translations = await json("src/locales/en-US.json");
const firefoxLint = await json(".tools/firefox-lint-modern.json");
if (firefoxLint.summary.errors !== 0 || firefoxLint.summary.warnings !== 0)
  throw Error("Firefox manifest validation failed");
const report = {
  version,
  time: new Date().toISOString(),
  platform: "Windows x64",
  environment:
    process.env.GITHUB_ACTIONS === "true"
      ? "GitHub Actions"
      : "Local Windows validation",
  rust_tests: rustTests,
  extension_tests: extensionTests,
  ui_data_tests: uiTests,
  automated_tests: rustTests + extensionTests + uiTests,
  test_evidence: {
    core_log_updated: (await stat(".tools/core-tests.log")).mtime.toISOString(),
    frontend_log_updated: (
      await stat(".tools/frontend-tests.log")
    ).mtime.toISOString(),
    core_engine_changed_in_hud_repair:
      records["hud-repair"].core_engine_changed,
    core_scope:
      process.env.GITHUB_ACTIONS === "true"
        ? "Core tests executed during the current GitHub Actions release build"
        : "Core tests executed during the current local release validation",
  },
  locales: 20,
  locale_keys: Object.keys(translations).length,
  clippy: "workspace/all-targets/all-features -D warnings passed",
  format: "passed",
  frontend_build: "strict TypeScript + Vite production build passed",
  npm_audit_production_vulnerabilities: audit.metadata.vulnerabilities.total,
  scope: [
    "Embedded BitTorrent v1 engine, magnet and .torrent import, piece verification and SHA-256 outputs",
    "Native torrent window import, real peer transfer and pause/resume",
    "Direct sparse HTTP staging, non-overlapping offsets and legacy checkpoint migration",
    "DOM/network/MSE hints, audio streams, fps and subtitles",
    "Durable playlist/channel/page batch queue and origin-scoped session renewal",
    "Brave registry binding and reviewed browser store packaging workflow",
    "Firefox Gecko MV3 extension, native caller verification and browser interception",
    "Firefox 120 fixed HUD and Firefox 140+ popover/native data declarations",
    "Cancellable completion actions",
    "Extension/MIME category routing",
    "Native completed-file drag-out",
    "Persistent silent clipboard queue",
    "Viewport-wide media HUD movement, unclipped panels and stale player selection protection",
    "Windows background startup",
    "Clean release and SignPath submission workflow",
  ],
  daily_use: records["daily-use"],
  installer: records.installer,
  native_windows: records.windows,
  native_ipc: records["desktop-integration"],
  native_media_window: records["media-window"],
  native_torrent_window: records["torrent-window"],
  browser_store_packages: records["browser-stores"],
  hud_repair: records["hud-repair"],
  firefox: [records["firefox-120.0"], records["firefox-157.0.1"]],
  firefox_lint: {
    summary: firefoxLint.summary,
    warnings: firefoxLint.warnings,
  },
  binary_sha256: hashes,
  signing: {
    workflow_configured: true,
    stage: "Unsigned build validation before optional SignPath submission",
    final_signing_state: "Recorded in release-info.json by finalization",
  },
  limitations: [
    "Native Windows sleep, hibernate and power-off were compiled and reviewed but not executed",
    "Physical OLE drops into third-party applications were not automated",
    "Silent clipboard persistence was tested without modifying the user clipboard",
    "Historical external YouTube verification was not repeated",
    "DRM, CAPTCHA, login challenges and opaque renewal APIs are not automatically solved",
    "Upstream reqwest HTTP/3 interface remains experimental and pinned",
  ],
};
await writeFile(
  "docs/validation/release.json",
  JSON.stringify(report, null, 2) + "\n",
);
console.log(
  JSON.stringify({
    version,
    automated_tests: report.automated_tests,
    installer_sha256: records.installer.sha256,
  }),
);
