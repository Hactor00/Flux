param([Parameter(Mandatory)][string]$RunId)
$ErrorActionPreference = 'Stop'
if ($RunId -notmatch '^\d+$') { throw 'A numeric candidate workflow run ID is required.' }
$workspace = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
Set-Location -LiteralPath $workspace
$headers = @{Authorization="Bearer $env:GITHUB_TOKEN"; Accept='application/vnd.github+json'}
$base = "https://api.github.com/repos/$env:GITHUB_REPOSITORY"
$candidate = Invoke-RestMethod -Uri "$base/actions/runs/$RunId" -Headers $headers
if ($candidate.path -ne '.github/workflows/release.yml' -or $candidate.head_repository.full_name -ne $env:GITHUB_REPOSITORY -or $candidate.head_sha -notmatch '^[a-f0-9]{40}$') { throw 'Candidate did not originate from this release workflow.' }
$jobs = Invoke-RestMethod -Uri "$base/actions/runs/$RunId/jobs?per_page=100" -Headers $headers
$prepared = @($jobs.jobs | ForEach-Object { $_.steps } | Where-Object { $_.name -in @('Compile Windows packages','Prepare Windows packages') -and $_.conclusion -eq 'success' })
if ($prepared.Count -ne 1) { throw 'Candidate compilation was not successful.' }
git fetch --no-tags --depth=1 origin $candidate.head_sha
if ($LASTEXITCODE -ne 0) { throw 'Candidate source is unavailable.' }
$paths = @('crates','src','src-tauri','extension','extension-firefox','native','vendor','.cargo','Cargo.toml','Cargo.lock','package.json','package-lock.json','scripts/build-torrent-runtime.ps1','scripts/prepare-torrent-runtime.py','scripts/build-extensions.mjs','scripts/prepare-release.ps1','scripts/build-release.ps1','scripts/build-media-runtime.sh','scripts/prepare-extractor.ps1')
git diff --quiet $candidate.head_sha HEAD -- @paths
if ($LASTEXITCODE -ne 0) { throw 'Runtime or build sources changed; compile new packages instead.' }
$coreChecks = @($jobs.jobs | ForEach-Object { $_.steps } | Where-Object { $_.name -eq 'Core and desktop checks' -and $_.conclusion -eq 'success' })
if ($coreChecks.Count -eq 1) {
    $artifacts = Invoke-RestMethod -Uri "$base/actions/runs/$RunId/artifacts?per_page=100" -Headers $headers
    $coreArtifact = $artifacts.artifacts | Where-Object { $_.name -in @('core-validation-windows-x64','validation-diagnostics-windows-x64') -and -not $_.expired } | Select-Object -First 1
    if ($coreArtifact -and $env:GITHUB_ENV) {
        @("FLUX_CORE_TEST_RUN=$RunId", "FLUX_CORE_SOURCE_COMMIT=$($candidate.head_sha)", "FLUX_CORE_TEST_ARTIFACT=$($coreArtifact.name)") | Out-File -LiteralPath $env:GITHUB_ENV -Append -Encoding utf8
    }
}
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
foreach ($line in Get-Content release/SHA256SUMS.txt) {
    $parts = $line -split '  ',2
    if ($parts.Count -ne 2 -or [IO.Path]::GetFileName($parts[1]) -ne $parts[1]) { throw 'Invalid candidate checksum path.' }
    if ((Get-FileHash -LiteralPath (Join-Path "$workspace/release" $parts[1])).Hash.ToLowerInvariant() -ne $parts[0]) { throw "Candidate checksum mismatch: $($parts[1])" }
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead("$workspace/release/Flux-$version-source.zip")
try {
    $entry = $archive.GetEntry('package.json')
    if (-not $entry) { throw 'Candidate source version is missing.' }
    $reader = [IO.StreamReader]::new($entry.Open())
    try { $source = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    if ($source.version -ne $version) { throw 'Candidate version differs from the current release.' }
} finally { $archive.Dispose() }
$extracted = Join-Path $workspace ('.tools/candidate-runtime-' + [guid]::NewGuid().ToString('N'))
if (-not ([IO.Path]::GetFullPath($extracted)).StartsWith($workspace + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Candidate staging escaped the workspace.' }
Expand-Archive -LiteralPath "$workspace/release/Flux-$version-windows-x64.zip" -DestinationPath $extracted
$portable = Join-Path $extracted "Flux-$version-windows-x64"
foreach ($name in 'flux-desktop.exe','flux-cli.exe','flux-native-host.exe','ffmpeg.exe','ffprobe.exe','yt-dlp.exe','node.exe','WebView2Loader.dll','FluxTorrentBridge.dll') {
    if (-not (Test-Path -LiteralPath (Join-Path $portable $name) -PathType Leaf)) { throw "Candidate omitted $name" }
}
New-Item -ItemType Directory -Force target/release,target/debug,.tools/package,target/release/bundle/nsis | Out-Null
foreach ($entry in Get-ChildItem -LiteralPath $portable) {
    Copy-Item -LiteralPath $entry.FullName -Destination "$workspace/target/release" -Recurse -Force
    if ($entry.Name -notin @('README.md','README.tr.md','docs')) { Copy-Item -LiteralPath $entry.FullName -Destination "$workspace/.tools/package" -Recurse -Force }
}
foreach ($entry in Get-ChildItem -LiteralPath $portable -Filter '*.dll' -File) { Copy-Item -LiteralPath $entry.FullName -Destination "$workspace/target/debug" -Force }
Copy-Item -LiteralPath "$workspace/release/Flux_${version}_x64-setup.exe" -Destination "$workspace/target/release/bundle/nsis" -Force
$provenancePath = Join-Path $workspace 'release/candidate-provenance.json'
$provenance = if (Test-Path -LiteralPath $provenancePath) { Get-Content -LiteralPath $provenancePath -Raw | ConvertFrom-Json } else { @{build_run=$RunId; source_commit=$candidate.head_sha} }
$provenance | ConvertTo-Json | Set-Content -LiteralPath $provenancePath -Encoding utf8
$env:FLUX_BINARY_BUILD_RUN = [string]$provenance.build_run
$env:FLUX_BINARY_SOURCE_COMMIT = [string]$provenance.source_commit
if ($env:GITHUB_ENV) { @("FLUX_BINARY_BUILD_RUN=$env:FLUX_BINARY_BUILD_RUN","FLUX_BINARY_SOURCE_COMMIT=$env:FLUX_BINARY_SOURCE_COMMIT") | Out-File -LiteralPath $env:GITHUB_ENV -Append -Encoding utf8 }
Write-Output "Verified candidate bytes and unchanged runtime sources from run $env:FLUX_BINARY_BUILD_RUN."
