# Sends real keystrokes to whatever window is in the foreground after a delay (used to drive native
# Windows file dialogs). Keys use System.Windows.Forms.SendKeys syntax, e.g. "C:\a\b.pdf{ENTER}".
param([string]$Keys, [int]$DelayMs = 2500)
Add-Type -AssemblyName System.Windows.Forms
Start-Sleep -Milliseconds $DelayMs
[System.Windows.Forms.SendKeys]::SendWait($Keys)
