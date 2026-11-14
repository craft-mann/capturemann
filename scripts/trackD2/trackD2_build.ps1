# Track D2: unpack the branch source into C:\wt_d2\main and build the CLI-side binaries (debug, own target dir)
# for cm-regress. Runs detached; progress in C:\lab\trackD2-build.log. Own folders: never touches C:\wt.
$ErrorActionPreference = 'Continue'
$log = 'C:\lab\trackD2-build.log'
"start $(Get-Date)" | Out-File $log
Remove-Item -Recurse -Force C:\wt_d2\main -ErrorAction SilentlyContinue
New-Item -ItemType Directory C:\wt_d2\main -Force | Out-Null
tar -xzf C:\lab\trackD2_src.tgz -C C:\wt_d2\main
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;" + $env:Path
$env:CARGO_TARGET_DIR = 'C:\wt_d2\target'
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_INCREMENTAL = '0'
Set-Location C:\wt_d2\main
cargo --version 2>&1 | Out-File $log -Append
cargo build --locked --no-default-features -j 3 -p cm-daemon -p cm-cli -p cm-mcp -p cm-regress 2>&1 | Out-File $log -Append
"exit $LASTEXITCODE $(Get-Date)" | Out-File $log -Append
