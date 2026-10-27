#!/usr/bin/env bash
# Fetches the two speaker-label models (AI-8) that ship INSIDE the app, so "Find speakers" needs
# no setup: pyannote segmentation-3.0 (MIT) and the 3D-Speaker CAM++ English VoxCeleb embedding
# model (Apache-2.0). Run by `make sherpa` after fetch-macos.sh (that script replaces packaging/
# sherpa/out, so this one must run second). Pins and sources: docs/project/spikes.md (S7).
#
# Output: packaging/sherpa/out/models/{pyannote-segmentation-3-0.onnx,
#         campplus-sv-en-voxceleb-16k.onnx,BUILD-INFO.txt,LICENSES/}
#         (git-ignored; packaging/macos/bundle.sh copies them to Contents/Resources/Models/speaker/).
#
# Environment: CM_SHERPA_SRC_DIR (download cache, default packaging/sherpa/.downloads; put the two
# files there to work offline), CM_SHERPA_OUT (default packaging/sherpa/out).
# Needs: curl, shasum, tar (the system tar unpacks the .tar.bz2 here at BUILD time; nothing in the
# app unpacks archives). A download is checked against the pinned SHA-256 and size; a mismatch
# stops the script and deletes the file.
set -euo pipefail

die() { echo "fetch-models.sh: $*" >&2; exit 1; }

# ---- pins (read and hashed on Oct 27, 2026) ----
SEG_ARCHIVE="sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
SEG_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/${SEG_ARCHIVE}"
SEG_ARCHIVE_SHA256="24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488"
SEG_ARCHIVE_BYTES="6958444"
SEG_DIR="sherpa-onnx-pyannote-segmentation-3-0"
SEG_FILE="pyannote-segmentation-3-0.onnx"
SEG_SHA256="220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079"
SEG_BYTES="5992913"

EMB_DOWNLOAD="3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx"
EMB_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/${EMB_DOWNLOAD}"
EMB_FILE="campplus-sv-en-voxceleb-16k.onnx"
EMB_SHA256="357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b"
EMB_BYTES="29596978"

