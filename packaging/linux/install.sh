#!/bin/sh
# Installs CaptureMann from the unpacked tarball. No root needed: the default prefix is ~/.local.
#
#   ./install.sh                              install for you (~/.local)
#   sudo ./install.sh --prefix /usr/local     install for everyone
#   ./install.sh --uninstall [--prefix P]     remove what this script installed
#
# Layout under the prefix: lib/capturemann/ holds every program (cm-desktop, cm-overlay,
# capturemann, cm-mcp and, when present, ffmpeg and ffprobe: the app finds FFmpeg next to its own
# executable), bin/ holds symlinks to them, share/ the .desktop entry, icons and documents.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
prefix="${HOME}/.local"
uninstall=0
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) [ $# -ge 2 ] || { echo "--prefix needs a folder" >&2; exit 2; }; prefix=$2; shift 2 ;;
    --prefix=*) prefix=${1#--prefix=}; shift ;;
    --uninstall) uninstall=1; shift ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
case "$prefix" in /*) ;; *) echo "the prefix must be an absolute path" >&2; exit 2 ;; esac

lib="$prefix/lib/capturemann"
bin="$prefix/bin"
share="$prefix/share"
app_id=dev.capturemann.app

refresh_caches() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$share/applications" >/dev/null 2>&1 || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "$share/icons/hicolor" >/dev/null 2>&1 || true
  fi
}

if [ "$uninstall" = 1 ]; then
  for l in cm-desktop cm-overlay capturemann cm-mcp; do
    if [ -L "$bin/$l" ] && [ "$(readlink "$bin/$l")" = "$lib/$l" ]; then rm -f "$bin/$l"; fi
  done
  rm -rf "$lib" "$prefix/lib/Resources/Models/speaker" "$share/doc/capturemann" "$share/applications/$app_id.desktop"
  rmdir "$prefix/lib/Resources/Models" "$prefix/lib/Resources" 2>/dev/null || true
  if [ -d "$share/icons/hicolor" ]; then
    find "$share/icons/hicolor" -name "$app_id.*" -exec rm -f {} + 2>/dev/null || true
  fi
  refresh_caches
  echo "CaptureMann removed from $prefix (your library and settings are untouched)."
  exit 0
fi

[ -x "$here/lib/capturemann/capturemann" ] || { echo "run this script from the unpacked CaptureMann folder" >&2; exit 1; }
mkdir -p "$lib" "$bin" "$share/applications" "$share/doc/capturemann"
# Replace the programs by renaming a new file over the old one: a running copy keeps working.
for f in "$here"/lib/capturemann/*; do
  [ -f "$f" ] || continue
  cp "$f" "$lib/.new.$$" && chmod 755 "$lib/.new.$$" && mv -f "$lib/.new.$$" "$lib/$(basename "$f")"
done
# Licence folders (FFmpeg/, Llama/) next to the programs, and the data cm-ai reads from <lib>/../Resources
# (the speaker models) when the tarball has them.
for d in "$here"/lib/capturemann/*/; do
  [ -d "$d" ] || continue
  rm -rf "$lib/$(basename "$d")"; cp -R "$d" "$lib/$(basename "$d")"
done
if [ -d "$here/lib/Resources" ]; then rm -rf "$prefix/lib/Resources"; cp -R "$here/lib/Resources" "$prefix/lib/Resources"; fi
for l in cm-desktop cm-overlay capturemann cm-mcp; do [ ! -x "$lib/$l" ] || ln -sf "$lib/$l" "$bin/$l"; done
cp -R "$here"/share/doc/capturemann/. "$share/doc/capturemann/"
(cd "$here/share/icons" && find hicolor -type f) | while read -r f; do
  mkdir -p "$share/icons/$(dirname "$f")"
  cp "$here/share/icons/$f" "$share/icons/$f"
done
sed "s|@BINDIR@|$bin|g" "$here/share/applications/$app_id.desktop" > "$share/applications/$app_id.desktop"
chmod 644 "$share/applications/$app_id.desktop"
refresh_caches

echo "CaptureMann installed in $prefix."
case ":$PATH:" in *":$bin:"*) ;; *) echo "Note: $bin is not on your PATH; add it to use the capturemann and cm-mcp commands." ;; esac
[ -x "$lib/ffmpeg" ] || echo "Note: this build has no FFmpeg of its own. Export needs ffmpeg 5.1 or newer on PATH (sudo apt install ffmpeg) or CM_FFMPEG=/path/to/ffmpeg."
if [ -x "$here/check-runtime.sh" ] && ! "$here/check-runtime.sh" "$lib" >/tmp/cm-runtime-check.$$ 2>&1; then
  echo "Some system libraries are missing:"; cat /tmp/cm-runtime-check.$$
  echo "(packaging/linux/README.md, \"What the user needs\", has the full list.)"
fi
rm -f /tmp/cm-runtime-check.$$
echo "Start it from your application menu (CaptureMann) or run: $bin/cm-desktop"
