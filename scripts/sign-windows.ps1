param([Parameter(Mandatory)][string[]]$Files, [Parameter(Mandatory)][string]$Thumbprint, [string]$TimestampUrl = 'https://timestamp.digicert.com')
$ErrorActionPreference = 'Stop'
if ($Thumbprint -notmatch '^[A-Fa-f0-9]{40}$') { throw 'Code-signing certificate thumbprint must contain 40 hexadecimal characters.' }
$timestamp = [Uri]$TimestampUrl
if ($timestamp.Scheme -ne 'https' -or $timestamp.UserInfo) { throw 'Timestamp authority must use HTTPS without URL credentials.' }
$certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$Thumbprint" -ErrorAction Stop
if (-not $certificate.HasPrivateKey -or $certificate.NotAfter -le [DateTime]::Now -or -not ($certificate.EnhancedKeyUsageList.ObjectId -contains '1.3.6.1.5.5.7.3.3')) { throw 'A valid code-signing certificate and its private key are required.' }
$tool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if (-not $tool) {
    $tools = @(Get-ChildItem -LiteralPath (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin') -Filter signtool.exe -File -Recurse | Where-Object FullName -Match '\\x64\\' | Sort-Object FullName -Descending)
    if (-not $tools) { throw 'Windows SDK signtool.exe is required.' }
    $toolPath = $tools[0].FullName
} else { $toolPath = $tool.Source }
foreach ($file in $Files) {
    $path = (Resolve-Path -LiteralPath $file).Path
    & $toolPath sign /sha1 $Thumbprint /s My /fd SHA256 /tr $TimestampUrl /td SHA256 $path
    if ($LASTEXITCODE -ne 0) { throw "Code signing failed: $path" }
    & $toolPath verify /pa /all $path
    if ($LASTEXITCODE -ne 0 -or (Get-AuthenticodeSignature -LiteralPath $path).Status -ne 'Valid') { throw "Code-signing verification failed: $path" }
}
