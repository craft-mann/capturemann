#!/usr/bin/env bash
# Fetches the prebuilt static libraries of the speaker engine (AI-8, spike S7): sherpa-onnx v2.0.1
# with its ONNX Runtime, for macOS arm64. cm-ai links them statically (feature `sherpa`), so the
# app has no extra dylib to sign or notarize.
#
# Usage (any directory):   packaging/sherpa/fetch-macos.sh      or   make sherpa
# Output: packaging/sherpa/out/{lib/*.a,BUILD-INFO.txt,LICENSES/}
#         (git-ignored; `.cargo/config.toml` points SHERPA_ONNX_LIB_DIR at out/lib, so a plain
#         `cargo build` finds the libraries once this has run).
#
# Why this script exists: the upstream `sherpa-onnx-sys` build script downloaded this archive
# during `cargo build`, without a checksum, and unpacked it with the `ureq`, `zip`, `bzip2` and
# `tar` crates (licences CDLA-Permissive-2.0 and bzip2-1.0.6 in the dependency graph). The copy in
# vendor/sherpa-onnx-sys only links what is here, never touches the network and has none of those
# dependencies. This script is the one place that downloads, and it checks the SHA-256 first.
#
# Environment:
#   CM_SHERPA_SRC_DIR  where the archive is cached (default packaging/sherpa/.downloads,
#                      git-ignored). A missing archive is downloaded with curl and checked against
#                      the SHA-256 pinned below; a mismatch stops the script and deletes the file.
#                      Put the archive there by hand to work offline.
#   CM_SHERPA_OUT      output directory (default packaging/sherpa/out).
#   CM_SOURCE_REPO     the project URL written into BUILD-INFO.txt.
#
# What it needs: curl, shasum, tar (the system tar unpacks .tar.bz2; nothing is added to the app
# for it). Apple Silicon only, like `make ffmpeg`.
#
# Licence: sherpa-onnx is Apache-2.0 and ONNX Runtime is MIT. The release archive carries no
# licence files (only lib/*.a), so the notes written to LICENSES/ say where the texts are; the
# Apache-2.0 text is also in vendor/sherpa-onnx/LICENSE and ONNX Runtime's MIT text in
# packaging/sherpa/licenses/. See packaging/sherpa/README.md for what the maintainer still has to
# add (ONNX Runtime's ThirdPartyNotices).
set -euo pipefail

die() { echo "fetch-macos.sh: $*" >&2; exit 1; }

# ---- pinned archive (read and hashed on Oct 27, 2026; docs/project/spikes.md, S7) ----
SHERPA_VERSION="2.0.1"
SHERPA_ARCHIVE="sherpa-onnx-v${SHERPA_VERSION}-osx-arm64-static-lib.tar.bz2"
SHERPA_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_VERSION}/${SHERPA_ARCHIVE}"
SHERPA_SHA256="d7a9b14f337958e7373c8c650f046df33d210bbaff2f33cb8322dda663ee5e1c"
SHERPA_BYTES="20829454"
SHERPA_DIR="sherpa-onnx-v${SHERPA_VERSION}-osx-arm64-static-lib"

# The libraries vendor/sherpa-onnx-sys/build.rs links (STATIC_LIBS there must match).
LIBS=(sherpa-onnx-c-api sherpa-onnx-core kaldi-decoder-core sherpa-onnx-kaldifst-core
      sherpa-onnx-fstfar sherpa-onnx-fst kaldi-native-fbank-core kissfft-float onnxruntime
      ssentencepiece_core utf8proc)

SOURCE_REPO="${CM_SOURCE_REPO:-https://github.com/craft-mann/capturemann}"

[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
[ "$(uname -m)" = "arm64" ] || die "arm64 only (see the header)"
for tool in curl shasum tar; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done
case "$SHERPA_SHA256" in
  *[!0-9a-f]* | "") die "SHERPA_SHA256 must be 64 lowercase hex digits" ;;
esac
[ "${#SHERPA_SHA256}" = "64" ] || die "SHERPA_SHA256 must be 64 lowercase hex digits"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src_cache="${CM_SHERPA_SRC_DIR:-$root/packaging/sherpa/.downloads}"
out="${CM_SHERPA_OUT:-$root/packaging/sherpa/out}"
archive="$src_cache/$SHERPA_ARCHIVE"

sha_of() { shasum -a 256 "$1" | awk '{print $1}'; }

mkdir -p "$src_cache"
if [ -f "$archive" ] && [ "$(sha_of "$archive")" != "$SHERPA_SHA256" ]; then
  echo "fetch-macos.sh: the cached $SHERPA_ARCHIVE does not match the pinned SHA-256; deleting it" >&2
  rm -f "$archive"
