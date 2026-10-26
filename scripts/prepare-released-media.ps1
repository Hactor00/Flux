$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$directory = Join-Path $workspace '.tools/released-media'
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$archive = Join-Path $directory 'Flux-2.6.0-windows-x64.zip'
$expected = '5480495b4e72fb1537e4ad29e41b148a320365322c555b35dc65cb74ca5efed2'
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri 'https://github.com/Hactor00/Flux/releases/download/v2.6.0/Flux-2.6.0-windows-x64.zip' -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'Released media package checksum mismatch.' }
Expand-Archive -LiteralPath $archive -DestinationPath $directory -Force
$runtime = Join-Path $directory 'Flux-2.6.0-windows-x64'
$sourceArchive = Join-Path $directory 'Flux-2.6.0-media-source.zip'
if (-not (Test-Path -LiteralPath $sourceArchive)) {
    Invoke-WebRequest -Uri 'https://github.com/Hactor00/Flux/releases/download/v2.6.0/Flux-2.6.0-media-source.zip' -OutFile $sourceArchive
}
if ((Get-FileHash -LiteralPath $sourceArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne 'd5f48634a70a749ce6020c104ab8cbbc72d53e4b9c2b6b7929a4f8e185c3fdbc') { throw 'Released media source checksum mismatch.' }
Expand-Archive -LiteralPath $sourceArchive -DestinationPath (Join-Path $runtime 'media-source') -Force
foreach ($name in 'ffmpeg.exe','ffprobe.exe','libunwind.dll','media-source/LLVM-runtime-LICENSE.txt') {
    if (-not (Test-Path -LiteralPath (Join-Path $runtime $name) -PathType Leaf)) { throw "Released runtime omitted $name" }
}
$encoders = & (Join-Path $runtime 'ffmpeg.exe') -encoders 2>&1 | Out-String
if ($LASTEXITCODE -ne 0 -or $encoders -notmatch '\bmjpeg\b') { throw 'Released media runtime lacks video preview encoding.' }
[IO.File]::WriteAllText((Join-Path $workspace '.tools/released-media-path.txt'), $runtime)
Write-Output 'Released LGPL runtime identity verified; application and torrent binaries will be rebuilt.'
