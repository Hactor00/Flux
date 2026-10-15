$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
foreach ($line in Get-Content release/SHA256SUMS.txt) {
    $parts = $line -split '  ',2
    $actual = (Get-FileHash -LiteralPath "release/$($parts[1])").Hash.ToLowerInvariant()
    if ($actual -ne $parts[0]) { throw "Artifact checksum mismatch: $($parts[1])" }
}
$test = Get-Content docs/validation/installer.json -Raw | ConvertFrom-Json
$installer = "release/Flux_${version}_x64-setup.exe"
if ((Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant() -ne $test.sha256) { throw 'Release installer differs from the tested installer.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path "release/Flux-$version-windows-x64.zip").Path)
try {
    foreach ($name in @('flux-desktop.exe','flux-cli.exe','flux-native-host.exe','ffmpeg.exe','ffprobe.exe','yt-dlp.exe','node.exe','libunwind.dll','WebView2Loader.dll','FluxTorrentBridge.dll','libtorrent-rasterbar.dll','libssl-3-x64.dll','libcrypto-3-x64.dll','libstdc++-6.dll','libgcc_s_seh-1.dll','libwinpthread-1.dll')) {
        $entry = $zip.GetEntry("Flux-$version-windows-x64/$name")
        if (-not $entry) { throw "Portable archive omitted: $name" }
        $stream = $entry.Open()
        try { $hash = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($stream)) } finally { $stream.Dispose() }
        if ($hash -ne (Get-FileHash -LiteralPath "target/release/$name").Hash) { throw "Archived executable differs: $name" }
    }
} finally { $zip.Dispose() }
foreach ($target in @(@{Directory='extension';Suffix='chrome'},@{Directory='extension-firefox-modern';Suffix='firefox';Minimum='140.0'},@{Directory='extension-firefox';Suffix='firefox-120';Minimum='120.0'})) {
    $archive = "release/Flux-$version-$($target.Suffix).zip"
    $zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $archive).Path)
    try {
        foreach ($file in Get-ChildItem -LiteralPath $target.Directory -File -Recurse) {
            $relative = [IO.Path]::GetRelativePath((Resolve-Path -LiteralPath $target.Directory).Path,$file.FullName).Replace('\','/')
            $entry = $zip.GetEntry($relative)
            if (-not $entry) { throw "Extension archive omitted ($($target.Suffix)): $relative" }
            $stream = $entry.Open()
            try { $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)) } finally { $stream.Dispose() }
            if ($hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) { throw "Extension archived source differs ($($target.Suffix)): $relative" }
        }
        $manifest = Get-Content -LiteralPath "$($target.Directory)/manifest.json" -Raw | ConvertFrom-Json
        if ($manifest.version -ne $version) { throw 'Extension archive version mismatch.' }
        if ($target.Minimum -and ($manifest.browser_specific_settings.gecko.strict_min_version -ne $target.Minimum -or $manifest.background.service_worker)) { throw 'Firefox archive manifest mismatch.' }
    } finally { $zip.Dispose() }
}
$zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path "release/Flux-$version-source.zip").Path)
try {
    if (-not $zip.GetEntry('scripts/verify-browser-stores.ps1')) { throw 'Source archive omitted the browser store verifier.' }
    foreach ($name in @('crates/core/src/disk.rs','crates/core/src/torrent.rs','crates/core/tests/torrents.rs','extension/media-observer.js','extension/session.js','src/components/TorrentDetails.tsx','src/components/CollectionPicker.tsx','scripts/package-browser-stores.ps1','.github/workflows/browser-stores.yml','native-host/chromium-extension-ids.json','docs/releases/v2.0.0.md')) {
        if (-not $zip.GetEntry($name)) { throw "Source archive omitted: $name" }
    }
    foreach ($name in @('crates/core/src/torrent_native.rs','crates/core/src/disk_writer.rs','crates/core/src/debrid.rs','crates/core/src/manifest.rs','crates/core/src/signing.rs','native/torrent/bridge.cpp','native/torrent/windows-runtime.lock.json','vendor/reqwest/FLUX-PATCH.md','vendor/librqbit/FLUX-PATCH.md','src/components/Updater.tsx','scripts/sign-update.ps1','docs/release-2.1.tr.md')) {
        if (-not $zip.GetEntry($name)) { throw "Source archive omitted: $name" }
    }
    foreach ($name in @('.gitignore','.cargo/config.toml','.github/workflows/ci.yml','.github/workflows/release.yml','Cargo.lock','src-tauri/src/main.rs','src-tauri/src/power.rs','src-tauri/src/startup.rs','src-tauri/src/file_drag.rs','src-tauri/capabilities/power.json','crates/core/src/preferences.rs','crates/core/src/transport.rs','crates/core/src/healing.rs','extension/browser-api.js','extension-firefox/manifest.json','extension-firefox/extension-id.txt','native-host/io.flux.download.firefox.json','crates/native-host/src/caller.rs','scripts/build-extensions.mjs','scripts/test-firefox.mjs','scripts/prepare-firefox-tests.ps1','extension/background.js','extension/healing.js','docs/validation/release.json')) {
        if (-not $zip.GetEntry($name)) { throw "Source archive omitted: $name" }
    }
} finally { $zip.Dispose() }
Write-Output 'All release hashes, tested installer identity, archived binaries and required source files verified.'
