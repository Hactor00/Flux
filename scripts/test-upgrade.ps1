param([string]$Installer)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
if (-not $env:CI) { throw 'The installer upgrade test runs only in an isolated CI worker.' }
if (Test-Path -LiteralPath 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Flux') { throw 'Upgrade validation will not replace an existing installation.' }
$version = (Get-Content "$workspace/package.json" -Raw | ConvertFrom-Json).version
if (-not $Installer) { $Installer = "$workspace/target/release/bundle/nsis/Flux_${version}_x64-setup.exe" }
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ('flux-upgrade-' + [guid]::NewGuid().ToString('N'))
$installed = Join-Path $root 'installed'
$local = Join-Path $root 'local'
$data = Join-Path $local 'Flux'
$downloads = Join-Path $root 'downloads'
New-Item -ItemType Directory -Force -Path $root,$local,$downloads | Out-Null
& "$PSScriptRoot/native-test-state.ps1" -Mode Read | Set-Content -LiteralPath (Join-Path $root 'registration.json')
$previousLocal = $env:LOCALAPPDATA
$old = Join-Path $root 'Flux_2.6.0_x64-setup.exe'
Invoke-WebRequest 'https://github.com/Hactor00/Flux/releases/download/v2.6.0/Flux_2.6.0_x64-setup.exe' -OutFile $old
if ((Get-FileHash -LiteralPath $old).Hash.ToLowerInvariant() -ne '5e72c44b3010f3a613356c3c1b6ce4320b2e7cf45ac4c386ae29bc4c266898f4') { throw '2.6.0 installer identity mismatch.' }
try {
    $env:LOCALAPPDATA = $local
    $process = Start-Process -FilePath $old -ArgumentList "/S /D=$installed" -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw '2.6.0 installation failed.' }
    node "$PSScriptRoot/upgrade-profile.mjs" create $installed $data $downloads (Join-Path $root 'profile.json')
    if ($LASTEXITCODE -ne 0) { throw '2.6.0 profile creation failed.' }
    $process = Start-Process -FilePath $Installer -ArgumentList "/S /D=$installed" -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw '2.6.0 to current installer upgrade failed.' }
    if ((Get-FileHash -LiteralPath "$installed/flux-desktop.exe").Hash -ne (Get-FileHash -LiteralPath "$workspace/target/release/flux-desktop.exe").Hash) { throw 'Upgrade did not replace the desktop executable.' }
    node "$PSScriptRoot/upgrade-profile.mjs" verify $installed $data $downloads (Join-Path $root 'profile.json')
    if ($LASTEXITCODE -ne 0) { throw 'Upgrade profile preservation failed.' }
    $firefox = Get-Content -LiteralPath "$data/io.flux.download.firefox.json" -Raw | ConvertFrom-Json
    if (@($firefox.allowed_extensions).Count -ne 2 -or 'firefox@fluxdownload.org' -notin $firefox.allowed_extensions -or 'flux@flux-download.local' -notin $firefox.allowed_extensions) { throw 'Upgrade omitted a Firefox identity.' }
    @{version=$version;from_version='2.6.0';passed=$true;history_preserved=$true;settings_preserved=$true;file_hash_preserved=$true;firefox_identities_registered=$true;installer_sha256=(Get-FileHash -LiteralPath $Installer).Hash.ToLowerInvariant()} | ConvertTo-Json | Set-Content -LiteralPath "$workspace/docs/validation/upgrade.json" -Encoding utf8
} finally {
    if (Test-Path -LiteralPath "$installed/uninstall.exe") {
        Start-Process -FilePath "$installed/uninstall.exe" -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait | Out-Null
        $deadline = (Get-Date).AddSeconds(30)
        while (Test-Path -LiteralPath "$installed/flux-desktop.exe") { if ((Get-Date) -gt $deadline) { throw 'Temporary upgrade installation cleanup failed.' }; Start-Sleep -Milliseconds 100 }
    }
    & "$PSScriptRoot/native-test-state.ps1" -Mode Restore -Snapshot (Join-Path $root 'registration.json')
    $env:LOCALAPPDATA = $previousLocal
}
