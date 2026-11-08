#!/usr/bin/env bash
# Runs ON the lab laptop (X11, lab lock taken): starts cm-desktop privately and takes small stills of the GNOME top bar
# so the tray icon and its menu can be looked at. Usage: desk-panel.sh <target-dir> <out-dir> [step]
#   step "bar":   still of the top bar only
#   step "click X Y": click at X,Y (xdotool) and take a still of the whole screen after 1 s
set -u
T="$1"; OUT="$2"; shift 2; mkdir -p "$OUT"
bash "$(dirname "$0")/desk-env.sh" >/dev/null
set -a; . ~/lab/session.env; set +a
W="$(mktemp -d /tmp/cmd-panel.XXXXXX)"
export CAPTUREMANN_CONFIG_DIR="$W/config" CAPTUREMANN_LIBRARY="$W/library" CAPTUREMANN_SOCKET="$W/s.sock"
mkdir -p "$W/config" "$W/library"
"$T/debug/cm-desktop" > "$W/app.log" 2>&1 &
APP=$!
trap 'kill $MON $APP 2>/dev/null; sleep 1; kill -9 $APP 2>/dev/null; rm -rf "$W"' EXIT
sleep 15
SVC="org.kde.StatusNotifierItem-$APP-1"
dbus-monitor --session "destination='$SVC'" "sender='$SVC'" > "$OUT/mon.txt" 2>&1 &
MON=$!
sleep 1
import -window root "$OUT/screen0.png"
convert "$OUT/screen0.png" -crop 1920x40+0+0 +repage "$OUT/bar.png"
for step in "$@"; do
  case "$step" in
    click:*) xy="${step#click:}"; xdotool mousemove "${xy%,*}" "${xy#*,}" click 1; sleep 1; import -window root "$OUT/after-${xy/,/x}.png" ;;
    key:*) xdotool key "${step#key:}"; sleep 1 ;;
    shot:*) import -window root "$OUT/${step#shot:}.png" ;;
  esac
done
