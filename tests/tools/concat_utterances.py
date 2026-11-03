#!/usr/bin/env python3
"""Joins short corpus utterances into 3 to 5 minute clips with a reference text, for `asr_bench`
(release 0.2.7, M2a and M2e). It downloads nothing: you fetch a corpus yourself (see
verification/benchmarks, the data section of a report, for what and under which licence), keep it
outside the repository, and list the utterances in a TSV.

    python3 -I tests/tools/concat_utterances.py --tsv <list.tsv> --out <dir> --name vi_cv --secs 240 [--gap 0.6]

`list.tsv` has one utterance per line: `<audio file path><TAB><reference text>` (Common Voice's
`validated.tsv` columns, LibriSpeech transcripts or FLEURS rows reduce to this in one line of awk or
Python). Utterances are taken in file order until `--secs` is reached; `--skip N` starts later in the
list, so a tuning clip and a held-out clip never share an utterance. Writes
<name>.wav (16 kHz mono 16-bit), <name>.txt (reference words) and <name>.speech.json
([[start, end], ...] per utterance, for the coverage figure). Needs ffmpeg.
"""
import argparse, json, subprocess, sys, tempfile, wave
from pathlib import Path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tsv", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--secs", type=float, default=240.0)
    ap.add_argument("--gap", type=float, default=0.6, help="silence between utterances, seconds")
    ap.add_argument("--skip", type=int, default=0)
    a = ap.parse_args()
    rows = [l.rstrip("\n").split("\t", 1) for l in Path(a.tsv).read_text(encoding="utf-8").splitlines() if "\t" in l]
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    pcm, texts, speech = bytearray(), [], []
    with tempfile.TemporaryDirectory() as tmp:
        for i, (path, text) in enumerate(rows[a.skip:]):
            if len(pcm) / 32000 >= a.secs:
                break
            wav = Path(tmp) / f"{i}.wav"
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", path, "-ar", "16000", "-ac", "1", "-sample_fmt", "s16", str(wav)], check=True)
            with wave.open(str(wav)) as w:
                data = w.readframes(w.getnframes())
            start = len(pcm) / 32000
            pcm += data
            speech.append([round(start, 3), round(len(pcm) / 32000, 3)])
            texts.append(" ".join(text.split()))
            pcm += b"\x00\x00" * int(16000 * a.gap)
    if not texts:
        sys.exit("no utterances")
    with wave.open(str(out / f"{a.name}.wav"), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(bytes(pcm))
    (out / f"{a.name}.txt").write_text(" ".join(texts) + "\n", encoding="utf-8")
    (out / f"{a.name}.speech.json").write_text(json.dumps(speech))
    print(f"{a.name}: {len(pcm) / 32000:.1f} s, {len(texts)} utterances, {sum(len(t.split()) for t in texts)} words")


main()
