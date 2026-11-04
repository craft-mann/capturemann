#!/bin/sh
# Prints the ERROR and WARN lines of the app logs written since a marker (LOGS-1).
#
#   scripts/check_logs_clean.sh mark     # remember where each log ends now
#   ... use the app ...
#   scripts/check_logs_clean.sh check    # print the new ERROR/WARN lines; exit 1 if there are any
#
# The logs are in $CAPTUREMANN_CONFIG_DIR/logs, else ~/Library/Application Support/CaptureMann/logs
# (macOS) or ~/.config/CaptureMann/logs. Only line counts are stored (in the logs folder, file
# .logs-1-mark); no log text leaves the computer.
set -eu

dir="${CAPTUREMANN_LOG_DIR:-}"
if [ -z "$dir" ]; then
  if [ -n "${CAPTUREMANN_CONFIG_DIR:-}" ]; then
    dir="$CAPTUREMANN_CONFIG_DIR/logs"
  elif [ -d "$HOME/Library/Application Support/CaptureMann/logs" ]; then
    dir="$HOME/Library/Application Support/CaptureMann/logs"
  else
    dir="$HOME/.config/CaptureMann/logs"
  fi
fi
mark="$dir/.logs-1-mark"
logs="desktop.log daemon.log overlay.log"

case "${1:-check}" in
  mark)
    : >"$mark"
    for f in $logs; do
      n=0
      [ -f "$dir/$f" ] && n=$(wc -l <"$dir/$f" | tr -d ' ')
      echo "$f $n" >>"$mark"
    done
    echo "marked: $(tr '\n' ' ' <"$mark")"
    ;;
  check)
    bad=0
    for f in $logs; do
      [ -f "$dir/$f" ] || continue
      from=0
      if [ -f "$mark" ]; then
        from=$(awk -v f="$f" '$1 == f { print $2 }' "$mark")
        from=${from:-0}
      fi
      # A log that was rotated since the mark is read from its start.
      total=$(wc -l <"$dir/$f" | tr -d ' ')
      [ "$total" -lt "$from" ] && from=0
      hits=$(tail -n +"$((from + 1))" "$dir/$f" | grep -E ' (ERROR|WARN) ' || true)
      if [ -n "$hits" ]; then
        bad=1
        echo "== $f: $(echo "$hits" | wc -l | tr -d ' ') ERROR/WARN line(s) since the mark =="
        echo "$hits" | cut -c1-240 | sort -k4 | uniq -c -f3 | sort -rn | head -20
      fi
    done
    if [ "$bad" -eq 0 ]; then
      echo "clean: no ERROR or WARN line in $logs since the mark"
    fi
    exit "$bad"
    ;;
  *)
    echo "usage: $0 mark|check" >&2
    exit 2
    ;;
esac
