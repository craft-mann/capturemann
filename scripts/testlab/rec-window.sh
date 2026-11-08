#!/usr/bin/env bash
# Runs on the lab laptop (lock held by the caller, no sound): a window recording of a mostly still window
# (a silent flashing clip in its own window), then the late-frame numbers.   rec-window.sh <secs>
secs="${1:-20}"
. "$HOME/lab/rec-env.sh"
RUN="${RUN:-$HOME/rec-g}"; mkdir -p "$RUN"
clip="$RUN/win-clip.mkv"
[ -f "$clip" ] || ffmpeg -v error -y -f lavfi -i "color=c=black:s=640x360:r=30:d=120" \
  -vf "drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable='lt(mod(t,4),0.1)'" -c:v libx264 -preset ultrafast "$clip"
ffplay -v quiet -an -loop 0 -window_title cmwin -x 640 -y 360 "$clip" > /dev/null 2>&1 &
fp=$!
sleep 3
BIN="${BIN:-$HOME/wt/rec-target/release}"
export CAPTUREMANN_CONFIG_DIR="$RUN/config" CAPTUREMANN_LIBRARY="$RUN/lib" CAPTUREMANN_SOCKET="$RUN/sock"
rm -rf "$RUN/lib" "$CAPTUREMANN_CONFIG_DIR"; mkdir -p "$CAPTUREMANN_CONFIG_DIR"
echo '{"agents.tool_policy":{"start_recording":"allow"}}' > "$CAPTUREMANN_CONFIG_DIR/settings.json"
pkill -x cm-daemon 2>/dev/null; sleep 0.5
"$BIN/cm-daemon" > "$RUN/daemon.log" 2>&1 &
sleep 2
"$BIN/capturemann" record start --json --window cmwin --size source --countdown 0 > "$RUN/start.json" 2>&1
echo "start rc=$? $(tr -d '\n ' < "$RUN/start.json" | head -c 500)"
sleep "$secs"
"$BIN/capturemann" record status --json 2>&1 | tr -d '\n ' | head -c 600; echo
"$BIN/capturemann" record stop --json > "$RUN/stop.json" 2>&1
echo "stop: $(tr -d '\n ' < "$RUN/stop.json" | head -c 1200)"
kill "$fp" 2>/dev/null
b=$(find "$RUN/lib" -name '*.cmrec' | tail -1)
ffprobe -v error -show_entries stream=codec_name,width,height,r_frame_rate,duration -of compact "$b/screen.mp4"
pkill -x cm-daemon
