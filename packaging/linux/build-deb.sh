#!/usr/bin/env bash
# Builds capturemann_<version>_<arch>.deb (+ .sha256) from the tarball that build-linux.sh made, so the
# package holds exactly the programs, licence files, icons and .desktop entry of that tarball.
#
# Usage (any Linux, needs dpkg-deb):   packaging/linux/build-deb.sh [path/to/capturemann-<version>-linux-<arch>.tar.gz]
# Default input: the newest capturemann-*-linux-<this machine's arch>.tar.gz in packaging/linux/out.
# Output: packaging/linux/out/capturemann_<version>_<amd64|arm64>.deb and .deb.sha256 (or $CM_LINUX_OUT).
#
# Layout of the package:
#   /opt/capturemann/lib/capturemann/{cm-desktop,cm-overlay,capturemann,cm-mcp[,ffmpeg,ffprobe,llama-server,FFmpeg/,Llama/]}
#   /opt/capturemann/lib/Resources/Models/speaker/   (the speaker models; cm-ai reads <executable dir>/../Resources/Models/speaker,
#                                                     and the programs find ffmpeg and llama-server next to their own
#                                                     real path, so all of it stays under one prefix)
#   /usr/bin/{cm-desktop,cm-overlay,capturemann,cm-mcp} -> symlinks into /opt/capturemann/lib/capturemann
#   /usr/share/applications/dev.capturemann.app.desktop, /usr/share/icons/hicolor/..., /usr/share/doc/capturemann/
# The only maintainer scripts refresh the desktop-entry and icon caches (postinst, postrm); nothing is
# created outside the package (the user's library and settings are never touched, also not by purge).
#
# Depends: is the list verified on a clean ubuntu:24.04 container (packaging/linux/README.md, "What the user
# needs"; check-runtime.sh has the same soname -> package table), with the libc6 version taken from the glibc
# the programs need. Package names are the Ubuntu 24.04 / Debian 13 "t64" ones; build the .deb for another
# release on that release (the 22.04 names differ, and glibc 2.39 is not there anyway).
#
# Environment:
#   CM_LINUX_OUT        output folder (default packaging/linux/out; also where the default tarball is looked for).
#   CM_DEB_MAINTAINER   "Name <address>" for the Maintainer field (default: the project's GitHub noreply form).
#   CM_DEB_COMPRESS     xz (default; every dpkg reads it), gzip (fast, larger) or zstd (Ubuntu 24.04 / Debian 12 and later).
#   CM_DEB_LEVEL        compression level (default 6 for xz, 6 gzip, 9 zstd).
#   CM_DEB_STRIP=1      run `strip --strip-debug` on cm-desktop, cm-overlay, capturemann and cm-mcp in the package
#                       (the release profile keeps debug info; the cm-desktop in the tarball is about 700 MB). Default 0:
#                       the package holds the same programs as the tarball.
set -euo pipefail
die() { echo "build-deb.sh: $*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || die "this script only runs on Linux"
for t in dpkg-deb tar gzip install find du md5sum sha256sum; do
  command -v "$t" >/dev/null 2>&1 || die "required tool not found: $t (sudo apt install dpkg)"
done
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${CM_LINUX_OUT:-$root/packaging/linux/out}"

tarball="${1:-}"
if [ -z "$tarball" ]; then
  # shellcheck disable=SC2012
  tarball="$(ls -t "$out"/capturemann-*-linux-"$(uname -m)".tar.gz 2>/dev/null | head -1 || true)"
  [ -n "$tarball" ] || die "no capturemann-*-linux-$(uname -m).tar.gz in $out (run packaging/linux/build-linux.sh first)"
fi
[ -f "$tarball" ] || die "no such file: $tarball"
base="$(basename "$tarball" .tar.gz)"
case "$base" in
  capturemann-*-linux-x86_64) arch=x86_64; debarch=amd64; version="${base#capturemann-}"; version="${version%-linux-x86_64}" ;;
  capturemann-*-linux-aarch64) arch=aarch64; debarch=arm64; version="${base#capturemann-}"; version="${version%-linux-aarch64}" ;;
  *) die "the file name must be capturemann-<version>-linux-<x86_64|aarch64>.tar.gz (got $base)" ;;
