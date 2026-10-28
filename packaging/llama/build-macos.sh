#!/usr/bin/env bash
# Builds the text engine that ships inside CaptureMann.app (AI-7, provider "embedded"): llama.cpp's
# `llama-server` (MIT), statically linked, Metal on, nothing else beyond the macOS system libraries.
# It is a HELPER PROGRAM next to the app's other executables, never linked into the app: whisper.cpp
# (in cm-ai) and llama.cpp both embed ggml, and two copies in one process clash.
#
# Usage (any directory):   packaging/llama/build-macos.sh      or   make llama
# Output: packaging/llama/out/{llama-server,BUILD-INFO.txt,LICENSES/}
#         (git-ignored; `make bundle` picks it up). With CM_ARCH=x86_64 the output is
#         packaging/llama/out-x86_64/ instead, beside the arm64 one.
#
# THE MAINTAINER MUST PIN THE RELEASE BEFORE THE FIRST RUN: LLAMA_TAG and LLAMA_SHA256 below are
# placeholders and the script refuses to run while they are. Pick a release of
# https://github.com/ggml-org/llama.cpp/releases, take the source tarball of its tag
# (https://github.com/ggml-org/llama.cpp/archive/refs/tags/<tag>.tar.gz), compute `shasum -a 256`
# of it and write both here. Check the flags in `cmake_args` and the arguments built by
# `engine_args` in crates/cm-ai/src/text_model/engine/mod.rs against `llama-server --help` of that
# release: llama.cpp renames options between releases.
#
# Environment:
#   CM_LLAMA_SRC_DIR    where the source tarball is cached (default packaging/llama/.downloads,
#                       git-ignored). A missing tarball is downloaded and checked against the
#                       SHA-256 pinned below; a mismatch stops the build.
#   CM_LLAMA_BUILD_DIR  scratch build directory (default: a new folder under $TMPDIR, removed at
#                       the end; set it to keep the build tree).
#   CM_LLAMA_OUT        output directory (default packaging/llama/out, or out-x86_64 for x86_64).
#   CM_ARCH             arm64 (default on Apple Silicon) or x86_64 (the Intel build, cross-built on an
#                       Apple Silicon Mac; the self-check runs it under Rosetta).
#   CM_LLAMA_METAL      ON or OFF. Default ON for arm64, OFF for x86_64: until the Intel Mac has run
#                       the measurements (specs/plans/intel-mac-plan.md, I3.1) the Intel build is CPU
#                       only, because Metal on old Intel and AMD GPUs can be slower than the CPU or fail.
#   CM_LLAMA_JOBS       parallel jobs (default: the number of CPUs).
#   CM_SOURCE_REPO      the project URL written into BUILD-INFO.txt (default
#                       https://github.com/craft-mann/capturemann).
#
# What it needs on the build machine: Xcode command line tools (clang, make), cmake (`brew install
# cmake`, the same tool cm-ai's whisper.cpp build needs), curl, shasum, tar. No Python: the
# server's web page is a prebuilt file in the source tree and the app starts the server with it
# switched off. No network is needed after the tarball is cached.
#
# Architecture: one per run, never universal. The x86_64 CPU code is built for a fixed baseline
# (AVX2, FMA, F16C, BMI2: every Mac that runs macOS 13 has them), never `-march=native`, so the result
# does not depend on the Mac that built it.
#
# Licence: llama.cpp is MIT. Its source tree vendors a few small MIT/BSD libraries (cpp-httplib,
# nlohmann/json, stb_image, miniaudio); their licence files are copied from the tree's `licenses/`
# folder next to the main LICENSE. LLAMA_CURL and LLAMA_OPENSSL are off, so the server cannot
# download models or call out by itself (the app gives it a local GGUF file and nothing else), and
# the self-check fails on any non-system library.
set -euo pipefail

die() { echo "build-macos.sh: $*" >&2; exit 1; }

