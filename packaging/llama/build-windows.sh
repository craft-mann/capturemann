#!/usr/bin/env bash
# Builds the text engine for the Windows build (AI-7, provider "embedded"): llama.cpp's
# `llama-server.exe` (MIT), statically linked, CPU only by default (optional Vulkan). Like the macOS
# and Linux ones (build-macos.sh, build-linux.sh) it is a HELPER PROGRAM next to the app's other
# executables, never linked into the app: whisper.cpp (in cm-ai) and llama.cpp both embed ggml, and
# two copies in one process clash.
#
# Usage (Windows, in an MSYS2 MINGW64 shell, the same one packaging/ffmpeg/build-windows.sh uses):
#   packaging/llama/build-windows.sh
# Output: packaging/llama/out-windows-x64/{llama-server.exe,BUILD-INFO.txt,LICENSES/}  (ignored by version control).
#
# The release is pinned in the top lines of build-macos.sh (LLAMA_TAG, LLAMA_SHA256); this script reads
# them from there so the platforms can never run different llama.cpp versions. It refuses to run while
# they are the placeholder PENDING_PIN. Check the flags in `cmake_args` and the arguments built by
# `engine_args` in crates/cm-ai/src/text_model/engine/mod.rs against `llama-server --help` of the
# pinned release: llama.cpp renames options between releases.
#
# Environment:
#   CM_LLAMA_SRC_DIR    where the source tarball is cached (default packaging/llama/.downloads).
#                       A missing tarball is downloaded and checked against the pinned SHA-256; a
#                       mismatch stops the build.
#   CM_LLAMA_BUILD_DIR  scratch build directory (default: a new folder under $TMPDIR, removed at the end).
#   CM_LLAMA_OUT        output directory (default packaging/llama/out-windows-x64).
#   CM_LLAMA_VULKAN     ON or OFF (default OFF). ON needs the Vulkan headers, loader and glslc in the
#                       MSYS2 environment (mingw-w64-x86_64-vulkan-headers, -vulkan-loader, -shaderc) and
#                       makes the binary need vulkan-1.dll at run time (untested on the project's hardware).
#   CM_LLAMA_JOBS       parallel jobs (default: the number of CPUs).
#   CM_SOURCE_REPO      the project URL written into BUILD-INFO.txt.
#
# What it needs, in MSYS2 MINGW64: pacman -S --needed mingw-w64-x86_64-gcc mingw-w64-x86_64-cmake
# mingw-w64-x86_64-ninja curl tar. No Python, no libcurl, no OpenSSL.
#
# CPU code: a fixed x86-64 baseline, never `-march=native`: AVX2, FMA, F16C, BMI2 (any CPU since about 2015).
# OpenMP is OFF (GGML_OPENMP=OFF): ggml uses its own thread pool, so there is no libgomp dependency.
# libstdc++, libgcc and winpthread are linked statically (GCC Runtime Library Exception), so the
# executable needs only Windows system DLLs.
#
# Licence: llama.cpp is MIT. Its tree vendors a few small MIT/BSD libraries (cpp-httplib, nlohmann/json,
# stb_image, miniaudio); their licence files are copied next to the main LICENSE. LLAMA_CURL and
# LLAMA_OPENSSL are off, so the server cannot download models or call out by itself (the app gives it a
# local GGUF file and nothing else), and the self-check fails on any DLL outside the allowed list.
set -euo pipefail

die() { echo "build-windows.sh: $*" >&2; exit 1; }

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

# ---- pinned source: one place, the macOS script ----
pin() { sed -n "s/^$1=\"\([^\"]*\)\".*/\1/p" "$here/build-macos.sh" | head -1; }
LLAMA_TAG="$(pin LLAMA_TAG)"
LLAMA_SHA256="$(pin LLAMA_SHA256)"
[ -n "$LLAMA_TAG" ] && [ -n "$LLAMA_SHA256" ] || die "cannot read LLAMA_TAG and LLAMA_SHA256 from build-macos.sh"
LLAMA_URL="https://github.com/ggml-org/llama.cpp/archive/refs/tags/${LLAMA_TAG}.tar.gz"

if [ "$LLAMA_TAG" = "PENDING_PIN" ] || [ "$LLAMA_SHA256" = "PENDING_PIN" ]; then
  die "LLAMA_TAG and LLAMA_SHA256 are not pinned yet. Edit the top of packaging/llama/build-macos.sh (see its header), then run this again. Nothing was downloaded."
fi
case "$LLAMA_SHA256" in
  *[!0-9a-f]* | "") die "LLAMA_SHA256 must be 64 lowercase hex digits" ;;
esac
[ "${#LLAMA_SHA256}" = "64" ] || die "LLAMA_SHA256 must be 64 lowercase hex digits"
case "$LLAMA_TAG" in
  *[!A-Za-z0-9._-]* | "") die "LLAMA_TAG may only contain letters, digits, '.', '_' and '-'" ;;
esac

