<#
.SYNOPSIS
    Packages OpenRig for Windows: creates .zip bundle and .msi installer.
    Mirrors exactly what GitHub Actions does for the Windows build.

.DESCRIPTION
    Assumes cargo build --release -p adapter-gui has already run.
    Stages all required files (binary, NAM DLL, LV2 libs, data, assets, captures),
    then uses WiX Toolset v3 (heat + candle + light) to build the MSI.

.PARAMETER Version
    Release version string, e.g. "1.2.3" or "dev" (default: dev)

.EXAMPLE
    .\scripts\package-windows.ps1 1.2.3
    .\scripts\package-windows.ps1          # uses "dev"
#>
param([string]$Version = "dev")

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $RepoRoot

try {
    # ── 1. Locate WiX Toolset v3 ─────────────────────────────────────────────────
    Write-Host "==> Locating WiX Toolset v3..."
    $wixBin = $null
    $candidates = @(
        "$env:WIX\bin",
        "C:\Program Files (x86)\WiX Toolset v3.14\bin",
        "C:\Program Files (x86)\WiX Toolset v3.11\bin",
        "C:\Program Files\WiX Toolset v3.14\bin",
        "C:\Program Files\WiX Toolset v3.11\bin"
    )
    foreach ($c in $candidates) {
        if ($c -and (Test-Path "$c\heat.exe")) { $wixBin = $c; break }
    }
    if (-not $wixBin) {
        $heat = Get-Command "heat.exe" -ErrorAction SilentlyContinue
        if ($heat) { $wixBin = Split-Path $heat.Source }
    }
    if (-not $wixBin) {
        throw "WiX Toolset v3 not found. Install with: choco install wixtoolset"
    }
    Write-Host "    WiX: $wixBin"

    # Normalize version for WiX (must be X.Y.Z or X.Y.Z.W — digits only)
    $wixVersion = ($Version -replace '-.*', '') -replace '^v', ''
    if ($wixVersion -notmatch '^\d+(\.\d+)*$') { $wixVersion = "0.0.0" }
    Write-Host "    Version: $Version  (WiX: $wixVersion)"

    # ── 2. Stage all files ───────────────────────────────────────────────────────
    Write-Host "==> Staging install tree..."
    $stageDir = "dist\stage"
    Remove-Item -Recurse -Force $stageDir -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $stageDir | Out-Null

    Copy-Item "target\release\adapter-gui.exe"              "$stageDir\openrig.exe"
    Copy-Item "libs\nam\windows-x64\libNeuralAudioCAPI.dll" "$stageDir\"

    New-Item -ItemType Directory -Force "$stageDir\libs\lv2" | Out-Null
    New-Item -ItemType Directory -Force "$stageDir\libs\nam" | Out-Null
    Copy-Item -Recurse "libs\lv2\windows-x64" "$stageDir\libs\lv2\windows-x64"
    Copy-Item -Recurse "libs\nam\windows-x64" "$stageDir\libs\nam\windows-x64"
    Copy-Item -Recurse "data"                 "$stageDir\data"
    Copy-Item -Recurse "assets"               "$stageDir\assets"
    Copy-Item -Recurse "captures"             "$stageDir\captures"

    Write-Host "    Stage ready ($(Get-ChildItem -Recurse $stageDir | Measure-Object).Count files)"

    # ── 3. Create .zip bundle ────────────────────────────────────────────────────
    Write-Host "==> Creating .zip..."
    $zipName = "OpenRig-${Version}-windows-x64"
    $zipDir  = "dist\$zipName"
    Remove-Item -Recurse -Force $zipDir -ErrorAction SilentlyContinue
    Copy-Item -Recurse $stageDir $zipDir
    $zipOut  = "dist\${zipName}.zip"
    Remove-Item -Force $zipOut -ErrorAction SilentlyContinue
    Compress-Archive -Path $zipDir -DestinationPath $zipOut
    Write-Host "    $zipOut"

    # ── 4. Harvest files with heat.exe ───────────────────────────────────────────
    Write-Host "==> Harvesting files with heat.exe..."
    $stageDirAbs = (Resolve-Path $stageDir).Path
    New-Item -ItemType Directory -Force "wix" | Out-Null
    & "$wixBin\heat.exe" dir $stageDirAbs `
        -o "wix\heat_stage.wxs" `
        -cg StageFiles `
        -dr INSTALLFOLDER `
        -var "var.SourceDir" `
        -gg -sreg -srd `
        -sw5150 -sw5151 -sw5152
    if ($LASTEXITCODE -ne 0) { throw "heat.exe failed with exit code $LASTEXITCODE" }
    Write-Host "    wix\heat_stage.wxs generated"

    # ── 5. Compile WiX sources with candle.exe ───────────────────────────────────
    Write-Host "==> Compiling WiX sources..."
    & "$wixBin\candle.exe" -arch x64 `
        "-dVersion=$wixVersion" `
        "-dSourceDir=$stageDirAbs" `
        "wix\main.wxs" `
        -o "wix\main.wixobj"
    if ($LASTEXITCODE -ne 0) { throw "candle.exe (main.wxs) failed" }

    & "$wixBin\candle.exe" -arch x64 `
        "-dSourceDir=$stageDirAbs" `
        "wix\heat_stage.wxs" `
        -o "wix\heat_stage.wixobj"
    if ($LASTEXITCODE -ne 0) { throw "candle.exe (heat_stage.wxs) failed" }

    # ── 6. Link MSI with light.exe ───────────────────────────────────────────────
    Write-Host "==> Linking MSI..."
    $msiOut = "dist\OpenRig-${Version}-windows-x64.msi"
    New-Item -ItemType Directory -Force "dist" | Out-Null
    & "$wixBin\light.exe" `
        -ext WixUIExtension `
        "wix\main.wixobj" `
        "wix\heat_stage.wixobj" `
        -o $msiOut `
        -b $stageDirAbs `
        -sw1076
    if ($LASTEXITCODE -ne 0) { throw "light.exe failed with exit code $LASTEXITCODE" }

    Write-Host ""
    Write-Host "==> Done:"
    Write-Host "    $zipOut"
    Write-Host "    $msiOut"
    Write-Host ""
    Write-Host "Para instalar:"
    Write-Host "    msiexec /i $msiOut"
} finally {
    Pop-Location
}
