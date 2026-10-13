param([switch]$SkipCompile, [switch]$UseLocalToolchain, [string]$MediaRuntime, [string]$PortableStage)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
if ($UseLocalToolchain) { . "$PSScriptRoot/use-local-rust.ps1" }
& "$PSScriptRoot/prepare-release.ps1" -SkipCompile:$SkipCompile -UseLocalToolchain:$UseLocalToolchain -MediaRuntime $MediaRuntime
npm run tauri build -- --config src-tauri/tauri.release.conf.json --bundles nsis
if ($LASTEXITCODE -ne 0) { throw 'Desktop build/package failed.' }
& "$PSScriptRoot/export-release.ps1" -PortableStage $PortableStage
