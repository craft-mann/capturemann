$ErrorActionPreference = 'Continue'
$root = "$env:LOCALAPPDATA\CaptureMannSpike"
New-Item -ItemType Directory -Force "$env:APPDATA\CaptureMannSpike" | Out-Null
'{"fake":"settings"}' | Set-Content "$env:APPDATA\CaptureMannSpike\settings.json"
Remove-Item C:\lab\velospike\spike.log -ErrorAction SilentlyContinue
Write-Output "--- registry uninstall string"
$k = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CaptureMannSpike'
$k.QuietUninstallString
Write-Output "--- run quiet uninstall (silent)"
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath "$root\Update.exe" -ArgumentList '--uninstall', '--silent' -Wait -PassThru
$sw.Stop()
"exit $($p.ExitCode) seconds $([math]::Round($sw.Elapsed.TotalSeconds,1))"
Start-Sleep 3
Write-Output "--- after"
"root exists: $(Test-Path $root)"
if (Test-Path $root) { Get-ChildItem $root -Force -Recurse | Select-Object -First 10 FullName | Format-Table -Auto | Out-String }
"settings survive: $(Test-Path "$env:APPDATA\CaptureMannSpike\settings.json")"
"uninstall key exists: $(Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CaptureMannSpike')"
"start menu lnk exists: $(Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\CaptureMann.lnk")"
"NSIS install still there: $(Test-Path "$env:LOCALAPPDATA\Programs\CaptureMann\cm-desktop.exe")"
Get-Content C:\lab\velospike\spike.log -ErrorAction SilentlyContinue
Get-Content "$env:LOCALAPPDATA\velopack\velopack_CaptureMannSpike.log" -Tail 12 -ErrorAction SilentlyContinue
