param([string]$MediaRuntime, [switch]$SkipCompile, [switch]$UseLocalToolchain, [string]$RuntimePackage)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
& "$PSScriptRoot/prepare-extractor.ps1"
if ($UseLocalToolchain) { . "$PSScriptRoot/use-local-rust.ps1" }
& "$PSScriptRoot/build-torrent-runtime.ps1"
if ($RuntimePackage) {
    $RuntimePackage = (Resolve-Path -LiteralPath $RuntimePackage).Path
    $MediaRuntime = $RuntimePackage
    $sourceHashes = @{
        'ffmpeg-9.0.2.tar.xz'='8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e'
        'libxml2-2.15.4.tar.xz'='98087fd181d9070724f3fbc65c7377db03038eb92bd882374daff44940138821'
        'lame-3.100.tar.gz'='ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e'
    }
    foreach ($name in $sourceHashes.Keys) {
        if ((Get-FileHash -LiteralPath (Join-Path $RuntimePackage "media-source/$name")).Hash.ToLowerInvariant() -ne $sourceHashes[$name]) { throw "Reused media source checksum mismatch: $name" }
    }
    $configuration = & (Join-Path $RuntimePackage 'ffmpeg.exe') -version 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $configuration -notmatch 'ffmpeg version 9\.0\.2' -or $configuration -match '--enable-(gpl|nonfree)') { throw 'Reused media runtime must be the pinned LGPL build.' }
}
if (-not $MediaRuntime) {
    if (-not (Test-Path -LiteralPath "$workspace/.tools/media-build-path.txt")) { throw 'Build the media runtime with scripts/bootstrap-media-toolchain.ps1 or provide -MediaRuntime.' }
    $mediaWork = Get-Content "$workspace/.tools/media-build-path.txt"
    $MediaRuntime = Join-Path $mediaWork 'runtime/bin'
}
if (-not $SkipCompile) {
    cargo build --locked --release -p flux-cli -p flux-native-host
    if ($LASTEXITCODE -ne 0) { throw 'CLI/native host compilation failed.' }
}
& "$PSScriptRoot/copy-local-runtime.ps1" -Profile release
$stage = Join-Path $workspace '.tools/package'
New-Item -ItemType Directory -Force -Path $stage | Out-Null
foreach ($name in @('flux-cli.exe','flux-native-host.exe','libunwind.dll')) {
    Copy-Item -LiteralPath "$workspace/target/release/$name" -Destination $stage -Force
}
foreach ($name in @('FluxTorrentBridge.dll','libtorrent-rasterbar.dll','libssl-3-x64.dll','libcrypto-3-x64.dll','libstdc++-6.dll','libgcc_s_seh-1.dll','libwinpthread-1.dll')) {
    Copy-Item -LiteralPath "$workspace/target/release/$name" -Destination $stage -Force
}
$nativeLicenses = Join-Path $workspace '.tools/torrent-native/ucrt64/share/licenses'
if (-not (Test-Path -LiteralPath $nativeLicenses)) { throw 'Native torrent dependency licenses are missing.' }
$nativeLicenseStage = Join-Path $stage 'native-licenses'
New-Item -ItemType Directory -Force -Path $nativeLicenseStage | Out-Null
foreach ($entry in Get-ChildItem -LiteralPath $nativeLicenses) { Copy-Item -LiteralPath $entry.FullName -Destination $nativeLicenseStage -Recurse -Force }
foreach ($name in @('ffmpeg.exe','ffprobe.exe')) {
    if (-not (Test-Path -LiteralPath "$MediaRuntime/$name")) { throw "Missing media runtime: $name" }
    Copy-Item -LiteralPath "$MediaRuntime/$name" -Destination $stage -Force
}
foreach ($name in @('yt-dlp.exe','node.exe')) {
    Copy-Item -LiteralPath "$workspace/.tools/extractor/$name" -Destination $stage -Force
    Copy-Item -LiteralPath "$workspace/.tools/extractor/$name" -Destination "$workspace/target/release/$name" -Force
}
node "$PSScriptRoot/build-extensions.mjs"
if ($LASTEXITCODE -ne 0) { throw 'Extension generation failed.' }
foreach ($name in 'extension','extension-firefox','extension-firefox-modern') { Copy-Item -LiteralPath "$workspace/$name" -Destination $stage -Recurse -Force }
Copy-Item -LiteralPath "$workspace/LICENSE" -Destination $stage -Force
$sourceDirectory = Join-Path $stage 'media-source'
New-Item -ItemType Directory -Force -Path $sourceDirectory | Out-Null
if ($RuntimePackage) {
    foreach ($name in @('ffmpeg-9.0.2.tar.xz','libxml2-2.15.4.tar.xz','lame-3.100.tar.gz','FFmpeg-LICENSE.txt','libxml2-LICENSE.txt','LAME-LICENSE.txt','LLVM-runtime-LICENSE.txt')) {
        Copy-Item -LiteralPath (Join-Path $RuntimePackage "media-source/$name") -Destination $sourceDirectory -Force
    }
    Copy-Item -LiteralPath "$PSScriptRoot/build-media-runtime.sh" -Destination $sourceDirectory -Force
} else {
Copy-Item -LiteralPath "$workspace/.tools/media-sources/ffmpeg-9.0.2.tar.xz" -Destination $sourceDirectory -Force
Copy-Item -LiteralPath "$workspace/.tools/media-sources/libxml2-2.15.4.tar.xz" -Destination $sourceDirectory -Force
Copy-Item -LiteralPath "$workspace/.tools/media-sources/lame-3.100.tar.gz" -Destination $sourceDirectory -Force
Copy-Item -LiteralPath "$PSScriptRoot/build-media-runtime.sh" -Destination $sourceDirectory -Force
$mediaWork = Split-Path -Parent (Split-Path -Parent $MediaRuntime)
Copy-Item -LiteralPath "$mediaWork/ffmpeg-9.0.2/COPYING.LGPLv2.1" -Destination "$sourceDirectory/FFmpeg-LICENSE.txt" -Force
Copy-Item -LiteralPath "$mediaWork/libxml2-2.15.4/Copyright" -Destination "$sourceDirectory/libxml2-LICENSE.txt" -Force
Copy-Item -LiteralPath "$mediaWork/lame-3.100/COPYING" -Destination "$sourceDirectory/LAME-LICENSE.txt" -Force
$llvm = Get-ChildItem "$workspace/.tools/mingw" -Directory | Select-Object -First 1
Copy-Item -LiteralPath "$($llvm.FullName)/LICENSE.TXT" -Destination "$sourceDirectory/LLVM-runtime-LICENSE.txt" -Force
}
node "$PSScriptRoot/third-party-notices.mjs" $stage
if ($LASTEXITCODE -ne 0) { throw 'License collection failed.' }
$extractorLicense = Join-Path $stage 'licenses/Media-Extractor'
New-Item -ItemType Directory -Force $extractorLicense | Out-Null
Copy-Item -LiteralPath "$workspace/.tools/extractor/Node-LICENSE.txt","$workspace/.tools/extractor/yt-dlp-LICENSE.txt","$workspace/.tools/extractor/yt-dlp-THIRD-PARTY-LICENSES.txt","$workspace/.tools/extractor/manifest.json" -Destination $extractorLicense -Force
Copy-Item -LiteralPath "$workspace/.tools/extractor/yt-dlp-2026.08.19.tar.gz" -Destination $sourceDirectory -Force
$webviewLicense = Join-Path $stage 'licenses/Microsoft-WebView2-SDK'
$sdkCache = Join-Path $workspace '.tools/webview2-sdk'
New-Item -ItemType Directory -Force -Path $sdkCache | Out-Null
if (-not (Test-Path -LiteralPath "$sdkCache/sdk.zip")) {
    Invoke-WebRequest -Uri 'https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/1.0.3800.47/microsoft.web.webview2.1.0.3800.47.nupkg' -OutFile "$sdkCache/sdk.zip"
}
if ((Get-FileHash -LiteralPath "$sdkCache/sdk.zip").Hash -ne '56C9F26BDD07916A2D1949FB58A5C7E434DFA1173577DCA879206050C4E718DB') { throw 'WebView2 SDK checksum mismatch.' }
Expand-Archive -LiteralPath "$sdkCache/sdk.zip" -DestinationPath "$sdkCache/extracted" -Force
New-Item -ItemType Directory -Force -Path $webviewLicense | Out-Null
foreach ($name in @('LICENSE.txt','NOTICE.txt')) {
    Copy-Item -LiteralPath "$workspace/.tools/webview2-sdk/extracted/$name" -Destination $webviewLicense -Force
}
Write-Output "Release resources prepared: $stage"
