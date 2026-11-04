#!/usr/bin/env bash
# Builds the LGPL FFmpeg for the Windows zip and installer (NFR-12, NFR-15): `ffmpeg.exe` and `ffprobe.exe`, with
# libvpx (VP9) and libopus (Opus) linked statically for WebM, Media Foundation (`h264_mf`, `hevc_mf`) for H.264
# and HEVC, and nothing GPL. Same pinned sources, same component lists (Media Foundation in place of
# VideoToolbox/VAAPI) and the same self-check as build-macos.sh and build-linux.sh.
#
# Usage, in an MSYS2 MINGW64 shell on Windows x64:   packaging/ffmpeg/build-windows.sh
#   (on windows-latest: C:\msys64 is preinstalled; the workflow installs the packages below and runs this script
#    with `msys2 -c` or `C:\msys64\usr\bin\bash.exe -lc` with MSYSTEM=MINGW64)
# Output: packaging/ffmpeg/out-windows-x64/{ffmpeg.exe,ffprobe.exe,BUILD-INFO.txt,SOURCE-OFFER.txt,LICENSES/}
#         (git-ignored). Give that folder to packaging\windows\build-windows.ps1 as CM_FFMPEG_BUNDLE.
#
# Environment: CM_FFMPEG_SRC_DIR (tarball cache, default packaging/ffmpeg/.downloads), CM_FFMPEG_BUILD_DIR (keep
# the scratch tree), CM_FFMPEG_OUT, CM_FFMPEG_JOBS (default: CPUs), CM_SOURCE_REPO.
#
# Needs (MSYS2 packages):
#   pacman -S --needed make diffutils perl curl xz tar \
#     mingw-w64-x86_64-gcc mingw-w64-x86_64-pkgconf mingw-w64-x86_64-nasm mingw-w64-x86_64-zlib
# No network is needed after the tarballs are cached.
#
# Licence: configured with --disable-gpl --disable-nonfree --disable-version3, so the result is LGPL v2.1 or later.
# libvpx (BSD-3-Clause + patent grant), libopus (BSD-3-Clause) and zlib (zlib licence) are linked statically,
# together with the mingw-w64 runtime. No libx264, libx265, fdk-aac or other GPL/nonfree component is enabled.
#
# H.264 on Windows: there is no libx264 (GPL) and no software H.264 encoder is built into FFmpeg. `h264_mf` is
# the Windows Media Foundation wrapper: it asks the operating system for an encoder, which is a GPU's hardware
# encoder when one exists. (Windows also has a Microsoft software encoder; whether FFmpeg may fall back to it
# is the open question in specs/plans/release-0.2.8-plan.md; this build does not decide it.)
#
# NOT RUN ON WINDOWS YET (written on a Mac): the first run on windows-latest will show what needs adjusting.
set -euo pipefail
die() { echo "build-windows.sh: $*" >&2; exit 1; }

# ---- pinned sources: the same files as build-macos.sh and build-linux.sh (keep the three in step) ----
FFMPEG_VERSION="9.0.2"
FFMPEG_URL="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
FFMPEG_SHA256="8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"
LIBVPX_VERSION="1.17.0"
LIBVPX_URL="https://github.com/webmproject/libvpx/archive/refs/tags/v${LIBVPX_VERSION}.tar.gz"
LIBVPX_SHA256="1020f184046187baa2985dbde38e0691f49c44088bca7a1842b0236c6081dc0a"
OPUS_VERSION="1.6.1"
OPUS_URL="https://downloads.xiph.org/releases/opus/opus-${OPUS_VERSION}.tar.gz"
OPUS_SHA256="6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1"

case "$(uname -s)" in MINGW64_NT-*) ;; *) die "this script runs in an MSYS2 MINGW64 shell on Windows (uname -s is $(uname -s); MSYSTEM=${MSYSTEM:-unset})" ;; esac
[ "$(uname -m)" = "x86_64" ] || die "unsupported machine $(uname -m) (x64 only)"
ARCH=x86_64
VPX_TARGET="x86_64-win64-gcc"
for t in gcc make perl pkg-config curl xz sha256sum tar objdump strip nasm diff; do
  command -v "$t" >/dev/null 2>&1 || die "required tool not found: $t (see the pacman line at the top of this script)"
done
[ -f "$(dirname "$(command -v gcc)")/../lib/libz.a" ] || die "static zlib not found (pacman -S mingw-w64-x86_64-zlib)"

