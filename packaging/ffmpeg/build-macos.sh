#!/usr/bin/env bash
# Builds the LGPL FFmpeg that ships inside CaptureMann.app (NFR-12, NFR-15): `ffmpeg` and
# `ffprobe`, statically linked with libvpx (VP9) and libopus (Opus) for WebM, VideoToolbox for
# H.264, and nothing else beyond the macOS system libraries.
#
# Usage (any directory):   packaging/ffmpeg/build-macos.sh      or   make ffmpeg
# Output: packaging/ffmpeg/out/{ffmpeg,ffprobe,BUILD-INFO.txt,SOURCE-OFFER.txt,LICENSES/}
#         (git-ignored; `make bundle` picks it up). With CM_ARCH=x86_64 the output is
#         packaging/ffmpeg/out-x86_64/ instead, beside the arm64 one.
#
# Environment:
#   CM_FFMPEG_SRC_DIR    where the source tarballs are cached (default packaging/ffmpeg/.downloads,
#                        git-ignored). Missing tarballs are downloaded from the official sites and
#                        checked against the SHA-256 values pinned below; a mismatch stops the build.
#   CM_FFMPEG_BUILD_DIR  scratch build directory (default: a new folder under $TMPDIR, removed at
#                        the end; set it to keep the build tree).
#   CM_FFMPEG_OUT        output directory (default packaging/ffmpeg/out, or out-x86_64 for x86_64).
#   CM_ARCH              arm64 (default on Apple Silicon) or x86_64 (the Intel build). The x86_64
#                        build is a cross-build on an Apple Silicon Mac: the binaries are Intel
#                        code and the self-check runs them under Rosetta (`arch -x86_64`).
#   CM_FFMPEG_JOBS       parallel jobs (default: the number of CPUs).
#   CM_SOURCE_REPO       the project URL written into SOURCE-OFFER.txt (default
#                        https://github.com/craft-mann/capturemann).
#
# What it needs on the build machine: Xcode command line tools (clang, make, perl), curl, xz,
# shasum. Nothing else: the extra build tools (pkgconf for FFmpeg's configure, and nasm for the x86
# assembly of libvpx and FFmpeg when CM_ARCH=x86_64) are built from pinned tarballs in the scratch
# directory and are not shipped. No network is needed after the
# tarballs are cached.
#
# Architecture: one per run, never universal (separate DMGs per chip, specs/plans/intel-mac-plan.md).
# Each output holds a single architecture; there is no `lipo`.
#
# Licence: configured with --disable-gpl --disable-nonfree --disable-version3, so the result is
# LGPL v2.1 or later. libvpx (BSD-3-Clause + patent grant) and libopus (BSD-3-Clause) are linked
# statically. No libx264, libx265, fdk-aac or other GPL/nonfree component is enabled. H.264 is
# encoded by Apple's VideoToolbox (the operating system), AAC by FFmpeg's own LGPL encoder.
set -euo pipefail

die() { echo "build-macos.sh: $*" >&2; exit 1; }

# ---- pinned sources (official sites; hashes cross-checked, see packaging/ffmpeg/README.md) ----
FFMPEG_VERSION="9.0.2"
FFMPEG_URL="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
FFMPEG_SHA256="8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"

LIBVPX_VERSION="1.17.0"
LIBVPX_URL="https://github.com/webmproject/libvpx/archive/refs/tags/v${LIBVPX_VERSION}.tar.gz"
LIBVPX_SHA256="1020f184046187baa2985dbde38e0691f49c44088bca7a1842b0236c6081dc0a"

OPUS_VERSION="1.6.1"
OPUS_URL="https://downloads.xiph.org/releases/opus/opus-${OPUS_VERSION}.tar.gz"
OPUS_SHA256="6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1"

# Build-time only, never shipped: FFmpeg's configure finds libopus through pkg-config.
PKGCONF_VERSION="2.5.1"
PKGCONF_URL="https://distfiles.ariadne.space/pkgconf/pkgconf-${PKGCONF_VERSION}.tar.xz"
PKGCONF_SHA256="cd05c9589b9f86ecf044c10a2269822bc9eb001eced2582cfffd658b0a50c243"

# Build-time only, never shipped: the x86 assembler for libvpx and FFmpeg (x86_64 builds only).
NASM_VERSION="2.16.03"
NASM_URL="https://www.nasm.us/pub/nasm/releasebuilds/${NASM_VERSION}/nasm-${NASM_VERSION}.tar.xz"
NASM_SHA256="1412a1c760bbd05db026b6c0d1657affd6631cd0a63cddb6f73cc6d4aa616148"

