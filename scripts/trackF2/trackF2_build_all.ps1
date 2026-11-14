# Track F2: build cm-desktop then cm-regress in C:\wt (sequential), logs in C:\lab\trackF2-build-*.log
& C:\lab\trackF_build.ps1 -Packages desktop
Copy-Item C:\lab\trackF-build.log C:\lab\trackF2-build-desktop.log -Force
& C:\lab\trackF_build.ps1 -Packages regress
Copy-Item C:\lab\trackF-build.log C:\lab\trackF2-build-regress.log -Force
"done $(Get-Date)" | Out-File C:\lab\trackF2-build-done.txt
