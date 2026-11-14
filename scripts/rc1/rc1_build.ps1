# Release candidate build of the Windows package on the lab laptop (rc1). Log: C:\lab\rc1-build.log
# Source: C:\lab\rc1_src.tgz (archive of the HEAD commit) -> C:\wt-rc1\main; target C:\wt-rc1\target (fresh, repo release profile).
# FFmpeg, llama-server and the speaker models are taken from the installed 0.3.3 (same pins; not rebuilt here).
$ErrorActionPreference = 'Continue'
$log = 'C:\lab\rc1-build.log'
"start $(Get-Date)" | Out-File $log
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;C:\Program Files (x86)\NSIS;$env:LOCALAPPDATA\Programs\Python\Python312;" + $env:Path
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:CARGO_TARGET_DIR = 'C:\wt-rc1\target'
Remove-Item -Recurse -Force C:\wt-rc1\main -ErrorAction SilentlyContinue
New-Item -ItemType Directory C:\wt-rc1\main -Force | Out-Null
tar -xzf C:\lab\rc1_src.tgz -C C:\wt-rc1\main
Set-Location C:\wt-rc1\main
$inst = "$env:LOCALAPPDATA\Programs\CaptureMann"
$b = 'C:\wt-rc1\bundles'
Remove-Item -Recurse -Force $b -ErrorAction SilentlyContinue
New-Item -ItemType Directory "$b\ffmpeg", "$b\llama" -Force | Out-Null
Copy-Item $inst\ffmpeg.exe, $inst\ffprobe.exe, $inst\FFmpeg\BUILD-INFO.txt, $inst\FFmpeg\SOURCE-OFFER.txt "$b\ffmpeg"
Copy-Item -Recurse $inst\FFmpeg\LICENSES "$b\ffmpeg\LICENSES"
Copy-Item $inst\llama-server.exe, $inst\Llama\BUILD-INFO.txt "$b\llama"
Copy-Item -Recurse $inst\Llama\LICENSES "$b\llama\LICENSES"
"== sherpa $(Get-Date)" | Out-File $log -Append
& powershell -NoProfile -ExecutionPolicy Bypass -File packaging\sherpa\fetch-windows.ps1 -Crt mt 2>&1 | Out-File $log -Append
& powershell -NoProfile -ExecutionPolicy Bypass -File packaging\sherpa\fetch-models.ps1 2>&1 | Out-File $log -Append
$env:CM_FFMPEG_BUNDLE = "$b\ffmpeg"; $env:CM_LLAMA_BUNDLE = "$b\llama"
$env:CM_REQUIRE_FFMPEG = '1'; $env:CM_REQUIRE_LLAMA = '1'; $env:CM_REQUIRE_SHERPA = '1'; $env:CM_REQUIRE_SPEAKER_MODELS = '1'
$env:CM_WINDOWS_OUT = 'C:\wt-rc1\out'; $env:CM_CARGO_JOBS = '3'
"== build $(Get-Date)" | Out-File $log -Append
& powershell -NoProfile -ExecutionPolicy Bypass -File packaging\windows\build-windows.ps1 2>&1 | Out-File $log -Append
"exit $LASTEXITCODE $(Get-Date)" | Out-File $log -Append
