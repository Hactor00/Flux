param([switch]$SkipCompile, [switch]$UseLocalToolchain, [string]$MediaRuntime, [string]$PortableStage, [string]$RuntimePackage, [string]$SigningThumbprint = $env:FLUX_SIGNING_THUMBPRINT)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
if ($UseLocalToolchain) { . "$PSScriptRoot/use-local-rust.ps1" }
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    $signingPath = Join-Path $env:LOCALAPPDATA 'FluxSigning/updater.key'
    if (-not (Test-Path -LiteralPath $signingPath)) { throw 'TAURI_SIGNING_PRIVATE_KEY is required for release builds.' }
    $env:TAURI_SIGNING_PRIVATE_KEY = $signingPath
}
& "$PSScriptRoot/prepare-release.ps1" -SkipCompile:$SkipCompile -UseLocalToolchain:$UseLocalToolchain -MediaRuntime $MediaRuntime -RuntimePackage $RuntimePackage
$configuration = @('--config', 'src-tauri/tauri.release.conf.json')
if ($SigningThumbprint) {
    & "$PSScriptRoot/sign-windows.ps1" -Files @("$workspace/target/release/flux-cli.exe", "$workspace/target/release/flux-native-host.exe", "$workspace/target/release/FluxTorrentBridge.dll") -Thumbprint $SigningThumbprint
    foreach ($name in 'flux-cli.exe','flux-native-host.exe','FluxTorrentBridge.dll') { Copy-Item -LiteralPath "$workspace/target/release/$name" -Destination "$workspace/.tools/package/$name" -Force }
    $signingConfig = Join-Path $workspace '.tools/tauri-certificate.json'
    @{bundle=@{windows=@{signCommand=@{cmd='powershell.exe';args=@('-NoProfile','-NonInteractive','-File',"$PSScriptRoot/sign-windows.ps1",'-Thumbprint',$SigningThumbprint,'-Files','%1')}}}} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $signingConfig -Encoding utf8
    $configuration += @('--config', $signingConfig)
}
npm run tauri build -- @configuration --bundles nsis --no-binary-patching
if ($LASTEXITCODE -ne 0) { throw 'Desktop build/package failed.' }
& "$PSScriptRoot/export-release.ps1" -PortableStage $PortableStage
