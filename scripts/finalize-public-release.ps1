param([switch]$Signed)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $workspace 'package.json') -Raw | ConvertFrom-Json).version
$source = Join-Path $workspace $(if ($Signed) {'signed'} else {'release'})
$destination = Join-Path $workspace 'public-release'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$names = @("Flux_${version}_x64-setup.exe","Flux-$version-windows-x64.zip","Flux-$version-chrome.zip","Flux-$version-firefox.zip","Flux-$version-firefox-120.zip","Flux-$version-source.zip")
foreach ($name in $names) {
    $files = @(Get-ChildItem -LiteralPath $source -File -Recurse | Where-Object Name -eq $name)
    if ($files.Count -ne 1) { throw "Expected exactly one release file: $name" }
    if ($Signed -and $name.EndsWith('.exe') -and (Get-AuthenticodeSignature -LiteralPath $files[0].FullName).Status -ne 'Valid') { throw "Installer has no valid Authenticode signature: $name" }
    Copy-Item -LiteralPath $files[0].FullName -Destination (Join-Path $destination $name) -Force
}
if ($Signed) {
    $temporary = Join-Path $workspace ('.tools/signed-check-' + [guid]::NewGuid().ToString('N'))
    Expand-Archive -LiteralPath (Join-Path $destination "Flux-$version-windows-x64.zip") -DestinationPath $temporary
    foreach ($name in 'flux-desktop.exe','flux-cli.exe','flux-native-host.exe') {
        $files = @(Get-ChildItem -LiteralPath $temporary -Filter $name -File -Recurse)
        if ($files.Count -ne 1 -or (Get-AuthenticodeSignature -LiteralPath $files[0].FullName).Status -ne 'Valid') { throw "Portable executable has no valid Authenticode signature: $name" }
    }
}
$checksums = foreach ($name in $names) { (Get-FileHash -LiteralPath (Join-Path $destination $name) -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $name }
$checksums | Set-Content -LiteralPath (Join-Path $destination 'SHA256SUMS.txt') -Encoding ascii
@{version=$version;signed=[bool]$Signed;platform='Windows x64'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'release-info.json') -Encoding utf8
