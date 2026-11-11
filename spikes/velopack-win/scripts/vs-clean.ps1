Get-Process Update, velospike, vcli -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$env:LOCALAPPDATA\CaptureMannSpike*" } | Stop-Process -Force
Start-Sleep 1
Get-Process powershell -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $PID } | ForEach-Object { $c = (Get-CimInstance Win32_Process -Filter "ProcessId=$($_.Id)").CommandLine; if ($c -like '*vs-*') { Stop-Process -Id $_.Id -Force } }
$root = "$env:LOCALAPPDATA\CaptureMannSpike"
Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CaptureMannSpike' -ErrorAction SilentlyContinue
Remove-Item "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\CaptureMann.lnk" -ErrorAction SilentlyContinue
"root exists: $(Test-Path $root)"