[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
for tool in curl shasum tar; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src_cache="${CM_SHERPA_SRC_DIR:-$root/packaging/sherpa/.downloads}"
out="${CM_SHERPA_OUT:-$root/packaging/sherpa/out}"
sha_of() { shasum -a 256 "$1" | awk '{print $1}'; }
size_of() { stat -f %z "$1"; }

# fetch <file in cache> <url> <sha256> <bytes>
fetch() {
  local name="$1" url="$2" sha="$3" bytes="$4" f="$src_cache/$1"
  mkdir -p "$src_cache"
  if [ -f "$f" ] && { [ "$(size_of "$f")" != "$bytes" ] || [ "$(sha_of "$f")" != "$sha" ]; }; then
    echo "fetch-models.sh: the cached $name does not match the pin; deleting it" >&2
    rm -f "$f"
  fi
  if [ ! -f "$f" ]; then
    echo "Downloading $url"
    rm -f "$f.part"
    curl --fail --location --proto '=https' --tlsv1.2 --retry 3 --output "$f.part" "$url" \
      || { rm -f "$f.part"; die "download failed: $url"; }
    [ "$(size_of "$f.part")" = "$bytes" ] || { rm -f "$f.part"; die "$name: wrong size, expected $bytes bytes"; }
    [ "$(sha_of "$f.part")" = "$sha" ] || { rm -f "$f.part"; die "$name: SHA-256 mismatch; deleted"; }
    mv "$f.part" "$f"
  fi
  echo "OK: $name (sha256 $sha)"
}

fetch "$SEG_ARCHIVE" "$SEG_URL" "$SEG_ARCHIVE_SHA256" "$SEG_ARCHIVE_BYTES"
fetch "$EMB_DOWNLOAD" "$EMB_URL" "$EMB_SHA256" "$EMB_BYTES"

# Only these members are taken out of the archive, and only after the listing shows they are
# under the archive's own folder.
archive="$src_cache/$SEG_ARCHIVE"
tar -tjf "$archive" | while IFS= read -r m; do
  case "$m" in
    *..*) die "unexpected archive member: $m" ;;
    "$SEG_DIR"|"$SEG_DIR"/*) ;;
    *) die "unexpected archive member: $m" ;;
  esac
done
work="$(mktemp -d "${TMPDIR:-/tmp}/cm-speaker-models.XXXXXX")"
trap 'rm -rf "$work"' EXIT
tar -xjf "$archive" -C "$work" "$SEG_DIR/model.onnx" "$SEG_DIR/LICENSE"
seg_src="$work/$SEG_DIR/model.onnx"
[ -f "$seg_src" ] && [ ! -L "$seg_src" ] || die "the archive has no plain model.onnx"
[ -f "$work/$SEG_DIR/LICENSE" ] && [ ! -L "$work/$SEG_DIR/LICENSE" ] || die "the archive has no plain LICENSE"

mkdir -p "$out"
new="$out/models.new.$$"
rm -rf "$new"
mkdir -p "$new/LICENSES"
cp "$seg_src" "$new/$SEG_FILE"
cp "$src_cache/$EMB_DOWNLOAD" "$new/$EMB_FILE"
cp "$work/$SEG_DIR/LICENSE" "$new/LICENSES/pyannote-segmentation-3.0-LICENSE.txt"
grep -q "MIT" "$new/LICENSES/pyannote-segmentation-3.0-LICENSE.txt" || { rm -rf "$new"; die "the segmentation LICENSE is not the MIT text"; }

# Final files must match the pins.
[ "$(size_of "$new/$SEG_FILE")" = "$SEG_BYTES" ] && [ "$(sha_of "$new/$SEG_FILE")" = "$SEG_SHA256" ] \
  || { rm -rf "$new"; die "$SEG_FILE does not match the pinned SHA-256/size"; }
[ "$(size_of "$new/$EMB_FILE")" = "$EMB_BYTES" ] && [ "$(sha_of "$new/$EMB_FILE")" = "$EMB_SHA256" ] \
  || { rm -rf "$new"; die "$EMB_FILE does not match the pinned SHA-256/size"; }

cat >"$new/LICENSES/campplus-3D-Speaker-NOTICE.txt" <<NOTICE
CAM++ speaker embedding model (CaptureMann, speaker labels, AI-8)

File: $EMB_FILE (published as $EMB_DOWNLOAD)
Model: CAM++ English VoxCeleb 16 kHz, from the 3D-Speaker project of Alibaba / ModelScope
       (https://github.com/modelscope/3D-Speaker), licensed under the Apache License, Version 2.0
       (https://www.apache.org/licenses/LICENSE-2.0).
Obtained from: $EMB_URL
Converted/hosted by the sherpa-onnx project (https://github.com/k2-fsa/sherpa-onnx).
The file is shipped unmodified. The download carries no licence file of its own; the Apache-2.0
text is in the LICENSE file of the 3D-Speaker repository and in vendor/sherpa-onnx/LICENSE.
NOTICE

cat >"$new/BUILD-INFO.txt" <<INFO
CaptureMann speaker models: fetch record
Fetched by packaging/sherpa/fetch-models.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)

$SEG_FILE   (model.onnx from $SEG_ARCHIVE)
  archive $SEG_URL
  archive sha256 $SEG_ARCHIVE_SHA256
  sha256 $SEG_SHA256   size $SEG_BYTES bytes   licence MIT (Copyright 2022 CNRS)
$EMB_FILE
  source  $EMB_URL
  sha256 $EMB_SHA256   size $EMB_BYTES bytes   licence Apache-2.0
INFO

rm -rf "$out/models"
mv "$new" "$out/models"
echo "Done: $out/models ($SEG_FILE, $EMB_FILE, LICENSES/)."
