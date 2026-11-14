Remove-Item -Recurse -Force C:\wt_d2 -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force C:\lab\trackD2-regress-out,C:\lab\trackD2_src.tgz -ErrorAction SilentlyContinue
Test-Path C:\wt_d2
