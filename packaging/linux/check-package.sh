#!/usr/bin/env bash
# Checks a Linux release file (the tarball or the .deb made by build-linux.sh / build-deb.sh) before it is
# published. It looks at the file, never installs it.
#
#   packaging/linux/check-package.sh [options] <capturemann-X-linux-ARCH.tar.gz | capturemann_X_ARCH.deb>
#
# Options:
#   --allow-missing a,b   programs that may be absent: ffmpeg, llama (testing only; a release has both)
#   --no-run              do not start the programs (they are started only when the package is for this machine's CPU)
#   --no-ldd              do not require the shared libraries of cm-desktop to resolve on this machine
#   --max-glibc V         the newest glibc symbol version the programs may need (default 2.39, Ubuntu 24.04)
#
# What it checks (each failure is listed, the exit status is 1 when there is any):
#   1. the file name gives version and architecture; the .deb control file agrees (Architecture, Version, Package)
#   2. every ELF program is built for that architecture; the glibc ceiling; nothing bundled that belongs to the system:
#      no libwayland, libX11/xcb, libGL/EGL/GLES, libvulkan, libdrm, libgbm, Mesa or other GPU driver, no libstdc++/libc
#   3. the licence files: LICENSE (Apache-2.0), NOTICE, THIRD_PARTY_LICENSES.md; in the .deb also the Debian
#      `copyright` file (DEP-5) and `changelog.gz`; with FFmpeg: FFmpeg/SOURCE-OFFER.txt, BUILD-INFO.txt, LICENSES/ and
#      an FFmpeg configuration with no GPL, nonfree, libx264 or libx265; with llama-server: Llama/LICENSES/llama.cpp-LICENSE.txt
#   4. the desktop entry validates (desktop-file-validate) and names a program that exists; the icon it names exists
#   5. the programs start (capturemann --version, cm-mcp and cm-daemon with a bad flag, cm-overlay --help, ffmpeg, ffprobe,
#      llama-server --version) and the shared libraries of every program resolve (ldd)
set -uo pipefail

allow_missing=""; run=1; use_ldd=1; max_glibc="2.39"; file=""
while [ $# -gt 0 ]; do
  case "$1" in
    --allow-missing) allow_missing="${2:-}"; shift 2 ;;
    --no-run) run=0; shift ;;
    --no-ldd) use_ldd=0; shift ;;
    --max-glibc) max_glibc="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) file="$1"; shift ;;
  esac
done
[ -n "$file" ] && [ -f "$file" ] || { echo "usage: check-package.sh [options] <tarball or .deb>" >&2; exit 2; }
for t in objdump tar sort; do command -v "$t" >/dev/null 2>&1 || { echo "required tool not found: $t" >&2; exit 2; }; done

fails=0
fail() { echo "FAIL: $*"; fails=$((fails + 1)); }
ok() { echo "ok:   $*"; }
note() { echo "note: $*"; }
allowed() { [[ ",$allow_missing," == *",$1,"* ]]; }

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cm-check-pkg.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
name="$(basename "$file")"

# ---- 1. what the file is ----
case "$name" in
  capturemann-*-linux-x86_64.tar.gz) kind=tar; arch=x86_64; elfarch=elf64-x86-64; ver="${name#capturemann-}"; ver="${ver%-linux-x86_64.tar.gz}" ;;
  capturemann-*-linux-aarch64.tar.gz) kind=tar; arch=aarch64; elfarch=elf64-littleaarch64; ver="${name#capturemann-}"; ver="${ver%-linux-aarch64.tar.gz}" ;;
  capturemann_*_amd64.deb) kind=deb; arch=x86_64; elfarch=elf64-x86-64; ver="${name#capturemann_}"; ver="${ver%_amd64.deb}" ;;
  capturemann_*_arm64.deb) kind=deb; arch=aarch64; elfarch=elf64-littleaarch64; ver="${name#capturemann_}"; ver="${ver%_arm64.deb}" ;;
  *) echo "FAIL: the file name must be capturemann-<v>-linux-<x86_64|aarch64>.tar.gz or capturemann_<v>_<amd64|arm64>.deb (got $name)"; exit 1 ;;
esac
echo "checking $name ($kind, $arch, version $ver)"

root="$tmp/x"; mkdir -p "$root"
if [ "$kind" = tar ]; then
  tar -xzf "$file" -C "$root" --no-same-owner || { fail "cannot unpack the tarball"; exit 1; }
  top="$(find "$root" -mindepth 1 -maxdepth 1 -type d | head -1)"
  [ -n "$top" ] || { fail "the tarball is empty"; exit 1; }
  progs="$top/lib/capturemann"        # where the programs are
  docs="$top/share/doc/capturemann"
  desktop="$top/share/applications/dev.capturemann.app.desktop"
  iconroot="$top/share/icons"
  bindir=""
