param([string]$LlvmBin, [string]$PkgconfBin, [string]$Bash)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
if (-not $Bash) {
    $gitBash = Join-Path $env:ProgramFiles 'Git/bin/bash.exe'
    if (Test-Path -LiteralPath $gitBash) { $Bash = $gitBash }
    else { $Bash = (Get-Command bash.exe -ErrorAction Stop).Source }
}
if (-not (Test-Path -LiteralPath $Bash)) { throw 'A Git Bash or MSYS2 Bash executable is required.' }
if (-not $LlvmBin) {
    $llvm = Get-ChildItem "$workspace/.tools/mingw" -Directory | Select-Object -First 1
    $LlvmBin = Join-Path $llvm.FullName 'bin'
}
if (-not $PkgconfBin) { $PkgconfBin = "$workspace/.tools/pkgconf/ucrt64/bin" }
if (-not (Test-Path -LiteralPath "$LlvmBin/x86_64-w64-mingw32-clang.exe")) { throw 'LLVM MinGW compiler is required.' }
if (-not (Test-Path -LiteralPath "$PkgconfBin/x86_64-w64-mingw32-pkg-config.exe")) { throw 'Target-aware pkgconf is required.' }
$sourceDirectory = "$workspace/.tools/media-sources"
New-Item -ItemType Directory -Force -Path $sourceDirectory | Out-Null
$sources = @(
    @{Name='ffmpeg-9.0.2.tar.xz'; Url='https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz'; Hash='8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e'},
    @{Name='libxml2-2.15.4.tar.xz'; Url='https://download.gnome.org/sources/libxml2/2.15/libxml2-2.15.4.tar.xz'; Hash='98087fd181d9070724f3fbc65c7377db03038eb92bd882374daff44940138821'},
    @{Name='lame-3.100.tar.gz'; Url='https://download.videolan.org/pub/contrib/lame/lame-3.100.tar.gz'; Hash='ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e'}
)
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('flux-media-build-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
foreach ($source in $sources) {
    $archive = Join-Path $sourceDirectory $source.Name
    if (-not (Test-Path -LiteralPath $archive)) { Invoke-WebRequest -Uri $source.Url -OutFile $archive }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $source.Hash) { throw "Source checksum mismatch: $($source.Name)" }
    tar -xf $archive -C $work
    if ($LASTEXITCODE -ne 0) { throw 'Source extraction failed.' }
}
$env:FLUX_MEDIA_WORK = $work
$env:FLUX_LLVM_BIN = $LlvmBin
$env:FLUX_PKGCONF_BIN = $PkgconfBin
& $Bash --noprofile --norc "$PSScriptRoot/build-media-runtime.sh"
if ($LASTEXITCODE -ne 0) { throw "Media build failed; logs: $work" }
Set-Content -LiteralPath "$workspace/.tools/media-build-path.txt" -Value $work
Write-Output "Media runtime: $work/runtime/bin"
