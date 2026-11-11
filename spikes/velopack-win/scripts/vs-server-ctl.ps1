param([string]$Action = 'start', [string]$Root = 'C:\lab\velospike\feed', [int]$Port = 8099)
New-Item -ItemType Directory -Force C:\lab\velospike\flags | Out-Null
if ($Action -eq 'start') {
  Remove-Item C:\lab\velospike\flags\stop_server -ErrorAction SilentlyContinue
  Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.CommandLine -like '*vs-serve.ps1*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
  Start-Sleep 1
  Start-Process -FilePath powershell.exe -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','C:\lab\velospike\vs-serve.ps1','-Root',$Root,'-Port',$Port
  Start-Sleep 2
  (Invoke-WebRequest "http://127.0.0.1:$Port/releases.win.json" -UseBasicParsing).StatusCode
  Get-NetTCPConnection -LocalPort $Port -State Listen | Select-Object LocalAddress, LocalPort, OwningProcess | Format-Table | Out-String
} else {
  New-Item C:\lab\velospike\flags\stop_server -ItemType File -Force | Out-Null
  Start-Sleep 2
  Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.CommandLine -like '*vs-serve.ps1*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
  'server stopped'
}
