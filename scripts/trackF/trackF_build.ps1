# One-off cm-desktop build on the lab laptop (Track F, Nov 14, 2026). Writes C:\lab\trackF-build.log.
# Source: C:\wt\main (unpacked from the tarball). Output: C:\wt\target\release\cm-desktop.exe (GUI subsystem).
param([string]$Packages = 'desktop')
$ErrorActionPreference = 'Continue'
$log = 'C:\lab\trackF-build.log'
"start $(Get-Date)" | Out-File $log
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;" + $env:Path
$env:CARGO_TARGET_DIR = 'C:\wt\target'
$env:CARGO_PROFILE_RELEASE_LTO = 'false'
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '16'
$env:CARGO_PROFILE_RELEASE_DEBUG = '0'
$env:CARGO_PROFILE_RELEASE_INCREMENTAL = 'false'
$env:RUSTFLAGS = '-C target-feature=+crt-static'
Set-Location C:\wt\main
cargo --version 2>&1 | Out-File $log -Append
if ($Packages -eq 'desktop') {
  cargo rustc --release --locked --no-default-features -j 3 -p cm-desktop --bin cm-desktop -- -C link-arg=/SUBSYSTEM:WINDOWS -C link-arg=/ENTRY:mainCRTStartup 2>&1 | Out-File $log -Append
} elseif ($Packages -eq 'regress') {
  cargo build --release --locked -j 3 -p cm-regress 2>&1 | Out-File $log -Append
}
"exit $LASTEXITCODE $(Get-Date)" | Out-File $log -Append