else
  command -v dpkg-deb >/dev/null 2>&1 || { echo "dpkg-deb is needed to check a .deb" >&2; exit 2; }
  dpkg-deb -x "$file" "$root" || { fail "cannot unpack the .deb"; exit 1; }
  dpkg-deb -e "$file" "$tmp/DEBIAN" || { fail "cannot read the control files"; exit 1; }
  top="$root"
  progs="$root/opt/capturemann/lib/capturemann"
  docs="$root/usr/share/doc/capturemann"
  desktop="$root/usr/share/applications/dev.capturemann.app.desktop"
  iconroot="$root/usr/share/icons"
  bindir="$root/usr/bin"
  c_pkg="$(dpkg-deb -f "$file" Package)"; c_arch="$(dpkg-deb -f "$file" Architecture)"; c_ver="$(dpkg-deb -f "$file" Version)"
  want_arch=amd64; [ "$arch" = aarch64 ] && want_arch=arm64
  [ "$c_pkg" = capturemann ] && ok "Package: capturemann" || fail "Package is '$c_pkg'"
  [ "$c_arch" = "$want_arch" ] && ok "Architecture: $c_arch" || fail "Architecture is '$c_arch', the file name says $want_arch"
  [ "$c_ver" = "$ver" ] && ok "Version: $c_ver" || fail "Version is '$c_ver', the file name says '$ver'"
  dpkg-deb -f "$file" Depends | grep -q 'libc6 (>= ' && ok "Depends names a libc6 minimum" || fail "Depends has no 'libc6 (>= x)'"
  # Nothing in /usr/bin may be a real file (the programs live under /opt/capturemann), and every link must resolve inside the package.
  for l in "$bindir"/*; do
    [ -e "$l" ] || [ -L "$l" ] || continue
    t="$(readlink "$l")"
    if [ -f "$root$t" ]; then :; else fail "/usr/bin/$(basename "$l") -> $t does not exist in the package"; fi
  done
fi
[ -d "$progs" ] || { fail "no programs folder ($progs)"; exit 1; }

# ---- 2. architecture, glibc, bundled system libraries ----
echo "-- programs"
required="capturemann cm-mcp cm-daemon cm-desktop cm-overlay"
allowed ffmpeg || required="$required ffmpeg ffprobe"
allowed llama || required="$required llama-server"
for p in $required; do
  if [ -f "$progs/$p" ] && [ -x "$progs/$p" ]; then :; else fail "missing program: $p"; fi
done
elfs=()
while IFS= read -r f; do
  [ "$(head -c4 "$f" 2>/dev/null | od -An -c | tr -d ' ')" = '177ELF' ] && elfs+=("$f")
done < <(find "$top" -type f)
[ "${#elfs[@]}" -gt 0 ] || fail "no ELF file in the package"
glibc_max_seen="0"
for f in "${elfs[@]}"; do
  rel="${f#"$top"/}"
  fmt="$(objdump -f "$f" 2>/dev/null | sed -n 's/.*file format //p' | head -1)"
  [ "$fmt" = "$elfarch" ] || fail "$rel is $fmt, expected $elfarch"
  g="$(objdump -T "$f" 2>/dev/null | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -V -u | tail -1)"
  if [ -n "$g" ]; then
    if [ "$(printf '%s\n%s\n' "$g" "$max_glibc" | sort -V | tail -1)" != "$max_glibc" ]; then
      fail "$rel needs glibc $g, newer than the ceiling $max_glibc"
    fi
    [ "$(printf '%s\n%s\n' "$g" "$glibc_max_seen" | sort -V | tail -1)" = "$g" ] && glibc_max_seen="$g"
  fi
  # A program may only load what the system has by design: no GPL-family, no codec libraries (FFmpeg is a separate program).
  bad="$(objdump -p "$f" 2>/dev/null | awk '/NEEDED/ {print $2}' | grep -E -i 'x264|x265|fdk|libav(codec|format|util|filter)|libsw(scale|resample)|libpostproc|readline|libgomp|libcurl' || true)"
  [ -z "$bad" ] || fail "$rel is linked to a library that must not ship or be required: $(echo $bad)"
  rp="$(objdump -p "$f" 2>/dev/null | awk '/RPATH|RUNPATH/ {print $2}')"
  case "$rp" in "" | '$ORIGIN'* ) ;; *) fail "$rel has a build-machine RPATH/RUNPATH: $rp" ;; esac
done
ok "${#elfs[@]} ELF files, all $elfarch; the newest glibc symbol needed is ${glibc_max_seen} (ceiling $max_glibc)"
shared="$(find "$top" \( -type f -o -type l \) \( -name '*.so' -o -name '*.so.*' \) | sed "s|^$top/||")"
sysbad="$(grep -E -i '(^|/)lib(wayland[-a-z]*|X11[-a-z0-9]*|xcb[-a-z0-9]*|xkbcommon[-a-z0-9]*|GL[a-zA-Z0-9]*|EGL|GLESv[0-9]|OpenGL|vulkan|drm[-a-z0-9_]*|gbm|gtk-[0-9]|gdk-[0-9]|stdc\+\+|gcc_s|c|m|pthread|va[-a-z0-9]*|pipewire[-0-9.]*|asound|ssl|crypto|dbus-1)\.so|(^|/)(dri|vdpau|vulkan|libdrm)/|mesa|_dri\.so|nvidia|amdgpu|radeon|iris|lvp_icd|libLLVM' <<<"$shared" || true)"
if [ -n "$sysbad" ]; then fail "the package bundles system or GPU libraries (it must use the user's): $(echo $sysbad)"
else ok "no bundled libwayland, X11, GPU/Mesa/Vulkan, libc or libstdc++ ($(echo "$shared" | grep -c . || true) shared objects in total)"; fi

# ---- 3. licences ----
echo "-- licences"
need_doc() {
  if [ -s "$docs/$1" ]; then ok "$1"; else fail "missing or empty: share/doc/capturemann/$1"; fi
}
need_doc LICENSE; need_doc NOTICE; need_doc THIRD_PARTY_LICENSES.md
if [ -s "$docs/LICENSE" ]; then grep -q 'Apache License' "$docs/LICENSE" || fail "LICENSE is not the Apache License text"; fi
if [ -s "$docs/THIRD_PARTY_LICENSES.md" ]; then
  [ "$(wc -c < "$docs/THIRD_PARTY_LICENSES.md")" -gt 50000 ] || fail "THIRD_PARTY_LICENSES.md is suspiciously small"
fi
if [ "$kind" = deb ]; then
  need_doc copyright; need_doc changelog.gz
  if [ -s "$docs/copyright" ]; then
    head -1 "$docs/copyright" | grep -q 'copyright-format/1.0' || fail "copyright is not in the machine-readable format"
    grep -q '^Files: \*' "$docs/copyright" || fail "copyright has no 'Files: *' paragraph"
  fi
  # The package must not ship a file that dpkg would treat as a conffile by accident, and no setuid files.
  find "$root" -perm /6000 -type f | grep -q . && fail "a file with setuid or setgid is in the package"
fi
if [ -x "$progs/ffmpeg" ]; then
  for f in FFmpeg/SOURCE-OFFER.txt FFmpeg/BUILD-INFO.txt; do
    [ -s "$progs/$f" ] && ok "$f" || fail "missing: lib/capturemann/$f (LGPL source offer)"
  done
  [ -n "$(ls "$progs/FFmpeg/LICENSES" 2>/dev/null)" ] && ok "FFmpeg/LICENSES/" || fail "missing: lib/capturemann/FFmpeg/LICENSES/ (LGPL texts)"
  if [ -r "$progs/FFmpeg/BUILD-INFO.txt" ] && grep -q -E -e '--enable-(gpl|nonfree|version3|libx264|libx265|libfdk-aac)' "$progs/FFmpeg/BUILD-INFO.txt"; then
    fail "BUILD-INFO.txt shows a GPL or nonfree FFmpeg configuration"
  fi
  objdump -p "$progs/ffmpeg" 2>/dev/null | awk '/NEEDED/ {print $2}' | grep -E -i 'x264|x265|fdk' >/dev/null && fail "ffmpeg links x264/x265/fdk"
  strings_gpl=0
  if command -v strings >/dev/null 2>&1; then
    strings -a "$progs/ffmpeg" | grep -q -E -e '--enable-(gpl|nonfree|libx264|libx265|libfdk-aac)' && strings_gpl=1
  fi
  [ "$strings_gpl" = 0 ] || fail "the ffmpeg binary reports a GPL or nonfree configuration"
fi
if [ -x "$progs/llama-server" ]; then
  [ -s "$progs/Llama/LICENSES/llama.cpp-LICENSE.txt" ] && ok "Llama/LICENSES/llama.cpp-LICENSE.txt (MIT)" || fail "missing: lib/capturemann/Llama/LICENSES/llama.cpp-LICENSE.txt"
  bad="$(objdump -p "$progs/llama-server" | awk '/NEEDED/ {print $2}' | grep -v -E '^(libc\.so\.6|libm\.so\.6|libdl\.so\.2|libpthread\.so\.0|librt\.so\.1|ld-linux-[a-z0-9_-]+\.so\.[0-9]+|libvulkan\.so\.1)$' || true)"
  [ -z "$bad" ] || fail "llama-server needs libraries outside the allowed list: $(echo $bad)"
fi
if [ -d "$progs/../Resources/Models/speaker" ]; then
  [ -n "$(ls "$progs/../Resources/Models/speaker/LICENSES" 2>/dev/null)" ] && ok "speaker model licences" || fail "speaker models without their LICENSES/"
  [ -s "$docs/SHERPA-ONNX/ThirdPartyNotices-onnxruntime.txt" ] && ok "SHERPA-ONNX notices" || fail "missing share/doc/capturemann/SHERPA-ONNX/ThirdPartyNotices-onnxruntime.txt"
else
  note "no speaker models in the package"
fi

# ---- 4. desktop entry and icon ----
echo "-- desktop entry"
if [ -f "$desktop" ]; then
  if command -v desktop-file-validate >/dev/null 2>&1; then
    # The tarball's file still holds @BINDIR@ (install.sh fills it in); validate a copy with a real path.
    sed 's|@BINDIR@|/usr/bin|g' "$desktop" > "$tmp/validate.desktop"
    if desktop-file-validate "$tmp/validate.desktop"; then ok "desktop-file-validate"; else fail "desktop-file-validate rejects the entry"; fi
  else
    note "desktop-file-validate is not installed (apt install desktop-file-utils); skipped"
  fi
  exec_line="$(sed -n 's/^Exec=//p' "$desktop" | head -1)"; exec_prog="$(basename "${exec_line%% *}")"
  [ -x "$progs/$exec_prog" ] && ok "Exec= names $exec_prog, which is in the package" || fail "Exec= names $exec_prog, which is not in the package"
  icon="$(sed -n 's/^Icon=//p' "$desktop" | head -1)"
  if [ -n "$(find "$iconroot" -name "$icon.*" 2>/dev/null | head -1)" ]; then ok "icon $icon is in the package"; else fail "icon $icon is not in the package"; fi
else
  fail "no desktop entry (dev.capturemann.app.desktop)"
fi

# ---- 5. the programs start ----
echo "-- start"
host="$(uname -m)"
if [ "$run" = 0 ]; then note "--no-run: programs not started"
elif [ "$host" != "$arch" ]; then note "the package is $arch and this machine is $host: programs not started"
else
  # The package tree is read-only for the check: run from a copy of the folder so that nothing is written next to it.
  runner() { # name, want exit, args...
    local p="$1" want="$2"; shift 2
    [ -x "$progs/$p" ] || return 0
    local rc
    # The programs must not touch a real library or settings: point them at the scratch folder.
    HOME="$tmp/home" CAPTUREMANN_LIBRARY="$tmp/home/lib" CAPTUREMANN_CONFIG_DIR="$tmp/home/cfg" \
      timeout 30 "$progs/$p" "$@" >"$tmp/out.txt" 2>&1 </dev/null
    rc=$?
    if [ "$rc" = "$want" ]; then ok "$p $* -> exit $rc"; else fail "$p $* exited $rc, expected $want ($(head -c 200 "$tmp/out.txt" | tr '\n' ' '))"; fi
  }
  mkdir -p "$tmp/home"
  runner capturemann 0 --version
  runner cm-mcp 2 --bogus
  runner cm-daemon 2 --bogus
  runner cm-overlay 0 --help
  runner ffmpeg 0 -hide_banner -version
  runner ffprobe 0 -hide_banner -version
  runner llama-server 0 --version
fi
if [ "$use_ldd" = 1 ] && [ "$host" = "$arch" ] && command -v ldd >/dev/null 2>&1; then
  for p in $required; do
    [ -x "$progs/$p" ] || continue
    miss="$(ldd "$progs/$p" 2>&1 | grep 'not found' || true)"
    if [ -n "$miss" ]; then fail "$p: libraries not found on this machine: $(echo $miss)"; else ok "ldd $p resolves"; fi
  done
fi

echo
if [ "$fails" = 0 ]; then echo "PASS: $name"; exit 0; fi
echo "FAILED: $name has $fails problem(s)"; exit 1
