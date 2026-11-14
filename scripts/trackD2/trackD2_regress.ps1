# Runs the guide cases of cm-regress in the desktop session (hardware tier includes guide.real_clicks).
$out = 'C:\lab\trackD2-regress-out'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
& C:\wt_d2\target\debug\cm-regress.exe --tier hardware --only guide.edit_export,guide.refusals,guide.real_clicks --allow screen --package-dir C:\wt_d2\target\debug --out $out 2>&1 | Out-String
Get-ChildItem $out -Filter *.md -Recurse | Select-Object -First 1 | ForEach-Object { Get-Content $_.FullName | Select-Object -First 70 }