fi
if [ ! -f "$archive" ]; then
  echo "Downloading $SHERPA_URL"
  part="$archive.part"
  rm -f "$part"
  curl --fail --location --proto '=https' --tlsv1.2 --retry 3 --output "$part" "$SHERPA_URL" \
    || { rm -f "$part"; die "download failed"; }
  size="$(stat -f %z "$part")"
  [ "$size" = "$SHERPA_BYTES" ] || { rm -f "$part"; die "the download is $size bytes, expected $SHERPA_BYTES"; }
  [ "$(sha_of "$part")" = "$SHERPA_SHA256" ] || { rm -f "$part"; die "SHA-256 mismatch: the download is not the pinned archive; deleted"; }
  mv "$part" "$archive"
fi
echo "Archive OK: $SHERPA_ARCHIVE (sha256 $SHERPA_SHA256)"

# The archive must hold only files under its own folder (no absolute paths, no "..", no links).
members="$(tar -tjf "$archive")"
printf '%s\n' "$members" | while IFS= read -r m; do
  case "$m" in
    "$SHERPA_DIR"/*) ;;
    "$SHERPA_DIR") ;;
    *) die "unexpected archive member: $m" ;;
  esac
  case "$m" in
    *..*) die "unexpected archive member: $m" ;;
  esac
done

work="$(mktemp -d "${TMPDIR:-/tmp}/cm-sherpa.XXXXXX")"
trap 'rm -rf "$work"' EXIT
tar -xjf "$archive" -C "$work"
lib_src="$work/$SHERPA_DIR/lib"
[ -d "$lib_src" ] || die "the archive has no $SHERPA_DIR/lib"
for l in "${LIBS[@]}"; do
  [ -f "$lib_src/lib$l.a" ] || die "the archive has no lib$l.a"
done

# Build the new tree next to the old one and swap, so a failure never leaves half a folder.
new="$out.new.$$"
rm -rf "$new"
mkdir -p "$new/lib" "$new/LICENSES"
for l in "${LIBS[@]}"; do
  cp "$lib_src/lib$l.a" "$new/lib/"
done

cat >"$new/LICENSES/NOTICE.txt" <<EOF
sherpa-onnx static libraries (speaker engine of CaptureMann, AI-8)

The release archive $SHERPA_ARCHIVE contains only lib/*.a and no licence files. What is in them:

  sherpa-onnx (libsherpa-onnx-*, kaldi-*, kissfft, utf8proc, ssentencepiece)
      Apache License 2.0. Source and licence: https://github.com/k2-fsa/sherpa-onnx (tag v$SHERPA_VERSION).
      The Apache-2.0 text is also in vendor/sherpa-onnx/LICENSE of the CaptureMann repository.
      The small libraries it builds in keep their own permissive licences in its source tree
      (kaldi-native-fbank and kaldi-decoder: Apache-2.0; kissfft: BSD-3-Clause; utf8proc: MIT).
  ONNX Runtime (libonnxruntime.a)
      MIT License, Copyright (c) Microsoft Corporation. https://github.com/microsoft/onnxruntime
      (the MIT text is packaging/sherpa/licenses/onnxruntime-MIT.txt of the repository; the
      ThirdPartyNotices.txt of the ONNX Runtime version inside sherpa-onnx v$SHERPA_VERSION is the
      maintainer's to add, see packaging/sherpa/README.md).

The libraries are linked statically into cm-desktop; nothing here is shipped as a separate file.
The speaker MODEL files (pyannote segmentation 3.0, MIT; CAM++ English, Apache-2.0) are not part of
this archive: they are downloaded or imported by the user from the AI page.
EOF

cat >"$new/BUILD-INFO.txt" <<EOF
CaptureMann sherpa-onnx libraries: fetch record
Fetched by packaging/sherpa/fetch-macos.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
Host: $(uname -srm), $(sw_vers -productName) $(sw_vers -productVersion)
Project: $SOURCE_REPO

Archive (pinned, SHA-256 checked before unpacking):
  $SHERPA_URL
  sha256 $SHERPA_SHA256
  size   $SHERPA_BYTES bytes

Libraries copied to lib/ (the ones vendor/sherpa-onnx-sys links):
$(for l in "${LIBS[@]}"; do printf '  lib%s.a  sha256 %s\n' "$l" "$(sha_of "$new/lib/lib$l.a")"; done)

Not copied: libsherpa-onnx-cxx-api.a and libsherpa-onnx-portaudio_static.a (not used).
EOF

rm -rf "$out"
mv "$new" "$out"
echo "Done: $out/lib ($(ls "$out/lib" | wc -l | tr -d ' ') libraries)."
echo "cargo finds it through .cargo/config.toml (SHERPA_ONNX_LIB_DIR=packaging/sherpa/out/lib)."
