$root = "$env:LOCALAPPDATA\CaptureMannSpike"
& C:\lab\velospike\vs-reinstall.ps1 | Out-Null
Remove-Item C:\lab\velospike\spike.log -ErrorAction SilentlyContinue
Write-Output "### unreachable: nothing listens on 127.0.0.1:8099"
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process "$root\current\vcli.exe" -ArgumentList 'check', 'http://127.0.0.1:8099' -PassThru -Wait -WindowStyle Hidden
"exit $($p.ExitCode) after $($sw.ElapsedMilliseconds) ms"
Write-Output "### unresolvable host"
$sw.Restart()
$p = Start-Process "$root\current\vcli.exe" -ArgumentList 'check', 'http://updates.no-such-host.invalid/' -PassThru -Wait -WindowStyle Hidden
"exit $($p.ExitCode) after $($sw.ElapsedMilliseconds) ms"
Write-Output "### server accepts the connection and never answers (loopback TcpListener on 8098), watch 45 s"
$l = New-Object System.Net.Sockets.TcpListener([Net.IPAddress]::Loopback, 8098)
$l.Start()
$p = Start-Process "$root\current\vcli.exe" -ArgumentList 'check', 'http://127.0.0.1:8098' -PassThru -WindowStyle Hidden
$c = $l.AcceptTcpClient()
Start-Sleep 45
"client process still running after 45 s: $(-not $p.HasExited)"
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
$c.Close(); $l.Stop()
Get-Content C:\lab\velospike\spike.log | ForEach-Object { if ($_ -match '^(\d+) (.*)$') { $Matches[2] } }
