# Track C3 (Nov 14, 2026): unpack the worktree tarball on the lab laptop and check or build it.
# Usage: trackC3_check.ps1 check|overlay
# Source: C:\wt3\main (from C:\lab\trackC3_src.tgz). Target dir shared with the earlier builds: C:\wt\target.
param([string]$What = 'check')
$ErrorActionPreference = 'Continue'
$log = 'C:\lab\trackC3-build.log'
"start $What $(Get-Date)" | Out-File $log
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;" + $env:Path
$env:CARGO_TARGET_DIR = 'C:\wt\target'
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:CARGO_PROFILE_RELEASE_LTO = 'false'
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '16'
$env:CARGO_PROFILE_RELEASE_DEBUG = '0'
$env:CARGO_PROFILE_RELEASE_INCREMENTAL = 'false'
$env:RUSTFLAGS = '-C target-feature=+crt-static'
if (Test-Path C:\wt3\main) { Remove-Item -Recurse -Force C:\wt3\main -ErrorAction SilentlyContinue }
New-Item -ItemType Directory C:\wt3\main -Force | Out-Null
tar -xzf C:\lab\trackC3_src.tgz -C C:\wt3\main
Set-Location C:\wt3\main
if ($What -eq 'check') {
  cargo check --locked --no-default-features -j 3 -p cm-desktop --lib 2>&1 | Out-File $log -Append
} elseif ($What -eq 'overlay') {
  cargo build --release --locked --no-default-features -j 3 -p cm-overlay -p cm-daemon 2>&1 | Out-File $log -Append
}
"exit $LASTEXITCODE $(Get-Date)" | Out-File $log -Append
