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

    # ── 2. Generate Windows icon (.ico) ──────────────────────────────────────────
    Write-Host "==> Generating Windows icon..."
    New-Item -ItemType Directory -Force "wix" | Out-Null
    $svgPath = "crates\adapter-gui\ui\assets\openrig-logomark.svg"
    $icoPath = (Resolve-Path "wix").Path + "\openrig.ico"
    $iconOk = $false
    try {
        # Convert SVG to PNG at multiple sizes, then combine into ICO
        $tmpPngs = @()
        foreach ($size in @(16, 32, 48, 64, 128, 256)) {
            $png = "wix\icon_${size}.png"
            & magick -background none -density 300 $svgPath -resize "${size}x${size}" $png 2>$null
            if ($LASTEXITCODE -eq 0 -and (Test-Path $png)) { $tmpPngs += $png }
        }
        if ($tmpPngs.Count -gt 0) {
            & magick @tmpPngs $icoPath 2>$null
            $iconOk = ($LASTEXITCODE -eq 0 -and (Test-Path $icoPath))
            foreach ($png in $tmpPngs) { Remove-Item $png -ErrorAction SilentlyContinue }
        }
    } catch {}

    if (-not $iconOk) {
        # Fallback: use the exe itself as icon source (no dedicated icon)
        Write-Host "    WARNING: ImageMagick not available, using exe as icon source"
        $stageDirAbs0 = (Resolve-Path "target\release").Path
        $icoPath = $stageDirAbs0 + "\adapter-gui.exe"
    }
    Write-Host "    Icon: $icoPath"

    # ── 3. Stage all files ───────────────────────────────────────────────────────
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

    # ── Copy MinGW runtime DLLs (required by libNeuralAudioCAPI.dll) ────────────
    Write-Host "==> Copying MinGW runtime DLLs..."
    $mingwDlls = @("libgcc_s_seh-1.dll", "libstdc++-6.dll", "libwinpthread-1.dll")
    $mingwSearchPaths = @(
        "C:\msys64\mingw64\bin",
        "C:\msys64\ucrt64\bin",
        "C:\Strawberry\c\bin",
        "C:\ProgramData\chocolatey\lib\mingw\tools\install\mingw64\bin"
    )
    foreach ($dll in $mingwDlls) {
        $found = $false
        foreach ($p in $mingwSearchPaths) {
            $src = "$p\$dll"
            if (Test-Path $src) {
                Copy-Item $src "$stageDir\"
                Write-Host "    $dll  <-  $p"
                $found = $true
                break
            }
        }
        if (-not $found) { Write-Host "    WARNING: $dll not found (may not be needed)" }
    }

    $stageDirAbs = (Resolve-Path $stageDir).Path
    Write-Host "    Stage ready"

    # ── 4. Create .zip bundle ────────────────────────────────────────────────────
    Write-Host "==> Creating .zip..."
    $zipName = "OpenRig-${Version}-windows-x64"
    $zipDir  = "dist\$zipName"
    Remove-Item -Recurse -Force $zipDir -ErrorAction SilentlyContinue
    Copy-Item -Recurse $stageDir $zipDir
    $zipOut  = "dist\${zipName}.zip"
    Remove-Item -Force $zipOut -ErrorAction SilentlyContinue
    Compress-Archive -Path $zipDir -DestinationPath $zipOut
    Write-Host "    $zipOut"

    # ── 5. Harvest files with heat.exe ───────────────────────────────────────────
    Write-Host "==> Harvesting files with heat.exe..."
    & "$wixBin\heat.exe" dir $stageDirAbs `
        -o "wix\heat_stage.wxs" `
        -cg StageFiles `
        -dr INSTALLFOLDER `
        -var "var.SourceDir" `
        -gg -sreg -srd `
        -sw5150 -sw5151 -sw5152
    if ($LASTEXITCODE -ne 0) { throw "heat.exe failed with exit code $LASTEXITCODE" }
    Write-Host "    wix\heat_stage.wxs generated"

    # ── 6. Compile WiX sources with candle.exe ───────────────────────────────────
    Write-Host "==> Compiling WiX sources..."
    & "$wixBin\candle.exe" -arch x64 `
        "-dVersion=$wixVersion" `
        "-dSourceDir=$stageDirAbs" `
        "-dIconFile=$icoPath" `
        "wix\main.wxs" `
        -o "wix\main.wixobj"
    if ($LASTEXITCODE -ne 0) { throw "candle.exe (main.wxs) failed" }

    & "$wixBin\candle.exe" -arch x64 `
        "-dSourceDir=$stageDirAbs" `
        "wix\heat_stage.wxs" `
        -o "wix\heat_stage.wixobj"
    if ($LASTEXITCODE -ne 0) { throw "candle.exe (heat_stage.wxs) failed" }

    # ── 7. Link MSI with light.exe ───────────────────────────────────────────────
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
