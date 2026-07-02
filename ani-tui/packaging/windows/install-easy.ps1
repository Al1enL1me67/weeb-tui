# Compatibility wrapper for the old easy installer name.
# The supported Windows installer is install.ps1.

[CmdletBinding()]
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\weeb-tui",
    [switch]$SkipDependencies,
    [switch]$SkipChafa
)

$ErrorActionPreference = "Stop"

$localInstaller = Join-Path $PSScriptRoot "install.ps1"
if (Test-Path $localInstaller) {
    & $localInstaller @PSBoundParameters
    exit
}

$remoteInstaller = "https://github.com/silent9669/weeb-tui/releases/latest/download/install.ps1"
$tempInstaller = Join-Path $env:TEMP "weeb-tui-install.ps1"
Invoke-WebRequest -Uri $remoteInstaller -OutFile $tempInstaller -UseBasicParsing -ErrorAction Stop
& $tempInstaller @PSBoundParameters
