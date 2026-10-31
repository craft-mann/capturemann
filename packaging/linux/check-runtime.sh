#!/bin/sh
# Checks that the system libraries the CaptureMann programs need are present, and names the apt package
# (Ubuntu 24.04 / Debian 13) that provides each missing one.
#
#   packaging/linux/check-runtime.sh [DIR]     DIR holds the programs (default: lib/capturemann of the
#                                              unpacked tarball this script is in, else ~/.local/lib/capturemann)
#
# Exit 0: nothing missing. Exit 1: something is missing (the install command is printed). Exit 2: usage error.
# Uses `ldd`, so it also finds libraries that a library itself needs. The soname -> package table below
# was produced by `dpkg -S` in a clean ubuntu:24.04 container (see
# verification/L1-packaging-clean-linux-2026-10-31-claude.md); a soname that is not in the table is looked up
# with apt-file when it is installed.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
if [ $# -ge 1 ]; then dir=$1
elif [ -d "$here/lib/capturemann" ]; then dir=$here/lib/capturemann
else dir="${HOME}/.local/lib/capturemann"; fi
command -v ldd >/dev/null 2>&1 || { echo "ldd not found (it comes with libc-bin)" >&2; exit 2; }
[ -d "$dir" ] || { echo "no such folder: $dir" >&2; exit 2; }

pkg_for() {
  case "$1" in
    libfreetype.so.*) echo libfreetype6 ;;
    libfontconfig.so.*) echo libfontconfig1 ;;
    libxkbcommon.so.*) echo libxkbcommon0 ;;
    libxkbcommon-x11.so.*) echo libxkbcommon-x11-0 ;;
    libglib-2.0.so.*|libgobject-2.0.so.*|libgio-2.0.so.*|libgmodule-2.0.so.*) echo libglib2.0-0t64 ;;
    libgtk-3.so.*|libgdk-3.so.*) echo libgtk-3-0t64 ;;
    libpango-1.0.so.*) echo libpango-1.0-0 ;;
    libpangocairo-1.0.so.*) echo libpangocairo-1.0-0 ;;
    libpangoft2-1.0.so.*) echo libpangoft2-1.0-0 ;;
    libcairo-gobject.so.*) echo libcairo-gobject2 ;;
    libcairo.so.*) echo libcairo2 ;;
    libgdk_pixbuf-2.0.so.*) echo libgdk-pixbuf-2.0-0 ;;
    libatk-1.0.so.*) echo libatk1.0-0t64 ;;
    libstdc++.so.*) echo libstdc++6 ;;
    libgcc_s.so.*) echo libgcc-s1 ;;
    libasound.so.*) echo libasound2t64 ;;
    libssl.so.*|libcrypto.so.*) echo libssl3t64 ;;
    libEGL.so.*) echo libegl1 ;;
    libGL.so.*) echo libgl1 ;;
    libgbm.so.*) echo libgbm1 ;;
    libdrm.so.*) echo libdrm2 ;;
    libxcb.so.*) echo libxcb1 ;;
    libX11.so.*) echo libx11-6 ;;
    libX11-xcb.so.*) echo libx11-xcb1 ;;
    libpipewire-0.3.so.*) echo libpipewire-0.3-0t64 ;;
    libdbus-1.so.*) echo libdbus-1-3 ;;
    libwayland-client.so.*) echo libwayland-client0 ;;
    libwayland-cursor.so.*) echo libwayland-cursor0 ;;
    libwayland-egl.so.*) echo libwayland-egl1 ;;
    libvulkan.so.*) echo libvulkan1 ;;
    libva.so.*) echo libva2 ;;
    libva-drm.so.*) echo libva-drm2 ;;
    *)
      if command -v apt-file >/dev/null 2>&1; then
        apt-file -x search "/$1\$" 2>/dev/null | head -1 | cut -d: -f1
      fi ;;
  esac
}

pkgs=""
total=0
bad=0
for p in "$dir"/cm-desktop "$dir"/cm-overlay "$dir"/capturemann "$dir"/cm-mcp "$dir"/ffmpeg "$dir"/ffprobe "$dir"/llama-server; do
  [ -x "$p" ] || continue
  total=$((total + 1))
  for lib in $(ldd "$p" 2>/dev/null | awk '/not found/ {print $1}'); do
    pk=$(pkg_for "$lib")
    echo "missing: $lib  (needed by $(basename "$p"))  package: ${pk:-unknown}"
    bad=$((bad + 1))
    pkgs="$pkgs ${pk:-}"
  done
done
[ "$total" -gt 0 ] || { echo "no CaptureMann programs found in $dir" >&2; exit 2; }
if [ "$bad" = 0 ]; then
  echo "All libraries the programs link are present ($total programs checked)."
  if ! ldconfig -p 2>/dev/null | grep -q 'libvulkan.so.1'; then
    echo "Note: libvulkan.so.1 is not installed. cm-desktop loads it at run time: sudo apt install libvulkan1 mesa-vulkan-drivers"
  fi
  exit 0
fi
list=$(echo "$pkgs" | tr ' ' '\n' | grep -v '^$' | sort -u | tr '\n' ' ')
if [ -n "$list" ]; then echo; echo "Install on Ubuntu/Debian:  sudo apt install $list"; fi
exit 1
