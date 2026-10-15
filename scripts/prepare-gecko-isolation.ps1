param([Parameter(Mandatory=$true)][string]$Version)
$ErrorActionPreference = 'Stop'
if ($Version -notin @('120.0','157.0.1')) { throw 'Unsupported isolated Gecko version.' }
$workspace = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$directory = [IO.Path]::GetFullPath((Join-Path $workspace ".tools/firefox/$Version/core"))
if (-not $directory.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Gecko test runtime escaped the workspace.' }
$original = Join-Path $directory 'firefox.exe'
if ($env:GITHUB_ACTIONS -eq 'true') {
    Write-Output $original
    exit 0
}
$isolated = Join-Path $directory 'flux-gecko-test.exe'
if (-not (Test-Path -LiteralPath $isolated) -or (Get-FileHash -LiteralPath $original).Hash -ne (Get-FileHash -LiteralPath $isolated).Hash) {
    Copy-Item -LiteralPath $original -Destination $isolated -Force
}
Write-Output $isolated
