$ErrorActionPreference = 'Continue'
$root = "$env:LOCALAPPDATA\CaptureMannSpike"
$cur = "$root\current"
Remove-Item C:\lab\velospike\spike.log -ErrorAction SilentlyContinue
Write-Output "--- vcli info from current\ (installed)"
& "$cur\vcli.exe" info
Write-Output "--- vcli info from a plain folder (not installed)"
& "C:\lab\velospike\bin\vcli.exe" info
Write-Output "--- timing: 60 runs of --version each (ms per run, PowerShell call operator incl. process start)"
function Time-Run($exe, $n, $envname) {
  if ($envname) { Set-Item "Env:$envname" '1' }
  & $exe --version | Out-Null
  $sw = [Diagnostics.Stopwatch]::StartNew()
  for ($i = 0; $i -lt $n; $i++) { & $exe --version | Out-Null }
  $sw.Stop()
  if ($envname) { Remove-Item "Env:$envname" }
  [math]::Round($sw.Elapsed.TotalMilliseconds / $n, 1)
}
"vplain (no velopack lib, plain folder):        " + (Time-Run "C:\lab\velospike\bin\vplain.exe" 60 $null)
"vcli VS_NOVELO=1 (lib linked, startup skipped): " + (Time-Run "$cur\vcli.exe" 60 'VS_NOVELO')
"vcli with VelopackApp::run, installed:          " + (Time-Run "$cur\vcli.exe" 60 $null)
"vcli with VelopackApp::run, NOT installed:      " + (Time-Run "C:\lab\velospike\bin\vcli.exe" 60 $null)
"capturemann.exe --version (real 0.3.1, no velopack) in current: " + (Time-Run "$cur\capturemann.exe" 30 $null)
Write-Output "--- start the STUB (CaptureMann.exe) with 'sleep 12', list processes after 3s"
$t0 = Get-Date
$p = Start-Process -FilePath "$root\CaptureMann.exe" -ArgumentList 'sleep','12' -PassThru
Start-Sleep -Seconds 3
$all = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like "$root*" }
$all | Select-Object ProcessId, ParentProcessId, Name, CommandLine | Format-Table -Auto -Wrap | Out-String
"stub pid was $($p.Id), HasExited=$($p.HasExited)"
Start-Sleep -Seconds 11
Write-Output "--- after exit, anything left?"
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like "$root*" } | Select-Object ProcessId, Name | Format-Table -Auto | Out-String
Write-Output "--- spike.log"
Get-Content C:\lab\velospike\spike.log
