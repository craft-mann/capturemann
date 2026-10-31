#!/usr/bin/env bash
# Builds the Linux "early build" tarball:
#   capturemann-<version>-linux-<arch>.tar.gz   (+ .sha256)
# holding cm-desktop, cm-overlay, capturemann, cm-mcp, the licence files, the .desktop entry, the
# icons from design/brand/app-icon, install.sh and, when given, an LGPL FFmpeg.
#
# Usage (on Linux, from any directory):   packaging/linux/build-linux.sh
# Output: packaging/linux/out/ (git-ignored), or $CM_LINUX_OUT.
#
# Environment:
#   CM_SKIP_BUILD=1    do not run cargo; reuse the binaries in <target>/release (CARGO_TARGET_DIR respected).
#   CM_FFMPEG_BUNDLE   a folder with ffmpeg and ffprobe (and SOURCE-OFFER.txt, LICENSES/ from
#                      packaging/ffmpeg/build-linux.sh) to ship. Without it the tarball has no FFmpeg
#                      and says so (install.sh prints a note). Refused when its build line says GPL.
#   CM_REQUIRE_FFMPEG=1  fail unless an LGPL FFmpeg with its licence files is bundled.
#   CM_LINUX_OUT       output folder.
#   CM_LINUX_ALLOW_MISSING  space-separated program names that may be absent (testing the packaging only).
#   CM_CARGO_JOBS      cargo -j value (default: cargo's own).
#   CM_SKIP_LICENSES=1 use the checked-in THIRD_PARTY_LICENSES.md (macOS crate set) instead of
#                      generating the Linux one with packaging/macos/gen-third-party.sh.
#
# The speaker engine (sherpa-onnx, feature `sherpa`) is built in when its static libraries are there:
# run `make sherpa` (packaging/sherpa/fetch-linux.sh and fetch-models.sh) first, or point
# SHERPA_ONNX_LIB_DIR at a `lib` folder. Without them this builds with --no-default-features and
# "Find speakers" answers that the build has no speaker engine (CM_REQUIRE_SHERPA=1 makes that an
# error). With them the tarball also carries the two speaker models (lib/Resources/Models/speaker/, where
# cm-ai looks: <executable dir>/../Resources/Models/speaker) and the sherpa-onnx / ONNX Runtime licence
# texts (share/doc/capturemann/SHERPA-ONNX/).
#   CM_SPEAKER_MODELS_BUNDLE  folder with the two models and LICENSES/ (default packaging/sherpa/out/models
#                      when it exists, made by `make sherpa`).
#   CM_REQUIRE_SPEAKER_MODELS=1  fail unless both models and their licence files are bundled.set -euo pipefail
die() { echo "build-linux.sh: $*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || die "this script only runs on Linux"
case "$(uname -m)" in
  x86_64) arch=x86_64; triple=x86_64-unknown-linux-gnu ;;
  aarch64) arch=aarch64; triple=aarch64-unknown-linux-gnu ;;
  *) die "unsupported machine $(uname -m) (x86_64 and aarch64 only)" ;;
esac
for t in cargo python3 sha256sum tar ldd objdump install; do
  command -v "$t" >/dev/null 2>&1 || die "required tool not found: $t"
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${CM_LINUX_OUT:-$root/packaging/linux/out}"
jobs=()
[ -z "${CM_CARGO_JOBS:-}" ] || jobs=(-j "$CM_CARGO_JOBS")

version="$(cd "$root" && cargo metadata --no-deps --format-version 1 --locked \
  | python3 -c 'import json,sys; print(sorted({p["version"] for p in json.load(sys.stdin)["packages"]})[-1])')"
[ -n "$version" ] || die "cannot read the workspace version"

# whisper.cpp (cm-ai) is compiled by cargo. ggml defaults to -march=native, which would tie the
# binary to the CPU of the build machine (SIGILL elsewhere); build a fixed baseline instead.
export GGML_NATIVE=OFF
if [ "$arch" = "x86_64" ]; then
  export GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON GGML_BMI2=ON GGML_AVX512=OFF
fi
sherpa_lib="${SHERPA_ONNX_LIB_DIR:-$root/packaging/sherpa/out/lib}"
if [ -f "$sherpa_lib/libsherpa-onnx-c-api.a" ] && [ -f "$sherpa_lib/libonnxruntime.a" ]; then
  features=()
  sherpa_on=1
  sherpa_note="with the speaker engine (sherpa-onnx from $sherpa_lib)"
elif [ "${CM_REQUIRE_SHERPA:-0}" = "1" ]; then
  die "CM_REQUIRE_SHERPA=1 but there are no sherpa-onnx libraries in $sherpa_lib (run: make sherpa)"