# ---- pinned source (the maintainer fills these in; see above) ----
LLAMA_TAG="v0.7.0" # latest release when pinned (published Oct 15, 2026; read from the GitHub releases API on Oct 27, 2026)
LLAMA_SHA256="55d67acf3ad2ad747a9bbeb75f9f0e173dbcc0d1753a471038a90c819421c5e1" # shasum -a 256 of the tag's source tarball (38183379 bytes), computed Oct 27, 2026
LLAMA_URL="https://github.com/ggml-org/llama.cpp/archive/refs/tags/${LLAMA_TAG}.tar.gz"

MIN_MACOS="13.0"
ARCH="${CM_ARCH:-$(uname -m)}"
case "$ARCH" in
  arm64)  default_out="out";        metal_default="ON" ;;
  x86_64) default_out="out-x86_64"; metal_default="OFF" ;;
  *) die "CM_ARCH must be arm64 or x86_64 (got $ARCH)" ;;
esac
METAL="${CM_LLAMA_METAL:-$metal_default}"
case "$METAL" in ON|OFF) ;; *) die "CM_LLAMA_METAL must be ON or OFF" ;; esac
if [ "$ARCH" = "$(uname -m)" ]; then run_target=(); else run_target=(arch "-$ARCH"); fi
SOURCE_REPO="${CM_SOURCE_REPO:-https://github.com/craft-mann/capturemann}"

if [ "$LLAMA_TAG" = "PENDING_PIN" ] || [ "$LLAMA_SHA256" = "PENDING_PIN" ]; then
  die "LLAMA_TAG and LLAMA_SHA256 are not pinned yet. Edit the top of this script (see its header), then run it again. Nothing was downloaded."
fi
case "$LLAMA_SHA256" in
  *[!0-9a-f]* | "") die "LLAMA_SHA256 must be 64 lowercase hex digits" ;;
esac
[ "${#LLAMA_SHA256}" = "64" ] || die "LLAMA_SHA256 must be 64 lowercase hex digits"
case "$LLAMA_TAG" in
  *[!A-Za-z0-9._-]* | "") die "LLAMA_TAG may only contain letters, digits, '.', '_' and '-'" ;;
esac

