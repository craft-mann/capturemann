param([string]$Flags = '', [string]$Setup = 'C:\lab\velospike\Setup-spike1.exe')
$ErrorActionPreference = 'Continue'
$root = "$env:LOCALAPPDATA\CaptureMannSpike"
if (Test-Path "$root\Update.exe") { Start-Process "$root\Update.exe" -ArgumentList '--uninstall', '--silent' -Wait; Start-Sleep 4 }
Remove-Item C:\lab\velospike\flags\* -Force -ErrorAction SilentlyContinue
foreach ($f in ($Flags -split ',' | Where-Object { $_ })) { New-Item "C:\lab\velospike\flags\$f" -ItemType File -Force | Out-Null }
Remove-Item C:\lab\velospike\spike.log -ErrorAction SilentlyContinue
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Setup -ArgumentList '--silent' -Wait -PassThru
$sw.Stop()
"install exit $($p.ExitCode) in $([math]::Round($sw.Elapsed.TotalSeconds,1)) s; flags: $((Get-ChildItem C:\lab\velospike\flags | ForEach-Object Name) -join ',')"
Start-Sleep 2
(Get-Content "$root\current\sq.version" | Select-String '<version>').ToString()
Get-Content C:\lab\velospike\spike.log
"user PATH: " + [Environment]::GetEnvironmentVariable('Path', 'User')