esac
case "$version" in [0-9]*) ;; *) die "the version must start with a digit (got $version)" ;; esac
debversion="${version//-/~}"   # 0.3.0-rc.1 -> 0.3.0~rc.1 sorts before 0.3.0
if [ -f "$tarball.sha256" ]; then
  (cd "$(dirname "$tarball")" && sha256sum -c "$(basename "$tarball").sha256" >/dev/null) \
    || die "the checksum of $tarball does not match its .sha256"
fi
# A tarball of the other architecture cannot be installed by the dpkg that built it; that is fine, but warn.
[ "$arch" = "$(uname -m)" ] || echo "    note: the tarball is $arch, this machine is $(uname -m); the package is still labelled $debarch" >&2

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cm-deb.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
tar -xzf "$tarball" -C "$tmp" --no-same-owner --no-same-permissions
src="$tmp/$base"
[ -x "$src/lib/capturemann/capturemann" ] || die "$tarball does not look like a CaptureMann tarball (no lib/capturemann/capturemann)"
[ -f "$src/share/applications/dev.capturemann.app.desktop" ] || die "the tarball has no .desktop file"

pkg="$tmp/pkg"
opt=opt/capturemann
mkdir -p "$pkg/$opt/lib" "$pkg/usr/bin" "$pkg/usr/share/applications" "$pkg/usr/share/doc/capturemann" "$pkg/DEBIAN"

# ---- the programs and everything that must sit next to them ----
cp -R "$src/lib/capturemann" "$pkg/$opt/lib/capturemann"
[ ! -d "$src/lib/Resources" ] || cp -R "$src/lib/Resources" "$pkg/$opt/lib/Resources"
for p in cm-desktop cm-overlay capturemann cm-mcp; do
  [ -x "$pkg/$opt/lib/capturemann/$p" ] && ln -s "/$opt/lib/capturemann/$p" "$pkg/usr/bin/$p"
done
has_ffmpeg=0; [ -x "$pkg/$opt/lib/capturemann/ffmpeg" ] && has_ffmpeg=1
has_llama=0; [ -x "$pkg/$opt/lib/capturemann/llama-server" ] && has_llama=1
has_models=0; [ -d "$pkg/$opt/lib/Resources/Models/speaker" ] && has_models=1
# Modes: directories 755, data 644, the programs (top level of lib/capturemann) 755.
find "$pkg" -type d -exec chmod 755 {} +
find "$pkg/$opt" -type f -exec chmod 644 {} +
find "$pkg/$opt/lib/capturemann" -maxdepth 1 -type f \( -name cm-desktop -o -name cm-overlay -o -name capturemann \
  -o -name cm-mcp -o -name ffmpeg -o -name ffprobe -o -name llama-server \) -exec chmod 755 {} +

if [ "${CM_DEB_STRIP:-0}" = "1" ]; then
  command -v strip >/dev/null 2>&1 || die "CM_DEB_STRIP=1 needs strip (sudo apt install binutils)"
  for p in cm-desktop cm-overlay capturemann cm-mcp; do strip --strip-debug "$pkg/$opt/lib/capturemann/$p"; done
fi

# ---- desktop entry, icons, documents ----
sed 's|@BINDIR@|/usr/bin|g' "$src/share/applications/dev.capturemann.app.desktop" > "$pkg/usr/share/applications/dev.capturemann.app.desktop"
chmod 644 "$pkg/usr/share/applications/dev.capturemann.app.desktop"
if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$pkg/usr/share/applications/dev.capturemann.app.desktop" || die "the .desktop file does not validate"
fi
mkdir -p "$pkg/usr/share/icons"
cp -R "$src/share/icons/hicolor" "$pkg/usr/share/icons/hicolor"
find "$pkg/usr/share/icons" -type d -exec chmod 755 {} + -o -type f -exec chmod 644 {} +
doc="$pkg/usr/share/doc/capturemann"
for f in NOTICE THIRD_PARTY_LICENSES.md LIBRARIES.txt; do cp "$src/share/doc/capturemann/$f" "$doc/"; done
[ ! -d "$src/share/doc/capturemann/SHERPA-ONNX" ] || cp -R "$src/share/doc/capturemann/SHERPA-ONNX" "$doc/SHERPA-ONNX"
# Upstream change log (Markdown, not the Debian format, so not named changelog.gz), compressed.
if [ -f "$src/share/doc/capturemann/CHANGELOG.md" ]; then gzip -9n -c "$src/share/doc/capturemann/CHANGELOG.md" > "$doc/CHANGELOG.md.gz"; fi

