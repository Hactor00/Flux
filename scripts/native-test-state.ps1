param([ValidateSet('Read','Restore','Clear')][string]$Mode='Read',[string]$Snapshot,[string]$DataDirectory)
$ErrorActionPreference='Stop'
$registry=[Microsoft.Win32.Registry]::CurrentUser
$entries=@(
    @{path='Software\Google\Chrome\NativeMessagingHosts\io.flux.download';name='';file='native-host.json'},
    @{path='Software\Microsoft\Edge\NativeMessagingHosts\io.flux.download';name='';file='native-host.json'},
    @{path='Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\io.flux.download';name='';file='native-host.json'},
    @{path='Software\Chromium\NativeMessagingHosts\io.flux.download';name='';file='native-host.json'},
    @{path='Software\Mozilla\NativeMessagingHosts\io.flux.download';name='';file='io.flux.download.firefox.json'},
    @{path='Software\Flux';name='BridgeDirectory';file=$null}
)
if ($Mode -eq 'Read') {
    $state=foreach($entry in $entries) {
        $key=$registry.OpenSubKey($entry.path)
        try {
            $exists=$key -and $key.GetValueNames().Contains([string]$entry.name)
            $record=@{path=$entry.path;name=$entry.name;key_exists=[bool]$key;exists=[bool]$exists}
            if($exists) {
                $kind=$key.GetValueKind($entry.name).ToString()
                if($kind -notin 'String','ExpandString') {throw 'Native registration tests do not replace non-string values.'}
                $record.kind=$kind
                $record.value=$key.GetValue($entry.name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            }
            $record
        } finally {if($key){$key.Dispose()}}
    }
    ConvertTo-Json -InputObject @($state) -Compress
} elseif($Mode -eq 'Restore') {
    $state=Get-Content -LiteralPath $Snapshot -Raw | ConvertFrom-Json
    if($state.Count -ne $entries.Count){throw 'Invalid native registration snapshot.'}
    foreach($entry in $entries) {
        $record=$state | Where-Object {$_.path -eq $entry.path -and $_.name -eq $entry.name}
        if(@($record).Count -ne 1){throw 'Native registration snapshot entry is missing.'}
        $key=$registry.CreateSubKey($entry.path)
        try {
            if($record.exists){$kind=[Enum]::Parse([Microsoft.Win32.RegistryValueKind],[string]$record.kind);$key.SetValue($entry.name,[string]$record.value,$kind)}
            else{$key.DeleteValue($entry.name,$false)}
            $empty=$key.ValueCount -eq 0 -and $key.SubKeyCount -eq 0
        } finally {$key.Dispose()}
        if(-not $record.key_exists -and $empty){$registry.DeleteSubKey($entry.path,$false)}
    }
} else {
    if(-not [IO.Path]::IsPathRooted($DataDirectory)){throw 'An absolute test data directory is required.'}
    foreach($entry in $entries | Where-Object {$_.file}) {
        $key=$registry.OpenSubKey($entry.path,$true)
        if(-not $key){continue}
        try {
            $expected=Join-Path $DataDirectory $entry.file
            if($key.GetValue('') -ne $expected){throw 'Refusing to clear a native registration outside this test profile.'}
            $key.DeleteValue('',$false)
        } finally {$key.Dispose()}
    }
}
