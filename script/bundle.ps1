# Builds a release Burrdisk.exe for Windows, then its installer and a portable zip.
#
# Neither is code signed, so SmartScreen may ask to confirm running them
# (More info > Run anyway). The installer needs Inno Setup 6 (`iscc`), from
# https://jrsoftware.org/isinfo.php or `winget install JRSoftware.InnoSetup`.
#
#   script\bundle.ps1    writes dist\Burrdisk-<version>-windows-<arch>-setup.exe and .zip
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$Stage = Join-Path ([System.IO.Path]::GetTempPath()) "burrdisk-bundle-$PID"
New-Item -ItemType Directory -Force $Stage | Out-Null
try {
    Copy-Item target\release\burrdisk.exe (Join-Path $Stage 'Burrdisk.exe')
    New-Item -ItemType Directory -Force dist | Out-Null
    $Zip = "dist\Burrdisk-$Version-windows-$Arch.zip"
    if (Test-Path $Zip) { Remove-Item $Zip }
    Compress-Archive -Path (Join-Path $Stage 'Burrdisk.exe') -DestinationPath $Zip
    Write-Output $Zip
} finally {
    Remove-Item -Recurse -Force $Stage
}

$Iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $Iscc) {
    $Iscc = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Iscc) { throw 'Inno Setup 6 (iscc) not found; it builds the installer.' }
& $Iscc /Q "/DAppVersion=$Version" "/DSourceExe=$Root\target\release\burrdisk.exe" "/DOutputDir=$Root\dist" installer\burrdisk.iss
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Output "dist\Burrdisk-$Version-windows-$Arch-setup.exe"