# The Debian-format change log of the package itself (policy: a package without a revision is "native" and
# needs changelog.gz); the project's own history is CHANGELOG.md.gz above. The date is the tarball's.
{
  printf 'capturemann (%s) unstable; urgency=medium\n\n' "$debversion"
  printf '  * Early Linux build packaged from %s.\n' "$(basename "$tarball")"
  printf '    The project change log is in CHANGELOG.md.gz.\n\n'
  printf ' -- %s  %s\n' "${CM_DEB_MAINTAINER:-The CaptureMann contributors <noreply@users.noreply.github.com>}" \
    "$(date -u -R -d "@$(stat -c %Y "$tarball")")"
} | gzip -9n -c > "$doc/changelog.gz"

# copyright (machine-readable, DEP-5). The licence of CaptureMann is Apache-2.0 (the text is in base-files);
# what else is in the package and under what terms is listed, with the full texts in the files named.
{
  cat <<'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: CaptureMann
Upstream-Contact: https://github.com/craft-mann/capturemann/issues
Source: https://github.com/craft-mann/capturemann
Comment: The Rust crates linked into the programs, the fonts and the other third-party material have their own
 licences; THIRD_PARTY_LICENSES.md in this folder has the list and the texts.

Files: *
Copyright: 2026 The CaptureMann contributors
License: Apache-2.0
EOF
  if [ "$has_ffmpeg" = 1 ]; then cat <<'EOF'

Files: opt/capturemann/lib/capturemann/ffmpeg opt/capturemann/lib/capturemann/ffprobe
Copyright: the FFmpeg developers, and the authors of libvpx and libopus
License: LGPL-2.1+
Comment: An LGPL build without GPL or nonfree components, made from the sources named in
 /opt/capturemann/lib/capturemann/FFmpeg/SOURCE-OFFER.txt (BUILD-INFO.txt and LICENSES/ are in the same folder).
 /usr/share/common-licenses/LGPL-2.1 has the licence text.
EOF
  fi
  if [ "$has_llama" = 1 ]; then cat <<'EOF'

Files: opt/capturemann/lib/capturemann/llama-server
Copyright: The ggml authors (llama.cpp)
License: MIT
Comment: llama.cpp; the licence texts, with those of the libraries it includes, are in
 /opt/capturemann/lib/capturemann/Llama/LICENSES/.
EOF
  fi
  if [ -d "$doc/SHERPA-ONNX" ]; then cat <<'EOF'

Files: opt/capturemann/lib/capturemann/cm-desktop
Copyright: The CaptureMann contributors; sherpa-onnx authors; Microsoft Corporation (ONNX Runtime)
License: Apache-2.0
Comment: cm-desktop contains sherpa-onnx (Apache-2.0) and ONNX Runtime (MIT) as static libraries; their texts and the
 notices of ONNX Runtime's own third-party components are in /usr/share/doc/capturemann/SHERPA-ONNX/.
EOF
  fi
  if [ "$has_models" = 1 ]; then cat <<'EOF'

Files: opt/capturemann/lib/Resources/Models/speaker/*.onnx
Copyright: CNRS (pyannote segmentation 3.0, MIT); Alibaba / the 3D-Speaker authors (CAM++, Apache-2.0)
License: MIT and Apache-2.0
Comment: Unmodified model files; their licence texts are in
 /opt/capturemann/lib/Resources/Models/speaker/LICENSES/.
EOF
  fi
  cat <<'EOF'

License: Apache-2.0
 On Debian and Ubuntu systems the full text of the Apache License, Version 2.0 is in
 /usr/share/common-licenses/Apache-2.0.
EOF
} > "$doc/copyright"
find "$doc" -type d -exec chmod 755 {} +
find "$doc" -type f -exec chmod 644 {} +

# ---- control ----
glibc="$(grep -o 'GLIBC_[0-9][0-9.]*' "$src/share/doc/capturemann/LIBRARIES.txt" | head -1 | sed 's/GLIBC_//')"
[ -n "$glibc" ] || glibc=2.39
# The verified runtime list (packaging/linux/README.md). libva only for the bundled FFmpeg.
deps=("libc6 (>= $glibc)" "libgcc-s1" "libstdc++6" "libgtk-3-0t64" "libasound2t64" "libxkbcommon0" "libxkbcommon-x11-0"
      "libwayland-client0" "libwayland-cursor0" "libwayland-egl1" "libegl1" "libgbm1" "libdrm2" "libx11-6" "libxcb1"
      "libfontconfig1" "libfreetype6" "libssl3t64" "libdbus-1-3" "libpipewire-0.3-0t64" "libvulkan1"
      "mesa-vulkan-drivers | vulkan-icd")
[ "$has_ffmpeg" = 0 ] || deps+=("libva2" "libva-drm2")
recommends=("gnome-keyring | kwalletmanager" "xdg-desktop-portal" "pipewire")
[ "$has_ffmpeg" = 1 ] || recommends+=("ffmpeg")
join() { local IFS=,; echo "$*" | sed 's/,/, /g'; }
maintainer="${CM_DEB_MAINTAINER:-The CaptureMann contributors <noreply@users.noreply.github.com>}"
installed_kb="$(du -sk --exclude=DEBIAN "$pkg" | cut -f1)"
cat > "$pkg/DEBIAN/control" <<EOF
Package: capturemann
Version: $debversion
Architecture: $debarch
Maintainer: $maintainer
Installed-Size: $installed_kb
Depends: $(join "${deps[@]}")
Recommends: $(join "${recommends[@]}")
Section: graphics
Priority: optional
Homepage: https://github.com/craft-mann/capturemann
Description: screenshots and screen recording for people and for AI agents
 CaptureMann takes screenshots, annotates them, records the screen and keeps
 everything in a local library. Every feature is also available to AI agents
 through an MCP server and a command-line client (capturemann, cm-mcp).
 .
 This is an early Linux build: screenshots, annotation, the library, local
 speech-to-text, speaker labels and a local text model, the CLI and MCP.
 Recording is hidden in the app. Secrets need a Secret Service provider
 (gnome-keyring or KWallet). MP4 export needs a VA-API GPU or falls back to VP9.
EOF
for s in postinst postrm; do
  cat > "$pkg/DEBIAN/$s" <<'EOF'
#!/bin/sh
# Only refreshes the desktop-entry and icon caches of the system; nothing else.
set -e
case "$1" in
  configure|remove|purge)
    if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database -q /usr/share/applications || true; fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d /usr/share/icons/hicolor ]; then
      gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
    fi
    ;;
esac
exit 0
EOF
  chmod 755 "$pkg/DEBIAN/$s"
done
chmod 755 "$pkg/DEBIAN"
(cd "$pkg" && find . -path ./DEBIAN -prune -o -type f -print | sed 's|^\./||' | LC_ALL=C sort | xargs -d '\n' md5sum > DEBIAN/md5sums)
chmod 644 "$pkg/DEBIAN/control" "$pkg/DEBIAN/md5sums"

# ---- build ----
comp="${CM_DEB_COMPRESS:-xz}"
case "$comp" in xz|gzip|zstd) ;; *) die "CM_DEB_COMPRESS must be xz, gzip or zstd" ;; esac
case "$comp" in zstd) level="${CM_DEB_LEVEL:-9}" ;; *) level="${CM_DEB_LEVEL:-6}" ;; esac
deb="capturemann_${debversion}_${debarch}.deb"
mkdir -p "$out"
rm -f "$out/$deb" "$out/$deb.sha256"
echo "==> dpkg-deb -Z$comp -z$level  ($installed_kb KiB installed)"
dpkg-deb --root-owner-group -Z"$comp" -z"$level" --build "$pkg" "$out/$deb" >/dev/null
(cd "$out" && sha256sum "$deb" > "$deb.sha256")
echo "==> $out/$deb"
ls -l "$out/$deb"; cat "$out/$deb.sha256"
dpkg-deb -I "$out/$deb" | sed -n '/Package:/,/Recommends:/p'
