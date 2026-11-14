#!/usr/bin/env bash
# trackD2_wait.sh: returns when the ThinkPad build log holds an "exit" line.
here="$(cd "$(dirname "$0")/.." && pwd)"
until bash "$here/scripts/trackF/trackF_win.sh" "$here/scripts/trackD2_tail.ps1" 2>&1 | grep -q '^exit '; do sleep 90; done
bash "$here/scripts/trackF/trackF_win.sh" "$here/scripts/trackD2_tail.ps1" 2>&1 | grep -E '^exit |error' | head
