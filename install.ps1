$ErrorActionPreference = 'Stop'

$source = Join-Path $PSScriptRoot 'target\release\codex-window-meter.exe'
$installDirectory = Join-Path $env:LOCALAPPDATA 'Programs\CodexWindowMeter'
$target = Join-Path $installDirectory 'codex-window-meter.exe'
$uninstaller = Join-Path $installDirectory 'uninstall.ps1'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexWindowMeter'
$backupKey = 'HKCU:\Software\CodexWindowMeter'

if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw 'Build the release executable with cargo build --release first.'
}

Get-Process -Name 'codex-window-meter' -ErrorAction SilentlyContinue | Stop-Process -Force
New-Item -ItemType Directory -Path $installDirectory -Force | Out-Null
Copy-Item -LiteralPath $source -Destination $target -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'uninstall.ps1') -Destination $uninstaller -Force

$previousStartup = Get-ItemPropertyValue -Path $runKey -Name 'CodexTitlebarMeter' -ErrorAction SilentlyContinue
if ($previousStartup) {
    New-Item -Path $backupKey -Force | Out-Null
    Set-ItemProperty -Path $backupKey -Name 'PreviousMeterStartup' -Value $previousStartup
    Remove-ItemProperty -Path $runKey -Name 'CodexTitlebarMeter'
    Get-Process -Name 'CodexTitlebarMeter' -ErrorAction SilentlyContinue | Stop-Process -Force
}

Set-ItemProperty -Path $runKey -Name 'CodexWindowMeter' -Value ('"{0}"' -f $target)
if (-not (Test-Path -Path $uninstallKey)) {
    New-Item -Path $uninstallKey -Force | Out-Null
}
Set-ItemProperty -Path $uninstallKey -Name 'DisplayName' -Value 'Codex Window Meter'
Set-ItemProperty -Path $uninstallKey -Name 'DisplayVersion' -Value '0.1.0'
Set-ItemProperty -Path $uninstallKey -Name 'Publisher' -Value 'Local'
Set-ItemProperty -Path $uninstallKey -Name 'InstallLocation' -Value $installDirectory
Set-ItemProperty -Path $uninstallKey -Name 'UninstallString' -Value ('powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{0}"' -f $uninstaller)
Set-ItemProperty -Path $uninstallKey -Name 'NoModify' -Value 1 -Type DWord
Set-ItemProperty -Path $uninstallKey -Name 'NoRepair' -Value 1 -Type DWord

Start-Process -FilePath $target -WindowStyle Hidden
Write-Output "Codex Window Meter installed: $target"
