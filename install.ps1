# Builds Airtight, copies it to %LOCALAPPDATA%\Programs\Airtight, adds a Start
# menu shortcut and starts it. One fixed path: Windows Firewall asks once for
# it, and every later build keeps the same rule.
#   powershell -ExecutionPolicy Bypass -File install.ps1
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:LOCALAPPDATA 'Programs\Airtight'
$exe = Join-Path $dir 'airtight.exe'

cargo build -p airtight-app --release --manifest-path "$PSScriptRoot\Cargo.toml"
if ($LASTEXITCODE) { exit $LASTEXITCODE }

# Quit the running copy: it sends TEARDOWN first, so the receiver is free.
$running = Get-Process airtight -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }
if ($running) {
    & $exe --quit
    $running | ForEach-Object { $_.WaitForExit(10000) | Out-Null }
}
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item "$PSScriptRoot\target\release\airtight.exe" $exe -Force

$link = Join-Path ([Environment]::GetFolderPath('Programs')) 'Airtight.lnk'
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($link)
$shortcut.TargetPath = $exe
$shortcut.WorkingDirectory = $dir
$shortcut.Save()

Start-Process $exe
Write-Output "Installed $exe"
