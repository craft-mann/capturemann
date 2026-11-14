# Runs the cm-regress case(s) in the desktop session against the staged package folder.
param([string]$Only = 'desktop.hud_drag', [string]$Inst = 'C:\lab\trackF-inst', [string]$Tier = 'full')
$out = 'C:\lab\trackF-regress-out'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
& C:\wt\target\release\cm-regress.exe --tier $Tier --only $Only --allow screen --package-dir $Inst --out $out 2>&1 | Out-String
Get-ChildItem $out -Recurse -File | ForEach-Object { $_.FullName }
Get-ChildItem $out -Filter *.md -Recurse | Select-Object -First 1 | ForEach-Object { Get-Content $_.FullName | Select-Object -First 60 }
