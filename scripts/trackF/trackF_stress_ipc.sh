#!/usr/bin/env bash
# Runs the ipc tests of the cm-daemon lib test binary many times under CPU load, to find the flake.
#   trackF_stress_ipc.sh <test binary> <runs> [filter]
set -u
bin="$1"; runs="${2:-50}"; filter="${3:-ipc::tests}"
pids=()
for _ in 1 2 3 4 5 6 7 8; do ( while :; do :; done ) & pids+=($!); done
fails=0
for i in $(seq 1 "$runs"); do
  out="$("$bin" "$filter" 2>&1)"
  if ! printf '%s' "$out" | grep -q "test result: ok"; then
    fails=$((fails + 1)); echo "--- run $i failed"; printf '%s\n' "$out" | grep -v "^$" | head -30
  fi
done
kill "${pids[@]}" 2>/dev/null
echo "runs=$runs failures=$fails"
