param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $workspace '.tools/browser-stores' }
$version = (Get-Content -LiteralPath package.json -Raw | ConvertFrom-Json).version
Add-Type -AssemblyName System.IO.Compression.FileSystem
$packages = @(
    @{Name='chrome';Source='extension'},
    @{Name='edge';Source='extension'},
    @{Name='firefox';Source='extension-firefox-modern'}
)
$records = foreach ($package in $packages) {
    $archivePath = Join-Path $OutputDirectory "Flux-$version-$($package.Name)-store.zip"
    $source = (Resolve-Path -LiteralPath $package.Source).Path
    $expected = @{}
    foreach ($file in Get-ChildItem -LiteralPath $source -File -Recurse) {
        $relative = [IO.Path]::GetRelativePath($source,$file.FullName).Replace('\','/')
        if (($relative -notmatch '/' -and ($file.Extension -in @('.js','.html','.css','.json') -or $file.Name -eq 'LICENSE')) -or $relative -match '^(icons|_locales|locales)/') {
            $expected[$relative] = (Get-FileHash -LiteralPath $file.FullName).Hash
        }
    }
    $expected['LICENSE'] = (Get-FileHash -LiteralPath LICENSE).Hash
    $zip = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $archivePath).Path)
    try {
        $entries = @($zip.Entries | Where-Object { -not $_.FullName.EndsWith('/') })
        if ($entries.Count -ne $expected.Count) { throw "Unexpected store package file count: $($package.Name)" }
        $seen = @{}
        foreach ($entry in $entries) {
            if (-not $expected.ContainsKey($entry.FullName) -or $seen.ContainsKey($entry.FullName)) { throw "Unexpected or duplicate store file: $($entry.FullName)" }
            $seen[$entry.FullName] = $true
            $stream = $entry.Open()
            try { $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)) } finally { $stream.Dispose() }
            if ($hash -ne $expected[$entry.FullName]) { throw "Store source mismatch: $($package.Name)/$($entry.FullName)" }
        }
        @{name=$package.Name;files=$entries.Count;sha256=(Get-FileHash -LiteralPath $archivePath).Hash.ToLowerInvariant()}
    } finally { $zip.Dispose() }
}
$report = @{version=$version;passed=$true;runtime_only=$true;source_hashes_verified=$true;packages=@($records);time=(Get-Date).ToUniversalTime().ToString('o')}
$report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath docs/validation/browser-stores.json -Encoding utf8
Write-Output ($report | ConvertTo-Json -Depth 4 -Compress)
