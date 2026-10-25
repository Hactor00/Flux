param([Parameter(Mandatory)][ValidatePattern('^\d+$')][string]$RunId)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
if (-not $env:GITHUB_ACTIONS -or -not $env:GITHUB_REPOSITORY) { throw 'Media artifact reuse requires GitHub Actions provenance.' }
$workspace = Split-Path -Parent $PSScriptRoot
$env:GH_TOKEN = $env:GITHUB_TOKEN
$run = gh api "repos/$env:GITHUB_REPOSITORY/actions/runs/$RunId" | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $run.path -notin '.github/workflows/release.yml','.github/workflows/windows-media.yml' -or $run.head_repository.full_name -ne $env:GITHUB_REPOSITORY) { throw 'Unexpected media build provenance.' }
$artifacts = gh api "repos/$env:GITHUB_REPOSITORY/actions/runs/$RunId/artifacts" | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Media artifact provenance is unavailable.' }
$crossCompiled = $run.path -eq '.github/workflows/windows-media.yml' -or @($artifacts.artifacts | Where-Object name -eq 'pinned-windows-media').Count -eq 1
if ($crossCompiled) {
    $jobs = gh api "repos/$env:GITHUB_REPOSITORY/actions/runs/$RunId/jobs" | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or -not @($jobs.jobs | Where-Object { $_.conclusion -eq 'success' -and @($_.steps | Where-Object { $_.name -eq 'Cross-compile pinned LGPL tools' -and $_.conclusion -eq 'success' }).Count -eq 1 }).Count) { throw 'Media tools job has not passed.' }
}
git fetch --no-tags --depth=1 origin $run.head_sha
if ($LASTEXITCODE -ne 0) { throw 'Could not verify media source commit.' }
git diff --quiet $run.head_sha HEAD -- scripts/build-media-runtime.sh scripts/build-media-runtime.ps1 scripts/bootstrap-media-toolchain.ps1 scripts/build-windows-media.sh .github/workflows/windows-media.yml
if ($LASTEXITCODE -ne 0) { throw 'Media runtime build sources changed.' }
$stage = Join-Path $workspace ('.tools/reused-media-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $stage | Out-Null
$artifactName = if ($crossCompiled) { 'pinned-windows-media' } else { 'candidate-windows-x64' }
gh run download $RunId --repo $env:GITHUB_REPOSITORY --name $artifactName --dir $stage
if ($LASTEXITCODE -ne 0) { throw 'Verified media candidate is not available.' }
$version = (Get-Content "$workspace/package.json" -Raw | ConvertFrom-Json).version
foreach ($line in Get-Content -LiteralPath (Join-Path $stage 'SHA256SUMS.txt')) {
    $parts = $line -split '  ',2
    if ($parts.Count -ne 2 -or $parts[0] -notmatch '^[a-f0-9]{64}$' -or [IO.Path]::IsPathRooted($parts[1])) { throw 'Unsafe candidate checksum path.' }
    $file = [IO.Path]::GetFullPath((Join-Path $stage $parts[1]))
    if (-not $file.StartsWith($stage + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Candidate checksum path escaped the artifact.' }
    if ((Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant() -ne $parts[0]) { throw 'Candidate media identity mismatch.' }
}
if ($crossCompiled) {
    $provenance = Get-Content -LiteralPath (Join-Path $stage 'provenance.json') -Raw | ConvertFrom-Json
    if ($provenance.run_id -ne $RunId -or $provenance.source_commit -ne $run.head_sha -or $provenance.toolchain_sha256 -ne '5f9c6ed95b2d4bdb2869a488c5fd5857fbdabcf288a0aa3eb1da43f6a08d8ab4' -or $provenance.target -ne 'x86_64-w64-mingw32' -or $provenance.ffmpeg -ne '9.0.2') { throw 'Cross-compiled media provenance mismatch.' }
    $runtime = $stage
} else {
$runtime = Join-Path $stage 'runtime'
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
$zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $stage "Flux-$version-windows-x64.zip"))
try {
    foreach ($name in 'ffmpeg.exe','ffprobe.exe','libunwind.dll') {
        $entry = $zip.GetEntry("Flux-$version-windows-x64/$name")
        if (-not $entry) { throw "Media candidate omitted $name" }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,(Join-Path $runtime $name),$true)
    }
} finally { $zip.Dispose() }
Expand-Archive -LiteralPath (Join-Path $stage "Flux-$version-media-source.zip") -DestinationPath (Join-Path $runtime 'media-source')
}
$configuration = & (Join-Path $runtime 'ffmpeg.exe') -version 2>&1 | Out-String
if ($LASTEXITCODE -ne 0 -or $configuration -notmatch 'ffmpeg version 9\.0\.2' -or $configuration -match '--enable-(gpl|nonfree)') { throw 'Media candidate is not the pinned LGPL runtime.' }
$encoders = & (Join-Path $runtime 'ffmpeg.exe') -encoders 2>&1 | Out-String
if ($LASTEXITCODE -ne 0 -or $encoders -notmatch '\bmjpeg\b') { throw 'Media candidate lacks video preview encoding.' }
[IO.File]::WriteAllText((Join-Path $workspace '.tools/reused-media-path.txt'),$runtime)
Write-Output "Verified pinned media runtime from build $RunId; application binaries will be compiled from the current source."
