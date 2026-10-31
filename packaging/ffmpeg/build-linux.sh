#!/usr/bin/env bash
# Builds the LGPL FFmpeg for the Linux tarball (NFR-12, NFR-15): `ffmpeg` and `ffprobe`, with libvpx
# (VP9) and libopus (Opus) linked statically for WebM, and nothing GPL. Same pinned sources, same
# component lists and the same self-check as build-macos.sh, for the host's own architecture
# (x86_64 or aarch64; build each on its own machine, no cross-build).
#
# Usage:   packaging/ffmpeg/build-linux.sh
# Output:  packaging/ffmpeg/out-linux-<arch>/{ffmpeg,ffprobe,BUILD-INFO.txt,SOURCE-OFFER.txt,LICENSES/}
#          (git-ignored). Give that folder to packaging/linux/build-linux.sh as CM_FFMPEG_BUNDLE.
#
# Environment: CM_FFMPEG_SRC_DIR (tarball cache, default packaging/ffmpeg/.downloads), CM_FFMPEG_BUILD_DIR
# (keep the scratch tree), CM_FFMPEG_OUT, CM_FFMPEG_JOBS (default: CPUs), CM_SOURCE_REPO,
# CM_FFMPEG_VAAPI=auto|1|0 (default auto: on when libva's development files are found).
#
# Needs: gcc, make, perl, pkg-config, curl, xz, sha256sum, and nasm on x86_64
#   sudo apt install build-essential pkg-config nasm curl xz-utils libva-dev
# (libva-dev only for VAAPI; nasm only on x86_64).
#
# H.264 on Linux: there is no libx264 (GPL) and no software H.264 encoder ships (OpenH264 was removed
# for the patent exposure of a source build, see CLAUDE.md). The only H.264 and HEVC encoders in this
# build are VAAPI (Intel and AMD GPUs through the system's libva and driver). Without VAAPI the build
# still exports WebM (VP9 + Opus), GIF and audio; MP4 export needs an FFmpeg with an H.264 encoder on
# PATH (or CM_FFMPEG). That is the known gap of the first Linux builds (packaging/linux/README.md).
set -euo pipefail
die() { echo "build-linux.sh: $*" >&2; exit 1; }

FFMPEG_VERSION="9.0.2"
FFMPEG_URL="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
FFMPEG_SHA256="8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"
LIBVPX_VERSION="1.17.0"
LIBVPX_URL="https://github.com/webmproject/libvpx/archive/refs/tags/v${LIBVPX_VERSION}.tar.gz"
LIBVPX_SHA256="1020f184046187baa2985dbde38e0691f49c44088bca7a1842b0236c6081dc0a"
OPUS_VERSION="1.6.1"
OPUS_URL="https://downloads.xiph.org/releases/opus/opus-${OPUS_VERSION}.tar.gz"
OPUS_SHA256="6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1"

[ "$(uname -s)" = "Linux" ] || die "this script only runs on Linux"
case "$(uname -m)" in
  x86_64)  ARCH=x86_64;  VPX_TARGET="x86_64-linux-gcc" ;;
  aarch64) ARCH=aarch64; VPX_TARGET="arm64-linux-gcc" ;;
  *) die "unsupported machine $(uname -m)" ;;
esac
tools_needed="gcc make perl pkg-config curl xz sha256sum tar objdump strip"
[ "$ARCH" != "x86_64" ] || tools_needed="$tools_needed nasm"
for t in $tools_needed; do command -v "$t" >/dev/null 2>&1 || die "required tool not found: $t (sudo apt install build-essential pkg-config nasm curl xz-utils)"; done

vaapi="${CM_FFMPEG_VAAPI:-auto}"
if [ "$vaapi" = "auto" ]; then
  if pkg-config --exists libva libva-drm 2>/dev/null; then vaapi=1; else vaapi=0; fi
fi
if [ "$vaapi" = "1" ]; then pkg-config --exists libva libva-drm || die "CM_FFMPEG_VAAPI=1 but libva-dev is not installed"; fi

# Same lists as build-macos.sh, with VAAPI in place of VideoToolbox.
ENCODERS="aac,libvpx_vp9,libopus,gif,ffv1,mjpeg,png,pcm_f32le,pcm_s16le,pcm_s24le,rawvideo,wrapped_avframe"
DECODERS="h264,hevc,vp9,aac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8,png,mjpeg,ffv1,gif,rawvideo"
HWACCELS=""
if [ "$vaapi" = "1" ]; then
  ENCODERS="$ENCODERS,h264_vaapi,hevc_vaapi"
  HWACCELS="h264_vaapi,hevc_vaapi"
