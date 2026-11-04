#!/usr/bin/env python3
"""Turns the maintainer's answers about the silent-stretch clips into reference-silent spans.

    python3 -I tests/tools/apply_silent_answers.py --clips <clips.txt> --speaks "3 7 12" --out <dir>
    python3 -I tests/tools/apply_silent_answers.py --clips <clips.txt> --answers <file> --out <dir>

`clips.txt` is what the clip exporter wrote: a header line, then one tab-separated line per clip,
`number  meeting  start_s  length_s  detectors`. The answers are the numbers of the clips where
someone speaks (or any sound the listener counts as speech); every other clip is taken as silent,
unless it is listed after `unsure:` in the answers file (such clips are left out of both lists).
In an answers file, numbers are separated by spaces, commas or lines; `#` starts a comment.

Writes `<out>/<meeting>.silent.json` (a list of `[start, end]` seconds that `silence_probe
--silent` reads) and `<out>/<meeting>.speech.json` (the clips marked as speech, same shape).
Spans only, no text: the files describe private recordings and stay outside the repository.
"""
import argparse, json, re
from pathlib import Path


def numbers(text):
    return [int(n) for n in re.findall(r"\d+", text)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--clips", required=True)
    ap.add_argument("--speaks", default="")
    ap.add_argument("--answers")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    speaks = set(numbers(a.speaks))
    unsure = set()
    if a.answers:
        body = ""
        for line in Path(a.answers).read_text(encoding="utf-8").splitlines():
            body += line.split("#", 1)[0] + "\n"
        head, _, tail = body.partition("unsure:")
        speaks |= set(numbers(head))
        unsure = set(numbers(tail))
    rows = []
    for line in Path(a.clips).read_text(encoding="utf-8").splitlines()[1:]:
        f = line.split("\t")
        if len(f) >= 4 and f[0].strip().isdigit():
            rows.append((int(f[0]), f[1], float(f[2]), float(f[3])))
    known = {r[0] for r in rows}
    bad = (speaks | unsure) - known
    if bad:
        raise SystemExit(f"not in clips.txt: {sorted(bad)}")
    silent, speech = {}, {}
    for n, meeting, start, length in rows:
        if n in unsure:
            continue
        d = speech if n in speaks else silent
        d.setdefault(meeting, []).append([round(start, 3), round(start + length, 3)])
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    for meeting in sorted(set(silent) | set(speech)):
        (out / f"{meeting}.silent.json").write_text(json.dumps(silent.get(meeting, [])))
        (out / f"{meeting}.speech.json").write_text(json.dumps(speech.get(meeting, [])))
        print(f"{meeting}: {len(silent.get(meeting, []))} silent, {len(speech.get(meeting, []))} speech")


if __name__ == "__main__":
    main()
