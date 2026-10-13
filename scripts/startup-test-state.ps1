param([ValidateSet('Read','Restore')][string]$Mode='Read',[string]$Snapshot)
$ErrorActionPreference='Stop'
$registry=[Microsoft.Win32.Registry]::CurrentUser
$name='Software\Microsoft\Windows\CurrentVersion\Run'
if ($Mode -eq 'Read') {
    $key=$registry.OpenSubKey($name)
    try {
        if ($key -and $key.GetValueNames().Contains('Flux')) {
            $kind=$key.GetValueKind('Flux').ToString()
            if ($kind -notin 'String','ExpandString') { throw 'Startup test does not replace non-string registry values.' }
            @{exists=$true;kind=$kind;value=$key.GetValue('Flux',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)} | ConvertTo-Json -Compress
        } else { @{exists=$false} | ConvertTo-Json -Compress }
    } finally { if ($key) {$key.Dispose()} }
} else {
    $data=Get-Content -LiteralPath $Snapshot -Raw | ConvertFrom-Json
    $key=$registry.CreateSubKey($name)
    try {
        if ($data.exists) { $kind=[Enum]::Parse([Microsoft.Win32.RegistryValueKind],[string]$data.kind); $key.SetValue('Flux',[string]$data.value,$kind) }
        else { $key.DeleteValue('Flux',$false) }
    } finally { $key.Dispose() }
}
