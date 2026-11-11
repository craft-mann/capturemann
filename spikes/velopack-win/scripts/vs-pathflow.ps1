$root = "$env:LOCALAPPDATA\CaptureMannSpike"
& C:\lab\velospike\vs-reinstall.ps1 -Flags add_path | Out-Null
"PATH after install: " + [Environment]::GetEnvironmentVariable('Path', 'User')
& C:\lab\velospike\vs-upd.ps1 -Tag pathflow -WithStand 2 -Feed C:\lab\velospike\feed -WaitSec 60 2>&1 | Where-Object { $_ -match 'exited at|--- after' }
"PATH after update: " + [Environment]::GetEnvironmentVariable('Path', 'User')
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'User')
$c = Get-Command capturemann.exe -ErrorAction SilentlyContinue
"capturemann.exe resolves to: $($c.Source)"
& capturemann.exe --version 2>&1 | Select-Object -First 2
"cm-mcp.exe resolves to: $((Get-Command cm-mcp.exe -ErrorAction SilentlyContinue).Source)"
Start-Process "$root\Update.exe" -ArgumentList '--uninstall', '--silent' -Wait
Start-Sleep 3
"PATH after uninstall: " + [Environment]::GetEnvironmentVariable('Path', 'User')
Get-Content C:\lab\velospike\spike.log | Where-Object { $_ -match 'path_hook|before_uninstall' } | ForEach-Object { $_ -replace '^\d+ ', '' }
