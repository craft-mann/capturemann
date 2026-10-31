#!/usr/bin/env bash
# Prepares a DEDICATED Ubuntu 24.04 test machine (the ThinkPad T490) so that CaptureMann can be tested
# and driven over ssh: ssh with a key, auto-login, a session that never sleeps or locks, the runtime
# packages of the Linux tarball, the Intel VA-API driver, the tools the manual tests use, and GRUB set up
# so another OS can be chosen for the next boot. Run it once as the test user with sudo rights.
#
#   scripts/testlab/linux-setup.sh --pubkey ~/.ssh/id_ed25519.pub [--hostname cm-t490] [--kde] [--dry-run]
#
# It is meant for a machine with nothing else on it: it turns off screen locking, sleep and the lock
# screen, enables auto-login and passwordless sudo for the test user. Do not run it on a personal computer.
# Safe to run again (every step checks first). --dry-run prints what it would do and changes nothing.
set -euo pipefail

pubkey=""; host=""; kde=0; dry=0
while [ $# -gt 0 ]; do
  case "$1" in
    --pubkey) pubkey="$2"; shift 2 ;;
    --hostname) host="$2"; shift 2 ;;
    --kde) kde=1; shift ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

user="${SUDO_USER:-$USER}"
home="$(getent passwd "$user" | cut -d: -f6)"
say() { printf '\n== %s\n' "$*"; }
run() { if [ "$dry" = 1 ]; then printf '   [dry-run] %s\n' "$*"; else "$@"; fi; }
write_file() { # write_file <path> <mode> ; content on stdin
  local path="$1" mode="$2" tmp; tmp="$(mktemp)"; cat >"$tmp"
  if [ -f "$path" ] && cmp -s "$tmp" "$path"; then rm -f "$tmp"; echo "   unchanged: $path"; return; fi
  if [ "$dry" = 1 ]; then echo "   [dry-run] would write $path"; rm -f "$tmp"; return; fi
  sudo install -m "$mode" "$tmp" "$path"; rm -f "$tmp"; echo "   wrote: $path"
}

[ "$(uname -s)" = Linux ] || { echo "Linux only" >&2; exit 1; }
. /etc/os-release
[ "${VERSION_ID:-}" = "24.04" ] || echo "warning: tested on Ubuntu 24.04, this is ${PRETTY_NAME:-unknown}" >&2

say "Packages (runtime list of the tarball, test tools, Intel VA-API)"
pkgs=(openssh-server avahi-daemon curl xz-utils rsync git jq
  libgtk-3-0t64 libasound2t64 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libwayland-cursor0
  libwayland-egl1 libegl1 libgbm1 libdrm2 libx11-6 libxcb1 libfontconfig1 libfreetype6 libssl3t64
  libdbus-1-3 libpipewire-0.3-0t64 libvulkan1 mesa-vulkan-drivers libva2 libva-drm2
  libayatana-appindicator3-1 libxdo3 gnome-keyring xdg-desktop-portal xdg-desktop-portal-gnome ffmpeg
  pipewire pipewire-pulse wireplumber
  intel-media-va-driver vainfo intel-gpu-tools vulkan-tools mesa-utils
  xdotool xclip wl-clipboard x11-apps imagemagick scrot minisign desktop-file-utils
  python3-gi python3-dbus gir1.2-gtk-3.0 sysstat lm-sensors)
[ "$kde" = 1 ] && pkgs+=(kde-plasma-desktop xdg-desktop-portal-kde)
run sudo apt-get update -qq
run sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "${pkgs[@]}"

say "Hostname"
if [ -n "$host" ]; then run sudo hostnamectl set-hostname "$host"; else echo "   kept: $(hostname) (use --hostname)"; fi

say "ssh: key login, no password login"
if [ -n "$pubkey" ]; then
  [ -f "$pubkey" ] || { echo "no such file: $pubkey" >&2; exit 1; }
  run install -d -m 700 "$home/.ssh"
  if ! grep -qsF "$(cut -d' ' -f2 "$pubkey")" "$home/.ssh/authorized_keys" 2>/dev/null; then
    if [ "$dry" = 1 ]; then echo "   [dry-run] would add $pubkey to authorized_keys"; else
      cat "$pubkey" >>"$home/.ssh/authorized_keys"; chmod 600 "$home/.ssh/authorized_keys"; fi
  else echo "   key already authorised"; fi
