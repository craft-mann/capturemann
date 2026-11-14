#!/usr/bin/env bash
# rc1_wait.sh <remote log> [poll seconds]: prints the last log line each poll and exits when a line starts with "exit ".
here="$(cd "$(dirname "$0")/../.." && pwd)"
log="$1"; every="${2:-300}"
while true; do
  l="$("$here/scripts/testlab/lab.sh" windows "Get-Content $log -Tail 1" 2>&1 | tail -1)"
  echo "$l"
  case "$l" in exit*) break ;; esac
  sleep "$every"
done
