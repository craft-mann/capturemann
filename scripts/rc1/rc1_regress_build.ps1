# Builds cm-regress from the rc1 source tree and puts it where regress.sh expects it (C:\lab\tests\cm-regress.exe).
# Log: C:\lab\rc1-regress-build.log
$ErrorActionPreference = 'Continue'
$log = 'C:\lab\rc1-regress-build.log'
"start $(Get-Date)" | Out-File $log
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;" + $env:Path
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:CARGO_TARGET_DIR = 'C:\wt-rc1\target'
Set-Location C:\wt-rc1\main
cargo build --release --locked -j 3 -p cm-regress 2>&1 | Out-File $log -Append
New-Item -ItemType Directory C:\lab\tests -Force | Out-Null
Copy-Item C:\wt-rc1\target\release\cm-regress.exe C:\lab\tests\cm-regress.exe -Force
"exit $LASTEXITCODE $(Get-Date)" | Out-File $log -Append