MIN_MACOS="13.0"
ARCH="${CM_ARCH:-$(uname -m)}"
case "$ARCH" in
  arm64)  VPX_TARGET="arm64-darwin22-gcc";  default_out="out" ;;
  x86_64) VPX_TARGET="x86_64-darwin22-gcc"; default_out="out-x86_64" ;;
  *) die "CM_ARCH must be arm64 or x86_64 (got $ARCH)" ;;
esac
# Running a binary of the target architecture: directly when it is the host's, else under Rosetta.
if [ "$ARCH" = "$(uname -m)" ]; then run_target=(); else run_target=(arch "-$ARCH"); fi
SOURCE_REPO="${CM_SOURCE_REPO:-https://github.com/craft-mann/capturemann}"

# ---- the components the product uses (everything else is disabled) ----
# Derived from every ffmpeg/ffprobe call in crates/cm-media (graph.rs, edited.rs, decode.rs,
# dsp_audio.rs), crates/cm-ai (audio.rs, suggest/silence.rs) and apps/desktop (poster frames,
# the preview source). Keep in step with those; `make ffmpeg` fails its own self-check below when
# a listed encoder or filter is missing, and the export tests exercise the rest.
# (The buffer, abuffer, buffersink, abuffersink, fifo and afifo filters are always built into
# libavfilter and are not selectable components.)
ENCODERS="h264_videotoolbox,hevc_videotoolbox,aac,libvpx_vp9,libopus,gif,ffv1,mjpeg,png,pcm_f32le,pcm_s16le,pcm_s24le,rawvideo,wrapped_avframe"
DECODERS="h264,hevc,aac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8,png,mjpeg,ffv1,gif,rawvideo"
HWACCELS="h264_videotoolbox,hevc_videotoolbox"
MUXERS="mp4,mov,webm,matroska,gif,image2,rawvideo,wav,pcm_f32le,pcm_s16le,null"
DEMUXERS="mov,matroska,wav,image2,rawvideo,gif,pcm_f32le,pcm_s16le"
PARSERS="h264,hevc,aac,mjpeg,png,opus,vp9,ffv1,gif"
BSFS="aac_adtstoasc,h264_mp4toannexb,hevc_mp4toannexb,extract_extradata,null,vp9_superframe,vp9_superframe_split"
PROTOCOLS="file,pipe"
FILTERS="alphamerge,aformat,aresample,atempo,atrim,adelay,amix,anull,apad,alimiter,asetpts,asplit,crop,format,fps,hflip,null,overlay,paletteuse,palettegen,scale,setpts,setsar,split,tpad,trim,volume,pan,loudnorm,ebur128,concat,anullsrc,anullsink"

# Encoders, decoders and filters the self-check requires in the finished binary.
REQUIRED_ENCODERS="h264_videotoolbox aac libvpx-vp9 libopus gif ffv1 mjpeg pcm_f32le pcm_s24le rawvideo"
REQUIRED_DECODERS="h264 hevc aac pcm_s16le pcm_f32le png mjpeg ffv1 rawvideo"
REQUIRED_MUXERS="mp4 webm matroska gif image2 rawvideo wav f32le null"
REQUIRED_DEMUXERS="mov matroska wav image2 rawvideo f32le gif"
REQUIRED_FILTERS="alphamerge aformat aresample atempo atrim adelay amix apad alimiter asetpts crop format fps hflip overlay paletteuse palettegen scale setpts setsar split tpad trim loudnorm ebur128 concat anullsrc anullsink"

