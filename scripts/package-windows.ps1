param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target" }
$exePath = Join-Path $targetDir "$Target\release\chronobase.exe"
if (!(Test-Path $exePath)) {
    $exePath = Join-Path $targetDir "release\chronobase.exe"
}
if (!(Test-Path $exePath)) {
    throw "Release binary not found at $exePath"
}

$stage = Join-Path $OutDir "ChronoBase-windows-x64-portable"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item $exePath (Join-Path $stage "chronobase.exe")
Copy-Item "README.md" (Join-Path $stage "README.md")
Copy-Item "LICENSE" (Join-Path $stage "LICENSE")
Copy-Item "crates\app\ui\fonts\OFL.txt" (Join-Path $stage "FONT-LICENSE-OFL.txt")
Set-Content -Path (Join-Path $stage "SLINT-ATTRIBUTION.txt") -Value "Built with Slint (https://slint.dev) - Licensed under Slint Royalty-Free Desktop License v2.0 / GNU GPLv3."

$zipPath = Join-Path $OutDir "ChronoBase-windows-x64-portable.zip"
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
Compress-Archive -Path "$stage\*" -DestinationPath $zipPath

if (!(Test-Path $zipPath)) {
    throw "Failed to create $zipPath"
}
Write-Host "Created $zipPath"

