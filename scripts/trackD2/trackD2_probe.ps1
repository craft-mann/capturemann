# Track D2: who is using the lab laptop, and what is in C:\wt and C:\lab.
Get-Process | Where-Object { $_.Name -match 'cargo|rustc|cm-|capturemann|link|notepad' } | Select-Object Name,Id,StartTime | Format-Table | Out-String
'--- C:\lab'
Get-ChildItem C:\lab | Select-Object Name,LastWriteTime | Format-Table | Out-String
'--- C:\wt'
Get-ChildItem C:\wt -ErrorAction SilentlyContinue | Select-Object Name,LastWriteTime | Format-Table | Out-String
'--- target'
Get-ChildItem C:\wt\target\release -Filter *.exe -ErrorAction SilentlyContinue | Select-Object Name,Length,LastWriteTime | Format-Table | Out-String
'--- displays'
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Screen]::AllScreens | ForEach-Object { "$($_.DeviceName) $($_.Bounds) primary=$($_.Primary)" }
(Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -ErrorAction SilentlyContinue).AppliedDPI
