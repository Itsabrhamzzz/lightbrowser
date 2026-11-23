# Removes the installed LightBrowser and its shortcuts.
# Your browsing data (settings, bookmarks, cookies, extensions) is kept unless you pass -RemoveData.
param([switch]$RemoveData)
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\LightBrowser'

Get-Process lightbrowser -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($dest, [StringComparison]::OrdinalIgnoreCase) } |
    Stop-Process -Force

foreach ($path in @(
        (Join-Path ([Environment]::GetFolderPath('Programs')) 'LightBrowser.lnk'),
        (Join-Path ([Environment]::GetFolderPath('Desktop')) 'LightBrowser.lnk'))) {
    if (Test-Path $path) { Remove-Item $path -Force }
}
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }

if ($RemoveData) {
    $data = Join-Path $env:LOCALAPPDATA 'LightBrowser'
    if (Test-Path $data) { Remove-Item $data -Recurse -Force }
    Write-Host 'Removed LightBrowser and its data.'
} else {
    Write-Host 'Removed LightBrowser. Your data is kept in %LOCALAPPDATA%\LightBrowser (use -RemoveData to delete it).'
}