# ---- the components the product uses (everything else is disabled) ----
# The same lists as build-macos.sh / build-linux.sh; Media Foundation encoders in place of VideoToolbox/VAAPI.
# Keep in step with the other two (the decoders follow build-linux.sh: it adds vp9).
ENCODERS="h264_mf,hevc_mf,aac,libvpx_vp9,libopus,gif,ffv1,mjpeg,png,pcm_f32le,pcm_s16le,pcm_s24le,rawvideo,wrapped_avframe"
DECODERS="h264,hevc,vp9,aac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8,png,mjpeg,ffv1,gif,rawvideo"
MUXERS="mp4,mov,webm,matroska,gif,image2,rawvideo,wav,pcm_f32le,pcm_s16le,null"
DEMUXERS="mov,matroska,wav,image2,rawvideo,gif,pcm_f32le,pcm_s16le"
PARSERS="h264,hevc,aac,mjpeg,png,opus,vp9,ffv1,gif"
BSFS="aac_adtstoasc,h264_mp4toannexb,hevc_mp4toannexb,extract_extradata,null,vp9_superframe,vp9_superframe_split"
PROTOCOLS="file,pipe"
FILTERS="alphamerge,aformat,aresample,atempo,atrim,adelay,amix,anull,apad,alimiter,asetpts,asplit,crop,format,fps,hflip,null,overlay,paletteuse,palettegen,scale,setpts,setsar,split,tpad,trim,volume,pan,loudnorm,ebur128,concat,anullsrc,anullsink"

REQUIRED_ENCODERS="h264_mf aac libvpx-vp9 libopus gif ffv1 mjpeg pcm_f32le pcm_s24le rawvideo"
REQUIRED_DECODERS="h264 hevc vp9 aac pcm_s16le pcm_f32le png mjpeg ffv1 rawvideo"
REQUIRED_MUXERS="mp4 webm matroska gif image2 rawvideo wav f32le null"
REQUIRED_DEMUXERS="mov matroska wav image2 rawvideo f32le gif"
REQUIRED_FILTERS="alphamerge aformat aresample atempo atrim adelay amix apad alimiter asetpts crop format fps hflip overlay paletteuse palettegen scale setpts setsar split tpad trim loudnorm ebur128 concat anullsrc anullsink"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src_cache="${CM_FFMPEG_SRC_DIR:-$root/packaging/ffmpeg/.downloads}"
out="${CM_FFMPEG_OUT:-$root/packaging/ffmpeg/out-windows-$ARCH}"
jobs="${CM_FFMPEG_JOBS:-$(nproc)}"
SOURCE_REPO="${CM_SOURCE_REPO:-https://github.com/craft-mann/capturemann}"
keep_work=1
if [ -n "${CM_FFMPEG_BUILD_DIR:-}" ]; then work="$CM_FFMPEG_BUILD_DIR"; else work="$(mktemp -d "${TMPDIR:-/tmp}/cm-ffmpeg-build.XXXXXX")"; keep_work=0; fi
mkdir -p "$work" "$src_cache"
work="$(cd "$work" && pwd)"; src_cache="$(cd "$src_cache" && pwd)"
cleanup() { [ "$keep_work" = "1" ] || rm -rf "$work"; }
trap cleanup EXIT
prefix="$work/prefix"; stage="$work/ffmpeg-stage"; mkdir -p "$prefix" "$stage"

fetch() { # fetch <file name> <url> <sha256>
  local name="$1" url="$2" sha="$3" path="$src_cache/$1" got
  if [ -f "$path" ]; then
    got="$(sha256sum "$path" | awk '{print $1}')"
    [ "$got" = "$sha" ] || { echo "cached $name has the wrong checksum; downloading again" >&2; rm -f "$path"; }
  fi
  if [ ! -f "$path" ]; then
    echo "==> downloading $url"
    curl -fsSL --retry 5 --retry-all-errors -o "$path.part" "$url" || die "download failed: $url"
    mv "$path.part" "$path"
  fi
  got="$(sha256sum "$path" | awk '{print $1}')"
  [ "$got" = "$sha" ] || die "checksum mismatch for $name: expected $sha, got $got (nothing was built from it)"
  echo "    $name sha256 ok"
}
unpack() { mkdir -p "$work/src"; tar -xf "$src_cache/$1" -C "$work/src"; }

