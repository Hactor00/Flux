$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $workspace '.tools/extractor'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$sources = @(
    @{Name='yt-dlp.exe'; Url='https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe'; Hash='66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a'},
    @{Name='node.exe'; Url='https://nodejs.org/dist/v24.18.0/win-x64/node.exe'; Hash='9a4eb5f1c29c6a2e93852ead46b999e284a6a5ca8bab4d4e241d587d025a52de'},
    @{Name='yt-dlp-2026.08.19.tar.gz'; Url='https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.tar.gz'; Hash='072aad4f2a7604e92155f61a275a4752dc64046c8f6d90df3710525d94cd37c1'},
    @{Name='Node-LICENSE.txt'; Url='https://raw.githubusercontent.com/nodejs/node/v24.18.0/LICENSE'; Hash='148eacf7863ef4329224a29398623077200a27194aa075569faf4a0a85566ca5'},
    @{Name='yt-dlp-LICENSE.txt'; Url='https://raw.githubusercontent.com/yt-dlp/yt-dlp/2026.08.19/LICENSE'; Hash='7e12e5df4bae12cb21581ba157ced20e1986a0508dd10d0e8a4ab9a4cf94e85c'},
    @{Name='yt-dlp-THIRD-PARTY-LICENSES.txt'; Url='https://raw.githubusercontent.com/yt-dlp/yt-dlp/2026.08.19/THIRD_PARTY_LICENSES.txt'; Hash='472aefe951c7db35e1657c1d13fd337140511ed6f2b329205105ad441c5a02b7'}
)
foreach ($source in $sources) {
    $file = Join-Path $destination $source.Name
    if (-not (Test-Path -LiteralPath $file)) { Invoke-WebRequest -Uri $source.Url -OutFile $file }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $source.Hash) { throw "Extractor checksum mismatch: $($source.Name)" }
}
if (-not (Test-Path -LiteralPath "$destination/manifest.json")) {
    $manifest = @{url=$sources[0].Url;version='2026.08.19';sha256=$sources[0].Hash}
    [IO.File]::WriteAllText("$destination/manifest.json",($manifest | ConvertTo-Json),[Text.UTF8Encoding]::new($false))
}
Write-Output 'Pinned media extractor, JavaScript runtime, source and licenses verified.'