else
  features=(--no-default-features)
  sherpa_on=0
  sherpa_note="WITHOUT the speaker engine (no sherpa-onnx libraries in $sherpa_lib; run: make sherpa)"
fi
if [ "${CM_SKIP_BUILD:-0}" != "1" ]; then
  echo "==> cargo build --release, $sherpa_note"
  # whisper-rs-sys does not re-run when GGML_* change: after changing them, cargo clean -p whisper-rs-sys.
  (cd "$root" && SHERPA_ONNX_LIB_DIR="$sherpa_lib" cargo build --release --locked ${features[@]+"${features[@]}"} ${jobs[@]+"${jobs[@]}"} \
    -p cm-desktop -p cm-overlay -p cm-cli -p cm-mcp)
fi
target="$(cd "$root" && cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release"
all_bins="cm-desktop cm-overlay capturemann cm-mcp"
bins=""
for b in $all_bins; do
  if [ -x "$target/$b" ]; then bins="$bins $b"
  elif [[ " ${CM_LINUX_ALLOW_MISSING:-} " == *" $b "* ]]; then echo "    warning: $b is missing; allowed by CM_LINUX_ALLOW_MISSING (a test package, not for release)" >&2
  else die "missing $target/$b (build it, or unset CM_SKIP_BUILD)"; fi
done
bins="${bins# }"

# ---- the libraries the programs need: a policy check, and a record for the README ----
echo "==> checking the linked libraries"
needed="$(for b in $bins; do objdump -p "$target/$b" | awk '/NEEDED/ {print $2}'; done | sort -u)"
# Nothing GPL, LGPL-with-static-restrictions or nonfree may be linked in. cargo deny covers the
# Rust crates; this covers the C libraries the dynamic linker will load.
bad="$(grep -E -i 'x264|x265|fdk|libcdio|readline|libgmp|libgsl|libfftw|libsmbclient|libpostproc|libavcodec|libavformat|libswscale|libswresample|libavutil|libsherpa|libonnxruntime' <<<"$needed" || true)"
[ -z "$bad" ] || die "a program links a library that is not allowed in a shipped build: $bad"
unresolved="$(for b in $bins; do ldd "$target/$b" 2>&1 | grep 'not found' || true; done | sort -u)"
[ -z "$unresolved" ] || echo "    warning: not found on this machine:$(printf '\n      %s' "$unresolved")"
glibc="$(for b in $bins; do objdump -T "$target/$b" | grep -o 'GLIBC_[0-9.]*' ; done | sort -V -u | tail -1)"
echo "    needs $glibc or newer; libraries:"; sed 's/^/      /' <<<"$needed"
# Is the speaker engine really in cm-desktop? sherpa-onnx and ONNX Runtime are static, so there is no
# NEEDED entry to look for (the deny list above rejects one); look for the C API the engine calls and for
# the "no speaker engine" answer of the build without it. This also catches CM_SKIP_BUILD with a stale binary.
if [ -x "$target/cm-desktop" ]; then
  has_api=0; has_stub=0
  grep -a -q 'SherpaOnnxCreateOfflineSpeakerDiarization' "$target/cm-desktop" && has_api=1
  grep -a -q 'has no speaker engine' "$target/cm-desktop" && has_stub=1
  if [ "$sherpa_on" = 1 ] && { [ "$has_api" != 1 ] || [ "$has_stub" = 1 ]; }; then
    die "$target/cm-desktop was built without the speaker engine but the sherpa-onnx libraries are present (rebuild: unset CM_SKIP_BUILD)"
  elif [ "$sherpa_on" = 0 ] && [ "$has_api" = 1 ]; then
    die "$target/cm-desktop contains the speaker engine but this run has no sherpa-onnx libraries (stale binary?)"
  fi
  echo "    speaker engine: $([ "$sherpa_on" = 1 ] && echo 'compiled in (sherpa-onnx C API present, static)' || echo 'not compiled in')"
fi

# ---- the FFmpeg ----
ffdir=""
if [ -n "${CM_FFMPEG_BUNDLE:-}" ]; then
  ffdir="$CM_FFMPEG_BUNDLE"
  [ -x "$ffdir/ffmpeg" ] || die "no executable ffmpeg in CM_FFMPEG_BUNDLE=$ffdir"
  conf="$("$ffdir/ffmpeg" -hide_banner -buildconf 2>&1 || true)"
  if grep -q -E -e '--enable-(gpl|nonfree|version3|libx264|libx265|libfdk-aac)' <<<"$conf"; then
    die "$ffdir/ffmpeg was built with --enable-gpl/nonfree/libx264/libx265/libfdk-aac: not allowed in a shipped build (NFR-12)"
  fi
  if [ -f "$ffdir/SOURCE-OFFER.txt" ] && [ -d "$ffdir/LICENSES" ]; then ff_lic=1; else ff_lic=0; fi
  if [ "$ff_lic" = 0 ]; then
    [ "${CM_REQUIRE_FFMPEG:-0}" != "1" ] || die "$ffdir has no SOURCE-OFFER.txt and LICENSES/"
    echo "    warning: no SOURCE-OFFER.txt / LICENSES next to ffmpeg; this tarball must not be distributed" >&2
  fi
