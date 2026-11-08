#!/usr/bin/env bash
# Runs ON the lab laptop: desk-lock.sh take|refresh|release  (owner "desktop"; file ~/lab/lab.lock holds "owner|epoch")
set -u
L="$HOME/lab/lab.lock"
case "${1:-}" in
  take)
    if [ -e "$L" ] && ! grep -q '^desktop|' "$L"; then echo "held by: $(cat "$L")"; exit 1; fi
    echo "desktop|$(date +%s)" > "$L"; echo taken ;;
  refresh) grep -q '^desktop|' "$L" 2>/dev/null && echo "desktop|$(date +%s)" > "$L" && echo refreshed ;;
  release) if grep -q '^desktop|' "$L" 2>/dev/null; then rm -f "$L"; echo released; fi ;;
  *) cat "$L" 2>/dev/null || echo free ;;
esac
