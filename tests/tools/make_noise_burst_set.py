#!/usr/bin/env python3
"""A public-data set for invented text over non-speech (0.2.7 transcription review, step 1).

    python3 -I tests/tools/make_noise_burst_set.py --utterances <list.tsv> --out <dir> [--count 8]

Each clip is 30 s of synthetic room noise (pink or brown noise) at -40, -45 or -50 dBFS RMS with one
FLEURS utterance (from the `path<TAB>reference` list that `concat_utterances.py` reads, 5 to 12 s
long) mixed in at 12 s at about -30 dBFS RMS. Writes `<name>.wav`, `<name>.txt` (the reference words)
and `<name>.speech.json` (the one speech span).
The noise is generated with FFmpeg (`anoisesrc`), not taken from MUSAN or DEMAND (large downloads
that were not fetched); the report says so. Needs ffmpeg.
"""
import argparse, json, subprocess, sys, wave
from pathlib import Path

COLORS = ["pink", "brown", "pink"]
LEVELS = [-40, -45, -50]


def duration(path):
    r = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
                       capture_output=True, text=True, check=True)
    return float(r.stdout.strip())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--utterances", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--count", type=int, default=8)
    ap.add_argument("--skip", type=int, default=100)
    a = ap.parse_args()
    rows = [l.rstrip("\n").split("\t", 1) for l in Path(a.utterances).read_text(encoding="utf-8").splitlines() if "\t" in l]
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    made = 0
    for path, text in rows[a.skip:]:
        d = duration(path)
        if not 5.0 <= d <= 12.0:
            continue
        i = made
        level = LEVELS[i % 3]
        color = COLORS[(i // 3) % 3]
        name = f"noise_{i:02d}_{color}_{-level}"
        amp = 10 ** ((level + 9.4) / 20) * 1.732  # anoisesrc's amplitude is a peak value: +9.4 dB gives the RMS named
        wav = out / f"{name}.wav"
        speech_start = 12.0
        subprocess.run([
            "ffmpeg", "-v", "error", "-y",
            "-f", "lavfi", "-i", f"anoisesrc=color={color}:amplitude={amp:.5f}:duration=30:sample_rate=16000:seed={i + 1}",
            "-i", path,
            "-filter_complex",
            f"[1:a]aresample=16000,aformat=channel_layouts=mono,loudnorm=I=-30:TP=-3:LRA=7,adelay={int(speech_start*1000)}|{int(speech_start*1000)}[s];[0:a][s]amix=inputs=2:normalize=0,atrim=0:30",
            "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", str(wav)], check=True)
        (out / f"{name}.txt").write_text(" ".join(text.split()) + "\n", encoding="utf-8")
        (out / f"{name}.speech.json").write_text(json.dumps([[speech_start, round(speech_start + d, 3)]]))
        made += 1
        if made >= a.count:
            break
    print(f"{made} clips in {out}")


main()
