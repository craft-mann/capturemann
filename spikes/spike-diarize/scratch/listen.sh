#!/bin/zsh
# Check a diarization by eye and ear (S7). Writes only into OUT_DIR (default: a new folder under /tmp).
#
#   spikes/spike-diarize/scratch/listen.sh <audio.m4a|wav> [num_clusters|auto] [out_dir]
#
# Needs the two models (set SEG and EMB, or the defaults below), FFmpeg (CM_FFMPEG or the bundled build)
# and a built spike: cargo build --release -p spike-diarize --manifest-path spikes/Cargo.toml
# Output: turns.json, timeline.txt (one row per speaker, # = talking), speaker_N.wav (full length, the
# other speakers muted, so a time in the timeline is the same time in every file; open them together in
# any player and compare with the original).
set -eu
IN=$1; K=${2:-auto}
HERE=${0:A:h}
BIN=${BIN:-$HERE/../../target/release/spike-diarize}
FFMPEG=${CM_FFMPEG:-$HERE/../../../packaging/ffmpeg/out/ffmpeg}
[[ -x $FFMPEG ]] || FFMPEG=$(command -v ffmpeg)
M=${MODELS:-/private/tmp/claude-502/s7-models/dl}
SEG=${SEG:-$M/seg/sherpa-onnx-pyannote-segmentation-3-0/model.onnx}
EMB=${EMB:-$M/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx}
OUT=${3:-$(mktemp -d /tmp/s7-listen.XXXXXX)}
mkdir -p "$OUT"
"$FFMPEG" -v error -y -i "$IN" -ar 16000 -ac 1 -c:a pcm_s16le "$OUT/audio16k.wav"
if [[ $K == auto ]]; then EXTRA=(--threshold "${THRESHOLD:-1.0}"); else EXTRA=(--num-clusters "$K"); fi
"$BIN" diarize --wav "$OUT/audio16k.wav" --segmentation "$SEG" --embedding "$EMB" "${EXTRA[@]}" --out "$OUT/turns.json"
"$BIN" timeline --turns "$OUT/turns.json" > "$OUT/timeline.txt"
"$BIN" split --wav "$OUT/audio16k.wav" --turns "$OUT/turns.json" --out-dir "$OUT"
echo "see $OUT/timeline.txt and $OUT/speaker_*.wav"