case "$(uname -s)" in MINGW64_NT-*) ;; *) die "this script runs in an MSYS2 MINGW64 shell on Windows (macOS: build-macos.sh, Linux: build-linux.sh)" ;; esac
ARCH="x86_64"
[ "$(uname -m)" = "x86_64" ] || die "unsupported machine $(uname -m) (Windows x64 only)"
VULKAN="${CM_LLAMA_VULKAN:-OFF}"
case "$VULKAN" in ON|OFF) ;; *) die "CM_LLAMA_VULKAN must be ON or OFF" ;; esac
for tool in curl sha256sum tar ninja cmake objdump strip; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool (MSYS2 MINGW64: pacman -S --needed mingw-w64-x86_64-gcc mingw-w64-x86_64-cmake mingw-w64-x86_64-ninja curl tar)"
done
command -v g++ >/dev/null 2>&1 || die "no C++ compiler (pacman -S mingw-w64-x86_64-gcc)"
if [ "$VULKAN" = "ON" ]; then
  command -v glslc >/dev/null 2>&1 || die "CM_LLAMA_VULKAN=ON needs glslc (pacman -S mingw-w64-x86_64-shaderc mingw-w64-x86_64-vulkan-headers mingw-w64-x86_64-vulkan-loader)"
fi

src_cache="${CM_LLAMA_SRC_DIR:-$root/packaging/llama/.downloads}"
out="${CM_LLAMA_OUT:-$root/packaging/llama/out-windows-x64}"
jobs="${CM_LLAMA_JOBS:-$(nproc)}"
SOURCE_REPO="${CM_SOURCE_REPO:-https://github.com/craft-mann/capturemann}"
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
  got="$(sha256sum "$path" | awk '{print $1}')"
  if [ "$got" != "$LLAMA_SHA256" ]; then
    echo "cached $tarball has the wrong checksum; downloading again" >&2
    rm -f "$path"
  fi
fi
if [ ! -f "$path" ]; then
  echo "==> downloading $LLAMA_URL"
  curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$path.part" "$LLAMA_URL" || die "download failed: $LLAMA_URL"
  mv "$path.part" "$path"
fi
got="$(sha256sum "$path" | awk '{print $1}')"
[ "$got" = "$LLAMA_SHA256" ] || die "checksum mismatch for $tarball: expected $LLAMA_SHA256, got $got (nothing was built from it)"
echo "    $tarball sha256 ok"

