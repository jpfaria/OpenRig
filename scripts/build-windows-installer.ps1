<#
.SYNOPSIS
    Builds the Windows installer (.msi + .zip) on a local Windows machine.

.DESCRIPTION
    Responsibility: runs the CI Windows packaging path end to end on one machine.

    Run it with PowerShell 7 (pwsh, winget Microsoft.PowerShell), as CI does:
    Windows PowerShell 5 misreads the UTF-8 packaging scripts.
    Prerequisites (winget): Rustlang.Rustup, Microsoft.VisualStudio.2022.BuildTools
    (C++ workload), Kitware.CMake, LLVM.LLVM, Git.Git (with Git LFS),
    WiXToolset.WiXToolset, MSYS2.MSYS2 plus
    `C:\msys64\usr\bin\pacman.exe -S mingw-w64-x86_64-gcc-libs mingw-w64-x86_64-winpthreads-git`,
    and optionally ImageMagick.ImageMagick for the installer icon.
    The LV2/VST3 plugins come from this repo's plugins\source (Git LFS, #1093).
    Output lands in dist\.

.PARAMETER Version
    Version stamped into the package, e.g. "0.7.0-beta.1" (default: dev)

.EXAMPLE
    pwsh .\scripts\build-windows-installer.ps1 0.7.0-beta.1
#>
param(
    [string]$Version = "dev"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $RepoRoot

try {
    # The cpal asio feature runs bindgen, which needs libclang.
    if (-not $env:LIBCLANG_PATH -and (Test-Path "C:\Program Files\LLVM\bin\libclang.dll")) {
        $env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
    }
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) { $env:PATH = "$cargoBin;$env:PATH" }

    # Stamp the version into the workspace manifest, as the release CI does: the
    # launcher footer renders env!("CARGO_PKG_VERSION"), so without this a local
    # build ships the manifest's stale version under a fresh artifact name.
    if ($Version -ne "dev") {
        $bash = "C:\Program Files\Git\bin\bash.exe"
        & $bash -c "source scripts/lib/release-version.sh && set_workspace_version Cargo.toml '$Version'"
        if ($LASTEXITCODE -ne 0) { throw "set_workspace_version failed with exit code $LASTEXITCODE" }
        $stamped = $true
    }

    Write-Host "==> Building release binaries..."
    cargo build --release -p adapter-gui -p adapter-console -p adapter-console-rig -p adapter-render
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }

    Write-Host "==> Fetching the plugin binaries (Git LFS)..."
    git lfs pull
    if ($LASTEXITCODE -ne 0) { throw "git lfs pull failed with exit code $LASTEXITCODE" }

    & (Join-Path $RepoRoot "scripts\package-windows.ps1") $Version
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) { throw "package-windows.ps1 failed with exit code $LASTEXITCODE" }

    Get-ChildItem dist -Filter "OpenRig-*-windows-x64.*" | ForEach-Object { Write-Host "    $($_.FullName)" }
}
finally {
    if ($stamped) { git checkout -q -- Cargo.toml Cargo.lock }
    Pop-Location
}
