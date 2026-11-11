$ErrorActionPreference = 'Continue'
Set-Location C:\lab\velospike
Remove-Item C:\lab\velospike\spike.log -ErrorAction SilentlyContinue
Write-Output "--- Setup --help"
$p = Start-Process -FilePath .\Setup-spike1.exe -ArgumentList '--help' -Wait -PassThru -RedirectStandardOutput help.out -RedirectStandardError help.err
"exit $($p.ExitCode)"; Get-Content help.out, help.err | Select-Object -First 30
Write-Output "--- MOTW and signature of Setup"
Get-Item .\Setup-spike1.exe -Stream * | Select-Object Stream, Length | Format-Table | Out-String
(Get-AuthenticodeSignature .\Setup-spike1.exe).Status
Write-Output "--- silent install"
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath .\Setup-spike1.exe -ArgumentList '--silent' -Wait -PassThru -RedirectStandardOutput inst.out -RedirectStandardError inst.err
$sw.Stop()
"exit $($p.ExitCode) seconds $([math]::Round($sw.Elapsed.TotalSeconds,1))"
Get-Content inst.out, inst.err | Select-Object -First 30
Start-Sleep 3
Write-Output "--- spike.log"
Get-Content C:\lab\velospike\spike.log
Write-Output "--- tree"
Get-ChildItem "$env:LOCALAPPDATA\CaptureMannSpike" | Select-Object Name, Length, LastWriteTime | Format-Table | Out-String
Get-ChildItem "$env:LOCALAPPDATA\CaptureMannSpike\current" | Select-Object Name, Length | Format-Table -Auto | Out-String
$sz = (Get-ChildItem "$env:LOCALAPPDATA\CaptureMannSpike" -Recurse -File | Measure-Object Length -Sum).Sum
"installed bytes: $sz"
Get-ChildItem "$env:LOCALAPPDATA\CaptureMannSpike\packages" | Select-Object Name, Length | Format-Table | Out-String
Write-Output "--- sq.version"
Get-Content "$env:LOCALAPPDATA\CaptureMannSpike\current\sq.version"
Write-Output "--- processes after install"
Get-Process | Where-Object { $_.Path -like "$env:LOCALAPPDATA\CaptureMannSpike*" } | Select-Object Id, Name, Path | Format-Table -Auto | Out-String
