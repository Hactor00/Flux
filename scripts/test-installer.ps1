param([string]$Installer)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
if (-not $Installer) { $Installer = "target/release/bundle/nsis/Flux_${version}_x64-setup.exe" }
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
if (Test-Path -LiteralPath 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Flux') {
    throw 'An existing Flux installation is present. Installer isolation test will not replace it.'
}
if (Get-Process -Name flux-desktop -ErrorAction SilentlyContinue | Where-Object { -not $_.HasExited }) {
    throw 'The isolated installer test requires the running Flux desktop to be closed first. Its application-wide single-instance lock prevents a separate test profile from starting.'
}
$root = Join-Path ([System.IO.Path]::GetTempPath()) ('flux-install-test-' + [guid]::NewGuid().ToString('N'))
$installDirectory = Join-Path $root 'installed'
$localData = Join-Path $root 'local'
New-Item -ItemType Directory -Path $root,$localData | Out-Null
$startupSnapshot = Join-Path $root 'startup.json'
& "$PSScriptRoot/startup-test-state.ps1" -Mode Read | Set-Content -LiteralPath $startupSnapshot
$previousLocalData = $env:LOCALAPPDATA
$previousPath = $env:PATH
$keys = @(
    'HKCU:/Software/Google/Chrome/NativeMessagingHosts/io.flux.download',
    'HKCU:/Software/Microsoft/Edge/NativeMessagingHosts/io.flux.download',
    'HKCU:/Software/BraveSoftware/Brave-Browser/NativeMessagingHosts/io.flux.download',
    'HKCU:/Software/Chromium/NativeMessagingHosts/io.flux.download',
    'HKCU:/Software/Mozilla/NativeMessagingHosts/io.flux.download'
)
$previousKeys = @{}
foreach ($key in $keys) { if (Test-Path -LiteralPath $key) { $previousKeys[$key] = (Get-Item -LiteralPath $key).GetValue('') } }
$previousOwner = if (Test-Path -LiteralPath 'HKCU:/Software/Flux') { (Get-Item -LiteralPath 'HKCU:/Software/Flux').GetValue('BridgeDirectory') } else { $null }
try {
    $env:LOCALAPPDATA = $localData
    $process = Start-Process -FilePath $installerPath -ArgumentList "/S /D=$installDirectory" -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
    foreach ($name in @('flux-desktop.exe','flux-native-host.exe','flux-cli.exe','ffmpeg.exe','ffprobe.exe','yt-dlp.exe','node.exe','libunwind.dll','WebView2Loader.dll','extension/manifest.json','extension-firefox/manifest.json','extension-firefox-modern/manifest.json')) {
        if (-not (Test-Path -LiteralPath "$installDirectory/$name")) { throw "Installer omitted: $name" }
    }
    foreach ($name in @('flux-desktop.exe','flux-native-host.exe','flux-cli.exe','ffmpeg.exe','ffprobe.exe','yt-dlp.exe','node.exe','libunwind.dll','WebView2Loader.dll')) {
        if ((Get-FileHash -LiteralPath "$installDirectory/$name").Hash -ne (Get-FileHash -LiteralPath "$workspace/target/release/$name").Hash) { throw "Installed executable differs from the current build: $name" }
    }
    foreach ($folder in 'extension','extension-firefox','extension-firefox-modern') {
        foreach ($name in 'manifest.json','browser-api.js','background.js','bridge.js','content.js','media-observer.js','session.js','healing.js','locales/en-US.json','locales/tr-TR.json') {
            if ((Get-FileHash -LiteralPath "$installDirectory/$folder/$name").Hash -ne (Get-FileHash -LiteralPath "$workspace/$folder/$name").Hash) { throw "Installed extension differs from validated source: $folder/$name" }
        }
    }
    $manifest = Get-Content -LiteralPath "$localData/Flux/native-host.json" -Raw | ConvertFrom-Json
    if ($manifest.path -ne "$installDirectory\flux-native-host.exe") { throw 'Installed host manifest has the wrong executable.' }
    foreach ($key in $keys) { $file = if ($key -like '*/Mozilla/*') {'io.flux.download.firefox.json'} else {'native-host.json'}; if ((Get-Item -LiteralPath $key).GetValue('') -ne "$localData\Flux\$file") { throw "Native host registry mismatch: $key" } }
    # Prove bundled tools work without the developer toolchain on PATH.
    $firefoxManifest = Get-Content -LiteralPath "$localData/Flux/io.flux.download.firefox.json" -Raw | ConvertFrom-Json
    if ($firefoxManifest.path -ne $manifest.path -or $firefoxManifest.allowed_extensions.Count -ne 1 -or $firefoxManifest.allowed_extensions[0] -ne 'flux@flux-download.local' -or $firefoxManifest.allowed_origins) { throw 'Firefox registration schema is invalid.' }
    1..2 | ForEach-Object { $repair = Start-Process -FilePath "$installDirectory/flux-desktop.exe" -ArgumentList '--register-browser' -WindowStyle Hidden -PassThru -Wait; if ($repair.ExitCode -ne 0) { throw 'Browser bridge repair failed.' } }
    1..2 | ForEach-Object { $remove = Start-Process -FilePath "$installDirectory/flux-desktop.exe" -ArgumentList '--unregister-browser' -WindowStyle Hidden -PassThru -Wait; if ($remove.ExitCode -ne 0) { throw 'Repeated browser unregister failed.' } }
    foreach ($key in $keys) { if (Test-Path -LiteralPath $key) { throw 'Explicit unregister left a browser key behind.' } }
    $repair = Start-Process -FilePath "$installDirectory/flux-desktop.exe" -ArgumentList '--register-browser' -WindowStyle Hidden -PassThru -Wait
    if ($repair.ExitCode -ne 0) { throw 'Registration after removal failed.' }
    foreach ($key in $keys) { $file = if ($key -like '*/Mozilla/*') {'io.flux.download.firefox.json'} else {'native-host.json'}; if ((Get-Item -LiteralPath $key).GetValue('') -ne "$localData\Flux\$file") { throw 'Repair did not restore a browser registration.' } }
    if (Test-Path -LiteralPath "$workspace/.tools/firefox/geckodriver.exe") {
        node "$PSScriptRoot/test-firefox.mjs" $installDirectory "$workspace/.tools/firefox/120.0/core/firefox.exe" "$workspace/.tools/firefox/geckodriver.exe"
        if ($LASTEXITCODE -ne 0) { throw 'Installed Firefox 120 integration failed.' }
        node "$PSScriptRoot/test-firefox.mjs" $installDirectory "$workspace/.tools/firefox/157.0.1/core/firefox.exe" "$workspace/.tools/firefox/geckodriver.exe" --modern
        if ($LASTEXITCODE -ne 0) { throw 'Installed modern Firefox integration failed.' }
    }
    $nodeExecutable = (Get-Command node.exe).Source
    $env:PATH = "$env:SystemRoot/System32;$env:SystemRoot"
    & $nodeExecutable "$workspace/scripts/test-native.mjs" $installDirectory --desktop
    if ($LASTEXITCODE -ne 0) { throw 'Installed desktop integration failed.' }
    & $nodeExecutable "$workspace/scripts/test-window-session.mjs" $installDirectory --automatic
    if ($LASTEXITCODE -ne 0) { throw 'Installed native window regression failed.' }
    & $nodeExecutable "$workspace/scripts/test-media-window.mjs" $installDirectory
    if ($LASTEXITCODE -ne 0) { throw 'Installed adaptive media progress regression failed.' }
    & $nodeExecutable "$workspace/scripts/test-torrent-window.mjs" $installDirectory
    if ($LASTEXITCODE -ne 0) { throw 'Installed native torrent regression failed.' }
    & "$installDirectory/ffmpeg.exe" -version | Select-Object -First 1
    if ($LASTEXITCODE -ne 0) { throw 'Bundled FFmpeg failed.' }
    & "$installDirectory/yt-dlp.exe" --version
    if ($LASTEXITCODE -ne 0) { throw 'Bundled media extractor failed.' }
    & "$installDirectory/node.exe" --version
    if ($LASTEXITCODE -ne 0) { throw 'Bundled JavaScript runtime failed.' }
    $encoders = & "$installDirectory/ffmpeg.exe" -hide_banner -encoders 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $encoders -notmatch 'libmp3lame') { throw 'Bundled MP3 encoder missing.' }
    $sentinel = "$localData/Flux/preserve-user-data.txt"
    Set-Content -LiteralPath $sentinel -Value 'User data must survive uninstall.'
    $startupKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
    try { $startupKey.SetValue('Flux',('"' + (Join-Path $installDirectory 'flux-desktop.exe') + '" --background')) } finally { $startupKey.Dispose() }
    $uninstall = Start-Process -FilePath "$installDirectory/uninstall.exe" -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
    if ($uninstall.ExitCode -ne 0) { throw "Uninstaller failed: $($uninstall.ExitCode)" }
    $deadline = (Get-Date).AddSeconds(15)
    while (Test-Path -LiteralPath "$installDirectory/flux-desktop.exe") {
        if ((Get-Date) -gt $deadline) { throw 'Uninstaller did not remove application files.' }
        Start-Sleep -Milliseconds 100
    }
    if (-not (Test-Path -LiteralPath $sentinel)) { throw 'Uninstaller removed user data.' }
    $startupState = & "$PSScriptRoot/startup-test-state.ps1" -Mode Read | ConvertFrom-Json
    if ($startupState.exists) { throw 'Uninstaller left its owned startup entry behind.' }
    foreach ($key in $keys) { if (Test-Path -LiteralPath $key) { throw 'Uninstaller left its browser registration behind.' } }
    $report = @{passed=$true; version=$version; installer_filename=(Split-Path -Leaf $installerPath); sha256=(Get-FileHash -LiteralPath $installerPath).Hash.ToLowerInvariant(); bundled_runtime_verified=$true; desktop_cold_start=$true; native_windows_verified=$true; native_registration_verified=$true; uninstall_preserves_data=$true; uninstall_removes_owned_startup=$true; time=(Get-Date).ToUniversalTime().ToString('o')}
} finally {
    # Failed test runs must not leave a temporary installation or new browser key behind.
    if (Test-Path -LiteralPath "$installDirectory/flux-desktop.exe") {
        try {
            $cleanup = Start-Process -FilePath "$installDirectory/uninstall.exe" -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
            if ($cleanup.ExitCode -ne 0) { Write-Warning 'Temporary installation cleanup failed.' }
        } catch { Write-Warning $_ }
    }
    $env:LOCALAPPDATA = $previousLocalData
    $env:PATH = $previousPath
    & "$PSScriptRoot/startup-test-state.ps1" -Mode Restore -Snapshot $startupSnapshot
    foreach ($key in $previousKeys.Keys) {
        New-Item -Path $key -Force | Out-Null
        Set-Item -LiteralPath $key -Value $previousKeys[$key]
    }
    foreach ($key in $keys) {
        if (-not $previousKeys.ContainsKey($key) -and (Test-Path -LiteralPath $key)) {
            $file = if ($key -like '*/Mozilla/*') {'io.flux.download.firefox.json'} else {'native-host.json'}
            if ((Get-Item -LiteralPath $key).GetValue('') -eq "$localData\Flux\$file") { Remove-Item -LiteralPath $key -Force }
        }
    }
    if ($previousOwner) {
        New-Item -Path 'HKCU:/Software/Flux' -Force | Out-Null
        New-ItemProperty -LiteralPath 'HKCU:/Software/Flux' -Name BridgeDirectory -Value $previousOwner -PropertyType String -Force | Out-Null
    }
}
foreach ($key in $previousKeys.Keys) {
    if ((Get-Item -LiteralPath $key).GetValue('') -ne $previousKeys[$key]) { throw 'Previous browser registry restoration failed.' }
}
$report.registry_restoration_verified = $true
$report.firefox_registration_verified = $true
$report.idempotent_repair_remove_verified = $true
$report.installed_extension_sources_verified = $true
$report.installed_binary_hashes_verified = $true
$report.native_torrent_window_verified = $true
$report.native_browser_bindings_verified = 5
$report | ConvertTo-Json | Set-Content -LiteralPath "$workspace/docs/validation/installer.json" -Encoding utf8
Write-Output ($report | ConvertTo-Json -Compress)
