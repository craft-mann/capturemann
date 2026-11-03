#!/usr/bin/env bash
# Converts a VinAI PhoWhisper checkpoint (Hugging Face transformers format) to a whisper.cpp ggml
# file, for the M2e speech-model option. Nothing here runs in the Cargo build and nothing is
# downloaded unless you run this script.
#
#   packaging/phowhisper/convert.sh small            # small | medium | large
#   OUT=/some/dir packaging/phowhisper/convert.sh small
#
# Output: $OUT/ggml-phowhisper-<size>.bin plus $OUT/ggml-phowhisper-<size>.SHA256 and .INFO.
# Work files (checkpoint, tools, Python environment) go to $WORK (default $OUT/work); keep both
# outside the repository. Needs git, python3 (3.10 to 3.12 for the pinned torch) and about 8 GB of
# disk for `large`. Licence of the weights: BSD-3-Clause (the model card); keep the card's text.
#
# Pinned tools (change them together with the README):
#   whisper.cpp  v1.9.5   d1be6fde11ac6e0407606b4e42fe72d34add8037   (models/convert-h5-to-ggml.py)
#   openai/whisper v20250625 31243bad24cc746f07d4c8bfdd2d974872cb1803 (assets/mel_filters.npz)
# The checkpoint revision is read from the Hugging Face repo and written to the .INFO file; pin it
# in the README when you upload.
set -euo pipefail

SIZE="${1:-}"
case "$SIZE" in small | medium | large) ;; *) echo "usage: $0 small|medium|large" >&2; exit 2 ;; esac
OUT="${OUT:-$PWD/phowhisper-out}"
WORK="${WORK:-$OUT/work}"
WHISPER_CPP_COMMIT=d1be6fde11ac6e0407606b4e42fe72d34add8037
OPENAI_WHISPER_COMMIT=31243bad24cc746f07d4c8bfdd2d974872cb1803
REPO="vinai/PhoWhisper-$SIZE"
case "$OUT" in "$(git rev-parse --show-toplevel 2>/dev/null || echo /nonexistent)"/*)
    echo "OUT must be outside the repository" >&2; exit 2 ;; esac
mkdir -p "$OUT" "$WORK"

fetch_commit() { # <url> <commit> <dir>
    if [ ! -d "$3/.git" ]; then
        git init -q "$3"
        git -C "$3" remote add origin "$1"
    fi
    git -C "$3" fetch -q --depth 1 origin "$2"
    git -C "$3" checkout -q FETCH_HEAD
}
fetch_commit https://github.com/ggml-org/whisper.cpp "$WHISPER_CPP_COMMIT" "$WORK/whisper.cpp"
fetch_commit https://github.com/openai/whisper "$OPENAI_WHISPER_COMMIT" "$WORK/openai-whisper"

if [ ! -x "$WORK/venv/bin/python" ]; then
    "${PYTHON:-python3}" -m venv "$WORK/venv"
    "$WORK/venv/bin/pip" install -q -r "$(dirname "$0")/requirements.txt"
fi
PY="$WORK/venv/bin/python"

# The checkpoint: weights and config files only (no TensorFlow or Flax copies).
if [ ! -f "$WORK/$REPO/config.json" ]; then
    "$PY" - "$REPO" "$WORK/$REPO" <<'PYEOF'
import sys
from huggingface_hub import snapshot_download
repo, dest = sys.argv[1], sys.argv[2]
path = snapshot_download(repo, local_dir=dest, allow_patterns=[
    "*.json", "*.txt", "*.safetensors", "pytorch_model*.bin", "README.md", "LICENSE*"])
from huggingface_hub import HfApi
print(HfApi().model_info(repo).sha, file=open(dest + "/.revision", "w"))
PYEOF
fi

mkdir -p "$WORK/convert-out"
# whisper.cpp reads the weights, the tokenizer files and the mel filters, and writes ggml-model.bin.
"$PY" "$WORK/whisper.cpp/models/convert-h5-to-ggml.py" \
    "$WORK/$REPO" "$WORK/openai-whisper" "$WORK/convert-out"
FILE="$OUT/ggml-phowhisper-$SIZE.bin"
mv "$WORK/convert-out/ggml-model.bin" "$FILE"

shasum -a 256 "$FILE" | cut -d' ' -f1 >"${FILE%.bin}.SHA256"
{
    echo "file: $(basename "$FILE")"
    echo "bytes: $(wc -c <"$FILE" | tr -d ' ')"
    echo "sha256: $(cat "${FILE%.bin}.SHA256")"
    echo "source: $REPO @ $(cat "$WORK/$REPO/.revision" 2>/dev/null || echo unknown)"
    echo "whisper.cpp: $WHISPER_CPP_COMMIT"
    echo "openai/whisper: $OPENAI_WHISPER_COMMIT"
    echo "licence: BSD-3-Clause (see the model card of $REPO)"
} >"${FILE%.bin}.INFO"
cat "${FILE%.bin}.INFO"

# Check 1: a whisper.cpp run (through the app's own loader) transcribes a short clip. Give a public
# Vietnamese clip (see the README) as CLIP, and the app's example does the rest.
if [ -n "${CLIP:-}" ]; then
    echo "check: transcribing $CLIP"
    cd "$(git rev-parse --show-toplevel)"
    CM_WHISPER_MODEL="$FILE" CM_WHISPER_AUDIO="$CLIP" CM_WHISPER_LANGUAGE=vi \
        cargo test -p cm-ai --release --test real_model -- --ignored --nocapture real_model_transcribes_speech
else
    echo "skipped the transcription check: set CLIP=<short public Vietnamese clip>" >&2
fi
echo "Next: see packaging/phowhisper/README.md (import check, upload, pin)."
