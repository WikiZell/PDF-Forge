# Dumps the Windows UI Automation tree of the PDF Forge window: this is what screen readers (Narrator,
# NVDA, JAWS) consume. Output lines: "<depth> <ControlType> | <Name>".
param([int]$MaxNodes = 1500)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$win = $null
foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
  if ($w.Current.Name -like "PDF Forge*") { $win = $w; break }
}
if (-not $win) { "NO WINDOW"; exit 2 }
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$script:count = 0
function Walk($el, $depth) {
  if ($script:count -ge $MaxNodes -or $depth -gt 40) { return }
  $script:count++
  $t = $el.Current.ControlType.ProgrammaticName -replace "ControlType\.", ""
  "{0} {1} | {2}" -f $depth, $t, $el.Current.Name
  $c = $walker.GetFirstChild($el)
  while ($c -ne $null) { Walk $c ($depth + 1); $c = $walker.GetNextSibling($c) }
}
# WebView2 builds its accessibility tree lazily once a UIA client asks; query twice with a pause.
Walk $win 0 | Out-Null
Start-Sleep -Seconds 3
$script:count = 0
Walk $win 0
