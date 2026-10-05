<#
.SYNOPSIS
    Builds the Windows installer (.msi + .zip) on a local Windows machine.

.DESCRIPTION
    Responsibility: runs the CI Windows packaging path end to end on one machine.

    Prerequisites (winget): Rustlang.Rustup, Microsoft.VisualStudio.2022.BuildTools
    (C++ workload), Kitware.CMake, LLVM.LLVM, Git.Git (with Git LFS),
    WiXToolset.WiXToolset, MSYS2.MSYS2 plus
    `C:\msys64\usr\bin\pacman.exe -S mingw-w64-x86_64-gcc-libs mingw-w64-x86_64-winpthreads-git`,
    and optionally ImageMagick.ImageMagick for the installer icon.
    The plugin tree comes from a local OpenRig-plugins checkout (with LFS).
    Output lands in dist\.

.PARAMETER Version
    Version stamped into the package, e.g. "0.6.2" (default: dev)

.PARAMETER PluginsRoot
    OpenRig-plugins checkout (default: ..\OpenRig-plugins next to this repo)

.EXAMPLE
    .\scripts\build-windows-installer.ps1 0.6.2 -PluginsRoot C:\openrig-plugins
#>
param(
    [string]$Version = "dev",
    [string]$PluginsRoot = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $PluginsRoot) { $PluginsRoot = Join-Path (Split-Path -Parent $RepoRoot) "OpenRig-plugins" }
Push-Location $RepoRoot

try {
    # The cpal asio feature runs bindgen, which needs libclang.
    if (-not $env:LIBCLANG_PATH -and (Test-Path "C:\Program Files\LLVM\bin\libclang.dll")) {
        $env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
    }
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) { $env:PATH = "$cargoBin;$env:PATH" }

    Write-Host "==> Building release binaries..."
    cargo build --release -p adapter-gui -p adapter-console -p adapter-console-rig -p adapter-render
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }

    Write-Host "==> Linking the plugin tree from $PluginsRoot..."
    $pluginSource = Join-Path $PluginsRoot "plugins\source"
    if (-not (Test-Path $pluginSource)) { throw "no plugins\source under $PluginsRoot — clone OpenRig-plugins with LFS" }
    New-Item -ItemType Directory -Force "plugins" | Out-Null
    if (Test-Path "plugins\source") { (Get-Item "plugins\source").Delete() }
    New-Item -ItemType Junction -Path "plugins\source" -Target $pluginSource | Out-Null

    & (Join-Path $RepoRoot "scripts\package-windows.ps1") $Version
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) { throw "package-windows.ps1 failed with exit code $LASTEXITCODE" }

    Get-ChildItem dist -Filter "OpenRig-*-windows-x64.*" | ForEach-Object { Write-Host "    $($_.FullName)" }
}
finally {
    Pop-Location
}
