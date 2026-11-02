# Saves a screenshot of the whole desktop (including native dialogs) after a delay.
param([string]$Path, [int]$DelayMs = 3000)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Start-Sleep -Milliseconds $DelayMs
$b = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
$bmp.Save($Path)
$g.Dispose(); $bmp.Dispose()
