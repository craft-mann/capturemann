#!/usr/bin/env bash
# Runs ON the lab laptop: writes ~/lab/session.env with the desktop session's environment, read from the
# running gnome-shell (the same way scripts/testlab/regress-linux.sh does), so an ssh command can reach the desktop.
uid=$(id -u)
pid=$(pgrep -u "$uid" -x gnome-shell | head -1)
[ -n "$pid" ] || { echo "SESSION=none"; exit 1; }
mkdir -p ~/lab
: > ~/lab/session.env
for v in DISPLAY WAYLAND_DISPLAY XAUTHORITY DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR XDG_CURRENT_DESKTOP; do
  val=$(tr '\0' '\n' < "/proc/$pid/environ" | sed -n "s/^$v=//p" | head -1)
  [ -n "$val" ] && printf '%s=%q\n' "$v" "$val" >> ~/lab/session.env
done
sid=$(loginctl show-user "$uid" -p Display --value 2>/dev/null)
type=$(loginctl show-session "$sid" -p Type --value 2>/dev/null)
echo "XDG_SESSION_TYPE=${type:-unknown}" >> ~/lab/session.env
echo "SESSION=${type:-unknown}"
