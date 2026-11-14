# Starts a script fully detached from the ssh session (WMI process create): rc1_start.ps1 <script path>
param([string]$Script)
$cmd = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $Script"
$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $cmd }
"created pid $($r.ProcessId) rc $($r.ReturnValue)"