elif [ "${CM_REQUIRE_FFMPEG:-0}" = "1" ]; then
  die "CM_REQUIRE_FFMPEG=1 but CM_FFMPEG_BUNDLE is not set (build one with packaging/ffmpeg/build-linux.sh)"
fi

# ---- licences ----
licenses="$root/THIRD_PARTY_LICENSES.md"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cm-linux-pack.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
if [ "${CM_SKIP_LICENSES:-0}" != "1" ]; then
  echo "==> generating THIRD_PARTY_LICENSES.md for the Linux crate set"
  if CM_LICENSE_TRIPLES="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu" \
       bash "$root/packaging/macos/gen-third-party.sh" "$tmp/THIRD_PARTY_LICENSES.md" >"$tmp/gen.log" 2>&1; then
    licenses="$tmp/THIRD_PARTY_LICENSES.md"
  else
    tail -5 "$tmp/gen.log" >&2
    echo "    warning: could not generate the Linux list (needs the crates fetched); shipping the checked-in macOS one" >&2
  fi
fi

# ---- assemble ----
name="capturemann-${version}-linux-${arch}"
stage="$tmp/$name"
mkdir -p "$stage/lib/capturemann" "$stage/share/applications" "$stage/share/doc/capturemann"
for b in $bins; do install -m 755 "$target/$b" "$stage/lib/capturemann/$b"; done
if [ -n "$ffdir" ]; then
  install -m 755 "$ffdir/ffmpeg" "$stage/lib/capturemann/ffmpeg"
  [ ! -x "$ffdir/ffprobe" ] || install -m 755 "$ffdir/ffprobe" "$stage/lib/capturemann/ffprobe"
  if [ "${ff_lic:-0}" = 1 ]; then
    mkdir -p "$stage/lib/capturemann/FFmpeg"
    cp "$ffdir/SOURCE-OFFER.txt" "$stage/lib/capturemann/FFmpeg/"
    [ ! -f "$ffdir/BUILD-INFO.txt" ] || cp "$ffdir/BUILD-INFO.txt" "$stage/lib/capturemann/FFmpeg/"
    cp -R "$ffdir/LICENSES" "$stage/lib/capturemann/FFmpeg/LICENSES"
  fi
