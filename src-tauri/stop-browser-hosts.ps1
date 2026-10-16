param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath((Join-Path $Directory 'flux-native-host.exe'))
foreach ($entry in Get-CimInstance Win32_Process -Filter "Name='flux-native-host.exe'") {
    if (-not [string]::Equals($entry.ExecutablePath, $target, [StringComparison]::OrdinalIgnoreCase)) { continue }
    $process = $null
    try {
        $process = [Diagnostics.Process]::GetProcessById([int]$entry.ProcessId)
        if ($process.HasExited) { continue }
        # Recheck the path in case the process exited after enumeration.
        if (-not [string]::Equals($process.MainModule.FileName, $target, [StringComparison]::OrdinalIgnoreCase)) { continue }
        $process.Kill()
        if (-not $process.WaitForExit(10000)) { throw 'Browser bridge did not stop.' }
    } catch [ArgumentException] {
        # The process already exited.
    } finally {
        if ($process) { $process.Dispose() }
    }
}
