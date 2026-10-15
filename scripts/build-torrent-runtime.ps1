param([switch]$SkipDownload)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
if (-not $SkipDownload) {
    python -m pip install --target .tools/torrent-bootstrap zstandard==0.25.0
    if ($LASTEXITCODE -ne 0) { throw 'Torrent bootstrap dependency installation failed.' }
    python scripts/prepare-torrent-runtime.py
    if ($LASTEXITCODE -ne 0) { throw 'Torrent runtime preparation failed.' }
}
$root = Join-Path $workspace '.tools/torrent-native/ucrt64'
$compiler = Join-Path $root 'bin/g++.exe'
$previousPath = $env:PATH
try {
    $env:PATH = (Join-Path $root 'bin') + ';' + $previousPath
    $definitions = @('-DBOOST_ASIO_NO_DEPRECATED','-DBOOST_SYSTEM_USE_UTF8','-DBOOST_ASIO_ENABLE_CANCELIO','-DWIN32_LEAN_AND_MEAN','-DTORRENT_ABI_VERSION=2','-DTORRENT_USE_OPENSSL','-DTORRENT_USE_LIBCRYPTO','-DTORRENT_SSL_PEERS','-DOPENSSL_NO_SSL2','-DOPENSSL_NO_SSL3','-DOPENSSL_NO_TLS1','-DOPENSSL_NO_TLS1_1','-DOPENSSL_NO_DTLS1')
    $output = Join-Path $root 'bin/FluxTorrentBridge.dll'
    & $compiler '-std=c++17' '-O2' '-shared' '-fvisibility=hidden' @definitions '-I' (Join-Path $root 'include') 'native/torrent/bridge.cpp' '-L' (Join-Path $root 'lib') '-ltorrent-rasterbar' '-lssl' '-lcrypto' '-lbcrypt' '-lws2_32' '-liphlpapi' '-lcrypt32' '-o' $output
    if ($LASTEXITCODE -ne 0) { throw 'Native torrent bridge compilation failed.' }
    New-Item -ItemType Directory -Force target/debug,target/release | Out-Null
    foreach ($directory in @('target/debug','target/release')) {
        foreach ($name in @('FluxTorrentBridge.dll','libtorrent-rasterbar.dll','libssl-3-x64.dll','libcrypto-3-x64.dll','libstdc++-6.dll','libgcc_s_seh-1.dll','libwinpthread-1.dll')) {
            Copy-Item -LiteralPath (Join-Path $root "bin/$name") -Destination $directory -Force
        }
    }
    Write-Output 'Native BEP 52 torrent bridge compiled and staged.'
} finally { $env:PATH = $previousPath }
