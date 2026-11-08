#!/usr/bin/env bash
# Runs on the lab laptop: take the lab lock as "recording" (waits until it is free or older than 45 min),
# or release it.   rec-lock.sh take | release
set -u
f="$HOME/lab/lab.lock"
case "${1:-}" in
  take)
    for _ in $(seq 1 340); do
      cur="$(cat "$f" 2>/dev/null || true)"
      if [ -z "$cur" ] || [ $(( $(date +%s) - ${cur##*|} )) -gt 2700 ] || [ "${cur%%|*}" = recording ]; then
        echo "recording|$(date +%s)" > "$f"; echo "lock taken"; exit 0
      fi
      sleep 5
    done
    echo "lock busy: $cur"; exit 1 ;;
  release)
    if grep -q '^recording|' "$f" 2>/dev/null; then rm -f "$f"; fi; echo released ;;
esac
