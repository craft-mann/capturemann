#!/usr/bin/env bash
# Runs on the lab laptop (lock held by the caller): the stimulus clip of rec-live-av.sh recorded by
# ffmpeg itself (x11grab + the default sink's monitor), as an independent reference for how far the
# clip's flash and beep are apart in this setup. X11 session only.   rec-live-ref.sh <seconds>
set -u
secs="$1"
RUN="${RUN:-$HOME/rec-g}"
here="$(cd "$(dirname "$0")" && pwd)"
. "$HOME/lab/rec-env.sh"
clip="$RUN/sync-clip.mkv"
[ -f "$clip" ] || { echo "run rec-live-av.sh once first (it makes the clip)"; exit 1; }
ffplay -v quiet -x 1700 -y 950 -left 100 -top 60 -noborder -autoexit -loop 0 "$clip" > /dev/null 2>&1 &
fp=$!
sleep 3
ffmpeg -v error -y -f x11grab -framerate 30 -video_size 1920x1080 -i "$DISPLAY" \
  -f pulse -i @DEFAULT_MONITOR@ -t "$secs" -c:v libx264 -preset ultrafast -crf 20 -c:a pcm_s16le "$RUN/ref.mkv"
kill "$fp" 2>/dev/null
ffmpeg -v error -y -i "$RUN/ref.mkv" -vn -ac 1 -ar 48000 "$RUN/ref.wav"
python3 "$here/rec-av-analyze.py" --ref "$RUN/ref.mkv" "$RUN/ref.wav"
