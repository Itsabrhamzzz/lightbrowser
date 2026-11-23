# Builds LightBrowser (release) and installs it for the current user - no admin rights needed.
# Creates Start Menu and Desktop shortcuts. Run again to update.
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$dest = Join-Path $env:LOCALAPPDATA 'Programs\LightBrowser'

# cargo reports progress on stderr, which Windows PowerShell treats as errors under 'Stop';
# rely on the exit code instead.
$ErrorActionPreference = 'Continue'
& (Join-Path $root 'build.ps1') build --release 2>&1 | ForEach-Object { "$_" } | Where-Object { $_ -match 'error|warning|Finished' }
if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
$ErrorActionPreference = 'Stop'

# Close a running installed copy so the exe can be replaced, and wait until it has really exited.
$running = Get-Process lightbrowser -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($dest, [StringComparison]::OrdinalIgnoreCase) }
if ($running) {
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
}

New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $root 'target\release\lightbrowser.exe') (Join-Path $dest 'LightBrowser.exe') -Force

$exe = Join-Path $dest 'LightBrowser.exe'
$shell = New-Object -ComObject WScript.Shell
$links = @(
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'LightBrowser.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'LightBrowser.lnk')
)
foreach ($path in $links) {
    $lnk = $shell.CreateShortcut($path)
    $lnk.TargetPath = $exe
    $lnk.WorkingDirectory = $dest
    $lnk.IconLocation = "$exe,0"
    $lnk.Description = 'LightBrowser'
    $lnk.Save()
}
Write-Host "Installed to $dest"
Write-Host 'Shortcuts: Start Menu and Desktop.'
