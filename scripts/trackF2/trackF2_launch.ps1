# Starts a script outside the ssh job object (WMI), so closing the ssh session does not kill it:
#   trackF2_launch.ps1 -Script <script.ps1>
param([string]$Script)
Remove-Item C:\lab\trackF2-build-done.txt -ErrorAction SilentlyContinue
$cmd = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $Script"
$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $cmd; CurrentDirectory = 'C:\lab' }
"started pid $($r.ProcessId) rc $($r.ReturnValue)"
