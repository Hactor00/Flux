param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
$version = (Get-Content -LiteralPath package.json -Raw | ConvertFrom-Json).version
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $workspace '.tools/browser-stores' }
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (-not $output.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Store packages must be staged inside the workspace.' }
node "$PSScriptRoot/build-extensions.mjs"
if ($LASTEXITCODE -ne 0) { throw 'Shared extension generation failed.' }
New-Item -ItemType Directory -Path $output -Force | Out-Null
$entries = @(
    @{Name='chrome';Source='extension'},
    @{Name='edge';Source='extension'},
    @{Name='firefox';Source='extension-firefox-modern'}
)
foreach ($entry in $entries) {
    $stage = Join-Path $output ($entry.Name + '-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stage | Out-Null
    $source = Join-Path $workspace $entry.Source
    foreach ($file in Get-ChildItem -LiteralPath $source -Force) {
        if ($file.PSIsContainer -and $file.Name -in @('icons','_locales','locales')) {
            Copy-Item -LiteralPath $file.FullName -Destination $stage -Recurse
        } elseif (-not $file.PSIsContainer -and $file.Extension -in @('.js','.html','.css','.json')) {
            Copy-Item -LiteralPath $file.FullName -Destination $stage
        }
    }
    Copy-Item -LiteralPath (Join-Path $workspace 'LICENSE') -Destination $stage
    $manifest = Get-Content -LiteralPath (Join-Path $stage 'manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.version -ne $version -or $manifest.manifest_version -ne 3) { throw 'Store extension version or manifest mismatch.' }
    if ($entry.Name -eq 'firefox') {
        if (-not $manifest.browser_specific_settings.gecko.id -or $manifest.background.service_worker -or -not $manifest.browser_specific_settings.gecko.data_collection_permissions) { throw 'Invalid production Gecko manifest.' }
    } else {
        if (-not $manifest.background.service_worker -or $manifest.browser_specific_settings) { throw 'Invalid Chromium service-worker manifest.' }
        # Store signing assigns the extension identity and update channel. Keep the
        # development key only in unpacked/GitHub packages for native-host pairing.
        foreach ($property in 'key','update_url') { $manifest.PSObject.Properties.Remove($property) }
        $manifest | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath (Join-Path $stage 'manifest.json') -Encoding utf8NoBOM
    }
    $archive = Join-Path $output "Flux-$version-$($entry.Name)-store.zip"
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -Force
    Write-Output "Store package prepared: $archive"
}
$checksums = foreach ($entry in $entries) {
    $name = "Flux-$version-$($entry.Name)-store.zip"
    (Get-FileHash -LiteralPath (Join-Path $output $name) -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $name
}
$checksums | Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding ascii