echo "==> fetching and checking the pinned sources"
fetch "ffmpeg-${FFMPEG_VERSION}.tar.xz" "$FFMPEG_URL" "$FFMPEG_SHA256"
fetch "libvpx-${LIBVPX_VERSION}.tar.gz" "$LIBVPX_URL" "$LIBVPX_SHA256"
fetch "opus-${OPUS_VERSION}.tar.gz" "$OPUS_URL" "$OPUS_SHA256"
unpack "ffmpeg-${FFMPEG_VERSION}.tar.xz"; unpack "libvpx-${LIBVPX_VERSION}.tar.gz"; unpack "opus-${OPUS_VERSION}.tar.gz"

cflags="-O2"

echo "==> libopus $OPUS_VERSION (static)"
(cd "$work/src/opus-${OPUS_VERSION}" \
  && CFLAGS="$cflags" ./configure --prefix="$prefix" --disable-shared --enable-static \
       --disable-doc --disable-extra-programs --disable-dependency-tracking >"$work/opus.log" 2>&1 \
  && make -j"$jobs" >>"$work/opus.log" 2>&1 && make install >>"$work/opus.log" 2>&1) \
  || { tail -20 "$work/opus.log" >&2; die "libopus build failed (log: $work/opus.log)"; }

echo "==> libvpx $LIBVPX_VERSION (static, VP9 only)"
(cd "$work/src/libvpx-${LIBVPX_VERSION}" \
  && ./configure --prefix="$prefix" --target="$VPX_TARGET" \
       --disable-shared --enable-static \
       --enable-vp9 --disable-vp8 \
       --disable-examples --disable-tools --disable-docs --disable-unit-tests \
       --disable-install-docs --disable-install-bins >"$work/libvpx.log" 2>&1 \
  && make -j"$jobs" >>"$work/libvpx.log" 2>&1 && make install >>"$work/libvpx.log" 2>&1) \
  || { tail -20 "$work/libvpx.log" >&2; die "libvpx build failed (log: $work/libvpx.log)"; }

