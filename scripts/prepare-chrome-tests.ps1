param([string]$ChromeDirectory = 'C:/Program Files/Google/Chrome/Application')
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$destination = [IO.Path]::GetFullPath((Join-Path $workspace '.tools/chrome-isolated'))
if (-not $destination.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Chrome test runtime escaped the workspace.' }
$version = Get-ChildItem -LiteralPath $ChromeDirectory -Directory | Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $version) { throw 'Installed Chrome runtime was not found.' }
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$runtime = Join-Path $destination $version.Name
if (-not (Test-Path -LiteralPath (Join-Path $runtime 'chrome.dll'))) {
    Copy-Item -LiteralPath $version.FullName -Destination $runtime -Recurse
}
$executable = Join-Path $destination 'flux-browser-test.exe'
$original = Join-Path $ChromeDirectory 'chrome.exe'
if (-not (Test-Path -LiteralPath $executable) -or (Get-FileHash -LiteralPath $executable).Hash -ne (Get-FileHash -LiteralPath $original).Hash) {
    Copy-Item -LiteralPath $original -Destination $executable -Force
}
Write-Output $executable
