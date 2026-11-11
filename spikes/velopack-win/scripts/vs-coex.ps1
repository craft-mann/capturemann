$ErrorActionPreference = 'Continue'
$root = "$env:LOCALAPPDATA\CaptureMannSpike"
$nsis = "$env:LOCALAPPDATA\Programs\CaptureMann"
function Ver { (Get-Content "$root\current\sq.version" -ErrorAction SilentlyContinue | Select-String '<version>').ToString().Trim() }
Write-Output "### 1. both installed: Apps entries, shortcuts, winget view"
Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' | Where-Object { $_.DisplayName -like 'CaptureMann*' } | ForEach-Object { "key=$($_.PSChildName) name=$($_.DisplayName) ver=$($_.DisplayVersion) publisher=$($_.Publisher) loc=$($_.InstallLocation)" }
Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter 'CaptureMann*.lnk' | ForEach-Object { $_.FullName.Replace("$env:APPDATA\Microsoft\Windows\Start Menu\Programs\", '') }
Write-Output "--- winget list (local, read-only)"
winget list --name CaptureMann --accept-source-agreements 2>&1 | Select-Object -Last 8
Write-Output "### 2. Setup(spike3) over installed spike1 (silent): upgrade in place?"
Write-Output "before: $(Ver)"
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process C:\lab\velospike\Setup-spike3.exe -ArgumentList '--silent' -Wait -PassThru
"exit $($p.ExitCode) in $([math]::Round($sw.Elapsed.TotalSeconds,1)) s; after: $(Ver)"
Get-ChildItem "$root\packages" | ForEach-Object { "$($_.Name) $($_.Length)" }
Write-Output "### 3. Setup(spike1) over installed spike3 (silent): downgrade by installer?"
$p = Start-Process C:\lab\velospike\Setup-spike1.exe -ArgumentList '--silent' -Wait -PassThru
"exit $($p.ExitCode); after: $(Ver)"
Write-Output "### 4. migration hook idea: NSIS silent uninstall from outside, with PATH entry"
$setup031 = 'C:\lab\capturemann-0.3.1-windows-x64-setup.exe'
$sw.Restart()
$p = Start-Process $setup031 -ArgumentList '/S', '/ADDPATH' -Wait -PassThru
"NSIS silent install (with /ADDPATH) exit $($p.ExitCode) in $([math]::Round($sw.Elapsed.TotalSeconds,1)) s"
"user PATH: " + [Environment]::GetEnvironmentVariable('Path', 'User')
$q = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CaptureMann').QuietUninstallString
"QuietUninstallString: $q"
New-Item -ItemType Directory -Force "$env:APPDATA\CaptureMann" | Out-Null
$had = Test-Path "$env:APPDATA\CaptureMann"
$sw.Restart()
$p = Start-Process "$nsis\uninstall.exe" -ArgumentList '/S', "_?=$nsis" -Wait -PassThru
"NSIS uninstall exit $($p.ExitCode) in $([math]::Round($sw.Elapsed.TotalSeconds,1)) s (with _?= it waits)"
Remove-Item $nsis -Recurse -Force -ErrorAction SilentlyContinue
"NSIS dir exists after: $(Test-Path $nsis); NSIS key exists: $(Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CaptureMann'); settings dir survives: $(Test-Path "$env:APPDATA\CaptureMann")"
"user PATH after: " + [Environment]::GetEnvironmentVariable('Path', 'User')
"Velopack install untouched: $(Ver)"
Write-Output "### 5. restore: reinstall NSIS 0.3.1 silent without PATH (as found)"
$p = Start-Process $setup031 -ArgumentList '/S' -Wait -PassThru
"exit $($p.ExitCode); NSIS exe present: $(Test-Path "$nsis\cm-desktop.exe")"
"user PATH: " + [Environment]::GetEnvironmentVariable('Path', 'User')
