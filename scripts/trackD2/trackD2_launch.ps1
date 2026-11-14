# Starts trackD2_build.ps1 detached so the ssh call returns.
Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','C:\lab\trackD2_build.ps1' -WindowStyle Hidden
'started'
