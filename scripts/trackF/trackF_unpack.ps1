Remove-Item -Recurse -Force C:\wt\main -ErrorAction SilentlyContinue
New-Item -ItemType Directory C:\wt\main -Force | Out-Null
tar -xzf C:\lab\trackF_src.tgz -C C:\wt\main
(Get-ChildItem C:\wt\main -Recurse -File | Measure-Object).Count
Test-Path C:\wt\main\Cargo.lock
