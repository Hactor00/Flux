param([ValidateSet('debug','release')][string]$Profile = 'debug')
$ErrorActionPreference = 'Stop'
$workspaceRoot = Split-Path -Parent $PSScriptRoot
$compilerDir = Get-ChildItem (Join-Path $workspaceRoot '.tools\mingw') -Directory | Select-Object -First 1
if (-not $compilerDir) { throw 'Local LLVM MinGW runtime not found.' }
$sourcePath = Join-Path $compilerDir.FullName 'bin\libunwind.dll'
$targetPath = Join-Path $workspaceRoot "target\$Profile"
if (-not (Test-Path -LiteralPath $targetPath)) { throw 'Build the binaries first.' }
Copy-Item -LiteralPath $sourcePath -Destination (Join-Path $targetPath 'libunwind.dll') -Force
Write-Output "LLVM runtime copied to target/$Profile. The runtime is covered by LLVM's Apache-2.0 license with LLVM exceptions."
