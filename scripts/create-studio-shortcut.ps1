# Opt-in shortcut creation. Never overwrites an existing shortcut.
[CmdletBinding()]
param([string]$StudioExecutable, [string]$Destination)
$ErrorActionPreference = 'Stop'
$shortcutRoot = Split-Path $PSScriptRoot -Parent
$shortcutLaunch = & (Join-Path $shortcutRoot 'launch-studio.ps1') -StudioExecutable $StudioExecutable -CheckOnly | ConvertFrom-Json
if (-not $Destination) { $Destination = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Lumeryn Studio.lnk' }
$shortcutPath = [IO.Path]::GetFullPath($Destination)
if ([IO.Path]::GetExtension($shortcutPath) -ne '.lnk') { throw 'Choose a .lnk shortcut path.' }
if (Test-Path -LiteralPath $shortcutPath) { throw 'The shortcut already exists; choose another destination.' }
if (-not (Test-Path -LiteralPath (Split-Path $shortcutPath -Parent) -PathType Container)) { throw 'The shortcut destination directory does not exist.' }
$shortcutShell = New-Object -ComObject WScript.Shell
try {
    $shortcutLink = $shortcutShell.CreateShortcut($shortcutPath)
    try {
        $shortcutLink.TargetPath = $shortcutLaunch.executable
        $shortcutLink.WorkingDirectory = $shortcutRoot
        $shortcutLink.IconLocation = $shortcutLaunch.executable + ',0'
        $shortcutLink.Description = 'Open Lumeryn Studio. Projects remain local.'
        $shortcutLink.Save()
    }
    finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcutLink) | Out-Null }
}
finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcutShell) | Out-Null }
Write-Output $shortcutPath