echo "==> FFmpeg $FFMPEG_VERSION (Media Foundation)"
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
configure_args=(
  --prefix="$stage"
  --cc=gcc
  --arch="$ARCH"
  --target-os=mingw32
  --pkg-config-flags=--static
  --extra-cflags="-I$prefix/include"
  --extra-ldflags="-L$prefix/lib -static"
  --enable-static --disable-shared
  --disable-gpl --disable-nonfree --disable-version3
  --disable-autodetect
  --disable-doc --disable-debug --disable-ffplay --disable-avdevice
  --disable-network
  --enable-ffmpeg --enable-ffprobe
  --enable-zlib
  --enable-mediafoundation
  --enable-libvpx --enable-libopus
  --disable-everything
  --enable-encoder="$ENCODERS"
  --enable-decoder="$DECODERS"
  --enable-muxer="$MUXERS"
  --enable-demuxer="$DEMUXERS"
  --enable-parser="$PARSERS"
  --enable-bsf="$BSFS"
  --enable-protocol="$PROTOCOLS"
  --enable-filter="$FILTERS"
)
check_components() { # check_components <kind> <comma list>   (configure silently ignores an unknown name)
  local kind="$1" list="$2" known name bad=""
  [ -n "$list" ] || return 0
  known="$(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" && ./configure --list-${kind}s 2>/dev/null | tr -s ' \t' '\n')"
  for name in ${list//,/ }; do grep -q -x -e "$name" <<<"$known" || bad="$bad $name"; done
  [ -z "$bad" ] || die "unknown FFmpeg $kind name(s) in build-windows.sh:$bad"
}
check_components encoder "$ENCODERS"; check_components decoder "$DECODERS"
check_components muxer "$MUXERS"; check_components demuxer "$DEMUXERS"; check_components parser "$PARSERS"
check_components bsf "$BSFS"; check_components protocol "$PROTOCOLS"; check_components filter "$FILTERS"
(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" && ./configure "${configure_args[@]}" >"$work/ffmpeg-configure.log" 2>&1) \
  || { tail -30 "$work/ffmpeg-configure.log" >&2; die "FFmpeg configure failed (log: $work/ffmpeg-configure.log)"; }
(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" && make -j"$jobs" >"$work/ffmpeg-make.log" 2>&1 && make install >>"$work/ffmpeg-make.log" 2>&1) \
  || { tail -30 "$work/ffmpeg-make.log" >&2; die "FFmpeg build failed (log: $work/ffmpeg-make.log)"; }

ff="$stage/bin/ffmpeg.exe"; fp="$stage/bin/ffprobe.exe"
[ -f "$ff" ] && [ -f "$fp" ] || die "the build did not produce ffmpeg.exe and ffprobe.exe"
strip "$ff" "$fp"

echo "==> self-check"
conf="$("$ff" -hide_banner -buildconf 2>&1)"
head="$("$ff" -hide_banner -version 2>&1 | head -3)"
if grep -q -E -e '--enable-(gpl|nonfree|version3|libx264|libx265|libfdk-aac)' <<<"$conf"; then
  die "the finished build is not LGPL v2.1: $(grep -E -e '--enable-(gpl|nonfree|version3|libx264|libx265|libfdk-aac)' <<<"$conf" | tr -d ' ' | tr '\n' ' ')"
fi
if "$ff" -hide_banner -L 2>&1 | grep -q -i "GNU General Public"; then die "ffmpeg -L reports the GPL"; fi
"$ff" -hide_banner -L 2>&1 | head -3 | sed 's/^/    /'
encoders="$("$ff" -hide_banner -encoders 2>&1)"; decoders="$("$ff" -hide_banner -decoders 2>&1)"
filters="$("$ff" -hide_banner -filters 2>&1)"; muxers="$("$ff" -hide_banner -muxers 2>&1)"
demuxers="$("$ff" -hide_banner -demuxers 2>&1)"
missing=""
for e in $REQUIRED_ENCODERS; do grep -q -E " $e( |$)" <<<"$encoders" || missing="$missing encoder:$e"; done
for d in $REQUIRED_DECODERS; do grep -q -E " $d( |$)" <<<"$decoders" || missing="$missing decoder:$d"; done
for m in $REQUIRED_MUXERS; do grep -q -E " $m( |,|$)" <<<"$muxers" || missing="$missing muxer:$m"; done
for m in $REQUIRED_DEMUXERS; do grep -q -E " $m( |,|$)" <<<"$demuxers" || missing="$missing demuxer:$m"; done
for f in $REQUIRED_FILTERS; do grep -q -E " $f( |$)" <<<"$filters" || missing="$missing filter:$f"; done
[ -z "$missing" ] || die "the finished build lacks:$missing"
# Only Windows' own DLLs may be loaded: no libwinpthread, libgcc, libstdc++ or libz (all linked statically).
for bin in "$ff" "$fp"; do
  dlls="$(objdump -p "$bin" | awk '/DLL Name:/ {print tolower($3)}')"
  bad="$(grep -v -E '^(kernel32|user32|advapi32|ole32|oleaut32|shell32|shlwapi|bcrypt|ntdll|ucrtbase|msvcrt|secur32|crypt32|gdi32|mfplat|mf|mfreadwrite|mfuuid|propsys|api-ms-win-[a-z0-9-]+)\.dll$' <<<"$dlls" || true)"
  [ -z "$bad" ] || die "$(basename "$bin") loads a DLL that is not a Windows component: $(tr '\n' ' ' <<<"$bad")"
done
objdump -f "$ff" | grep -q "pei-x86-64" || die "ffmpeg.exe is not x86-64"
echo "    LGPL configuration, required components present, Windows DLLs only, $ARCH"

echo "==> installing into $out"
rm -rf "$out"; mkdir -p "$out/LICENSES"
cp "$ff" "$out/ffmpeg.exe"; cp "$fp" "$out/ffprobe.exe"
cp "$work/src/ffmpeg-${FFMPEG_VERSION}/COPYING.LGPLv2.1" "$out/LICENSES/FFmpeg-COPYING.LGPLv2.1.txt"
cp "$work/src/ffmpeg-${FFMPEG_VERSION}/LICENSE.md" "$out/LICENSES/FFmpeg-LICENSE.md"
cp "$work/src/libvpx-${LIBVPX_VERSION}/LICENSE" "$out/LICENSES/libvpx-LICENSE.txt"
cp "$work/src/libvpx-${LIBVPX_VERSION}/PATENTS" "$out/LICENSES/libvpx-PATENTS.txt"
cp "$work/src/opus-${OPUS_VERSION}/COPYING" "$out/LICENSES/libopus-COPYING.txt"
# zlib is linked statically from the MSYS2 package; its licence text is installed with the package.
mingw_prefix="$(dirname "$(dirname "$(command -v gcc)")")"
zlib_license=""
for c in "$mingw_prefix/share/licenses/zlib/LICENSE" "$mingw_prefix/share/licenses/mingw-w64-zlib/LICENSE"; do
  [ -f "$c" ] && { zlib_license="$c"; break; }
done
if [ -n "$zlib_license" ]; then cp "$zlib_license" "$out/LICENSES/zlib-LICENSE.txt"
else echo "build-windows.sh: warning: no zlib licence file in the MSYS2 package; write LICENSES/zlib-LICENSE.txt by hand before distributing" >&2
fi
cmd_line="./configure ${configure_args[*]}"; cmd_line="${cmd_line//$work/<build-dir>}"
dll_list="$(objdump -p "$out/ffmpeg.exe" | awk '/DLL Name:/ {print "  " $3}')"
cat >"$out/BUILD-INFO.txt" <<EOF
CaptureMann bundled FFmpeg (Windows): build record
Built by packaging/ffmpeg/build-windows.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
Host: $(uname -srm), $(gcc --version | head -1)
Architecture: $ARCH only. Statically linked: libvpx, libopus, zlib, the mingw-w64 runtime.

Sources (official sites, SHA-256 checked by the script before use):
  FFmpeg   $FFMPEG_VERSION  $FFMPEG_URL
           sha256 $FFMPEG_SHA256
  libvpx   $LIBVPX_VERSION  $LIBVPX_URL
           sha256 $LIBVPX_SHA256
  libopus  $OPUS_VERSION  $OPUS_URL
           sha256 $OPUS_SHA256
  zlib     from the MSYS2 package mingw-w64-x86_64-zlib (static libz.a)

Configure line:
  $cmd_line

libopus: ./configure --prefix=<scratch> --disable-shared --enable-static --disable-doc --disable-extra-programs
libvpx:  ./configure --prefix=<scratch> --target=$VPX_TARGET --disable-shared --enable-static --enable-vp9 --disable-vp8 --disable-examples --disable-tools --disable-docs --disable-unit-tests

ffmpeg -version:
$head

ffmpeg -buildconf (as reported by the binary):
$(printf '%s\n' "$conf" | sed 's/^/  /')

DLLs loaded by ffmpeg.exe (objdump -p):
$dll_list

Sizes: ffmpeg.exe $(stat -c %s "$out/ffmpeg.exe") bytes, ffprobe.exe $(stat -c %s "$out/ffprobe.exe") bytes
EOF
cat >"$out/SOURCE-OFFER.txt" <<EOF
FFmpeg, libvpx and libopus in CaptureMann: source offer
========================================================

CaptureMann ships FFmpeg $FFMPEG_VERSION as two separate executables, "ffmpeg.exe" and "ffprobe.exe" (next to
cm-desktop.exe), and does not link to them: the app starts them as ordinary programs. FFmpeg is licensed under
the GNU Lesser General Public License, version 2.1 or later (LGPL-2.1+; the text is in
LICENSES/FFmpeg-COPYING.LGPLv2.1.txt). The build is configured without --enable-gpl, --enable-nonfree and
--enable-version3.

Statically linked into those executables:
  libvpx $LIBVPX_VERSION   BSD-3-Clause with the WebM patent grant   LICENSES/libvpx-LICENSE.txt, libvpx-PATENTS.txt
  libopus $OPUS_VERSION    BSD-3-Clause                              LICENSES/libopus-COPYING.txt
  zlib                     zlib licence                              LICENSES/zlib-LICENSE.txt
  the mingw-w64 C runtime  public domain / ZPL / MIT parts (https://www.mingw-w64.org/)
H.264 and HEVC video are encoded through Windows Media Foundation, a part of Windows; no H.264 encoder is
built into FFmpeg here.

Corresponding source (unmodified upstream releases; the exact files and their SHA-256):
  FFmpeg   $FFMPEG_URL
           sha256 $FFMPEG_SHA256
  libvpx   $LIBVPX_URL
           sha256 $LIBVPX_SHA256
  libopus  $OPUS_URL
           sha256 $OPUS_SHA256

How it was built: the script packaging/ffmpeg/build-windows.sh in the CaptureMann source repository
($SOURCE_REPO), which pins those three files, builds libopus and libvpx as static libraries with MSYS2 MINGW64
and configures FFmpeg with:

  $cmd_line

You can rebuild and replace the FFmpeg that CaptureMann uses: build your own (a modified FFmpeg is fine under the
LGPL), then either replace ffmpeg.exe and ffprobe.exe in the CaptureMann folder, or leave the folder alone and set
the environment variable CM_FFMPEG to your ffmpeg.exe (or to the folder that holds ffmpeg.exe and ffprobe.exe)
before starting CaptureMann or the capturemann command. The app looks for a replacement in that order: CM_FFMPEG,
then the copy next to cm-desktop.exe, then PATH.

If a link above no longer works, ask for the source at $SOURCE_REPO (open an issue quoting the FFmpeg version
above); the three source archives are kept with each CaptureMann release.
EOF
echo "built $out"
ls -l "$out"
