# Last lines of the Track D2 build log, and what is running.
Get-ChildItem C:\lab\trackD2* | Select-Object Name,Length,LastWriteTime | Format-Table | Out-String
if (Test-Path C:\lab\trackD2-build.log) { Get-Content C:\lab\trackD2-build.log -Tail 6 }
Get-Process | Where-Object { $_.Name -match 'cargo|rustc|powershell|cmake|cl|link' } | Select-Object Name,Id,StartTime | Format-Table | Out-String
