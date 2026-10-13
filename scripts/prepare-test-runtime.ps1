$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$directory = Join-Path $workspace '.tools/test-runtime'
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$archive = Join-Path $directory 'ffmpeg-8.1.2-full_build.zip'
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri 'https://github.com/GyanD/codexffmpeg/releases/download/8.1.2/ffmpeg-8.1.2-full_build.zip' -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne 'b8cdefab5f50590a076c27c2b56b0294a0e6154faded28ba1ba05ebc4f801f57') { throw 'Fixture FFmpeg checksum mismatch.' }
$executable = Join-Path $directory 'ffmpeg-8.1.2-full_build/bin/ffmpeg.exe'
if (-not (Test-Path -LiteralPath $executable)) { Expand-Archive -LiteralPath $archive -DestinationPath $directory -Force }
$env:FLUX_FIXTURE_FFMPEG = $executable
if ($env:GITHUB_ENV) { "FLUX_FIXTURE_FFMPEG=$executable" >> $env:GITHUB_ENV }
Write-Output 'Pinned fixture encoder ready. It is test-only and is not shipped with Flux.'
