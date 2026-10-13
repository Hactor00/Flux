$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $workspace
$directory = Join-Path $workspace '.tools/firefox'
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$artifacts = @(
    @{Name='120.0.exe';Uri='https://archive.mozilla.org/pub/firefox/releases/120.0/win64/en-US/Firefox%20Setup%20120.0.exe';Sha256='3adc86fe46345e5b219e9ac543539e9f6f4251aebf50d7b6fa14271de86478bf'},
    @{Name='157.0.1.exe';Uri='https://archive.mozilla.org/pub/firefox/releases/157.0.1/win64/en-US/Firefox%20Setup%20157.0.1.exe';Sha256='71dc620e7749bb65be6df67b99261a303805a804f9c787b76b81c095cfa7d9ca'},
    @{Name='7zr.exe';Uri='https://www.7-zip.org/a/7zr.exe';Sha256='256feca8e274e5da655e2a284fabafd9f554365eb164862089dacd4e8276d282'},
    @{Name='geckodriver.zip';Uri='https://github.com/mozilla/geckodriver/releases/download/v0.36.0/geckodriver-v0.36.0-win64.zip';Sha256='4c23d54681d431b42575a66c3c89202a40add3ef45e64b94acac620a89774ad2'}
)
foreach ($artifact in $artifacts) {
    $path = Join-Path $directory $artifact.Name
    if (-not (Test-Path -LiteralPath $path)) { Invoke-WebRequest -Uri $artifact.Uri -OutFile $path }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $artifact.Sha256) { throw "Firefox test artifact checksum mismatch: $($artifact.Name)" }
}
foreach ($version in '120.0','157.0.1') {
    if (-not (Test-Path -LiteralPath "$directory/$version/core/firefox.exe")) {
        & "$directory/7zr.exe" x "$directory/$version.exe" "-o$directory/$version" -y | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Firefox extraction failed.' }
    }
}
Expand-Archive -LiteralPath "$directory/geckodriver.zip" -DestinationPath $directory -Force
$stream = [IO.File]::Create((Join-Path $directory 'media.ts'))
try { foreach ($part in Get-ChildItem -LiteralPath "$workspace/tests/fixtures/media" -Filter 'part*.jpg' | Sort-Object Name) { $bytes = [IO.File]::ReadAllBytes($part.FullName); $stream.Write($bytes,0,$bytes.Length) } } finally { $stream.Dispose() }
& "$workspace/target/release/ffmpeg.exe" -hide_banner -loglevel error -i "$directory/media.ts" -c copy -y "$directory/media.mp4"
if ($LASTEXITCODE -ne 0) { throw 'Firefox media fixture preparation failed.' }
Write-Output 'Pinned Firefox 120.0, Firefox 157.0.1 and geckodriver 0.36.0 are ready.'
