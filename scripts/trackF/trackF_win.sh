#!/usr/bin/env bash
# trackF_win.sh <file.ps1>: push the script to C:\lab and run it on the ThinkPad (session 0).
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
f="$1"; shift
"$here/scripts/testlab/lab.sh" push windows "$f" >/dev/null
"$here/scripts/testlab/lab.sh" windows "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\lab\\$(basename "$f") $*"
