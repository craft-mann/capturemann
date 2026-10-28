#!/usr/bin/env python3
"""Cut 3 to 5 minute reference clips for the speaker and transcript accuracy measures (release 0.2.3).

    python3 -I tests/tools/make_reference_clips.py primock --out <dir> [--count 4]
    <venv>/bin/python tests/tools/make_reference_clips.py ami --out <dir> [--count 3]   # needs pyarrow

Each clip becomes `<name>.wav` (16 kHz mono 16-bit), `<name>.rttm.txt` (`start duration speaker`
per line, read by `cargo run -p cm-ai --example measure -- der`) and `<name>.txt` (the reference
words in time order, for `measure wer`). Clips start at a turn start and end at a turn end, so no
utterance is cut. Needs `ffmpeg` and `curl` on PATH. Downloads go to `<out>/cache`; keep <out>
outside the repository (the audio is not ours to redistribute, only numbers go in `verification/`).

Sources, both CC BY 4.0 as published (attribute them in any report):
  primock  PriMock57 (github.com/babylonhealth/primock57): doctor and patient recorded on separate
           channels with hand-timed utterances; the channels are mixed into one signal, which is
           what the speaker engine gets from a single microphone. 2 speakers, English.
  ami      AMI headset mix (huggingface.co/datasets/diarizers-community/ami_ihm_with_transcripts),
           test split. 3 to 4 speakers, English, overlapping speech.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

PRIMOCK_RAW = "https://raw.githubusercontent.com/babylonhealth/primock57/main/transcripts/"
PRIMOCK_LFS = "https://media.githubusercontent.com/media/babylonhealth/primock57/main/audio/"
AMI_TEST = (
    "https://huggingface.co/datasets/diarizers-community/ami_ihm_with_transcripts/"
    "resolve/main/data/test-{:05d}-of-00003.parquet"
)
MIN_SECS, MAX_SECS = 180.0, 300.0


def fetch(url: str, dest: Path) -> Path:
    if not dest.exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        tmp = dest.with_suffix(dest.suffix + ".part")
        subprocess.run(["curl", "-fsSL", "-o", str(tmp), url], check=True)
        tmp.rename(dest)
    return dest


def clean(text: str) -> str:
    """Words only: transcriber tags and bracketed events are not speech to compare."""
    text = re.sub(r"</?UNSURE>|<UNIN/>", " ", text)
    text = re.sub(r"[\[<{][^\]>}]*[\]>}]", " ", text)
    return " ".join(text.split())


def textgrid_turns(path: Path, speaker: str):
    body = path.read_text(encoding="utf-8")
    pat = re.compile(r'xmin = ([\d.]+)\s*\n\s*xmax = ([\d.]+)\s*\n\s*text = "(.*?)"\s*\n', re.S)
    out = []
    for a, b, text in pat.findall(body):
        words = clean(text)
        if words:
            out.append((float(a), float(b), speaker, words))
    return out


def window(turns, lo=MIN_SECS, hi=MAX_SECS):
    """The first stretch of `lo` to `hi` seconds that starts at a turn start and ends at a turn end.

    The longest one that fits is taken, so a clip holds as much conversation as allowed."""
    turns = sorted(turns)
    # Skip the opening (the call being set up) when there is enough left after it.
    for first in turns[len(turns) // 10 :] + turns:
        ends = [t[1] for t in turns if lo <= t[1] - first[0] <= hi]
        if ends:
            return first[0], max(ends)
    return None


def write_clip(out: Path, name: str, wav_cmd, turns, start: float, end: float):
    out.mkdir(parents=True, exist_ok=True)
    wav = out / f"{name}.wav"
    subprocess.run(
        wav_cmd(start, end - start, wav),
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    inside = sorted(t for t in turns if t[0] >= start - 1e-6 and t[1] <= end + 1e-6)
    with open(out / f"{name}.rttm.txt", "w", encoding="utf-8") as f:
        for a, b, who, _ in inside:
            f.write(f"{a - start:.3f} {b - a:.3f} {who}\n")
    with open(out / f"{name}.txt", "w", encoding="utf-8") as f:
        f.write("\n".join(t[3] for t in inside) + "\n")
    speakers = sorted({t[2] for t in inside})
    print(f"{name}: {end - start:.0f} s, {len(inside)} turns, speakers {speakers}")


def primock_ids():
    """Consultations spread over the five days (the same doctors recur within a day)."""
    for n in range(1, 16):
        for day in (1, 2, 3, 4, 5):
            yield f"day{day}_consultation{n:02d}"


def primock(out: Path, count: int):
    cache = out / "cache"
    done = 0
    for base in primock_ids():
        if done >= count:
            return
        try:
            turns = []
            for who in ("doctor", "patient"):
                tg = fetch(PRIMOCK_RAW + f"{base}_{who}.TextGrid", cache / f"{base}_{who}.TextGrid")
                turns += textgrid_turns(tg, who)
        except subprocess.CalledProcessError:
            continue  # that consultation does not exist
        w = window(turns)
        if not w:
            continue
        doctor = fetch(PRIMOCK_LFS + f"{base}_doctor.wav", cache / f"{base}_doctor.wav")
        patient = fetch(PRIMOCK_LFS + f"{base}_patient.wav", cache / f"{base}_patient.wav")

        def cmd(start, dur, wav, doctor=doctor, patient=patient):
            return [
                "ffmpeg", "-y",
                "-ss", f"{start:.3f}", "-t", f"{dur:.3f}", "-i", str(doctor),
                "-ss", f"{start:.3f}", "-t", f"{dur:.3f}", "-i", str(patient),
                "-filter_complex", "amix=inputs=2:normalize=0,aformat=sample_rates=16000:channel_layouts=mono",
                "-c:a", "pcm_s16le", str(wav),
            ]  # fmt: skip

        write_clip(out, f"primock_{base}", cmd, turns, *w)
        done += 1


def ami(out: Path, count: int):
    try:
        import pyarrow.parquet as pq
    except ImportError:
        sys.exit("ami needs pyarrow: python3 -m venv v && v/bin/pip install pyarrow, then run with v/bin/python")
    cache = out / "cache"
    done = 0
    for part in range(3):
        table = pq.read_table(fetch(AMI_TEST.format(part), cache / f"ami-test-{part}.parquet"))
        for row in table.to_pylist():
            if done >= count:
                return
            turns = [
                (a, b, s, clean(t))
                for a, b, s, t in zip(
                    row["timestamps_start"], row["timestamps_end"], row["speakers"], row["transcripts"]
                )
            ]
            turns = [t for t in turns if t[3]]
            w = window(turns)
            if not w:
                continue
            src = cache / f"{row['file_name']}.wav"
            if not src.exists():
                src.write_bytes(row["audio"]["bytes"])

            def cmd(start, dur, wav, src=src):
                return [
                    "ffmpeg", "-y", "-ss", f"{start:.3f}", "-t", f"{dur:.3f}", "-i", str(src),
                    "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", str(wav),
                ]  # fmt: skip

            write_clip(out, f"ami_{row['file_name']}", cmd, turns, *w)
            done += 1


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("source", choices=["primock", "ami"])
    p.add_argument("--out", required=True, type=Path)
    p.add_argument("--count", type=int, default=3)
    a = p.parse_args()
    (primock if a.source == "primock" else ami)(a.out, a.count)


if __name__ == "__main__":
    main()
