param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [string]$OutDir = "dist",
    [switch]$RequireInstallers
)

$ErrorActionPreference = "Stop"

$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $appVersion = $Matches[1]
} else {
    $appVersion = "0.2.0"
}

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target" }
$exePath = Join-Path $targetDir "$Target\release\chronobase.exe"
if (!(Test-Path $exePath)) {
    $exePath = Join-Path $targetDir "release\chronobase.exe"
}
if (!(Test-Path $exePath)) {
    throw "Release binary not found at $exePath"
}

New-Item -ItemType Directory -Force $OutDir | Out-Null
$outAbs = (Resolve-Path $OutDir).Path

$stage = Join-Path $outAbs "ChronoBase-windows-x64-portable"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item $exePath (Join-Path $stage "chronobase.exe")
Copy-Item "README.md" (Join-Path $stage "README.md")
Copy-Item "LICENSE" (Join-Path $stage "LICENSE")
Copy-Item "crates\app\ui\fonts\OFL.txt" (Join-Path $stage "FONT-LICENSE-OFL.txt")
Set-Content -Path (Join-Path $stage "SLINT-ATTRIBUTION.txt") -Value "Built with Slint (https://slint.dev) - Licensed under Slint Royalty-Free Desktop License v2.0 / GNU GPLv3."

$zipPath = Join-Path $outAbs "ChronoBase-windows-x64-portable.zip"
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
Compress-Archive -Path "$stage\*" -DestinationPath $zipPath
if (!(Test-Path $zipPath)) {
    throw "Failed to create $zipPath"
}
Write-Host "Created $zipPath"

$iconAbs = (Resolve-Path "crates\app\assets\icon.ico").Path

# NSIS Installer (.exe)
$makensis = Get-Command "makensis.exe" -ErrorAction SilentlyContinue
if (-not $makensis -and (Test-Path "C:\Program Files (x86)\NSIS\makensis.exe")) {
    $makensis = Get-Item "C:\Program Files (x86)\NSIS\makensis.exe"
}
if ($makensis) {
    $makensisExe = if ($makensis.Source) { $makensis.Source } else { $makensis.FullName }
    $nsisScript = (Resolve-Path "scripts\windows-installer.nsi").Path
    $setupExePath = Join-Path $outAbs "ChronoBase-windows-x64-setup.exe"
    if (Test-Path $setupExePath) { Remove-Item -Force $setupExePath }
    & $makensisExe "/DAPP_VERSION=$appVersion" "/DSTAGE_DIR=$stage" "/DOUT_FILE=$setupExePath" "/DICON_FILE=$iconAbs" $nsisScript
    if ($LASTEXITCODE -ne 0 -or !(Test-Path $setupExePath)) {
        throw "NSIS installer build failed"
    }
    Write-Host "Created $setupExePath"
} elseif ($RequireInstallers) {
    throw "makensis.exe is required to build ChronoBase-windows-x64-setup.exe"
} else {
    Write-Warning "makensis.exe not found; skipping NSIS .exe installer."
}

# WiX v3 MSI Installer (.msi)
$candle = Get-Command "candle.exe" -ErrorAction SilentlyContinue
$light = Get-Command "light.exe" -ErrorAction SilentlyContinue
if ((-not $candle -or -not $light) -and (Test-Path "C:\Program Files (x86)\WiX Toolset*")) {
    $wixCandle = Get-ChildItem "C:\Program Files (x86)\WiX Toolset*" -Recurse -Filter "candle.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($wixCandle) {
        $candle = $wixCandle
        $light = Get-Item (Join-Path $wixCandle.DirectoryName "light.exe")
    }
}
if ($candle -and $light) {
    $candleExe = if ($candle.Source) { $candle.Source } else { $candle.FullName }
    $lightExe = if ($light.Source) { $light.Source } else { $light.FullName }
    $wxsScript = (Resolve-Path "scripts\windows-installer.wxs").Path
    $wixObj = Join-Path $outAbs "windows-installer.wixobj"
    $msiPath = Join-Path $outAbs "ChronoBase-windows-x64.msi"
    $licenseRtf = Join-Path $outAbs "license.rtf"

    $licenseLines = Get-Content "LICENSE" | ForEach-Object {
        $escaped = $_.Replace("\", "\\").Replace("{", "\{").Replace("}", "\}")
        "$escaped\par"
    }
    $rtfContent = "{\rtf1\ansi\deff0{\fonttbl{\f0 Consolas;}}\fs18 " + ($licenseLines -join "`r`n") + "`r`n}"
    Set-Content -Path $licenseRtf -Value $rtfContent -Encoding Ascii

    if (Test-Path $msiPath) { Remove-Item -Force $msiPath }
    & $candleExe -nologo -arch x64 "-dAppVersion=$appVersion" "-dStageDir=$stage" "-dIconPath=$iconAbs" "-dLicenseRtf=$licenseRtf" -out $wixObj $wxsScript
    if ($LASTEXITCODE -ne 0) {
        throw "WiX candle.exe failed"
    }
    & $lightExe -nologo -ext WixUIExtension -out $msiPath $wixObj
    if ($LASTEXITCODE -ne 0 -or !(Test-Path $msiPath)) {
        throw "WiX light.exe failed"
    }
    Remove-Item -Force $wixObj, $licenseRtf -ErrorAction SilentlyContinue
    $wixPdb = Join-Path $outAbs "ChronoBase-windows-x64.wixpdb"
    if (Test-Path $wixPdb) { Remove-Item -Force $wixPdb }
    Write-Host "Created $msiPath"
} elseif ($RequireInstallers) {
    throw "WiX candle.exe and light.exe are required to build ChronoBase-windows-x64.msi"
} else {
    Write-Warning "WiX toolset not found; skipping .msi installer."
}
