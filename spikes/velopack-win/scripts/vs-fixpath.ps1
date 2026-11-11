$p = "$env:LOCALAPPDATA\Programs\CaptureMann\path.ps1"
powershell -NoProfile -ExecutionPolicy Bypass -File $p -Action remove -Dir 'C:\lab\velospike\dotnethome\.dotnet\tools'
[Environment]::GetEnvironmentVariable('Path','User')
