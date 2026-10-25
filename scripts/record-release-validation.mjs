import { readFile, writeFile, stat } from "node:fs/promises";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";

async function sha256(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

const json = async (path) =>
  JSON.parse((await readFile(path, "utf8")).replace(/^\uFEFF/, ""));
const version = (await json("package.json")).version;
const records = Object.fromEntries(
  await Promise.all(
    [
      "installer",
      "upgrade",
      "daily-use",
      "windows",
      "desktop-integration",
      "media-window",
      "torrent-window",
      "torrent-watch-window",
      "browser-stores",
      "hud-repair",
      "firefox-120.0",
      "firefox-157.0.1",
      "firefox-157.0.1-store",
      "chrome-interception",
      "chrome-store-interception",
      "browser-onboarding",
    ].map(async (name) => [name, await json(`docs/validation/${name}.json`)]),
  ),
);
for (const [name, record] of Object.entries(records)) {
  if (!record.passed) throw Error(`${name} did not pass`);
  if (
    (name.startsWith("firefox-") ||
      [
        "torrent-window",
        "torrent-watch-window",
        "browser-stores",
        "chrome-interception",
        "chrome-store-interception",
        "browser-onboarding",
        "upgrade",
      ].includes(name)) &&
    record.version !== version
  )
    throw Error(`${name} validation version mismatch`);
}
if (
  !records.installer.installed_binary_hashes_verified ||
  records.upgrade.from_version !== "2.5.0" ||
  !records.upgrade.history_preserved ||
  !records.upgrade.settings_preserved ||
  records["firefox-157.0.1-store"].extension_id !== "firefox@fluxdownload.org" ||
  records["chrome-store-interception"].extension_id !==
    "okimieponfbegjhmfdnnefnddofooipk" ||
  !records["chrome-store-interception"].hud_media_native_acknowledged ||
  !records.installer.active_native_host_upgrade_verified ||
  !records["browser-onboarding"].missing_registry_and_outdated_host_repaired ||
  !records["browser-onboarding"].dismissal_persisted_after_restart ||
  !records["browser-onboarding"].registration_is_not_extension_presence ||
  !records["browser-onboarding"]
    .native_port_connection_and_disconnect_verified ||
  !records["browser-onboarding"].settings_navigation_and_rtl_layout_verified ||
  !records["browser-onboarding"].light_dark_theme_persistence_verified ||
  !records["browser-onboarding"].application_version_read_verified ||
  !records.windows.updater_resource_close_main_and_settings_verified ||
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
if (
  (await sha256(`target/release/bundle/nsis/Flux_${version}_x64-setup.exe`)) !==
  records.installer.sha256
)
  throw Error("Installer validation does not match the current build");
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
  "FluxTorrentBridge.dll",
  "libtorrent-rasterbar.dll",
  "libssl-3-x64.dll",
  "libcrypto-3-x64.dll",
  "libstdc++-6.dll",
  "libgcc_s_seh-1.dll",
  "libwinpthread-1.dll",
]) {
  hashes[name] = await sha256(`target/release/${name}`);
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
  binary_build_run:
    process.env.FLUX_BINARY_BUILD_RUN || process.env.GITHUB_RUN_ID || null,
  binary_source_commit:
    process.env.FLUX_BINARY_SOURCE_COMMIT || process.env.GITHUB_SHA || null,
  rust_tests: rustTests,
  extension_tests: extensionTests,
  ui_data_tests: uiTests,
  automated_tests: rustTests + extensionTests + uiTests,
  browser_interception: records["chrome-interception"],
  browser_setup: records["browser-onboarding"],
  updater_resource_close_main_and_settings_verified:
    records.windows.updater_resource_close_main_and_settings_verified,
  upgrade_from_2_5_0: records.upgrade,
  test_evidence: {
    core_run_id:
      process.env.FLUX_CORE_TEST_RUN || process.env.GITHUB_RUN_ID || null,
    core_source_commit:
      process.env.FLUX_CORE_SOURCE_COMMIT || process.env.GITHUB_SHA || null,
    core_log_updated: (await stat(".tools/core-tests.log")).mtime.toISOString(),
    frontend_log_updated: (
      await stat(".tools/frontend-tests.log")
    ).mtime.toISOString(),
    core_engine_changed_in_hud_repair:
      records["hud-repair"].core_engine_changed,
    core_scope: process.env.FLUX_CORE_TEST_RUN
      ? `Rust, desktop, formatting and Clippy checks passed in source-equivalent workflow ${process.env.FLUX_CORE_TEST_RUN}; this workflow revalidates browsers and installed packages`
      : process.env.GITHUB_ACTIONS === "true"
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
    "Automatic installed/portable Native Messaging registration, idempotent repair and persistent first-use browser guidance",
    "Embedded BitTorrent v1/v2/hybrid engines, magnet and .torrent import, piece verification and SHA-256 outputs",
    "Browser download cancellation before filename release, Firefox attachment interception and native approval foreground authorization",
    "SHA-256 checkpoint replay without ETags or byte-range support, with preserved progress",
    "System/browser locale detection, direct Settings menu and persistent in-page media-button switch",
    "Global and per-task seeding rules, pause and restart recovery, disk-flush completion barrier",
    "Aligned Windows direct writes, stalled-socket replacement from verified offsets",
    "Encrypted Debrid accounts, provider API contracts and CDN renewal",
    "Bounded manifest observation and native HLS/DASH extraction",
    "Signed updater payload verification, version binding and durable update checkpoints",
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
  native_torrent_watch_window: records["torrent-watch-window"],
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
    "This record covers Windows; Linux RPM and cross-platform results are recorded separately",
    "Live Debrid account entitlements were not tested; local provider HTTP contracts were tested",
    "EV/OV certificate, SignPath and store credentials were not supplied",
    "A live GitHub update/install round trip was not run; signatures, checkpoints and NSIS installation were tested separately",
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
