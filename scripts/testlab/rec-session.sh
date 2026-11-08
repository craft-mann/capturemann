#!/usr/bin/env bash
# Runs on the lab laptop (lock held by the caller): switch the auto-login desktop session.
#   rec-session.sh x11 | wayland
# AccountsService keeps the user's session; GDM is restarted and auto-login brings the desktop up again.
case "${1:-}" in
  x11) s=ubuntu-xorg ;;
  wayland) s=ubuntu ;;
  *) echo "x11 or wayland"; exit 2 ;;
esac
f=/var/lib/AccountsService/users/trung
sudo -n sed -i "s/^Session=.*/Session=$s/; s/^XSession=.*/XSession=$s/" "$f" || exit 1
sudo -n grep -E "^(Session|XSession)=" "$f"
sudo -n systemctl restart gdm3 || exit 1
for _ in $(seq 1 40); do
  sleep 2
  sid="$(loginctl show-user "$(id -u)" -p Display --value 2>/dev/null)"
  t="$(loginctl show-session "$sid" -p Type --value 2>/dev/null)"
  if [ -n "$t" ] && pgrep -u "$(id -u)" -x gnome-shell > /dev/null; then sleep 8; echo "session type now: $t"; exit 0; fi
done
echo "no desktop session after 80 s"; exit 1
