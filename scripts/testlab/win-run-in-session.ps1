<#
.SYNOPSIS
  Runs a program in the LOGGED-IN user's desktop session and returns its output (the lab laptop, from ssh).

.DESCRIPTION
  A command started over ssh runs in session 0, which has no desktop: screen capture, window lists, the camera
  and the microphone do not work there. This script registers a one-shot scheduled task that runs as the
  interactive user (the one logged in on the console), starts it, waits for it to finish, prints what the program
  wrote and removes the task.

  From the Mac:
    scripts/testlab/lab.sh push windows scripts/testlab/win-run-in-session.ps1
    scripts/testlab/lab.sh windows 'C:\lab\win-run-in-session.ps1 -Exe C:\lab\tests\cm-capture__real_display.exe -Arguments "--ignored --nocapture --test-threads=1"'

  Output (stdout and stderr of the program) goes to -OutFile and is printed at the end; the last line is
  `EXIT <code>`. The script's own exit code is the program's, 124 on a timeout (the program is stopped), 2 when
  nobody is logged in.

  -EnvVars sets environment variables for the program: 'CM_OCR_REAL=1;CAPTUREMANN_LIBRARY=C:\lab\lib'.
  -WorkDir defaults to the program's folder.

  Needs: the interactive user is logged in (the laptop auto-logs in and never locks, see windows-setup.ps1) and
  is the same account as the ssh user. Not run on the laptop yet (written Nov 4, 2026): read its first output.
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$Arguments = '',
  [string]$OutFile = '',
  [int]$TimeoutSec = 600,
  [string]$EnvVars = '',
  [string]$WorkDir = ''
)
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Exe)) { Write-Error "not found: $Exe"; exit 3 }
$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (-not $WorkDir) { $WorkDir = Split-Path -Parent $Exe }
$id = [guid]::NewGuid().ToString('N').Substring(0, 8)
$labDir = 'C:\lab\run'
New-Item -ItemType Directory -Force -Path $labDir | Out-Null
if (-not $OutFile) { $OutFile = Join-Path $labDir "session-$id.txt" }
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null
Remove-Item -LiteralPath $OutFile -ErrorAction SilentlyContinue

# The user on the console (DOMAIN\name or COMPUTER\name); empty when nobody is logged in.
$user = (Get-CimInstance Win32_ComputerSystem).UserName
if (-not $user) { Write-Error 'nobody is logged in on the console'; exit 2 }

function Quote([string]$s) { "'" + $s.Replace("'", "''") + "'" }

# The program runs inside a small wrapper script so that quoting survives the task action. The wrapper writes
# the output file in UTF-8 and ends it with the exit code, which is how this script knows the program is done.
$wrapper = Join-Path $labDir "session-$id.ps1"
$lines = @(
  '$ErrorActionPreference = ''Continue''',
  "Set-Location -LiteralPath $(Quote $WorkDir)"
)
foreach ($pair in ($EnvVars -split ';' | Where-Object { $_ -match '=' })) {
  $k, $v = $pair -split '=', 2
  $lines += "`$env:$($k.Trim()) = $(Quote $v)"
}
$lines += @(
  "`$out = $(Quote $OutFile)",
  "`$cmd = $(Quote $Exe)",
  "`$argText = $(Quote $Arguments)",
  # cmd.exe does the redirection so that the program's stderr is interleaved with stdout, as in a terminal.
  '$line = "`"$cmd`" $argText > `"$out.tmp`" 2>&1"',
  '& cmd.exe /d /c $line',
  '$code = $LASTEXITCODE',
  'Move-Item -LiteralPath "$out.tmp" -Destination $out -Force',
  'Add-Content -LiteralPath $out -Value "EXIT $code" -Encoding utf8'
)
Set-Content -LiteralPath $wrapper -Value $lines -Encoding utf8

$taskName = "cm-lab-run-$id"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
  -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$wrapper`""
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
  -ExecutionTimeLimit ([TimeSpan]::FromSeconds($TimeoutSec + 60)) -StartWhenAvailable
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null

$exit = 124
try {
  Start-ScheduledTask -TaskName $taskName
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if (Test-Path -LiteralPath $OutFile) { $exit = $null; break }
    Start-Sleep -Milliseconds 500
  }
  if ($null -eq $exit) {
    $text = Get-Content -LiteralPath $OutFile -Raw -ErrorAction SilentlyContinue
    if ($text -match 'EXIT (-?\d+)\s*$') { $exit = [int]$Matches[1] } else { $exit = 1 }
  } else {
    # Timed out: stop the task and the program it started, keep what was written so far.
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($Exe)) -ErrorAction SilentlyContinue |
      Stop-Process -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath "$OutFile.tmp") {
      Move-Item -LiteralPath "$OutFile.tmp" -Destination $OutFile -Force
      Add-Content -LiteralPath $OutFile -Value 'EXIT 124 (timed out)' -Encoding utf8
    }
  }
} finally {
  Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $wrapper -ErrorAction SilentlyContinue
}

if (Test-Path -LiteralPath $OutFile) { Get-Content -LiteralPath $OutFile } else { Write-Output "(no output; timed out after $TimeoutSec s)" }
exit $exit
