param(
    [ValidatePattern('^[a-p]{32}$')][string]$ExtensionId,
    [Parameter(Mandatory=$true)][string]$HostExecutable
)
$ErrorActionPreference = 'Stop'
$hostPath = (Resolve-Path -LiteralPath $HostExecutable).Path
if ([IO.Path]::GetExtension($hostPath) -ne '.exe') { throw 'HostExecutable must reference the compiled native host executable.' }
$dataDir = Join-Path $env:LOCALAPPDATA 'Flux'
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
$manifestPath = Join-Path $dataDir 'native-host.json'
$extensionIds = @(Get-Content -LiteralPath "$PSScriptRoot/../native-host/chromium-extension-ids.json" -Raw | ConvertFrom-Json)
$extensionIds += (Get-Content -LiteralPath "$PSScriptRoot/../extension/extension-id.txt" -Raw).Trim()
if ($ExtensionId) { $extensionIds += $ExtensionId }
$extensionIds = @($extensionIds | Sort-Object -Unique)
if ($extensionIds.Count -gt 8 -or @($extensionIds | Where-Object { $_ -notmatch '^[a-p]{32}$' }).Count) { throw 'Invalid packaged Chromium extension IDs.' }
$origins = @($extensionIds | ForEach-Object { "chrome-extension://$_/" })
$manifest = @{ name='io.flux.download'; description='Flux download bridge'; path=$hostPath; type='stdio'; allowed_origins=$origins }
$utf8 = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 4), $utf8)
[IO.File]::WriteAllText((Join-Path $dataDir 'allowed-origins.json'), (ConvertTo-Json -InputObject $origins), $utf8)
$firefoxId = (Get-Content -LiteralPath "$PSScriptRoot/../extension-firefox/extension-id.txt" -Raw).Trim()
$firefoxIds = @(Get-Content -LiteralPath "$PSScriptRoot/../native-host/firefox-extension-ids.json" -Raw | ConvertFrom-Json)
$firefoxPath = Join-Path $dataDir 'io.flux.download.firefox.json'
$firefox = @{ name='io.flux.download'; description='Flux Native Messaging Bridge for Firefox'; path=$hostPath; type='stdio'; allowed_extensions=$firefoxIds }
[IO.File]::WriteAllText($firefoxPath, ($firefox | ConvertTo-Json -Depth 4), $utf8)
foreach ($browser in 'Google\Chrome','Microsoft\Edge','BraveSoftware\Brave-Browser','Chromium','Mozilla') {
    $registryPath = "HKCU:\Software\$browser\NativeMessagingHosts\io.flux.download"
    New-Item -Path $registryPath -Force | Out-Null
    Set-Item -Path $registryPath -Value $(if ($browser -eq 'Mozilla') { $firefoxPath } else { $manifestPath })
}
New-Item -Path 'HKCU:\Software\Flux' -Force | Out-Null
New-ItemProperty -Path 'HKCU:\Software\Flux' -Name 'BridgeDirectory' -Value (Split-Path -Parent $hostPath) -PropertyType String -Force | Out-Null
Write-Output "Native Messaging registered for Chrome, Edge, Brave, Chromium and Firefox ($firefoxId)."
