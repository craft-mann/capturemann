$root = "$env:LOCALAPPDATA\CaptureMannSpike"
foreach ($flags in 'add_path', 'add_path,abs_powershell') {
  Write-Output "######## flags: $flags"
  & C:\lab\velospike\vs-reinstall.ps1 -Flags $flags -Setup C:\lab\velospike\Setup-mini.exe | Where-Object { $_ -match 'path_hook|install exit|user PATH' } | ForEach-Object { $_ -replace '^\d+ ', '' }
  Start-Process "$root\Update.exe" -ArgumentList '--uninstall', '--silent' -Wait
  Start-Sleep 3
  Get-Content C:\lab\velospike\spike.log | Where-Object { $_ -match 'path_hook|before_uninstall' } | ForEach-Object { $_ -replace '^\d+ ', '' } | Select-Object -Last 3
  "PATH after uninstall: " + [Environment]::GetEnvironmentVariable('Path', 'User')
}
