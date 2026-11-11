$ErrorActionPreference = 'SilentlyContinue'
$root = "$env:LOCALAPPDATA\CaptureMannSpike"
if (Test-Path "$root\Update.exe") { Start-Process "$root\Update.exe" -ArgumentList '--uninstall', '--silent' -Wait; Start-Sleep 4 }
Get-Process | Where-Object { $_.Path -like "$root*" -or $_.Path -like 'C:\lab\velospike*' } | Stop-Process -Force
Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.CommandLine -like '*vs-serve*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
Remove-Item $root -Recurse -Force
Remove-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CaptureMannSpike'
Remove-Item "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\CaptureMann.lnk"
Remove-Item "$env:APPDATA\CaptureMannSpike" -Recurse -Force
Remove-Item "$env:LOCALAPPDATA\velopack" -Recurse -Force
Get-ChildItem $env:TEMP -Filter 'velopack*' | Remove-Item -Recurse -Force
Remove-Item C:\lab\velospike -Recurse -Force
Remove-Item C:\lab\vs-setup.ps1 -Force
Write-Output "--- verify"
"C:\lab\velospike exists: $(Test-Path C:\lab\velospike)"
"CaptureMannSpike dir: $(Test-Path $root)"
"user PATH: " + [Environment]::GetEnvironmentVariable('Path', 'User')
Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' | Where-Object { $_.DisplayName -like 'Capture*' } | ForEach-Object { "ARP: $($_.PSChildName) $($_.DisplayVersion) $($_.InstallLocation)" }
"NSIS 0.3.1 install present (as found): $(Test-Path "$env:LOCALAPPDATA\Programs\CaptureMann\cm-desktop.exe")"
Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter 'CaptureMann*.lnk' | ForEach-Object { $_.FullName }
Get-Process | Where-Object { $_.Name -like 'velospike*' -or $_.Name -like 'Update*' -or $_.Name -like 'cm-*' } | Select-Object Id, Name | Format-Table | Out-String
(Get-PSDrive C).Free / 1GB