else echo "   no --pubkey given: ssh keys not touched"; fi
write_file /etc/ssh/sshd_config.d/10-testlab.conf 644 <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
EOF
run sudo systemctl enable --now ssh avahi-daemon

say "Passwordless sudo for $user (dedicated test machine)"
write_file /etc/sudoers.d/90-testlab 440 <<EOF
$user ALL=(ALL) NOPASSWD:ALL
EOF

say "Auto-login, no lock, no sleep"
write_file /etc/gdm3/custom.conf 644 <<EOF
[daemon]
AutomaticLoginEnable=true
AutomaticLogin=$user
EOF
run sudo systemctl mask sleep.target suspend.target hibernate.target hybrid-sleep.target
write_file /etc/systemd/logind.conf.d/10-testlab.conf 644 <<'EOF'
[Login]
HandleLidSwitch=ignore
HandleLidSwitchExternalPower=ignore
HandleLidSwitchDocked=ignore
IdleAction=ignore
EOF
# The user's own GNOME settings (needs the user's session bus when run over ssh).
gs() { if [ "$dry" = 1 ]; then echo "   [dry-run] gsettings set $*"; else
  sudo -u "$user" env XDG_RUNTIME_DIR="/run/user/$(id -u "$user")" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$(id -u "$user")/bus" gsettings set "$@" 2>/dev/null \
    || echo "   skipped (no session yet): gsettings set $*"; fi; }
gs org.gnome.desktop.session idle-delay 0
gs org.gnome.desktop.screensaver lock-enabled false
gs org.gnome.desktop.screensaver idle-activation-enabled false
gs org.gnome.settings-daemon.plugins.power sleep-inactive-ac-type "'nothing'"
gs org.gnome.settings-daemon.plugins.power sleep-inactive-battery-type "'nothing'"
gs org.gnome.desktop.notifications show-banners false
gs org.gnome.desktop.interface enable-animations false

say "GRUB: remember the last OS, show the menu, find Windows"
if [ -f /etc/default/grub ]; then
  setkv() { local k="$1" v="$2"
    if grep -q "^$k=" /etc/default/grub; then run sudo sed -i "s|^$k=.*|$k=$v|" /etc/default/grub
    else run sudo sh -c "echo '$k=$v' >> /etc/default/grub"; fi; }
  setkv GRUB_DEFAULT saved
  setkv GRUB_SAVEDEFAULT true
  setkv GRUB_TIMEOUT_STYLE menu
  setkv GRUB_TIMEOUT 5
  setkv GRUB_DISABLE_OS_PROBER false
  run sudo update-grub
fi

say "Helper: switch the next boot to another OS (lab-next-boot)"
write_file /usr/local/bin/lab-next-boot 755 <<'EOF'
#!/bin/sh
# lab-next-boot windows|ubuntu [--now]   one boot into the chosen system, then back to the saved default.
# os-prober names the Windows entry "Windows Boot Manager (on /dev/...)": look it up in grub.cfg.
case "$1" in
  windows) pattern="Windows" ;;
  ubuntu) pattern="Ubuntu" ;;
  *) echo "usage: lab-next-boot windows|ubuntu [--now]" >&2; exit 2 ;;
esac
entry="$(sudo grep -o "^menuentry '[^']*$pattern[^']*'" /boot/grub/grub.cfg | head -n 1 | cut -d"'" -f2)"
[ -n "$entry" ] || { echo "no GRUB entry matching $pattern (run: sudo update-grub)" >&2; exit 1; }
sudo grub-reboot "$entry" || exit 1
echo "next boot: $entry"
[ "$2" = "--now" ] && exec sudo systemctl reboot
echo "run 'sudo reboot' when ready"
EOF

say "Hardware facts worth a look"
echo "   VA-API:  vainfo   (expect VAProfileH264... with VAEntrypointEncSlice on the T490's UHD 620)"
echo "   Session: echo \$XDG_SESSION_TYPE   (choose Wayland or X11 at the login screen's gear icon)"
echo
if [ "$dry" = 1 ]; then echo "Dry run: nothing was changed."; else echo "Done. Reboot once so auto-login and the masked sleep targets take effect."; fi
