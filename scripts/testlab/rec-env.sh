# Sourced by the rec-*.sh scripts on the lab laptop: the desktop session's variables, for X11 or Wayland.
_uid="$(id -u)"
_sid="$(loginctl show-user "$_uid" -p Display --value 2>/dev/null)"
_type="$(loginctl show-session "$_sid" -p Type --value 2>/dev/null)"
if [ "$_type" = wayland ]; then
  . "$HOME/lab/cm-env-wl.sh"
  export DISPLAY="${DISPLAY:-:0}"
else
  . "$HOME/lab/cm-env.sh"
fi
echo "session: ${XDG_SESSION_TYPE}"
