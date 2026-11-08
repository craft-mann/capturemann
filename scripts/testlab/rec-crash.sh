#!/usr/bin/env bash
# Runs on the lab laptop (lock held by the caller): REC-8 AC1 on the real desktop, silent.
# Records screen + microphone, kill -9 the daemon at <secs>, starts it again, prints what was recovered.
#   rec-kill9.sh <secs> [record start options...]      BIN, RUN as in rec-live.sh
set -u
secs="$1"; shift
BIN="${BIN:-$HOME/wt/rec-target/release}"
RUN="${RUN:-$HOME/rec-k}"
. "$HOME/lab/rec-env.sh"
export CAPTUREMANN_CONFIG_DIR="$RUN/config" CAPTUREMANN_LIBRARY="$RUN/lib" CAPTUREMANN_SOCKET="$RUN/sock"
rm -rf "$RUN"; mkdir -p "$CAPTUREMANN_CONFIG_DIR"
echo '{"agents.tool_policy":{"start_recording":"allow"}}' > "$CAPTUREMANN_CONFIG_DIR/settings.json"
pkill -x cm-daemon 2>/dev/null; sleep 0.5
"$BIN/cm-daemon" > "$RUN/daemon1.log" 2>&1 &
sleep 2
"$BIN/capturemann" record start --json --size 720p --countdown 0 --mic default "$@" > "$RUN/start.json" 2>&1; echo "start rc=$?"
sleep "$secs"
b=$(find "$RUN/lib" -name '*.cmrec' | tail -1)
echo "before the kill:"; ls -la "$b"
kill -9 "$(pgrep -x cm-daemon)"
sleep 1
echo "after the kill, holders of the owner file:"; fuser -v "$b"/recording.owner 2>&1 | head -5; ps -eo pid,ppid,etimes,cmd | grep "[f]fmpeg" | head -3; [ "${KEEP_CHILD:-0}" = 1 ] || pkill -x ffmpeg
"$BIN/cm-daemon" > "$RUN/daemon2.log" 2>&1 &
sleep 6
echo "after the restart:"; ls -la "$b"
grep -i "recover" "$RUN/daemon2.log" | head -5
for f in screen.mp4 mic.wav; do
  [ -f "$b/$f" ] && echo "$f: $(ffprobe -v error -show_entries format=duration -of csv=p=0 "$b/$f") s"
done
python3 - "$b/manifest.json" <<'PYEOF'
import json, sys
m = json.load(open(sys.argv[1]))
print("recovered:", m.get("recovered"), "duration ticks:", m.get("duration"), "=", (m.get("duration") or 0) / 120000.0, "s")
print("revision 0:", (m.get("revisions") or [{}])[0].get("action"))
print("stop_reason:", m.get("stop_reason"))
PYEOF
pkill -x cm-daemon