# The archive must hold only files under one folder.
tar -tzf "$path" | while IFS= read -r m; do
  case "$m" in /* | *..*) die "unexpected archive member: $m" ;; esac
done
mkdir -p "$work/src"
tar -xzf "$path" -C "$work/src"
srcdir="$(find "$work/src" -mindepth 1 -maxdepth 1 -type d | head -1)"
[ -n "$srcdir" ] && [ -f "$srcdir/CMakeLists.txt" ] || die "the tarball does not hold a llama.cpp source tree"

# A reproducible-ish environment: nothing from /usr/local or a user prefix is searched.
unset CPATH LIBRARY_PATH PKG_CONFIG_PATH

echo "==> llama-server $LLAMA_TAG ($ARCH, static, CPU, Vulkan $VULKAN)"
cmake_args=(
  -G Ninja -S "$srcdir" -B "$work/build"
  -DCMAKE_BUILD_TYPE=Release
  -DBUILD_SHARED_LIBS=OFF
  -DGGML_NATIVE=OFF
  -DGGML_OPENMP=OFF
  -DGGML_BLAS=OFF
  -DGGML_CUDA=OFF
  -DGGML_HIP=OFF
  -DGGML_SYCL=OFF
  -DGGML_VULKAN="$VULKAN"
  -DLLAMA_CURL=OFF
  -DLLAMA_OPENSSL=OFF
  -DLLAMA_BUILD_TESTS=OFF
  -DLLAMA_BUILD_EXAMPLES=OFF
  -DLLAMA_BUILD_TOOLS=ON
  -DLLAMA_BUILD_SERVER=ON
  # Same as the macOS build: v0.7.0 downloads a prebuilt Web UI from Hugging Face at build time
  # (network, unpinned). The app starts the server with --no-webui.
  -DLLAMA_USE_PREBUILT_UI=OFF
  -DCMAKE_EXE_LINKER_FLAGS="-static -static-libstdc++ -static-libgcc"
)
cmake_args+=(-DGGML_AVX=ON -DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON -DGGML_BMI2=ON
             -DGGML_AVX512=OFF -DGGML_AVX_VNNI=OFF)
cmake "${cmake_args[@]}" >"$work/cmake-configure.log" 2>&1 \
  || { tail -30 "$work/cmake-configure.log" >&2; die "cmake configure failed (log: $work/cmake-configure.log)"; }
cmake --build "$work/build" --config Release --target llama-server -j "$jobs" >"$work/cmake-build.log" 2>&1 \
  || { grep -n -E -B2 -A6 'error:|CMake Error|fatal error|FAILED:|No such file|undefined' "$work/cmake-build.log" | head -60 >&2
       tail -15 "$work/cmake-build.log" >&2
       cp "$work/cmake-build.log" "${TMPDIR:-/tmp}/cm-llama-build-failed.log" 2>/dev/null \
         && echo "the full log was kept in ${TMPDIR:-/tmp}/cm-llama-build-failed.log" >&2
       die "llama-server build failed (log: $work/cmake-build.log)"; }

bin="$(find "$work/build" -type f -name llama-server.exe | head -1)"
[ -n "$bin" ] && [ -f "$bin" ] || die "the build did not produce llama-server.exe"
cp "$bin" "$work/llama-server.exe"
bin="$work/llama-server.exe"
strip --strip-unneeded "$bin"

echo "==> self-check"
# 1. The licence: MIT, with the files that travel with the binary.
[ -f "$srcdir/LICENSE" ] || die "the source tree has no LICENSE"
grep -q -i "MIT License" "$srcdir/LICENSE" || die "$srcdir/LICENSE is not the MIT licence: do not ship this build without a licence review"
# 2. Only Windows system DLLs (the C runtime, kernel, sockets for the local server, and with Vulkan its loader).
#    No libgomp, libcurl, OpenSSL, BLAS, CUDA, libstdc++, libgcc_s or libwinpthread (all static).
needed="$(objdump -p "$bin" | awk '/DLL Name:/ {print tolower($3)}' | sort -u)"
allowed='^(kernel32\.dll|msvcrt\.dll|ucrtbase\.dll|api-ms-win-crt-[a-z0-9-]+-l[0-9]-[0-9]-[0-9]\.dll|advapi32\.dll|user32\.dll|shell32\.dll|ws2_32\.dll|bcrypt\.dll|iphlpapi\.dll|ntdll\.dll|ole32\.dll|vulkan-1\.dll)$'
bad="$(grep -v -E "$allowed" <<<"$needed" || true)"
[ -z "$bad" ] || die "llama-server.exe needs a DLL outside the allowed list: $(echo $bad)"
if [ "$VULKAN" = "OFF" ] && grep -q vulkan <<<"$needed"; then die "llama-server.exe needs Vulkan although CM_LLAMA_VULKAN=OFF"; fi
if [ "$VULKAN" = "ON" ] && ! grep -q vulkan <<<"$needed"; then die "llama-server.exe does not link Vulkan"; fi
# 3. The architecture, and the binary starts.
file_arch="$(objdump -f "$bin" | sed -n 's/.*file format //p' | head -1)"
[ "$file_arch" = "pei-x86-64" ] || die "llama-server.exe has the format $file_arch, not pei-x86-64"
ver="$("$bin" --version 2>&1 | head -3 || true)"
[ -n "$ver" ] || die "llama-server did not print a version (cannot start?)"
echo "$ver" | sed 's/^/    /'
# 4. The options the app passes exist in this release.
help="$("$bin" --help 2>&1 || true)"
missing=""
for opt in --host --port --model --alias --ctx-size --n-gpu-layers --parallel --no-webui; do
  grep -q -e "$opt" <<<"$help" || missing="$missing $opt"
done
[ -z "$missing" ] || die "this release of llama-server lacks the option(s):$missing (update engine_args in crates/cm-ai/src/text_model/engine/mod.rs and this check together)"
echo "    MIT licence, libraries: $(echo $needed), $ARCH only, Vulkan $VULKAN, options present"

echo "==> installing into $out"
rm -rf "$out"
mkdir -p "$out/LICENSES"
cp "$bin" "$out/llama-server.exe"
cp "$srcdir/LICENSE" "$out/LICENSES/llama.cpp-LICENSE.txt"
if [ -d "$srcdir/licenses" ]; then
  for f in "$srcdir"/licenses/*; do
    [ -f "$f" ] && cp "$f" "$out/LICENSES/vendored-$(basename "$f").txt"
  done
fi

cmake_line="cmake ${cmake_args[*]}"
cmake_line="${cmake_line//$work/<build-dir>}"
cat >"$out/BUILD-INFO.txt" <<EOF
CaptureMann bundled llama-server (Windows x64): build record
Built by packaging/llama/build-windows.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
Host: $(uname -srm), MSYS2 MINGW64, $(${CXX:-g++} --version | head -1)
Architecture: x86-64 only, CPU (OpenMP off), Vulkan $VULKAN

Source (official site, SHA-256 checked by the script before use):
  llama.cpp  $LLAMA_TAG  $LLAMA_URL
             sha256 $LLAMA_SHA256
Licence: MIT (LICENSES/llama.cpp-LICENSE.txt); vendored libraries: LICENSES/vendored-*.txt
CaptureMann starts it as a separate program on 127.0.0.1 and does not link it. It is given a local
model file; it has no libcurl or OpenSSL, so it cannot fetch models or call out by itself.
The project: $SOURCE_REPO

cmake configure line:
  $cmake_line

llama-server.exe --version:
$(printf '%s\n' "$ver" | sed 's/^/  /')

Needed DLLs (objdump -p llama-server.exe):
$(printf '%s\n' "$needed" | sed 's/^/  /')

Size: llama-server $(stat -c %s "$out/llama-server.exe") bytes
EOF

echo "built $out"
ls -l "$out/llama-server.exe" | awk '{print "  " $5 " bytes  " $9}'
