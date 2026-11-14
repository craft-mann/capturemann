$t = Get-Content C:\lab\trackF-build.log
$i = ($t | Select-String -Pattern '^error' | Select-Object -First 1).LineNumber
if ($i) { $t[([Math]::Max(0,$i-3))..([Math]::Min($t.Count-1,$i+25))] } else { $t | Select-Object -Last 6 }
