Get-Process cm-desktop, cm-daemon, cm-overlay, cm-regress, capturemann -ErrorAction SilentlyContinue | ForEach-Object { "still running: $($_.ProcessName) $($_.Path)" }
Get-Process cm-desktop, cm-daemon, cm-overlay, cm-regress -ErrorAction SilentlyContinue | Where-Object { $_.Path -like 'C:\lab\*' -or $_.Path -like 'C:\wt\*' } | Stop-Process -Force
foreach ($p in 'C:\lab\trackF-inst', 'C:\lab\trackF-regress-out', 'C:\lab\trackF-dev', 'C:\lab\trackF_src.tgz') { Remove-Item -Recurse -Force $p -ErrorAction SilentlyContinue }
Get-ChildItem C:\lab -Filter 'trackF-run-*' -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
Get-ChildItem C:\lab -Filter 'trackF_*.ps1' -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'trackF_cleanup.ps1' } | Remove-Item -Force
Get-ChildItem C:\lab -Filter 'trackF-*' -ErrorAction SilentlyContinue | ForEach-Object { "left: $($_.Name)" }
Get-ChildItem 'C:\Users\trung\Documents', "$env:USERPROFILE\Videos" -Recurse -Filter '*.cmrec' -ErrorAction SilentlyContinue | ForEach-Object { "recording folder: $($_.FullName)" }
"C:\wt size GB: " + [math]::Round(((Get-ChildItem C:\wt -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1GB, 1)
