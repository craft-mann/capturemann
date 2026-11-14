#!/usr/bin/env bash
# s2_mac_panes.sh <outdir> <prefix>: captures a few System Settings panes (window only).
set -u
out="$1"; pre="$2"
panes="Appearance-Settings:appearance Sound-Settings:sound Displays-Settings:displays Keyboard-Settings:keyboard Accessibility-Settings:accessibility Desktop-Settings:desktop ControlCenter-Settings:control Wallpaper-Settings:wallpaper"
for p in $panes; do
  id="${p%%:*}"; name="${p##*:}"
  open "x-apple.systempreferences:com.apple.${id}.extension"
  sleep 3
  w="$(/private/tmp/claude-502/s2_wins "System Settings" | head -1 | cut -d' ' -f3)"
  [ -n "$w" ] && screencapture -x -o -l "$w" "$out/${pre}_settings_${name}.png"
done
ls "$out"
