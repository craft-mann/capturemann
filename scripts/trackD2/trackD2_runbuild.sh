#!/usr/bin/env bash
# trackD2_runbuild.sh: run trackD2_build.ps1 on the ThinkPad as a scheduled task of the logged-in user
# (it survives the ssh call) with a long time limit. Output is in C:\lab\trackD2-build.log.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
lab="$here/scripts/testlab/lab.sh"
"$lab" windows "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\lab\\win-run-in-session.ps1 -Exe C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -Arguments '-NoProfile -ExecutionPolicy Bypass -File C:\\lab\\trackD2_build.ps1' -TimeoutSec 3300 -WorkDir C:\\lab"
