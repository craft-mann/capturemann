#!/usr/bin/env bash
# Runs on the lab laptop (copy with lab.sh push): one real recording through the built daemon and CLI,
# in an isolated library, then ffprobe. Needs the lab lock to be held by the caller.
#   rec-live.sh <secs> [record start options...]      e.g. rec-live.sh 10 --size 720p --region 0,0,640,360
# Env: BIN (default ~/wt/rec-target/debug), RUN (default ~/rec-t), plus anything the product reads.
set -u
secs="$1"; shift
BIN="${BIN:-$HOME/wt/rec-target/debug}"
RUN="${RUN:-$HOME/rec-t}"
. "$HOME/lab/rec-env.sh"
export CAPTUREMANN_CONFIG_DIR="$RUN/config" CAPTUREMANN_LIBRARY="$RUN/lib" CAPTUREMANN_SOCKET="$RUN/sock"
mkdir -p "$CAPTUREMANN_CONFIG_DIR"
echo '{"agents.tool_policy":{"start_recording":"allow"}}' > "$CAPTUREMANN_CONFIG_DIR/settings.json"
pkill -x cm-daemon 2>/dev/null; sleep 0.5
"$BIN/cm-daemon" > "$RUN/daemon.log" 2>&1 &
sleep 2
"$BIN/capturemann" record start --json "$@" > "$RUN/start.json" 2>&1; echo "start rc=$?"; head -c 600 "$RUN/start.json"; echo
for i in $(seq 1 "$secs"); do
  sleep 1
  echo "t=$i $("$BIN/capturemann" record status --json 2>&1 | tr -d '\n ' | head -c 300)"
done
"$BIN/capturemann" record stop --json > "$RUN/stop.json" 2>&1; echo "stop rc=$?"; head -c 800 "$RUN/stop.json"; echo
sleep 1
f=$(ls -d "$RUN"/lib/*/*.cmrec 2>/dev/null | tail -1)
[ -z "$f" ] && f=$(find "$RUN/lib" -name '*.cmrec' | tail -1)
echo "bundle: $f"; ls -la "$f" 2>/dev/null
[ -f "$f/screen.mp4" ] && ffprobe -v error -show_entries stream=codec_name,width,height,r_frame_rate,nb_frames,duration -of compact "$f/screen.mp4"
pkill -x cm-daemon