[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
if [ "${#run_target[@]}" -gt 0 ]; then
  [ "$(uname -m)" = "arm64" ] || die "cross-building is only supported from an Apple Silicon Mac"
  "${run_target[@]}" /usr/bin/true 2>/dev/null || die "cannot run $ARCH programs here (the self-check needs Rosetta: softwareupdate --install-rosetta)"
fi
for tool in curl shasum tar xz make clang perl otool strip install_name_tool; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool (install the Xcode command line tools)"
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src_cache="${CM_FFMPEG_SRC_DIR:-$root/packaging/ffmpeg/.downloads}"
out="${CM_FFMPEG_OUT:-$root/packaging/ffmpeg/$default_out}"
jobs="${CM_FFMPEG_JOBS:-$(sysctl -n hw.ncpu)}"
keep_work=1
if [ -n "${CM_FFMPEG_BUILD_DIR:-}" ]; then
  work="$CM_FFMPEG_BUILD_DIR"
else
  work="$(mktemp -d "${TMPDIR:-/tmp}/cm-ffmpeg-build.XXXXXX")"
  keep_work=0
fi
mkdir -p "$work" "$src_cache"
work="$(cd "$work" && pwd)"
src_cache="$(cd "$src_cache" && pwd)"
cleanup() { [ "$keep_work" = "1" ] || rm -rf "$work"; }
trap cleanup EXIT

prefix="$work/prefix"      # static libopus and libvpx
tools="$work/tools"        # pkgconf
stage="$work/ffmpeg-stage" # the FFmpeg install tree
mkdir -p "$prefix" "$tools" "$stage"

fetch() { # fetch <file name> <url> <sha256>
  local name="$1" url="$2" sha="$3" path="$src_cache/$1" got
  if [ -f "$path" ]; then
    got="$(shasum -a 256 "$path" | awk '{print $1}')"
    if [ "$got" != "$sha" ]; then
      echo "cached $name has the wrong checksum; downloading again" >&2
      rm -f "$path"
    fi
  fi
  if [ ! -f "$path" ]; then
    echo "==> downloading $url"
    curl -fsSL --retry 3 -o "$path.part" "$url" || die "download failed: $url"
    mv "$path.part" "$path"
  fi
  got="$(shasum -a 256 "$path" | awk '{print $1}')"
  [ "$got" = "$sha" ] || die "checksum mismatch for $name: expected $sha, got $got (nothing was built from it)"
  echo "    $name sha256 ok"
}

unpack() { # unpack <tarball> -> prints the source directory
  local name="$1" dir="$work/src"
  mkdir -p "$dir"
  tar -xf "$src_cache/$name" -C "$dir"
}

echo "==> fetching and checking the pinned sources"
fetch "ffmpeg-${FFMPEG_VERSION}.tar.xz" "$FFMPEG_URL" "$FFMPEG_SHA256"
fetch "libvpx-${LIBVPX_VERSION}.tar.gz" "$LIBVPX_URL" "$LIBVPX_SHA256"
fetch "opus-${OPUS_VERSION}.tar.gz" "$OPUS_URL" "$OPUS_SHA256"
fetch "pkgconf-${PKGCONF_VERSION}.tar.xz" "$PKGCONF_URL" "$PKGCONF_SHA256"
[ "$ARCH" != "x86_64" ] || fetch "nasm-${NASM_VERSION}.tar.xz" "$NASM_URL" "$NASM_SHA256"
unpack "ffmpeg-${FFMPEG_VERSION}.tar.xz"
unpack "libvpx-${LIBVPX_VERSION}.tar.gz"
unpack "opus-${OPUS_VERSION}.tar.gz"
unpack "pkgconf-${PKGCONF_VERSION}.tar.xz"
[ "$ARCH" != "x86_64" ] || unpack "nasm-${NASM_VERSION}.tar.xz"

export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"
cflags="-O2 -arch $ARCH -mmacosx-version-min=$MIN_MACOS"
# A reproducible-ish environment: nothing from Homebrew or /usr/local is searched.
export PATH="$tools/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/Developer/CommandLineTools/usr/bin"
unset CPATH LIBRARY_PATH PKG_CONFIG_PATH DYLD_LIBRARY_PATH

echo "==> pkgconf $PKGCONF_VERSION (build tool, not shipped)"
(cd "$work/src/pkgconf-${PKGCONF_VERSION}" \
  && ./configure --prefix="$tools" --disable-dependency-tracking >"$work/pkgconf.log" 2>&1 \
  && make -j"$jobs" >>"$work/pkgconf.log" 2>&1 && make install >>"$work/pkgconf.log" 2>&1) \
  || { tail -20 "$work/pkgconf.log" >&2; die "pkgconf build failed (log: $work/pkgconf.log)"; }
ln -sf pkgconf "$tools/bin/pkg-config"

if [ "$ARCH" = "x86_64" ]; then
  echo "==> nasm $NASM_VERSION (build tool for the x86 assembly, not shipped; runs on this Mac)"
  (cd "$work/src/nasm-${NASM_VERSION}" \
    && ./configure --prefix="$tools" >"$work/nasm.log" 2>&1 \
    && make -j"$jobs" >>"$work/nasm.log" 2>&1 && make install >>"$work/nasm.log" 2>&1) \
    || { tail -20 "$work/nasm.log" >&2; die "nasm build failed (log: $work/nasm.log)"; }
fi

echo "==> libopus $OPUS_VERSION (static)"
(cd "$work/src/opus-${OPUS_VERSION}" \
  && CFLAGS="$cflags" ./configure --prefix="$prefix" --disable-shared --enable-static \
       --disable-doc --disable-extra-programs --disable-dependency-tracking >"$work/opus.log" 2>&1 \
  && make -j"$jobs" >>"$work/opus.log" 2>&1 && make install >>"$work/opus.log" 2>&1) \
  || { tail -20 "$work/opus.log" >&2; die "libopus build failed (log: $work/opus.log)"; }

echo "==> libvpx $LIBVPX_VERSION (static, VP9 only)"
# `<arch>-darwin22-gcc` = macOS 13 (Darwin 22), built with clang. VP8 is not used (WebM export is
# VP9 + Opus), so it is left out.
(cd "$work/src/libvpx-${LIBVPX_VERSION}" \
  && ./configure --prefix="$prefix" --target="$VPX_TARGET" \
       --disable-shared --enable-static --enable-pic \
       --enable-vp9 --disable-vp8 \
       --disable-examples --disable-tools --disable-docs --disable-unit-tests \
       --disable-install-docs --disable-install-bins \
       --extra-cflags="-mmacosx-version-min=$MIN_MACOS" >"$work/libvpx.log" 2>&1 \
  && make -j"$jobs" >>"$work/libvpx.log" 2>&1 && make install >>"$work/libvpx.log" 2>&1) \
  || { tail -20 "$work/libvpx.log" >&2; die "libvpx build failed (log: $work/libvpx.log)"; }

echo "==> FFmpeg $FFMPEG_VERSION"
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
configure_args=(
  --prefix="$stage"
  --cc=clang
  --arch="$ARCH"
  --target-os=darwin
  --pkg-config="$tools/bin/pkgconf"
  --pkg-config-flags=--static
  --extra-cflags="-arch $ARCH -mmacosx-version-min=$MIN_MACOS -I$prefix/include"
  --extra-ldflags="-arch $ARCH -mmacosx-version-min=$MIN_MACOS -L$prefix/lib"
  --enable-static --disable-shared
  --disable-gpl --disable-nonfree --disable-version3
  --disable-autodetect
  --disable-doc --disable-debug --disable-ffplay --disable-avdevice
  --disable-network
  --enable-ffmpeg --enable-ffprobe
  --enable-videotoolbox
  --enable-zlib
  --enable-libvpx --enable-libopus
  --disable-everything
  --enable-encoder="$ENCODERS"
  --enable-decoder="$DECODERS"
  --enable-hwaccel="$HWACCELS"
  --enable-muxer="$MUXERS"
  --enable-demuxer="$DEMUXERS"
  --enable-parser="$PARSERS"
  --enable-bsf="$BSFS"
  --enable-protocol="$PROTOCOLS"
  --enable-filter="$FILTERS"
)
# `configure` silently ignores a component name it does not know, so check every name first.
if [ "${#run_target[@]}" -gt 0 ]; then
  # Cross-compiling: configure must be told (it then does not try to run what it builds).
  configure_args+=(--enable-cross-compile)
fi
check_components() { # check_components <kind> <comma list>   (kind: encoder decoder hwaccel muxer demuxer parser bsf protocol filter)
  local kind="$1" list="$2" known name bad=""
  known="$(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" && ./configure --list-${kind}s 2>/dev/null | tr -s ' \t' '\n')"
  for name in ${list//,/ }; do
    grep -q -x -e "$name" <<<"$known" || bad="$bad $name"
  done
  [ -z "$bad" ] || die "unknown FFmpeg $kind name(s) in build-macos.sh:$bad"
}
check_components encoder "$ENCODERS"
check_components decoder "$DECODERS"
check_components hwaccel "$HWACCELS"
check_components muxer "$MUXERS"
check_components demuxer "$DEMUXERS"
check_components parser "$PARSERS"
check_components bsf "$BSFS"
check_components protocol "$PROTOCOLS"
check_components filter "$FILTERS"
(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" \
  && ./configure "${configure_args[@]}" >"$work/ffmpeg-configure.log" 2>&1) \
  || { tail -30 "$work/ffmpeg-configure.log" >&2; die "FFmpeg configure failed (log: $work/ffmpeg-configure.log)"; }
(cd "$work/src/ffmpeg-${FFMPEG_VERSION}" \
  && make -j"$jobs" >"$work/ffmpeg-make.log" 2>&1 && make install >>"$work/ffmpeg-make.log" 2>&1) \
  || { tail -30 "$work/ffmpeg-make.log" >&2; die "FFmpeg build failed (log: $work/ffmpeg-make.log)"; }

ff="$stage/bin/ffmpeg"
fp="$stage/bin/ffprobe"
[ -x "$ff" ] && [ -x "$fp" ] || die "the build did not produce ffmpeg and ffprobe"
strip -x "$ff" "$fp"

echo "==> self-check"
ffx() { ${run_target[@]+"${run_target[@]}"} "$ff" "$@"; }
conf="$(ffx -hide_banner -buildconf 2>&1)"
head="$(ffx -hide_banner -version 2>&1 | head -3)"
if grep -q -E -e '--enable-(gpl|nonfree|version3|libx264|libx265|libfdk-aac)' <<<"$conf"; then
  die "the finished build is not LGPL v2.1: $(grep -E -e '--enable-(gpl|nonfree|version3|libx264|libx265|libfdk-aac)' <<<"$conf" | tr -d ' ' | tr '\n' ' ')"
fi
if ffx -hide_banner -L 2>&1 | grep -q -i "GNU General Public"; then die "ffmpeg -L reports the GPL"; fi
ffx -hide_banner -L 2>&1 | head -3 | sed 's/^/    /'
encoders="$(ffx -hide_banner -encoders 2>&1)"
decoders="$(ffx -hide_banner -decoders 2>&1)"
filters="$(ffx -hide_banner -filters 2>&1)"
muxers="$(ffx -hide_banner -muxers 2>&1)"
demuxers="$(ffx -hide_banner -demuxers 2>&1)"
missing=""
for e in $REQUIRED_ENCODERS; do grep -q -E " $e( |$)" <<<"$encoders" || missing="$missing encoder:$e"; done
for d in $REQUIRED_DECODERS; do grep -q -E " $d( |$)" <<<"$decoders" || missing="$missing decoder:$d"; done
for m in $REQUIRED_MUXERS; do grep -q -E " $m( |,|$)" <<<"$muxers" || missing="$missing muxer:$m"; done
for m in $REQUIRED_DEMUXERS; do grep -q -E " $m( |,|$)" <<<"$demuxers" || missing="$missing demuxer:$m"; done
for f in $REQUIRED_FILTERS; do grep -q -E " $f( |$)" <<<"$filters" || missing="$missing filter:$f"; done
[ -z "$missing" ] || die "the finished build lacks:$missing"
# Only system libraries may be linked (no Homebrew, no /usr/local, no @rpath).
for bin in "$ff" "$fp"; do
  bad="$(otool -L "$bin" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/Library/)' || true)"
  [ -z "$bad" ] || die "$(basename "$bin") links a non-system library: $bad"
done
lipo -info "$ff" "$fp" | grep -v -q -E "architecture: $ARCH\$" && die "ffmpeg or ffprobe is not $ARCH only: $(lipo -info "$ff" "$fp")"
echo "    LGPL configuration, required components present, system libraries only, $ARCH only"

echo "==> installing into $out"
rm -rf "$out"
mkdir -p "$out/LICENSES"
cp "$ff" "$out/ffmpeg"
cp "$fp" "$out/ffprobe"
cp "$work/src/ffmpeg-${FFMPEG_VERSION}/COPYING.LGPLv2.1" "$out/LICENSES/FFmpeg-COPYING.LGPLv2.1.txt"
cp "$work/src/ffmpeg-${FFMPEG_VERSION}/LICENSE.md" "$out/LICENSES/FFmpeg-LICENSE.md"
cp "$work/src/libvpx-${LIBVPX_VERSION}/LICENSE" "$out/LICENSES/libvpx-LICENSE.txt"
cp "$work/src/libvpx-${LIBVPX_VERSION}/PATENTS" "$out/LICENSES/libvpx-PATENTS.txt"
cp "$work/src/opus-${OPUS_VERSION}/COPYING" "$out/LICENSES/libopus-COPYING.txt"

cmd_line="./configure ${configure_args[*]}"
# The recorded line is for readers: keep this machine's scratch path out of it.
cmd_line="${cmd_line//$work/<build-dir>}"
cat >"$out/BUILD-INFO.txt" <<EOF
CaptureMann bundled FFmpeg: build record
Built by packaging/ffmpeg/build-macos.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
Host: $(uname -srm), $(sw_vers -productName) $(sw_vers -productVersion), $(clang --version | head -1)
Minimum macOS: $MIN_MACOS, architecture: $ARCH only

Sources (official sites, SHA-256 checked by the script before use):
  FFmpeg   $FFMPEG_VERSION  $FFMPEG_URL
           sha256 $FFMPEG_SHA256
  libvpx   $LIBVPX_VERSION  $LIBVPX_URL
           sha256 $LIBVPX_SHA256
  libopus  $OPUS_VERSION  $OPUS_URL
           sha256 $OPUS_SHA256
Build-time only, not shipped:
  pkgconf  $PKGCONF_VERSION  $PKGCONF_URL
           sha256 $PKGCONF_SHA256
$([ "$ARCH" = "x86_64" ] && printf '  nasm     %s  %s\n           sha256 %s\n' "$NASM_VERSION" "$NASM_URL" "$NASM_SHA256")

FFmpeg configure line:
  $cmd_line

libopus: ./configure --prefix=<scratch> --disable-shared --enable-static --disable-doc --disable-extra-programs
libvpx:  ./configure --prefix=<scratch> --target=$VPX_TARGET --disable-shared --enable-static --enable-pic --enable-vp9 --disable-vp8 --disable-examples --disable-tools --disable-docs --disable-unit-tests

ffmpeg -version:
$head

ffmpeg -buildconf (as reported by the binary):
$(printf '%s\n' "$conf" | sed 's/^/  /')

Linked libraries (otool -L ffmpeg):
$(otool -L "$out/ffmpeg" | tail -n +2 | sed 's/^/  /')

Sizes: ffmpeg $(stat -f %z "$out/ffmpeg") bytes, ffprobe $(stat -f %z "$out/ffprobe") bytes
EOF

cat >"$out/SOURCE-OFFER.txt" <<EOF
FFmpeg, libvpx and libopus in CaptureMann: source offer
========================================================

CaptureMann ships FFmpeg $FFMPEG_VERSION as two separate executables, "ffmpeg" and "ffprobe"
(next to the app's own executables, in CaptureMann.app/Contents/MacOS), and does not link to
them: the app starts them as ordinary programs. FFmpeg is licensed under the GNU Lesser General
Public License, version 2.1 or later (LGPL-2.1+; the text is in LICENSES/FFmpeg-COPYING.LGPLv2.1.txt).
The build is configured without --enable-gpl, --enable-nonfree and --enable-version3.

Statically linked into those executables:
  libvpx $LIBVPX_VERSION   BSD-3-Clause with the WebM patent grant   LICENSES/libvpx-LICENSE.txt, libvpx-PATENTS.txt
  libopus $OPUS_VERSION    BSD-3-Clause                              LICENSES/libopus-COPYING.txt
H.264 video is encoded by Apple VideoToolbox, a part of macOS; no H.264 encoder is built into FFmpeg here.

Corresponding source (unmodified upstream releases; the exact files and their SHA-256):
  FFmpeg   $FFMPEG_URL
           sha256 $FFMPEG_SHA256
  libvpx   $LIBVPX_URL
           sha256 $LIBVPX_SHA256
  libopus  $OPUS_URL
           sha256 $OPUS_SHA256

How it was built: the script packaging/ffmpeg/build-macos.sh in the CaptureMann source
repository ($SOURCE_REPO), which pins those three files,
builds libopus and libvpx as static libraries and configures FFmpeg with:

  $cmd_line

You can rebuild and replace the FFmpeg that CaptureMann uses: build your own (a modified FFmpeg
is fine under the LGPL), then either replace Contents/MacOS/ffmpeg and ffprobe in the app, or
leave the app alone and set the environment variable CM_FFMPEG to your ffmpeg executable (or to
the folder that holds ffmpeg and ffprobe) before starting CaptureMann or the capturemann command.
The app looks for a replacement in that order: CM_FFMPEG, then the copy inside the app.

If a link above no longer works, ask for the source at $SOURCE_REPO (open an issue quoting the
FFmpeg version above); the three source archives are kept with each CaptureMann release.
EOF

echo "built $out"
ls -l "$out/ffmpeg" "$out/ffprobe" | awk '{print "  " $5 " bytes  " $9}'
