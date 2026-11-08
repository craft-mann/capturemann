#!/usr/bin/env bash
# Runs on the lab laptop (lab lock held by the caller): the recording gate of release 0.3.0.
# Plays a fullscreen test clip (white flash + 1 kHz beep together every 2 s), records screen + microphone
# (+ system audio) with the built daemon/CLI into an isolated library, then measures flash against beep
# onset in both audio tracks with rec-av-analyze.py.
#   rec-gate.sh <seconds> [extra record start options...]     BIN, RUN as in rec-live.sh
set -u
secs="$1"; shift
here="$(cd "$(dirname "$0")" && pwd)"
BIN="${BIN:-$HOME/wt/rec-target/debug}"
RUN="${RUN:-$HOME/rec-g}"
mkdir -p "$RUN"
. "$HOME/lab/cm-env.sh"
[ -f "$HOME/lab/session.env" ] && { set -a; . "$HOME/lab/session.env"; set +a; }
clip="$RUN/sync-clip.mkv"
if [ ! -f "$clip" ] || [ "${REGEN:-0}" = 1 ]; then
  len=$((secs + 40))
  ffmpeg -v error -y -f lavfi -i "color=c=black:s=640x360:r=30:d=$len" \
    -f lavfi -i "aevalsrc='0.6*sin(2*PI*1000*t)*lt(mod(t,2),0.1)':s=48000:d=$len" \
    -vf "drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable='lt(mod(t,2),0.1)'" \
    -c:v libx264 -preset ultrafast -crf 18 -g 30 -c:a aac "$clip" || exit 1
fi
export CAPTUREMANN_CONFIG_DIR="$RUN/config" CAPTUREMANN_LIBRARY="$RUN/lib" CAPTUREMANN_SOCKET="$RUN/sock"
mkdir -p "$CAPTUREMANN_CONFIG_DIR"
echo '{"agents.tool_policy":{"start_recording":"allow"}}' > "$CAPTUREMANN_CONFIG_DIR/settings.json"
rm -rf "$RUN/lib"
pkill -x cm-daemon 2>/dev/null; sleep 0.5
"$BIN/cm-daemon" > "$RUN/daemon.log" 2>&1 &
sleep 2
ffplay -v quiet -x 1700 -y 950 -left 100 -top 60 -noborder -autoexit -loop 0 "$clip" > /dev/null 2>&1 &
fp=$!
sleep 3
"$BIN/capturemann" record start --json --size 720p --countdown 0 --mic default --system-audio "$@" > "$RUN/start.json" 2>&1
echo "start rc=$? $(tr -d '\n ' < "$RUN/start.json" | head -c 700)"
t0=$(date +%s)
while [ $(( $(date +%s) - t0 )) -lt "$secs" ]; do
  sleep 30
  echo "t=$(( $(date +%s) - t0 )) $("$BIN/capturemann" record status --json 2>&1 | tr -d '\n ' | head -c 160)"
done
"$BIN/capturemann" record stop --json > "$RUN/stop.json" 2>&1; echo "stop rc=$? $(tr -d '\n ' < "$RUN/stop.json" | head -c 900)"
kill "$fp" 2>/dev/null
sleep 2
f=$(find "$RUN/lib" -name '*.cmrec' | tail -1)
echo "bundle: $f"; ls -la "$f"
python3 "$here/rec-av-analyze.py" "$f"
pkill -x cm-daemon
