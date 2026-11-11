$ErrorActionPreference = 'SilentlyContinue'
Write-Output "--- uninstall entries (HKCU + HKLM) matching Capture*"
foreach ($h in 'HKCU:','HKLM:') {
  foreach ($k in 'Software\Microsoft\Windows\CurrentVersion\Uninstall','Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall') {
    Get-ItemProperty "$h\$k\*" | Where-Object { $_.DisplayName -like 'Capture*' -or $_.PSChildName -like 'Capture*' } | ForEach-Object {
      "[$h\$k\$($_.PSChildName)]"
      $_ | Select-Object DisplayName,DisplayVersion,Publisher,InstallLocation,UninstallString,QuietUninstallString,EstimatedSize,DisplayIcon,NoModify,NoRepair | Format-List | Out-String
    }
  }
}
Write-Output "--- local app folders"
Get-ChildItem $env:LOCALAPPDATA -Directory | Where-Object { $_.Name -like 'Capture*' } | ForEach-Object { $_.FullName }
Get-ChildItem "$env:LOCALAPPDATA\Programs" -Directory | ForEach-Object { 'Programs\' + $_.Name }
Write-Output "--- shortcuts"
Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter *.lnk | Where-Object { $_.FullName -like '*Capture*' } | ForEach-Object { $_.FullName }
Get-ChildItem ([Environment]::GetFolderPath('Desktop')) -Filter *.lnk | Where-Object { $_.Name -like 'Capture*' } | ForEach-Object { 'Desktop: ' + $_.FullName }
Write-Output "--- user PATH"
[Environment]::GetEnvironmentVariable('Path','User')
Write-Output "--- HKCU Run"
(Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run').PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' } | ForEach-Object { "$($_.Name)=$($_.Value)" }
Write-Output "--- processes"
Get-Process | Where-Object { $_.Path -like "$env:LOCALAPPDATA\Capture*" -or $_.Path -like "$env:LOCALAPPDATA\Programs\Capture*" -or $_.Name -like 'Update*' -or $_.Name -like 'velospike*' -or $_.Name -like 'cm-*' } | Select-Object Id,Name,Path | Format-Table -Auto | Out-String
