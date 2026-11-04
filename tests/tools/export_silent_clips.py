#!/usr/bin/env python3
"""Exports short clips of suspected-silent stretches for the maintainer to listen to (0.2.7, W2/W4).

    python3 -I tests/tools/export_silent_clips.py --out <dir outside the repo> --count 20
        --meeting nov2=<signals.json>=<audio file> [--meeting pat=... ...]

`signals.json` is the output of the `unit_signals` example (units with energy, Silero and
speaker-engine shares, and the quiet spans where all three agree there is no speech). Picks
  * every CANDIDATE unit (2 s or more, 4 words or more, at least two of the three shares below 0.25),
    up to 60 % of the clips, the weakest evidence first, and
  * CONTROLS: quiet spans of 3 s or more (no words needed), spread over the recordings,
and writes `NN.wav` (16 kHz mono, 5 to 10 s, peak-normalised to -3 dBFS so a faint voice can be
heard; the original RMS level is in clips.txt) plus `clips.txt`: number, meeting, start second,
length, what the detectors said (never words). The audio is private: keep `<dir>` outside the
repository. `apply_silent_answers.py` turns the listener's answers into reference-silent spans.
"""
import argparse, json, subprocess, sys
from pathlib import Path


def weak(u, th=0.4):
    vals = [u.get(k) for k in ("energy", "silero", "diar")]
    present = [v for v in vals if v is not None]
    return sum(1 for v in present if v < th), len(present)


def kind_of(u):
    """'candidate': 1.5 s or more, 3 words or more, two of the three shares below 0.4;
    'one-signal': 2 s or more, 4 words or more, one share below 0.1."""
    if u["b"] - u["a"] >= 1.5 and u["words"] >= 3 and weak(u, 0.4)[0] >= 2:
        return "candidate"
    if u["b"] - u["a"] >= 2.0 and u["words"] >= 4 and weak(u, 0.1)[0] >= 1:
        return "one-signal"
    return None


def window(a, b, total, lo=5.0, hi=10.0):
    """A window of 5 to 10 s that holds [a, b] (cut to 10 s if longer), centred, inside the file."""
    length = min(hi, max(lo, (b - a) + 2.0))
    mid = (a + b) / 2
    start = max(0.0, min(mid - length / 2, total - length))
    return start, min(length, total - start)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--count", type=int, default=20)
    ap.add_argument("--meeting", action="append", required=True, help="name=signals.json=audio")
    ap.add_argument("--ffmpeg", default="ffmpeg")
    ap.add_argument("--collapsed", action="append", default=[], help="name=transcript.json=N: N runs of 4+ words whose times have collapsed")
    a = ap.parse_args()
    cands, controls = [], []
    sources = {}
    for spec in a.meeting:
        name, sig, audio = spec.split("=", 2)
        d = json.load(open(sig))
        first = name not in sources
        sources.setdefault(name, (d, audio))
        for u in d["units"]:
            k = kind_of(u)
            if k and not any(c[3] == name and abs(c[4]["a"] - u["a"]) < 2.0 for c in cands):
                cands.append((0 if k == "candidate" else 1, 0, u["energy"] + (u.get("silero") or 0) + (u.get("diar") or 0), name, u))
        if first:
            for q, db in zip(d["quiet_spans"], d["quiet_db"]):
                controls.append((q[1] - q[0], name, q, db))
    cands.sort(key=lambda c: (c[0], c[2]))
    take_c = cands[: int(a.count * 0.6)]
    need = a.count - len(take_c)
    # controls: longest first, at most 3 per meeting at first, then fill; never two within 120 s
    controls.sort(key=lambda c: -c[0])
    chosen, per = [], {}
    for c in controls:
        if len(chosen) >= need:
            break
        if per.get(c[1], 0) >= 4 or any(abs(c[2][0] - o[2][0]) < 120 for o in chosen if o[1] == c[1]):
            continue
        per[c[1]] = per.get(c[1], 0) + 1
        chosen.append(c)
    items = []
    for w, n, s, name, u in take_c:
        total = sources[name][0]["audio_secs"]
        st, ln = window(u["a"], u["b"], total)
        items.append((name, st, ln, f"{'candidate' if w == 0 else 'one-signal'}: {u['words']} words in {u['b']-u['a']:.1f} s, energy {u['energy']:.2f}, "
                      f"silero {u.get('silero', float('nan')):.2f}, speaker engine {u.get('diar', float('nan')):.2f}, "
                      f"confidence {u['conf']:.2f}; words at {u['a']:.1f}-{u['b']:.1f}"))
    for dur, name, q, db in chosen:
        total = sources[name][0]["audio_secs"]
        st, ln = window(q[0], q[1], total)
        items.append((name, st, ln, f"control: no energy, Silero or speaker-engine speech for {dur:.1f} s ({q[0]:.1f}-{q[1]:.1f}), level {db:.0f} dBFS RMS, no words needed"))
    # Runs of four or more words, each 10 ms long or less: the engine lost the timing, which is how it
    # stores text it cannot place (invented text over silence has it, so does fast speech). The
    # clip ends 3 s after the run and reaches back 7 s, because the run's time is where the engine
    # gave up, not where the sound is.
    for spec in a.collapsed:
        name, tf, n = spec.split("=", 2)
        ws = json.load(open(tf))["words"]
        runs, cur = [], []
        for w in ws:
            if w["end"] - w["start"] <= 1200:
                cur.append(w)
            else:
                if len(cur) >= 4:
                    runs.append(cur)
                cur = []
        if len(cur) >= 4:
            runs.append(cur)
        runs.sort(key=lambda r: -len(r))
        picked = []
        for r in runs:
            t = r[0]["start"] / 120000
            if all(abs(t - q) > 60 for q in picked):
                picked.append(t)
            if len(picked) >= int(n):
                break
        for t in picked:
            r = next(r for r in runs if r[0]["start"] / 120000 == t)
            total = sources[name][0]["audio_secs"]
            st = max(0.0, t - 7.0)
            items.append((name, st, min(10.0, total - st), f"collapsed: {len(r)} words with no duration at {t:.1f} s; window reaches back 7 s from there"))
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    # shuffle deterministically so candidates and controls are not in blocks
    items.sort(key=lambda t: (hash((t[0], round(t[1]))) & 0xFFFF))
    lines = ["number\tmeeting\tstart_s\tlength_s\tdetectors"]
    for i, (name, st, ln, what) in enumerate(items, 1):
        audio = sources[name][1]
        subprocess.run([a.ffmpeg, "-nostdin", "-v", "error", "-y", "-ss", f"{st:.2f}", "-t", f"{ln:.2f}", "-i", audio,
                        "-ac", "1", "-ar", "16000", "-af", "loudnorm=I=-24:TP=-3:LRA=7", "-c:a", "pcm_s16le",
                        str(out / f"{i:02d}.wav")], check=True)
        lines.append(f"{i}\t{name}\t{st:.2f}\t{ln:.2f}\t{what}")
    (out / "clips.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(items)} clips in {out} ({len(take_c)} candidates, {len(chosen)} controls)")


if __name__ == "__main__":
    main()