[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
if [ "${#run_target[@]}" -gt 0 ]; then
  [ "$(uname -m)" = "arm64" ] || die "cross-building is only supported from an Apple Silicon Mac"
  "${run_target[@]}" /usr/bin/true 2>/dev/null || die "cannot run $ARCH programs here (the self-check needs Rosetta: softwareupdate --install-rosetta)"
fi
for tool in curl shasum tar make clang cmake otool strip codesign; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool (install the Xcode command line tools; cmake: brew install cmake)"
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src_cache="${CM_LLAMA_SRC_DIR:-$root/packaging/llama/.downloads}"
out="${CM_LLAMA_OUT:-$root/packaging/llama/$default_out}"
jobs="${CM_LLAMA_JOBS:-$(sysctl -n hw.ncpu)}"
keep_work=1
if [ -n "${CM_LLAMA_BUILD_DIR:-}" ]; then
  work="$CM_LLAMA_BUILD_DIR"
else
  work="$(mktemp -d "${TMPDIR:-/tmp}/cm-llama-build.XXXXXX")"
  keep_work=0
fi
mkdir -p "$work" "$src_cache"
work="$(cd "$work" && pwd)"
src_cache="$(cd "$src_cache" && pwd)"
cleanup() { [ "$keep_work" = "1" ] || rm -rf "$work"; }
trap cleanup EXIT

tarball="llama.cpp-${LLAMA_TAG}.tar.gz"
path="$src_cache/$tarball"
if [ -f "$path" ]; then
  got="$(shasum -a 256 "$path" | awk '{print $1}')"
  if [ "$got" != "$LLAMA_SHA256" ]; then
    echo "cached $tarball has the wrong checksum; downloading again" >&2
    rm -f "$path"
  fi
fi
if [ ! -f "$path" ]; then
  echo "==> downloading $LLAMA_URL"
  curl -fsSL --retry 3 -o "$path.part" "$LLAMA_URL" || die "download failed: $LLAMA_URL"
  mv "$path.part" "$path"
fi
got="$(shasum -a 256 "$path" | awk '{print $1}')"
[ "$got" = "$LLAMA_SHA256" ] || die "checksum mismatch for $tarball: expected $LLAMA_SHA256, got $got (nothing was built from it)"
echo "    $tarball sha256 ok"

mkdir -p "$work/src"
tar -xf "$path" -C "$work/src"
srcdir="$(find "$work/src" -mindepth 1 -maxdepth 1 -type d | head -1)"
[ -n "$srcdir" ] && [ -f "$srcdir/CMakeLists.txt" ] || die "the tarball does not hold a llama.cpp source tree"

# A reproducible-ish environment: nothing from Homebrew's libraries is searched.
export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:/Library/Developer/CommandLineTools/usr/bin:$(dirname "$(command -v cmake)")"
unset CPATH LIBRARY_PATH PKG_CONFIG_PATH DYLD_LIBRARY_PATH

echo "==> llama-server $LLAMA_TAG ($ARCH, static, Metal $METAL)"
cmake_args=(
  -S "$srcdir" -B "$work/build"
  -DCMAKE_BUILD_TYPE=Release
  -DCMAKE_OSX_ARCHITECTURES="$ARCH"
  -DCMAKE_OSX_DEPLOYMENT_TARGET="$MIN_MACOS"
  -DBUILD_SHARED_LIBS=OFF
  -DGGML_METAL="$METAL"
  -DGGML_METAL_EMBED_LIBRARY="$METAL"
  -DGGML_ACCELERATE=ON
  -DGGML_NATIVE=OFF
  -DLLAMA_CURL=OFF
  -DLLAMA_OPENSSL=OFF
  -DLLAMA_BUILD_TESTS=OFF
  -DLLAMA_BUILD_EXAMPLES=OFF
  -DLLAMA_BUILD_TOOLS=ON
  -DLLAMA_BUILD_SERVER=ON
  # v0.7.0 downloads a prebuilt Web UI from Hugging Face at build time (network, unpinned) and fails
  # to embed it from a path under /var (a symlink). The app starts the server with --no-webui.
  -DLLAMA_USE_PREBUILT_UI=OFF
)
if [ "$ARCH" = "x86_64" ]; then
  cmake_args+=(-DGGML_AVX=ON -DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON -DGGML_BMI2=ON
               -DGGML_AVX512=OFF -DGGML_AVX_VNNI=OFF)
fi
cmake "${cmake_args[@]}" >"$work/cmake-configure.log" 2>&1 \
  || { tail -30 "$work/cmake-configure.log" >&2; die "cmake configure failed (log: $work/cmake-configure.log)"; }
cmake --build "$work/build" --config Release --target llama-server -j "$jobs" >"$work/cmake-build.log" 2>&1 \
  || { grep -n -E -B2 -A6 'error:|CMake Error|fatal error|FAILED:|No such file|undefined' "$work/cmake-build.log" | head -60 >&2
       tail -15 "$work/cmake-build.log" >&2
       cp "$work/cmake-build.log" "${TMPDIR:-/tmp}/cm-llama-build-failed.log" 2>/dev/null \
         && echo "the full log was kept in ${TMPDIR:-/tmp}/cm-llama-build-failed.log" >&2
       die "llama-server build failed (log: $work/cmake-build.log)"; }

bin="$(find "$work/build" -type f -name llama-server -perm -u+x | head -1)"
[ -n "$bin" ] && [ -x "$bin" ] || die "the build did not produce llama-server"
cp "$bin" "$work/llama-server"
bin="$work/llama-server"
strip -x "$bin"

echo "==> self-check"
# 1. The licence: MIT, with the files that travel with the binary.
[ -f "$srcdir/LICENSE" ] || die "the source tree has no LICENSE"
grep -q -i "MIT License" "$srcdir/LICENSE" || die "$srcdir/LICENSE is not the MIT licence: do not ship this build without a licence review"
# 2. Only system libraries may be linked (no Homebrew, no /usr/local, no @rpath, no libcurl, no OpenSSL).
bad="$(otool -L "$bin" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/Library/)' || true)"
[ -z "$bad" ] || die "llama-server links a non-system library: $bad"
if otool -L "$bin" | grep -q -i -E 'curl|ssl|crypto'; then die "llama-server links curl or OpenSSL: it must not be able to reach the network by itself"; fi
# 3. Metal is in when asked for (the shader library is embedded), and the binary starts.
if [ "$METAL" = "ON" ]; then
  otool -L "$bin" | grep -q "Metal.framework" || die "llama-server does not link Metal"
elif otool -L "$bin" | grep -q "Metal.framework"; then
  die "llama-server links Metal although CM_LLAMA_METAL=OFF"
fi
lipo -info "$bin" | grep -q -E "architecture: $ARCH\$" || die "llama-server is not $ARCH only: $(lipo -info "$bin")"
ver="$(${run_target[@]+"${run_target[@]}"} "$bin" --version 2>&1 | head -3 || true)"
[ -n "$ver" ] || die "llama-server did not print a version (cannot start?)"
echo "$ver" | sed 's/^/    /'
# 4. The options the app passes exist in this release.
help="$(${run_target[@]+"${run_target[@]}"} "$bin" --help 2>&1 || true)"
missing=""
for opt in --host --port --model --alias --ctx-size --n-gpu-layers --parallel --no-webui; do
  grep -q -e "$opt" <<<"$help" || missing="$missing $opt"
done
[ -z "$missing" ] || die "this release of llama-server lacks the option(s):$missing (update engine_args in crates/cm-ai/src/text_model/engine/mod.rs and this check together)"
echo "    MIT licence, system libraries only, $ARCH only, Metal $METAL, options present"

echo "==> installing into $out"
rm -rf "$out"
mkdir -p "$out/LICENSES"
cp "$bin" "$out/llama-server"
cp "$srcdir/LICENSE" "$out/LICENSES/llama.cpp-LICENSE.txt"
if [ -d "$srcdir/licenses" ]; then
  for f in "$srcdir"/licenses/*; do
    [ -f "$f" ] && cp "$f" "$out/LICENSES/vendored-$(basename "$f").txt"
  done
fi

cmake_line="cmake ${cmake_args[*]}"
cmake_line="${cmake_line//$work/<build-dir>}"
cat >"$out/BUILD-INFO.txt" <<EOF
CaptureMann bundled llama-server: build record
Built by packaging/llama/build-macos.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
Host: $(uname -srm), $(sw_vers -productName) $(sw_vers -productVersion), $(clang --version | head -1)
Minimum macOS: $MIN_MACOS, architecture: $ARCH only, Metal $METAL

Source (official site, SHA-256 checked by the script before use):
  llama.cpp  $LLAMA_TAG  $LLAMA_URL
             sha256 $LLAMA_SHA256
Licence: MIT (LICENSES/llama.cpp-LICENSE.txt); vendored libraries: LICENSES/vendored-*.txt
CaptureMann starts it as a separate program on 127.0.0.1 and does not link it. It is given a local
model file; it has no libcurl or OpenSSL, so it cannot fetch models or call out by itself.
The project: $SOURCE_REPO

cmake configure line:
  $cmake_line

llama-server --version:
$(printf '%s\n' "$ver" | sed 's/^/  /')

Linked libraries (otool -L llama-server):
$(otool -L "$out/llama-server" | tail -n +2 | sed 's/^/  /')

Size: llama-server $(stat -f %z "$out/llama-server") bytes
EOF

echo "built $out"
ls -l "$out/llama-server" | awk '{print "  " $5 " bytes  " $9}'
