<#
.SYNOPSIS
  Smoke test of the product binaries on a Windows machine with an interactive desktop (the
  GitHub `windows-latest` runner): cm-daemon on a named pipe, then the CLI against it.

.DESCRIPTION
  1. starts cm-daemon on \\.\pipe\cm-smoke-<pid> with a scratch library and config folder;
  2. `capturemann targets` lists a display;
  3. `capturemann capture screenshot --display <id> --out <png>` writes a PNG whose size equals the
     display's physical size;
  4. `capturemann diagnostics` prints a report;
  5. stops the daemon (always, also on failure) and prints the daemon log when something failed.

  Nothing here records continuously or needs a camera, a microphone or a GPU. It takes one
  screenshot of the machine's own desktop into a scratch folder that is deleted at the end, so do
  not run it where the screen shows private content you do not want written to disk.

  Run from the repository root after a build:
    cargo build --locked --no-default-features -p cm-daemon -p cm-cli
    pwsh -File scripts/ci/windows-smoke.ps1
  (Windows PowerShell 5.1 also works.) Exit code 0 = pass. The endpoint is a pipe name, so this
  script is Windows only; on macOS and Linux the CLI tests in crates/cm-cli cover the same calls
  with a socket path.
#>
param(
  [string]$BinDir = 'target\debug',
  [int]$TimeoutSec = 60
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Join-Path ([IO.Path]::GetTempPath()) ('cm-smoke-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$library = Join-Path $root 'library'
$config = Join-Path $root 'config'
New-Item -ItemType Directory -Path $library, $config | Out-Null
$pipe = "\\.\pipe\cm-smoke-$PID"
$daemonOut = Join-Path $root 'daemon.out.log'
$daemonErr = Join-Path $root 'daemon.err.log'
$daemon = $null
$failed = $false

$env:CAPTUREMANN_LIBRARY = $library
$env:CAPTUREMANN_CONFIG_DIR = $config
$env:CAPTUREMANN_SOCKET = $pipe

function Step($text) { Write-Host "::group::$text"; Write-Host "== $text" }
function EndStep { Write-Host '::endgroup::' }

# Runs the CLI with a time limit; returns the exit code, stdout and stderr.
function Invoke-Cli {
  param([string[]]$CliArgs)
  $exe = Join-Path $BinDir 'capturemann.exe'
  $o = Join-Path $root ('cli-' + [Guid]::NewGuid().ToString('N') + '.out')
  $e = $o + '.err'
  # Start-Process joins the list with spaces and does not quote: an argument with a space must carry its quotes.
  $quoted = $CliArgs | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
  $p = Start-Process -FilePath $exe -ArgumentList $quoted -NoNewWindow -PassThru `
    -RedirectStandardOutput $o -RedirectStandardError $e
  # Windows PowerShell 5.1 reports a null ExitCode for a process started with -PassThru unless its
  # handle was read while it ran (PowerShell 7 does not need it).
  $null = $p.Handle
  if (-not $p.WaitForExit($TimeoutSec * 1000)) {
    try { $p.Kill() } catch {}
    throw "capturemann $($CliArgs -join ' ') did not finish within $TimeoutSec s"
  }
  $p.WaitForExit()
  [pscustomobject]@{
    Code   = $p.ExitCode
    Stdout = (Get-Content -Raw -ErrorAction SilentlyContinue $o)
    Stderr = (Get-Content -Raw -ErrorAction SilentlyContinue $e)
  }
}

function Require-Ok($r, $what) {
  if ($r.Code -ne 0) {
    throw "$what failed with exit code $($r.Code)`nstdout: $($r.Stdout)`nstderr: $($r.Stderr)"
  }
}

try {
  foreach ($b in 'cm-daemon.exe', 'capturemann.exe') {
    if (-not (Test-Path (Join-Path $BinDir $b))) {
      throw "$b not found in $BinDir (build first: cargo build --locked --no-default-features -p cm-daemon -p cm-cli)"
    }
  }

  Step "Start cm-daemon on $pipe"
  $daemon = Start-Process -FilePath (Join-Path $BinDir 'cm-daemon.exe') `
    -ArgumentList @('--library', $library, '--socket', $pipe) -NoNewWindow -PassThru `
    -RedirectStandardOutput $daemonOut -RedirectStandardError $daemonErr
  # Poll with a cheap CLI call rather than guessing a sleep. Note: the CLI may itself start a daemon
  # on a missing endpoint; the poll therefore also proves that path does not hang.
  $ready = $false
  $targets = ''
  $r = $null
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ($daemon.HasExited) { throw "cm-daemon exited early with code $($daemon.ExitCode)" }
    $r = Invoke-Cli @('--json', 'targets', '--displays-only')
    if ($r.Code -eq 0) { $ready = $true; $targets = $r.Stdout; break }
    Start-Sleep -Milliseconds 500
  }
  if (-not $ready) { throw "the daemon did not answer on $pipe within $TimeoutSec s; last stderr: $($r.Stderr)" }
  EndStep

  Step 'capturemann targets'
  Write-Host $targets
  $t = $targets | ConvertFrom-Json
  if (-not $t.displays -or @($t.displays).Count -lt 1) {
    throw 'targets lists no display: is this an interactive desktop session?'
  }
  $display = @($t.displays)[0]
  foreach ($d in @($t.displays)) { if ($d.is_primary) { $display = $d } }
  $w = [int]$display.bounds.size[0]
  $h = [int]$display.bounds.size[1]
  if ($w -le 0 -or $h -le 0) { throw "display $($display.id) has physical size ${w}x${h}" }
  Write-Host "display $($display.id) '$($display.name)' physical ${w}x${h}"
  $r = Invoke-Cli @('targets')
  Require-Ok $r 'capturemann targets (with windows)'
  Write-Host $r.Stdout
  EndStep

  Step 'capturemann capture screenshot'
  $png = Join-Path $root 'shot.png'
  $r = Invoke-Cli @('--json', 'capture', 'screenshot', '--display', "$($display.id)", '--out', $png, '--title', 'windows smoke')
  Require-Ok $r 'capturemann capture screenshot'
  Write-Host $r.Stdout
  if (-not (Test-Path $png)) { throw "no PNG written to $png" }
  $bytes = [IO.File]::ReadAllBytes($png)
  $sig = [byte[]](0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
  if ($bytes.Length -lt 24) { throw "$png is too short to be a PNG ($($bytes.Length) bytes)" }
  for ($i = 0; $i -lt 8; $i++) {
    if ($bytes[$i] -ne $sig[$i]) { throw "$png is not a PNG ($($bytes.Length) bytes)" }
  }
  # IHDR: width and height are big-endian u32 at offsets 16 and 20.
  $pw = ([int]$bytes[16] -shl 24) -bor ([int]$bytes[17] -shl 16) -bor ([int]$bytes[18] -shl 8) -bor [int]$bytes[19]
  $ph = ([int]$bytes[20] -shl 24) -bor ([int]$bytes[21] -shl 16) -bor ([int]$bytes[22] -shl 8) -bor [int]$bytes[23]
  Write-Host "PNG ${pw}x${ph}, $($bytes.Length) bytes"
  if ($pw -ne $w -or $ph -ne $h) { throw "PNG is ${pw}x${ph} but the display is ${w}x${h} physical" }
  EndStep

  Step 'capturemann diagnostics'
  $r = Invoke-Cli @('diagnostics')
  Require-Ok $r 'capturemann diagnostics'
  Write-Host $r.Stdout
  if ($r.Stdout -notmatch '(?i)windows') { throw 'the diagnostics report does not mention Windows' }
  EndStep

  Write-Host 'windows-smoke: PASS'
}
catch {
  $failed = $true
  Write-Host "::error::windows-smoke: $($_.Exception.Message)"
  Write-Host $_.ScriptStackTrace
}
finally {
  if ($daemon -and -not $daemon.HasExited) {
    # No console to send Ctrl+C to; the smoke daemon holds nothing that needs a clean exit.
    try { $daemon.Kill() } catch {}
    [void]$daemon.WaitForExit(10000)
  }
  if ($failed) {
    foreach ($f in @($daemonOut, $daemonErr)) {
      Write-Host "---- $f"
      if (Test-Path $f) { Get-Content $f -Tail 80 }
    }
    Get-ChildItem -Recurse -Filter '*.log' $config -ErrorAction SilentlyContinue | ForEach-Object {
      Write-Host "---- $($_.FullName)"
      Get-Content $_.FullName -Tail 80
    }
  }
  Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
if ($failed) { exit 1 }
