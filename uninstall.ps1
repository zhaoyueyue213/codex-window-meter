$ErrorActionPreference = 'Stop'

$installDirectory = Join-Path $env:LOCALAPPDATA 'Programs\CodexWindowMeter'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexWindowMeter'
$backupKey = 'HKCU:\Software\CodexWindowMeter'

Get-Process -Name 'codex-window-meter' -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-ItemProperty -Path $runKey -Name 'CodexWindowMeter' -ErrorAction SilentlyContinue
Remove-Item -Path $uninstallKey -ErrorAction SilentlyContinue

$previousStartup = Get-ItemPropertyValue -Path $backupKey -Name 'PreviousMeterStartup' -ErrorAction SilentlyContinue
if ($previousStartup) {
    $previousExecutable = $previousStartup.Trim('"')
    if (Test-Path -LiteralPath $previousExecutable -PathType Leaf) {
        Set-ItemProperty -Path $runKey -Name 'CodexTitlebarMeter' -Value $previousStartup
    }
    Remove-Item -Path $backupKey -ErrorAction SilentlyContinue
}

Remove-Item -LiteralPath (Join-Path $installDirectory 'codex-window-meter.exe') -ErrorAction SilentlyContinue
Remove-Item -LiteralPath (Join-Path $installDirectory 'uninstall.ps1') -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $installDirectory -ErrorAction SilentlyContinue
Write-Output 'Codex Window Meter uninstalled.'
