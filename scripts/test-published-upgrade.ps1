$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'This test uses a fresh disposable GitHub runner only.' }
$workspace = Split-Path -Parent $PSScriptRoot
$version = (Get-Content "$workspace/package.json" -Raw | ConvertFrom-Json).version
$data = Join-Path $env:LOCALAPPDATA 'Flux'
if ((Test-Path -LiteralPath (Join-Path $data 'tasks.sqlite')) -or (Test-Path 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Flux')) { throw 'Refusing to replace an existing profile or installation.' }
$feed = Invoke-RestMethod 'https://github.com/Hactor00/Flux/releases/latest/download/latest.json'
if ($feed.version -ne $version) { throw 'Publish the matching verified release before running this check.' }
$root = Join-Path ([IO.Path]::GetTempPath()) ('flux-published-update-' + [guid]::NewGuid().ToString('N'))
$installed = Join-Path $root 'installed'
$downloads = Join-Path $root 'downloads'
New-Item -ItemType Directory -Force $root,$downloads | Out-Null
$old = Join-Path $root 'Flux_2.6.5_x64-setup.exe'
Invoke-WebRequest 'https://github.com/Hactor00/Flux/releases/download/v2.6.5/Flux_2.6.5_x64-setup.exe' -OutFile $old
if ((Get-FileHash -LiteralPath $old).Hash.ToLowerInvariant() -ne '06e7a8d9a353fe9f182b259aa0a3a9dcb06c7bcf9ffc15ccffaf7d9788658978') { throw 'Previous installer identity mismatch.' }
try {
    $process = Start-Process -FilePath $old -ArgumentList "/S /D=$installed" -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw 'Previous version installation failed.' }
    node "$PSScriptRoot/upgrade-profile.mjs" create $installed $data $downloads (Join-Path $root 'profile.json')
    if ($LASTEXITCODE -ne 0) { throw 'Upgrade fixture profile failed.' }
    node "$PSScriptRoot/test-published-upgrade-ui.mjs" $installed $version $root
    if ($LASTEXITCODE -ne 0) { throw 'Published updater UI failed.' }
    $deadline = (Get-Date).AddSeconds(60)
    do {
        # NSIS replaces files between the old process exit and the relaunch.
        # A temporarily missing/locked executable is expected during that interval.
        $current = try { & "$installed/flux-cli.exe" --version 2>$null } catch { $null }
        $restarted = @(Get-Process -Name 'flux-desktop' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq (Join-Path $installed 'flux-desktop.exe') })
        # A process can exist before its engine opens the profile. Wait for its
        # authenticated broker discovery before requesting graceful shutdown.
        $broker = try { Get-Content -LiteralPath (Join-Path $data 'broker.json') -Raw | ConvertFrom-Json } catch { $null }
        if ($current -match "\b$([regex]::Escape($version))\b" -and $restarted.Count -gt 0 -and $broker.process_id -gt 0 -and $broker.process_id -in $restarted.Id) { break }
        if ((Get-Date) -gt $deadline) { throw 'In-app update did not replace installed binaries.' }
        Start-Sleep -Milliseconds 250
    } while ($true)
    Start-Process -FilePath "$installed/flux-desktop.exe" -ArgumentList '--shutdown' -WindowStyle Hidden -Wait
    $deadline = (Get-Date).AddSeconds(30)
    while (@(Get-Process -Name 'flux-desktop' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq (Join-Path $installed 'flux-desktop.exe') }).Count) {
        if ((Get-Date) -gt $deadline) { throw 'Updated application did not finish graceful shutdown.' }
        Start-Sleep -Milliseconds 100
    }
    node "$PSScriptRoot/upgrade-profile.mjs" verify $installed $data $downloads (Join-Path $root 'profile.json')
    if ($LASTEXITCODE -ne 0) { throw 'Published updater did not preserve the profile.' }
    @{version=$version;from_version='2.6.5';passed=$true;startup_notice=$true;repeated_manual_checks=$true;download_signature_verified_by_previous_app=$true;in_app_install=$true;history_preserved=$true;settings_preserved=$true;file_hash_preserved=$true;desktop_sha256=(Get-FileHash -LiteralPath "$installed/flux-desktop.exe").Hash.ToLowerInvariant()} | ConvertTo-Json | Set-Content "$workspace/docs/validation/published-upgrade.json" -Encoding utf8
} finally {
    if (Test-Path -LiteralPath "$installed/flux-desktop.exe") { Start-Process -FilePath "$installed/flux-desktop.exe" -ArgumentList '--shutdown' -WindowStyle Hidden -Wait }
    if (Test-Path -LiteralPath "$installed/uninstall.exe") { Start-Process -FilePath "$installed/uninstall.exe" -ArgumentList '/S' -WindowStyle Hidden -Wait }
}
