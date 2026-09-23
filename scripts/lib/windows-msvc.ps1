# Responsibility: locates the Visual Studio install that carries the MSVC toolset.
#
# Dot-sourced by the Windows packaging scripts, which need two things from it:
# dumpbin (to read a binary's DLL imports) and the redistributable C/C++
# runtime DLLs (to ship them next to openrig.exe).

function Get-MsvcInstallPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) { throw "vswhere.exe not found: is Visual Studio installed?" }
    $path = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
    if (-not $path) { throw "no Visual Studio install with the MSVC x64 toolset" }
    return $path
}

# Newest directory under $Root whose name is a dotted version (14.44.35112),
# skipping siblings such as `v143` that hold merge modules instead.
function Get-NewestVersionDir([string]$Root) {
    Get-ChildItem $Root -Directory |
        Where-Object { $_.Name -match '^\d+(\.\d+)+$' } |
        Sort-Object { [version]$_.Name } -Descending |
        Select-Object -First 1
}
