$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$directory = Join-Path $workspace '.tools/released-media'
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$archive = Join-Path $directory 'Flux-2.2.0-windows-x64.zip'
$expected = '256388fb3cef94d7759140d79af9e048fc38014996c2b9eb84f26a4b4170f941'
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri 'https://github.com/Hactor00/Flux/releases/download/v2.2.0/Flux-2.2.0-windows-x64.zip' -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'Released media package checksum mismatch.' }
Expand-Archive -LiteralPath $archive -DestinationPath $directory -Force
$runtime = Join-Path $directory 'Flux-2.2.0-windows-x64'
foreach ($name in 'ffmpeg.exe','ffprobe.exe','libunwind.dll','media-source/LLVM-runtime-LICENSE.txt') {
    if (-not (Test-Path -LiteralPath (Join-Path $runtime $name) -PathType Leaf)) { throw "Released runtime omitted $name" }
}
[IO.File]::WriteAllText((Join-Path $workspace '.tools/released-media-path.txt'), $runtime)
Write-Output 'Released LGPL runtime identity verified; application and torrent binaries will be rebuilt.'
