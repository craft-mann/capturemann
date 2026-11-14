#!/usr/bin/env bash
# s2_all.sh <suffix>: measures the old 36-shot set and the new macOS and Windows sets, prints the totals.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
sfx="$1"
bash "$here/s2_run.sh" old "old_$sfx" 1000
bash "$here/s2_run.sh" run1/mac "mac_$sfx" 25
bash "$here/s2_run.sh" run1/win "win_$sfx" 12
for l in "old_$sfx" "mac_$sfx" "win_$sfx"; do
  echo "== $l"
  bash "$here/s2_sum.sh" "$l" | grep -E '^(ALL|dark|light) '
done
