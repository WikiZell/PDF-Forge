<#
 Downloads the latest CI-built Windows installers (unsigned) into .\release next to where you run it.
   powershell -ExecutionPolicy Bypass -File scripts\get-windows-build.ps1
 Uses git if present, otherwise downloads the branch as a zip. No admin rights needed.
 The repository must be reachable from your machine (public, or sign in to git first).
#>
param([string]$Dest = (Join-Path (Get-Location) "release"))
$ErrorActionPreference = "Stop"
$repo = "https://github.com/WikiZell/PDF-Forge"
if (Test-Path $Dest) { Remove-Item $Dest -Recurse -Force }
if (Get-Command git -ErrorAction SilentlyContinue) {
  git clone --branch windows-builds --depth 1 "$repo.git" $Dest
  Remove-Item (Join-Path $Dest ".git") -Recurse -Force -ErrorAction SilentlyContinue
} else {
  $zip = Join-Path $env:TEMP "pdf-forge-windows-builds.zip"
  Invoke-WebRequest "$repo/archive/refs/heads/windows-builds.zip" -OutFile $zip
  $tmp = Join-Path $env:TEMP "pdf-forge-wb"
  if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
  Expand-Archive $zip $tmp
  Move-Item (Get-ChildItem $tmp | Select-Object -First 1).FullName $Dest
}
Get-ChildItem $Dest | Select-Object Name, @{n="MB";e={[math]::Round($_.Length/1MB,1)}}
"`nInstall (per-user, no admin): run the *-setup.exe. Windows SmartScreen may warn because the build is unsigned."