fi
cp "$root/LICENSE" "$root/NOTICE" "$stage/share/doc/capturemann/"
cp "$licenses" "$stage/share/doc/capturemann/THIRD_PARTY_LICENSES.md"
cp "$root/CHANGELOG.md" "$stage/share/doc/capturemann/" 2>/dev/null || true
speaker_note='This build has no speaker engine ("Find speakers" is off).'
if [ "$sherpa_on" = 1 ]; then
  # The static libraries are inside cm-desktop, so their licence texts must travel with it: the
  # generated THIRD_PARTY_LICENSES.md (sherpa-onnx Apache-2.0, ONNX Runtime MIT and its third-party
  # notices) plus the same texts and the fetch record as separate files.
  grep -q 'ONNX Runtime, third-party components' "$licenses" \
    || die "$licenses lacks the ONNX Runtime third-party notices; a build with sherpa-onnx must ship them (packaging/macos/gen-third-party.sh adds them)"
  sherpa_dir="$stage/share/doc/capturemann/SHERPA-ONNX"; mkdir -p "$sherpa_dir"
  sherpa_out="$(dirname "$sherpa_lib")"
  for f in "$sherpa_out/LICENSES/NOTICE.txt" "$sherpa_out/BUILD-INFO.txt"; do
    [ ! -f "$f" ] || cp "$f" "$sherpa_dir/"
  done
  [ -f "$sherpa_dir/NOTICE.txt" ] \
    || printf '%s\n' "sherpa-onnx (Apache-2.0, https://github.com/k2-fsa/sherpa-onnx, v2.0.1) and ONNX Runtime (MIT) are linked statically into cm-desktop; see THIRD_PARTY_LICENSES.md." > "$sherpa_dir/NOTICE.txt"
  cp "$root/packaging/sherpa/licenses/onnxruntime-MIT.txt" "$root/packaging/sherpa/licenses/ThirdPartyNotices-onnxruntime.txt" "$sherpa_dir/"
  # The speaker models: plain data files cm-ai reads from <executable dir>/../Resources/Models/speaker.
  spk_src="${CM_SPEAKER_MODELS_BUNDLE:-}"
  [ -n "$spk_src" ] || { [ ! -d "$root/packaging/sherpa/out/models" ] || spk_src="$root/packaging/sherpa/out/models"; }
  spk_files=(pyannote-segmentation-3-0.onnx campplus-sv-zh-en-16k-advanced.onnx)
  spk_licences=(pyannote-segmentation-3.0-LICENSE.txt campplus-3D-Speaker-NOTICE.txt)
  spk_ok=1
  if [ -z "$spk_src" ]; then spk_ok=0; else
    for f in "${spk_files[@]}"; do [ -f "$spk_src/$f" ] || spk_ok=0; done
    for f in "${spk_licences[@]}"; do [ -f "$spk_src/LICENSES/$f" ] || spk_ok=0; done
  fi
  if [ "$spk_ok" = 1 ]; then
    spk_dst="$stage/lib/Resources/Models/speaker"; mkdir -p "$spk_dst/LICENSES"
    for f in "${spk_files[@]}"; do cp "$spk_src/$f" "$spk_dst/"; done
    cp "$spk_src"/LICENSES/* "$spk_dst/LICENSES/"
    [ ! -f "$spk_src/BUILD-INFO.txt" ] || cp "$spk_src/BUILD-INFO.txt" "$spk_dst/"
    speaker_note='Speaker engine ("Find speakers"): built in, with its two models in lib/Resources/Models/speaker/.'
  elif [ "${CM_REQUIRE_SPEAKER_MODELS:-0}" = "1" ]; then
    die "CM_REQUIRE_SPEAKER_MODELS=1 but the two speaker models and their licence files are not in ${spk_src:-packaging/sherpa/out/models} (run: make sherpa)"
  else
    echo "    warning: no speaker models bundled; \"Find speakers\" will ask the user to import them" >&2
    speaker_note='Speaker engine: built in; its two models are not in this tarball (import them on the AI page).'
  fi
elif [ "${CM_REQUIRE_SPEAKER_MODELS:-0}" = "1" ]; then
  die "CM_REQUIRE_SPEAKER_MODELS=1 but this build has no speaker engine"
fi
{
  echo "Runtime libraries (DT_NEEDED) of the programs in this build, $arch, needs $glibc or newer:"
  sed 's/^/  /' <<<"$needed"
  echo
  echo "Install them on Ubuntu 24.04 / Debian 13 with: see packaging/linux/README.md (\"What the user needs\")."
} > "$stage/share/doc/capturemann/LIBRARIES.txt"
cp "$root/packaging/linux/dev.capturemann.app.desktop" "$stage/share/applications/"
install -m 755 "$root/packaging/linux/install.sh" "$stage/install.sh"
install -m 755 "$root/packaging/linux/check-runtime.sh" "$stage/check-runtime.sh"
# Icons: the full-bleed app icon (design/CLAUDE.md: "Windows, Linux, web, stores"), copied as is.
icons="$root/design/brand/app-icon"
for s in 16 24 32 48 64 128 256 512; do
  src="$icons/png/windows/app-icon-$s.png"
  [ -f "$src" ] || die "icon not found: $src"
  d="$stage/share/icons/hicolor/${s}x${s}/apps"; mkdir -p "$d"
  cp "$src" "$d/dev.capturemann.app.png"
done
mkdir -p "$stage/share/icons/hicolor/scalable/apps"
cp "$icons/svg/app-icon.svg" "$stage/share/icons/hicolor/scalable/apps/dev.capturemann.app.svg"
cat > "$stage/README.txt" <<EOF
CaptureMann $version for Linux ($arch), early build.

Install for yourself:   ./install.sh                 (into ~/.local, no root)
Install for everyone:   sudo ./install.sh --prefix /usr/local
Remove:                 ./install.sh --uninstall [--prefix ...]
Check libraries:        ./check-runtime.sh          (names the apt package for each missing library)

Needs a desktop session and the libraries listed in share/doc/capturemann/LIBRARIES.txt;
see https://github.com/craft-mann/capturemann (packaging/linux/README.md) for the apt packages.
Licences: share/doc/capturemann/. $speaker_note
EOF

mkdir -p "$out"
rm -f "$out/$name.tar.gz" "$out/$name.tar.gz.sha256"
# Reproducible-ish: sorted names, fixed owner, mtime of the commit when there is one.
mtime="$(cd "$root" && git log -1 --format=%cd --date=iso-strict 2>/dev/null || date -u +%Y-%m-%dT%H:%M:%S)"
(cd "$tmp" && tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="$mtime" -czf "$out/$name.tar.gz" "$name")
(cd "$out" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "==> $out/$name.tar.gz"
ls -l "$out/$name.tar.gz"; cat "$out/$name.tar.gz.sha256"
