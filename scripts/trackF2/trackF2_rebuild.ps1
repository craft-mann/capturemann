# Track F2: unpack the patch over C:\wt\main, rebuild cm-desktop (incremental), stage it. Marker: C:\lab\trackF2-build-done.txt
Remove-Item C:\lab\trackF2-build-done.txt -ErrorAction SilentlyContinue
tar -xzf C:\lab\trackF2_patch.tgz -C C:\wt\main
& C:\lab\trackF_build.ps1 -Packages desktop
Copy-Item C:\lab\trackF-build.log C:\lab\trackF2-build-desktop2.log -Force
& C:\lab\trackF2_stage.ps1 | Out-File C:\lab\trackF2-stage2.log
"done $(Get-Date)" | Out-File C:\lab\trackF2-build-done.txt
