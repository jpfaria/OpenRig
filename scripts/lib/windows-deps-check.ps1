# Responsibility: fails when a staged Windows binary imports a DLL the install does not provide.
#
# The build machine has the VC++ runtime (and whatever else Visual Studio
# brought) in System32, so launching a staged exe there proves nothing: a DLL
# missing from the package only shows up on a clean PC (#978). This reads each
# binary's import table instead. A dependency is satisfied when it is staged
# next to openrig.exe, or when it is part of Windows itself. The VC++ runtime
# never counts as part of Windows, whatever the build machine has installed.
#
#   scripts\lib\windows-deps-check.ps1 -StageDir dist\stage
#
# Checks the top-level exes/DLLs and every plugin DLL: the loader resolves a
# plugin's imports through the application directory, not the plugin's own.
param([Parameter(Mandatory = $true)][string]$StageDir)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows-msvc.ps1")

$msvcTools = Get-NewestVersionDir (Join-Path (Get-MsvcInstallPath) "VC\Tools\MSVC")
$dumpbin = Join-Path $msvcTools.FullName "bin\Hostx64\x64\dumpbin.exe"
if (-not (Test-Path $dumpbin)) { throw "dumpbin.exe not found at $dumpbin" }

$stage = (Resolve-Path $StageDir).Path
$system32 = Join-Path $env:SystemRoot "System32"
$vcRuntime = '^(vcruntime|msvcp|concrt|vccorlib|vcomp)\d'

function Get-Imports([string]$Binary) {
    $out = & $dumpbin /nologo /dependents $Binary
    if ($LASTEXITCODE -ne 0) { throw "dumpbin failed on $Binary" }
    $out | ForEach-Object { if ($_ -match '^\s+(\S+\.dll)\s*$') { $Matches[1] } }
}

function Test-Provided([string]$Dll) {
    if (Test-Path (Join-Path $stage $Dll)) { return $true }
    if ($Dll -match '^(api|ext)-ms-') { return $true }
    if ($Dll -match $vcRuntime) { return $false }
    return (Test-Path (Join-Path $system32 $Dll))
}

$binaries = @(Get-ChildItem -Path "$stage\*" -File -Include *.exe, *.dll)
if (Test-Path "$stage\plugins") {
    $binaries += Get-ChildItem "$stage\plugins" -Recurse -File -Filter *.dll
}

$missing = @()
foreach ($bin in $binaries) {
    foreach ($dll in (Get-Imports $bin.FullName | Sort-Object -Unique)) {
        if (-not (Test-Provided $dll)) {
            $missing += "$($bin.FullName.Substring($stage.Length + 1)) -> $dll"
        }
    }
}

Write-Host ("    checked {0} binaries" -f $binaries.Count)
if ($missing.Count -gt 0) {
    $missing | ForEach-Object { Write-Host "    MISSING: $_" }
    throw ("{0} DLL import(s) not provided by the package" -f $missing.Count)
}
