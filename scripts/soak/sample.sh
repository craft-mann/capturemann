#!/usr/bin/env bash
# Passive sampler for the M1 soak (tests/manual/M1-SOAK.md). It only READS process numbers: it
# records nothing from the screen, camera or microphone and starts no recording.
#
# usage: scripts/soak/sample.sh <out.csv> [interval_seconds=300] [duration_seconds=11400]
# Samples every CaptureMann process (cm-desktop, cm-overlay, cm-daemon, capturemann, cm-mcp):
# time, process, pid, rss_kb, open_files, threads, and the number of app windows the desktop log
# says are live. Stop it with Ctrl-C; the CSV is flushed line by line.
set -euo pipefail
out=${1:?usage: sample.sh <out.csv> [interval] [duration]}
interval=${2:-300}
duration=${3:-11400}
log="${CAPTUREMANN_CONFIG_DIR:-$HOME/Library/Application Support/CaptureMann}/logs/desktop.log"
[ -f "$out" ] || echo "iso_time,process,pid,rss_kb,open_files,threads,log_errors_since_start" >"$out"
start_errors=$(grep -c ' ERROR ' "$log" 2>/dev/null || true)
start_errors=${start_errors:-0}
end=$((SECONDS + duration))
while [ "$SECONDS" -lt "$end" ]; do
  now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  errs=$(grep -c ' ERROR ' "$log" 2>/dev/null || true)
  errs=$((${errs:-0} - start_errors))
  for name in cm-desktop cm-overlay cm-daemon capturemann cm-mcp; do
    for pid in $(pgrep -x "$name" || true); do
      rss=$(ps -o rss= -p "$pid" | tr -d ' ')
      files=$(lsof -p "$pid" 2>/dev/null | wc -l | tr -d ' ')
      threads=$(ps -M -p "$pid" | tail -n +2 | wc -l | tr -d ' ')
      echo "$now,$name,$pid,${rss:-0},$files,$threads,$errs" >>"$out"
    done
  done
  sleep "$interval"
done
