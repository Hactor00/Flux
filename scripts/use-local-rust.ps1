# Dot-source this file to use the isolated toolchain prepared in this workspace.
$workspaceRoot = Split-Path -Parent $PSScriptRoot
$env:RUSTUP_HOME = Join-Path $workspaceRoot '.tools\rustup'
$env:CARGO_HOME = Join-Path $workspaceRoot '.tools\cargo'
$compilerDir = Get-ChildItem (Join-Path $workspaceRoot '.tools\mingw') -Directory | Select-Object -First 1
if (-not $compilerDir) { throw 'Workspace LLVM MinGW toolchain is not installed.' }
$env:PATH = "$($compilerDir.FullName)\bin;$env:CARGO_HOME\bin;$env:PATH"
$env:CC = 'x86_64-w64-mingw32-clang'
$env:AR = 'llvm-ar'
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnullvm'
