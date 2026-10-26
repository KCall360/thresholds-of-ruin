[CmdletBinding()]
param(
    [ValidateRange(0, 10000)][int]$Cycles = 0,
    [string]$OutputRoot = ''
)
$ErrorActionPreference = 'Stop'
$repo = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$client = Join-Path $repo '.local/target-3d/release/tor-client-3d.exe'
if (-not (Test-Path -LiteralPath $client -PathType Leaf)) {
    throw "Build the 3D release client first: $client"
}
if (-not $OutputRoot) { $OutputRoot = Join-Path $repo '.local/reviews/showcase' }
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
$run = Join-Path $OutputRoot ((Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$report = Join-Path $run 'frame-timing.json'
$log = Join-Path $run 'playback.log'
Write-Host "Live playback diagnostics: $report"
Write-Host 'One window; Escape or closing the window stops playback.'
try {
    $ErrorActionPreference = 'Continue'
    & $client --live-playlist (Join-Path $PSScriptRoot 'live-showcase.json') --cycles $Cycles --diagnostics $report *> $log
    $exitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = 'Stop'
}
if ($exitCode -ne 0) { throw "Playback failed (exit $exitCode). Log: $log" }
Write-Host "Playback finished. Diagnostics: $report"
