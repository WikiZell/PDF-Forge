# Sends real keystrokes to whatever window is in the foreground after a delay (used to drive native
# Windows dialogs). Keys use System.Windows.Forms.SendKeys syntax, e.g. "C:\a\b.pdf{ENTER}".
# Folder pickers need a second Enter (the first navigates into the typed folder, the second selects it).
param([string]$Keys, [int]$DelayMs = 2500, [string]$Keys2 = "", [int]$Delay2Ms = 1800)
Add-Type -AssemblyName System.Windows.Forms
Start-Sleep -Milliseconds $DelayMs
[System.Windows.Forms.SendKeys]::SendWait($Keys)
if ($Keys2 -ne "") {
  Start-Sleep -Milliseconds $Delay2Ms
  [System.Windows.Forms.SendKeys]::SendWait($Keys2)
}
