param([string]$PortableStage)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
node "$PSScriptRoot/redact-public-validation.mjs"
if ($LASTEXITCODE -ne 0) { throw 'Public validation redaction failed.' }
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$releaseDirectory = Join-Path $workspace 'release'
New-Item -ItemType Directory -Force -Path $releaseDirectory | Out-Null
$portable = if ($PortableStage) { [IO.Path]::GetFullPath((Join-Path $workspace $PortableStage)) } else { Join-Path $releaseDirectory "Flux-$version-windows-x64" }
if (-not $portable.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $portable) -ne "Flux-$version-windows-x64") { throw 'Portable stage must be a versioned directory inside the workspace.' }
New-Item -ItemType Directory -Force -Path $portable | Out-Null
foreach ($name in @('flux-desktop.exe','flux-cli.exe','flux-native-host.exe','ffmpeg.exe','ffprobe.exe','yt-dlp.exe','node.exe','libunwind.dll','WebView2Loader.dll')) {
    Copy-Item -LiteralPath "$workspace/target/release/$name" -Destination $portable -Force
}
foreach ($name in @('FluxTorrentBridge.dll','libtorrent-rasterbar.dll','libssl-3-x64.dll','libcrypto-3-x64.dll','libstdc++-6.dll','libgcc_s_seh-1.dll','libwinpthread-1.dll')) {
    Copy-Item -LiteralPath "$workspace/target/release/$name" -Destination $portable -Force
}
$nativeLicenseStage = Join-Path $portable 'native-licenses'
New-Item -ItemType Directory -Force -Path $nativeLicenseStage | Out-Null
foreach ($entry in Get-ChildItem -LiteralPath "$workspace/.tools/package/native-licenses") { Copy-Item -LiteralPath $entry.FullName -Destination $nativeLicenseStage -Recurse -Force }
foreach ($name in @('extension','extension-firefox','extension-firefox-modern','licenses','LICENSE','THIRD-PARTY-NOTICES.md','dependency-inventory.json')) {
    Copy-Item -LiteralPath "$workspace/.tools/package/$name" -Destination $portable -Recurse -Force
}
Copy-Item -LiteralPath "$workspace/README.md" -Destination $portable -Force
Copy-Item -LiteralPath "$workspace/README.tr.md" -Destination $portable -Force
[IO.File]::WriteAllText((Join-Path $portable '.flux-portable'), 'Flux portable data layout 1')
[IO.File]::WriteAllText((Join-Path $portable 'MEDIA-SOURCE.txt'), "Matching media runtime sources and build instructions: https://github.com/Hactor00/Flux/releases/download/v$version/Flux-$version-media-source.zip`nDependency notices and licenses remain included in this package.`n")
Copy-Item -LiteralPath "$workspace/target/release/bundle/nsis/Flux_${version}_x64-setup.exe" -Destination $releaseDirectory -Force
function Write-Zip([string]$Directory, [string]$Archive, [bool]$IncludeRoot) {
    $temporary = $Archive + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    $directoryPath = (Resolve-Path -LiteralPath $Directory).Path.TrimEnd([System.IO.Path]::DirectorySeparatorChar)
    $stream = [System.IO.File]::Create($temporary)
    $zip = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($file in Get-ChildItem -LiteralPath $directoryPath -File -Recurse | Sort-Object FullName) {
            $relative = [System.IO.Path]::GetRelativePath($directoryPath, $file.FullName).Replace('\','/')
            if ($IncludeRoot) { $relative = (Split-Path -Leaf $directoryPath) + '/' + $relative }
            $entry = $zip.CreateEntry($relative, [System.IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(1980,1,1,0,0,0,[TimeSpan]::Zero)
            $entryStream = $entry.Open()
            $inputStream = [System.IO.File]::OpenRead($file.FullName)
            try { $inputStream.CopyTo($entryStream) } finally { $inputStream.Dispose(); $entryStream.Dispose() }
        }
    } finally { $zip.Dispose(); $stream.Dispose() }
    Move-Item -LiteralPath $temporary -Destination $Archive -Force
}
Write-Zip $portable "$releaseDirectory/Flux-$version-windows-x64.zip" $true
Write-Zip "$workspace/.tools/package/media-source" "$releaseDirectory/Flux-$version-media-source.zip" $false
$paf = Join-Path $workspace ('.tools/portableapps-' + [guid]::NewGuid().ToString('N') + '/FluxPortable')
$pafApp = Join-Path $paf 'App/Flux'
New-Item -ItemType Directory -Force -Path $pafApp,(Join-Path $paf 'App/AppInfo'),(Join-Path $paf 'Data/Downloads'),(Join-Path $paf 'Other/Source') | Out-Null
foreach ($entry in Get-ChildItem -LiteralPath $portable -Force) { Copy-Item -LiteralPath $entry.FullName -Destination $pafApp -Recurse -Force }
rustc "$PSScriptRoot/portable-launcher.rs" -C opt-level=z -C strip=symbols -o (Join-Path $paf 'FluxPortable.exe')
if ($LASTEXITCODE -ne 0) { throw 'Portable launcher compilation failed.' }
Copy-Item -LiteralPath "$PSScriptRoot/portable-launcher.rs" -Destination (Join-Path $paf 'Other/Source')
Copy-Item -LiteralPath "$workspace/src-tauri/icons/icon.ico" -Destination (Join-Path $paf 'App/AppInfo/appicon.ico')
@"
[Format]
Type=PortableApps.comFormat
Version=3.9
[Details]
Name=Flux Portable
AppID=FluxPortable
Publisher=Rıza Altıner
Homepage=https://fluxdownload.org
Category=Internet
Description=Open-source download manager
Language=Multilingual
[License]
Shareable=true
OpenSource=true
Freeware=true
CommercialUse=true
[Version]
PackageVersion=$version.0
DisplayVersion=$version
[Control]
Icons=1
Start=FluxPortable.exe
"@ | Set-Content -LiteralPath (Join-Path $paf 'App/AppInfo/appinfo.ini') -Encoding utf8NoBOM
[IO.File]::WriteAllText((Join-Path $paf 'help.html'), '<!doctype html><meta charset="utf-8"><title>Flux Portable</title><h1>Flux Portable</h1><p>Start FluxPortable.exe. Settings and history stay in Data/Flux; downloads default to Data/Downloads. Launch Flux after moving the folder to repair the browser bridge.</p><p>Browser session credentials remain protected by Windows and may need to be refreshed on a different computer. This package is not certified by PortableApps.com.</p><a href="https://fluxdownload.org">Help and downloads</a>')
Write-Zip $paf "$releaseDirectory/Flux-$version-portableapps.zip" $true
Write-Zip "$workspace/extension" "$releaseDirectory/Flux-$version-chrome.zip" $false
Write-Zip "$workspace/extension-firefox-modern" "$releaseDirectory/Flux-$version-firefox.zip" $false
Write-Zip "$workspace/extension-firefox" "$releaseDirectory/Flux-$version-firefox-120.zip" $false
# Enumerate source through Git's ignore rules; no build cache, user data or secrets enter the source archive.
$sourceDirectory = Join-Path $workspace ('.tools/export-source-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $sourceDirectory | Out-Null
$trackedSource = @(git ls-files)
$untrackedSource = @(git ls-files --others --exclude-standard)
$sourcePaths = $trackedSource + $untrackedSource
foreach ($relative in $sourcePaths | Select-Object -Unique) {
    if (-not $relative) { continue }
    $sourcePath = [System.IO.Path]::GetFullPath((Join-Path $workspace $relative))
    if (-not $sourcePath.StartsWith($workspace + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Source archive path escaped workspace.' }
    $targetPath = Join-Path $sourceDirectory $relative
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $targetPath) | Out-Null
    Copy-Item -LiteralPath $sourcePath -Destination $targetPath -Force
}
Write-Zip $sourceDirectory "$releaseDirectory/Flux-$version-source.zip" $false
$checksums = Get-ChildItem -LiteralPath $releaseDirectory -File | Where-Object Extension -In '.zip','.exe','.rpm' | Sort-Object Name | ForEach-Object {
    (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $_.Name
}
$checksums | Set-Content -LiteralPath "$releaseDirectory/SHA256SUMS.txt" -Encoding ascii
Write-Output "Release exported: $releaseDirectory"
