$env:DOTNET_ROOT = 'C:\lab\velospike\dotnet'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$pd = 'C:\lab\velospike\winpack-in'
$od = 'C:\lab\velospike\winpack-out'
Remove-Item $pd, $od -Recurse -Force -ErrorAction SilentlyContinue
Copy-Item C:\lab\velospike\portable\current $pd -Recurse
Remove-Item "$pd\sq.version" -Force -ErrorAction SilentlyContinue
$sw = [Diagnostics.Stopwatch]::StartNew()
& C:\lab\velospike\tools\vpk.exe pack -u CaptureMannSpike -v 0.3.2-spike1 -p $pd -e velospike.exe -o $od -r win-x64 --packTitle CaptureMann --packAuthors 'CaptureMann contributors' -i "$pd\app.ico" --shortcuts StartMenuRoot --msi 2>&1 | ForEach-Object { $_.ToString() } | Where-Object { $_ -notmatch 'Starting:|Complete:' } | ForEach-Object { if ($_.Length -gt 220) { $_.Substring(0, 220) } else { $_ } } | Select-Object -Last 25
"pack seconds: $([math]::Round($sw.Elapsed.TotalSeconds,1))"
Get-ChildItem $od | ForEach-Object { "{0,-50} {1,14:N0}" -f $_.Name, $_.Length }
Write-Output "--- version info of produced files"
Get-Item "$od\*.exe", "$od\*.msi" | ForEach-Object { "$($_.Name): desc='$($_.VersionInfo.FileDescription)' product='$($_.VersionInfo.ProductName)' company='$($_.VersionInfo.CompanyName)' ver=$($_.VersionInfo.FileVersion)" }
Write-Output "--- user PATH (must be unchanged)"
[Environment]::GetEnvironmentVariable('Path', 'User')
