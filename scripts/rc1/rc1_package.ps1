# Second step of the rc1 build: the binaries are built (rc1_build.ps1); fetch-models.ps1 failed on the lab laptop
# (Windows PowerShell 5.1), so the two pinned speaker models and their licence files are copied from the installed 0.3.3
# and the package is made with CM_SKIP_BUILD=1. Log: C:\lab\rc1-pack.log
$ErrorActionPreference = 'Continue'
$log = 'C:\lab\rc1-pack.log'
"start $(Get-Date)" | Out-File $log
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;C:\Program Files (x86)\NSIS;$env:LOCALAPPDATA\Programs\Python\Python312;" + $env:Path
$env:CARGO_TARGET_DIR = 'C:\wt-rc1\target'
Set-Location C:\wt-rc1\main
$inst = "$env:LOCALAPPDATA\Programs\CaptureMann"
$m = 'C:\wt-rc1\main\packaging\sherpa\out\models'
Remove-Item -Recurse -Force $m -ErrorAction SilentlyContinue
Copy-Item -Recurse "$inst\Resources\Models\speaker" $m
$b = 'C:\wt-rc1\bundles'
$env:CM_FFMPEG_BUNDLE = "$b\ffmpeg"; $env:CM_LLAMA_BUNDLE = "$b\llama"
$env:CM_REQUIRE_FFMPEG = '1'; $env:CM_REQUIRE_LLAMA = '1'; $env:CM_REQUIRE_SHERPA = '1'; $env:CM_REQUIRE_SPEAKER_MODELS = '1'
$env:CM_WINDOWS_OUT = 'C:\wt-rc1\out'; $env:CM_CARGO_JOBS = '3'; $env:CM_SKIP_BUILD = '1'
& powershell -NoProfile -ExecutionPolicy Bypass -File packaging\windows\build-windows.ps1 2>&1 | Out-File $log -Append
"exit $LASTEXITCODE $(Get-Date)" | Out-File $log -Append
