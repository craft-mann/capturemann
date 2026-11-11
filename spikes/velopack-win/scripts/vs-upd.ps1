param([string]$Url = 'http://127.0.0.1:8099', [string]$Cmd = 'update', [int]$WithStand = 1, [int]$WaitSec = 90, [string]$Tag = 'run', [string]$Feed = 'C:\lab\velospike\feedA', [int]$NoServer = 0)
$ErrorActionPreference = 'Continue'
$root = "$env:LOCALAPPDATA\CaptureMannSpike"
$cur = "$root\current"
Remove-Item C:\lab\velospike\spike.log -ErrorAction SilentlyContinue
Remove-Item C:\lab\velospike\serve.log -ErrorAction SilentlyContinue
Get-ChildItem "$env:LOCALAPPDATA\velopack" -Filter *.log -ErrorAction SilentlyContinue | ForEach-Object { Remove-Item $_.FullName -ErrorAction SilentlyContinue }
Write-Output "=== [$Tag] before: $(Get-Content $cur\sq.version | Select-String '<version>')"
$stand = New-Object System.Collections.ArrayList
if ($WithStand -ge 1) {
  foreach ($n in 'cm-daemon.exe', 'cm-overlay.exe', 'cm-mcp.exe') {
    Copy-Item "$cur\vcli.exe" "$cur\$n" -Force
    [void]$stand.Add((Start-Process "$cur\$n" -ArgumentList 'sleep', '600' -PassThru -WindowStyle Hidden))
  }
  if ($WithStand -eq 1) { $cmdp = Start-Process cmd.exe -ArgumentList '/c', 'timeout /t 600 >nul' -WorkingDirectory $cur -PassThru -WindowStyle Hidden; [void]$stand.Add($cmdp) }
  Start-Sleep 1
  Write-Output ("stand-ins: " + (($stand | ForEach-Object { "$($_.Id)" }) -join ','))
}
if (-not $NoServer) {
  Remove-Item C:\lab\velospike\flags\stop_server -ErrorAction SilentlyContinue
  $srv = Start-Process powershell.exe -WindowStyle Hidden -PassThru -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','C:\lab\velospike\vs-serve.ps1','-Root',$Feed,'-Port','8099'
  Start-Sleep 2
}
$t0 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$app = Start-Process "$cur\velospike.exe" -ArgumentList $Cmd, $Url -PassThru -WindowStyle Hidden
Write-Output "app pid $($app.Id) started"
$seenNew = $false; $oldGoneAt = $null; $deadline = (Get-Date).AddSeconds($WaitSec)
$timeline = @()
while ((Get-Date) -lt $deadline) {
  $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
  $alive = @($stand | Where-Object { -not $_.HasExited } | ForEach-Object { $_.Id })
  $appAlive = -not $app.HasExited
  $upd = @(Get-Process -Name Update -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$root*" }).Count
  $vel = @(Get-Process -Name velospike -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$root*" } | ForEach-Object { $_.Id })
  $timeline += [pscustomobject]@{ t = $now - $t0; app = $appAlive; upd = $upd; velo = ($vel -join '/'); stand = ($alive -join '/') }
  if (-not $appAlive -and -not $oldGoneAt) { $oldGoneAt = $now - $t0 }
  if ($vel.Count -gt 0 -and ($vel | Where-Object { $_ -ne $app.Id }).Count -gt 0) { $seenNew = $true }
  if ($seenNew -and $upd -eq 0) { Start-Sleep 2; break }
  Start-Sleep -Milliseconds 100
}
Write-Output "old app exited at +${oldGoneAt} ms; new instance seen: $seenNew"
Write-Output "--- timeline (changes only)"
$prev = ''
foreach ($r in $timeline) { $s = "$($r.app)|$($r.upd)|$($r.velo)|$($r.stand)"; if ($s -ne $prev) { "+$($r.t)ms app=$($r.app) Update.exe=$($r.upd) velospike=[$($r.velo)] standins_alive=[$($r.stand)]"; $prev = $s } }
Write-Output "--- spike.log (hooks, main-enter, check/download/apply lines)"
Get-Content C:\lab\velospike\spike.log | Where-Object { $_ -match 'HOOK|main-enter|check:|download:|update:|apply|info|restarted|ERR' } | ForEach-Object { if ($_ -match '^(\d+) (.*)$') { '{0}ms {1}' -f ([int64]$Matches[1] - $t0), $Matches[2] } else { $_ } }
Write-Output "--- after: $(Get-Content $cur\sq.version | Select-String '<version>')"
Write-Output "--- root listing"
Get-ChildItem $root -Force | Select-Object Name, Length | Format-Table -Auto | Out-String
Get-ChildItem "$root\packages" -Force | Select-Object Name, Length | Format-Table -Auto | Out-String
Write-Output "--- stand-in survivors"
Get-Process | Where-Object { $_.Path -like "$root*" } | Select-Object Id, Name | Format-Table -Auto | Out-String
Write-Output "--- server log"
Get-Content C:\lab\velospike\serve.log -ErrorAction SilentlyContinue | ForEach-Object { $_ -replace 'from=\S+', '' }
Write-Output "--- velopack logs"
Get-ChildItem "$env:LOCALAPPDATA\velopack", $root, "$root\packages" -Filter *.log -ErrorAction SilentlyContinue | ForEach-Object { "== $($_.FullName)"; Get-Content $_.FullName -Tail 70 }

if ($srv) { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }
Get-Process | Where-Object { $_.Path -like "$root*" -and $_.Name -in 'cm-daemon','cm-overlay','cm-mcp' } | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process cmd -ErrorAction SilentlyContinue | Where-Object { $_.Id -in ($stand | ForEach-Object { $_.Id }) } | Stop-Process -Force -ErrorAction SilentlyContinue
