param([string]$PkgconfBin, [string]$Bash)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$archive = Join-Path $workspace '.tools/llvm-mingw.zip'
$destination = Join-Path $workspace '.tools/mingw'
New-Item -ItemType Directory -Path (Split-Path -Parent $archive) -Force | Out-Null
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri 'https://github.com/mstorsjo/llvm-mingw/releases/download/20261006/llvm-mingw-20261006-ucrt-x86_64.zip' -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne '317492c456aa27ee607a5919f1d2d38dcdc1112516a24d0bf4b00d078f52d17a') { throw 'LLVM MinGW checksum mismatch.' }
$llvm = Join-Path $destination 'llvm-mingw-20261006-ucrt-x86_64/bin'
if (-not (Test-Path -LiteralPath (Join-Path $llvm 'x86_64-w64-mingw32-clang.exe'))) { Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force }
& "$PSScriptRoot/build-media-runtime.ps1" -LlvmBin $llvm -PkgconfBin $PkgconfBin -Bash $Bash
