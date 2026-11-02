# Runs the NVDA screen reader against the installed app and records what it speaks.
# CI-only, non-gating evidence: NVDA writes every utterance to its debug log.
param([string]$AppExe, [string]$Pdf, [string]$OutDir)
$ErrorActionPreference = "Continue"
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Windows.Forms, System.Drawing, Microsoft.VisualBasic
$nvda = Get-ChildItem "C:\Program Files*\NVDA\nvda.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $nvda) { "NVDA not installed"; exit 0 }
$cfg = "$env:RUNNER_TEMP\nvda-cfg"
New-Item -ItemType Directory -Force $cfg | Out-Null
@"
[general]
  showWelcomeDialogAtStartup = False
  loggingLevel = DEBUG
[update]
  autoCheckForUpdates = False
  startupNotification = False
[speech]
  synth = silence
"@ | Set-Content "$cfg\nvda.ini" -Encoding ascii
$log = Join-Path $OutDir "nvda.log"
Start-Process $nvda.FullName -ArgumentList "--minimal","-c","`"$cfg`"","--debug-logging","--log-file","`"$log`""
Start-Sleep -Seconds 20
$app = Start-Process $AppExe -ArgumentList "`"$Pdf`"" -PassThru
Start-Sleep -Seconds 20
try { [Microsoft.VisualBasic.Interaction]::AppActivate("PDF Forge") } catch { "AppActivate failed: $_" }
Start-Sleep -Seconds 2
for ($i = 0; $i -lt 30; $i++) {
  [System.Windows.Forms.SendKeys]::SendWait("{TAB}")
  Start-Sleep -Milliseconds 900
}
$b = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
$bmp.Save((Join-Path $OutDir "nvda-desktop.png")); $g.Dispose(); $bmp.Dispose()
Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
Get-Process nvda -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
if (Test-Path $log) {
  # keep only the speech lines
  Select-String -Path $log -Pattern "Speaking \[" | ForEach-Object { $_.Line } | Set-Content (Join-Path $OutDir "nvda-speech.txt") -Encoding utf8
  "speech lines: $((Get-Content (Join-Path $OutDir 'nvda-speech.txt')).Count)"
} else { "no NVDA log produced" }
