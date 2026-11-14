$env:CAPTUREMANN_LIBRARY='C:\lab\trackF-dev\lib'; $env:CAPTUREMANN_CONFIG_DIR='C:\lab\trackF-dev\cfg'; $env:CAPTUREMANN_SOCKET='\\.\pipe\cm-trackf-dev'
New-Item -ItemType Directory 'C:\lab\trackF-dev\cfg' -Force | Out-Null
(& 'C:\lab\trackF-inst\capturemann.exe' devices 2>&1) | Out-String
Get-Process cm-daemon -ErrorAction SilentlyContinue | Where-Object { $_.Path -like 'C:\lab\trackF-inst*' } | Stop-Process -Force
Remove-Item -Recurse -Force 'C:\lab\trackF-dev' -ErrorAction SilentlyContinue