fi
MUXERS="mp4,mov,webm,matroska,gif,image2,rawvideo,wav,pcm_f32le,pcm_s16le,null"
DEMUXERS="mov,matroska,wav,image2,rawvideo,gif,pcm_f32le,pcm_s16le"
PARSERS="h264,hevc,aac,mjpeg,png,opus,vp9,ffv1,gif"
BSFS="aac_adtstoasc,h264_mp4toannexb,hevc_mp4toannexb,extract_extradata,null,vp9_superframe,vp9_superframe_split"
PROTOCOLS="file,pipe"
FILTERS="alphamerge,aformat,aresample,atempo,atrim,adelay,amix,anull,apad,alimiter,asetpts,asplit,crop,format,fps,hflip,null,overlay,paletteuse,palettegen,scale,setpts,setsar,split,tpad,trim,volume,pan,loudnorm,ebur128,concat,anullsrc,anullsink"
# VAAPI upload/convert filters when VAAPI is on (frames go to the GPU for the encoder).
[ "$vaapi" != "1" ] || FILTERS="$FILTERS,hwupload,scale_vaapi,format"

REQUIRED_ENCODERS="aac libvpx-vp9 libopus gif ffv1 mjpeg pcm_f32le pcm_s24le rawvideo"
[ "$vaapi" != "1" ] || REQUIRED_ENCODERS="$REQUIRED_ENCODERS h264_vaapi"
REQUIRED_DECODERS="h264 hevc vp9 aac pcm_s16le pcm_f32le png mjpeg ffv1 rawvideo"
REQUIRED_MUXERS="mp4 webm matroska gif image2 rawvideo wav f32le null"
REQUIRED_DEMUXERS="mov matroska wav image2 rawvideo f32le gif"
REQUIRED_FILTERS="alphamerge aformat aresample atempo atrim adelay amix apad alimiter asetpts crop format fps hflip overlay paletteuse palettegen scale setpts setsar split tpad trim loudnorm ebur128 concat anullsrc anullsink"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src_cache="${CM_FFMPEG_SRC_DIR:-$root/packaging/ffmpeg/.downloads}"
out="${CM_FFMPEG_OUT:-$root/packaging/ffmpeg/out-linux-$ARCH}"
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

cflags="-O2 -fPIC"

echo "==> libopus $OPUS_VERSION (static)"
(cd "$work/src/opus-${OPUS_VERSION}" \
  && CFLAGS="$cflags" ./configure --prefix="$prefix" --disable-shared --enable-static \
       --disable-doc --disable-extra-programs --disable-dependency-tracking >"$work/opus.log" 2>&1 \
  && make -j"$jobs" >>"$work/opus.log" 2>&1 && make install >>"$work/opus.log" 2>&1) \
  || { tail -20 "$work/opus.log" >&2; die "libopus build failed (log: $work/opus.log)"; }

echo "==> libvpx $LIBVPX_VERSION (static, VP9 only)"
(cd "$work/src/libvpx-${LIBVPX_VERSION}" \
  && ./configure --prefix="$prefix" --target="$VPX_TARGET" \
       --disable-shared --enable-static --enable-pic \
       --enable-vp9 --disable-vp8 \
       --disable-examples --disable-tools --disable-docs --disable-unit-tests \
       --disable-install-docs --disable-install-bins >"$work/libvpx.log" 2>&1 \
  && make -j"$jobs" >>"$work/libvpx.log" 2>&1 && make install >>"$work/libvpx.log" 2>&1) \
  || { tail -20 "$work/libvpx.log" >&2; die "libvpx build failed (log: $work/libvpx.log)"; }

