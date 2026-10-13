param([Parameter(Mandatory)][string]$Artifact, [Parameter(Mandatory)][string]$OutputDirectory, [string]$NotesFile)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
$artifactPath = (Resolve-Path -LiteralPath $Artifact).Path
$configuration = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    $localKey = Join-Path $env:LOCALAPPDATA 'FluxSigning/updater.key'
    if (-not (Test-Path -LiteralPath $localKey)) { throw 'TAURI_SIGNING_PRIVATE_KEY is required for signed update artifacts.' }
    $env:TAURI_SIGNING_PRIVATE_KEY = $localKey
}
$previousSigningKey = $env:TAURI_SIGNING_PRIVATE_KEY
$previousSigningPath = $env:TAURI_SIGNING_PRIVATE_KEY_PATH
try {
    if ([IO.File]::Exists($env:TAURI_SIGNING_PRIVATE_KEY)) {
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $env:TAURI_SIGNING_PRIVATE_KEY
        $env:TAURI_SIGNING_PRIVATE_KEY = $null
    }
    & node node_modules/@tauri-apps/cli/tauri.js signer sign --app-version $version $artifactPath
    if ($LASTEXITCODE -ne 0) { throw 'Update artifact signing failed.' }
} finally { $env:TAURI_SIGNING_PRIVATE_KEY = $previousSigningKey; $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $previousSigningPath }
& (Join-Path $workspace 'target/release/flux-cli.exe') verify-update $artifactPath ($artifactPath + '.sig') $configuration.plugins.updater.pubkey --expected-version $version
if ($LASTEXITCODE -ne 0) { throw 'Update signature does not match the application verification key.' }
$signature = (Get-Content -LiteralPath ($artifactPath + '.sig') -Raw).Trim()
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$name = Split-Path -Leaf $artifactPath
$notes = if ($NotesFile) { Get-Content -LiteralPath $NotesFile -Raw } else { "Flux $version" }
$manifest = @{version=$version;notes=$notes;pub_date=[DateTimeOffset]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ');platforms=@{'windows-x86_64'=@{url="https://github.com/Hactor00/Flux/releases/download/v$version/$name";signature=$signature}}}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'latest.json'), ($manifest | ConvertTo-Json -Depth 8) + "`n")
$signatureTarget = [IO.Path]::GetFullPath((Join-Path $OutputDirectory ($name + '.sig')))
if (-not [string]::Equals(($artifactPath + '.sig'), $signatureTarget, [StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath ($artifactPath + '.sig') -Destination $signatureTarget -Force
}
