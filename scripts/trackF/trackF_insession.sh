#!/usr/bin/env bash
# trackF_insession.sh <script.ps1> [powershell script args...]: push and run a PowerShell script in the
# logged-in user's desktop session on the ThinkPad (needed for mouse input and window lists); prints its output.
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
f="$1"; shift
lab="$here/scripts/testlab/lab.sh"
"$lab" push windows "$f" "$here/scripts/testlab/win-run-in-session.ps1" >/dev/null
"$lab" windows "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\lab\\win-run-in-session.ps1 -Exe C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -Arguments '-NoProfile -ExecutionPolicy Bypass -File C:\\lab\\$(basename "$f") $*' -TimeoutSec 300 -WorkDir C:\\lab"