echo "==> FFmpeg $FFMPEG_VERSION (VAAPI: $([ "$vaapi" = 1 ] && echo on || echo off))"
# Find libopus and libvpx in our prefix first; libva (if on) from the system.
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
configure_args=(
  --prefix="$stage"
  --cc=gcc
  --pkg-config-flags=--static
  --extra-cflags="-I$prefix/include"
  --extra-ldflags="-L$prefix/lib"
  --enable-static --disable-shared
  --disable-gpl --disable-nonfree --disable-version3
  --disable-autodetect
  --disable-doc --disable-debug --disable-ffplay --disable-avdevice
  --disable-network
  --enable-ffmpeg --enable-ffprobe
  --enable-zlib
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
[ -z "$HWACCELS" ] || configure_args+=(--enable-hwaccel="$HWACCELS")
[ "$vaapi" != "1" ] || configure_args+=(--enable-vaapi)
check_components() { # check_components <kind> <comma list>
  local kind="$1" list="$2" known name bad=""
  [ -n "$list" ] || return 0
  known="$(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" && ./configure --list-${kind}s 2>/dev/null | tr -s ' \t' '\n')"
  for name in ${list//,/ }; do grep -q -x -e "$name" <<<"$known" || bad="$bad $name"; done
  [ -z "$bad" ] || die "unknown FFmpeg $kind name(s) in build-linux.sh:$bad"
}
check_components encoder "$ENCODERS"; check_components decoder "$DECODERS"; check_components hwaccel "$HWACCELS"
check_components muxer "$MUXERS"; check_components demuxer "$DEMUXERS"; check_components parser "$PARSERS"
check_components bsf "$BSFS"; check_components protocol "$PROTOCOLS"; check_components filter "$FILTERS"
(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" && ./configure "${configure_args[@]}" >"$work/ffmpeg-configure.log" 2>&1) \
  || { tail -30 "$work/ffmpeg-configure.log" >&2; die "FFmpeg configure failed (log: $work/ffmpeg-configure.log)"; }
(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" && make -j"$jobs" >"$work/ffmpeg-make.log" 2>&1 && make install >>"$work/ffmpeg-make.log" 2>&1) \
  || { tail -30 "$work/ffmpeg-make.log" >&2; die "FFmpeg build failed (log: $work/ffmpeg-make.log)"; }

ff="$stage/bin/ffmpeg"; fp="$stage/bin/ffprobe"
[ -x "$ff" ] && [ -x "$fp" ] || die "the build did not produce ffmpeg and ffprobe"
strip "$ff" "$fp"

echo "==> self-check"
conf="$("$ff" -hide_banner -buildconf 2>&1)"
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
# Only the C library, zlib and (with VAAPI) libva may be loaded.
for bin in "$ff" "$fp"; do
  bad="$(objdump -p "$bin" | awk '/NEEDED/ {print $2}' | grep -v -E '^(libc\.so|libm\.so|libdl\.so|libpthread\.so|librt\.so|libz\.so|ld-linux|libva\.so|libva-drm\.so)' || true)"
  [ -z "$bad" ] || die "$(basename "$bin") links an unexpected library: $bad"
done
objdump -f "$ff" | grep -q -E "file format elf64-($([ "$ARCH" = x86_64 ] && echo x86-64 || echo littleaarch64))" || die "ffmpeg is not $ARCH"
echo "    LGPL configuration, required components present, expected libraries only, $ARCH"

echo "==> installing into $out"
rm -rf "$out"; mkdir -p "$out/LICENSES"
cp "$ff" "$out/ffmpeg"; cp "$fp" "$out/ffprobe"
cp "$work/src/ffmpeg-${FFMPEG_VERSION}/COPYING.LGPLv2.1" "$out/LICENSES/FFmpeg-COPYING.LGPLv2.1.txt"
cp "$work/src/ffmpeg-${FFMPEG_VERSION}/LICENSE.md" "$out/LICENSES/FFmpeg-LICENSE.md"
cp "$work/src/libvpx-${LIBVPX_VERSION}/LICENSE" "$out/LICENSES/libvpx-LICENSE.txt"
cp "$work/src/libvpx-${LIBVPX_VERSION}/PATENTS" "$out/LICENSES/libvpx-PATENTS.txt"
cp "$work/src/opus-${OPUS_VERSION}/COPYING" "$out/LICENSES/libopus-COPYING.txt"
cmd_line="./configure ${configure_args[*]}"; cmd_line="${cmd_line//$work/<build-dir>}"
cat >"$out/BUILD-INFO.txt" <<EOF
CaptureMann bundled FFmpeg (Linux): build record
Built by packaging/ffmpeg/build-linux.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
Host: $(uname -srm), $(. /etc/os-release && echo "$PRETTY_NAME"), $(gcc --version | head -1)
Architecture: $ARCH only. VAAPI: $([ "$vaapi" = 1 ] && echo "on (libva $(pkg-config --modversion libva))" || echo off)

Sources (official sites, SHA-256 checked by the script before use):
  FFmpeg   $FFMPEG_VERSION  $FFMPEG_URL
           sha256 $FFMPEG_SHA256
  libvpx   $LIBVPX_VERSION  $LIBVPX_URL
           sha256 $LIBVPX_SHA256
  libopus  $OPUS_VERSION  $OPUS_URL
           sha256 $OPUS_SHA256

Configure line:
  $cmd_line
EOF
cat >"$out/SOURCE-OFFER.txt" <<EOF
The ffmpeg and ffprobe programs in this package are FFmpeg $FFMPEG_VERSION, unmodified, configured with
--disable-gpl --disable-nonfree --disable-version3, so they are licensed under the GNU Lesser General Public
License version 2.1 or later (LICENSES/FFmpeg-COPYING.LGPLv2.1.txt). They are separate programs; CaptureMann
only starts them. libvpx (BSD-3-Clause with the WebM patent grant) and libopus (BSD-3-Clause) are linked statically.

Source: $FFMPEG_URL (sha256 $FFMPEG_SHA256),
$LIBVPX_URL, $OPUS_URL.
The exact configure line is in BUILD-INFO.txt and the build script is packaging/ffmpeg/build-linux.sh in
$SOURCE_REPO. To use your own FFmpeg instead, delete these two files or set CM_FFMPEG=/path/to/ffmpeg.
EOF
ls -l "$out"
