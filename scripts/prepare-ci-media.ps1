$ErrorActionPreference = 'Stop'
choco install ffmpeg -y --no-progress
if ($LASTEXITCODE -ne 0) { throw 'CI FFmpeg installation failed.' }
$mediaRoot = Join-Path $env:ChocolateyInstall 'lib/ffmpeg/tools'
$encoders = @(Get-ChildItem -LiteralPath $mediaRoot -Filter 'ffmpeg.exe' -File -Recurse)
if ($encoders.Count -ne 1) { throw 'Expected exactly one native FFmpeg executable in the installed package.' }
$encoder = $encoders[0].FullName
$probe = Join-Path $encoders[0].DirectoryName 'ffprobe.exe'
if (-not (Test-Path -LiteralPath $probe -PathType Leaf)) { throw 'The native FFprobe executable is missing.' }
# Invoke the actual tools so cancellation terminates FFmpeg rather than its Chocolatey launcher.
$env:FLUX_FFMPEG = $encoder
$env:FLUX_FFPROBE = $probe
$env:FLUX_FIXTURE_FFMPEG = $encoder
if ($env:GITHUB_ENV) {
    @("FLUX_FFMPEG=$encoder", "FLUX_FFPROBE=$probe", "FLUX_FIXTURE_FFMPEG=$encoder") | Out-File -LiteralPath $env:GITHUB_ENV -Append -Encoding utf8
}
Write-Output 'CI media tools use native executables; launcher shims are excluded.'
