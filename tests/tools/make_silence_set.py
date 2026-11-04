#!/usr/bin/env python3
"""A public, synthetic set that tries to PROVOKE invented text over silence (0.2.7 plan, W4).

    python3 -I tests/tools/make_silence_set.py --out <dir> --utterances <list.tsv> [--lang vi]
        [--en-wav <meeting.wav> --en-lines <meeting.lines.json>] [--seed 1] [--only <family>]

Needs numpy and scipy (`python3 -m venv v && v/bin/pip install numpy scipy`) and ffmpeg. No audio
is downloaded: speech comes from the `path<TAB>reference` list that `concat_utterances.py` reads
(FLEURS vi_vn, CC BY 4.0) and, for English, from the lines of a synthetic meeting
(`make_synthetic_meeting.py`: text-to-speech, no human voice). Noise, room tone, music-like tones,
breaths, clicks and room impulse responses are generated here (exponentially decaying noise with a
direct path; the OpenSLR RIRs and MUSAN were not needed to build a reproduction).

Every clip is `<name>.wav` (16 kHz mono 16-bit), `<name>.txt` (the reference words),
`<name>.speech.json` (the reference speech spans, seconds); everything outside them, widened by
0.5 s, is reference-silent. `clips.json` lists the construction of each clip. Count with
`cargo run --release -p cm-ai --example silence_probe -- --dir <dir> ...`.

Families (what is being provoked):
  tail   near speech (-30 dBFS) then a long stretch of one filler type (room tone -45/-55/-60,
         digital zero, +-1 LSB, room tone with clicks and breaths, music-like tones), 30 s window
  long   120 s: 15 s of speech, then 105 s of the filler (several windows of nothing)
  gaps   90 s: speech and 20 to 30 s gaps of room tone, three times (speech back after silence)
  far    far field: speech convolved with a synthetic room (RT60 0.5/0.9 s), -35 dBFS, level steps of
         +-6 dB between utterances, noise at -50 dBFS, 25 to 30 s gaps
  lead   silence first (20 s), then speech, then silence
  quiet  speaker at -45 dBFS over room tone at -55 dBFS (SNR 10 dB), long gaps
  end31  speech until just past 30 s (the second window is almost all zero padding)
"""
import argparse, json, subprocess, sys, wave
from pathlib import Path

import numpy as np
from scipy import signal

SR = 16000


def db(x):
    return 10 ** (x / 20)


def load(path, start=None, end=None):
    cmd = ["ffmpeg", "-v", "error"]
    if start is not None:
        cmd += ["-ss", str(start), "-to", str(end)]
    cmd += ["-i", str(path), "-ar", str(SR), "-ac", "1", "-f", "f32le", "-"]
    return np.frombuffer(subprocess.run(cmd, capture_output=True, check=True).stdout, dtype=np.float32).astype(np.float64)


def rms(x):
    return float(np.sqrt(np.mean(x ** 2) + 1e-20))


def active_rms(x):
    """RMS over the frames within 25 dB of the loudest (speech, not its pauses)."""
    f = 400
    n = len(x) // f
    if n == 0:
        return rms(x)
    fr = x[: n * f].reshape(n, f)
    e = np.sqrt((fr ** 2).mean(axis=1) + 1e-20)
    keep = e > e.max() * db(-25)
    return float(np.sqrt((fr[keep] ** 2).mean()))


def set_level(x, level_db):
    return x * (db(level_db) / active_rms(x))


# ---- fillers (silence types) -------------------------------------------------------------

def pink(n, rng):
    w = rng.standard_normal(n + 4096)
    f = np.fft.rfft(w)
    k = np.arange(len(f))
    k[0] = 1
    f = f / np.sqrt(k)
    x = np.fft.irfft(f)[:n]
    return x / (np.std(x) + 1e-12)


def room_tone(n, rng, level_db, hum=True):
    """Pink noise rolled off above 6 kHz, a little 50 Hz mains hum and harmonics, slow level drift."""
    x = pink(n, rng)
    sos = signal.butter(2, 6000, "lowpass", fs=SR, output="sos")
    x = signal.sosfilt(sos, x)
    if hum:
        t = np.arange(n) / SR
        x = x + 0.25 * (np.sin(2 * np.pi * 50 * t) + 0.5 * np.sin(2 * np.pi * 100 * t + 1) + 0.25 * np.sin(2 * np.pi * 150 * t + 2))
    drift = 1 + 0.15 * np.sin(2 * np.pi * rng.uniform(0.02, 0.1) * np.arange(n) / SR)
    x = x * drift
    return x / rms(x) * db(level_db)


def transients(n, rng, level_db):
    """Room tone plus clicks (3 ms decaying bursts) and breath-like swells (band-passed noise, 0.4 to 0.9 s)."""
    x = room_tone(n, rng, level_db)
    t = 2.0
    while t < n / SR - 1.5:
        i = int(t * SR)
        if rng.random() < 0.5:
            m = int(0.003 * SR)
            burst = rng.standard_normal(m) * np.exp(-np.arange(m) / (0.0008 * SR))
            x[i : i + m] += burst * db(-38) / (rms(burst) + 1e-9) * 0.7
        else:
            m = int(rng.uniform(0.4, 0.9) * SR)
            b = signal.sosfilt(signal.butter(2, [250, 2500], "bandpass", fs=SR, output="sos"), rng.standard_normal(m))
            env = np.hanning(m)
            x[i : i + m] += b / (rms(b) + 1e-9) * env * db(level_db + 8)
        t += rng.uniform(3.0, 9.0)
    return x


def music_like(n, rng, level_db):
    """Slow chord changes of a few harmonic tones with an attack/decay envelope, over faint room tone."""
    out = np.zeros(n)
    t = np.arange(n) / SR
    notes = [220.0, 277.2, 329.6, 261.6, 329.6, 392.0, 196.0, 246.9, 293.7]
    step = 2.0
    for k in range(int(n / SR / step) + 1):
        root = notes[(k * 3 + int(rng.integers(0, 3))) % len(notes)]
        a, b = int(k * step * SR), min(n, int((k + 1) * step * SR + SR // 2))
        if a >= n:
            break
        tt = t[a:b] - t[a]
        env = np.minimum(1, tt / 0.05) * np.exp(-tt / 1.2)
        tone = sum(np.sin(2 * np.pi * root * h * tt) / h for h in (1, 2, 3, 4)) * (1 + 0.4 * np.sin(2 * np.pi * 5 * tt))
        out[a:b] += tone * env
    out = out / rms(out) * db(level_db)
    return out + room_tone(n, rng, level_db - 15)


def digital_zero(n, rng, level_db):
    return np.zeros(n)


def lsb_noise(n, rng, level_db):
    return rng.integers(-1, 2, n).astype(np.float64) / 32768


FILLERS = {
    "tone45": lambda n, r: room_tone(n, r, -45),
    "tone55": lambda n, r: room_tone(n, r, -55),
    "tone60": lambda n, r: room_tone(n, r, -60),
    "zero": lambda n, r: digital_zero(n, r, 0),
    "lsb": lambda n, r: lsb_noise(n, r, 0),
    "trans50": lambda n, r: transients(n, r, -50),
    "music45": lambda n, r: music_like(n, r, -45),
}


# ---- room impulse response ----------------------------------------------------------------

def make_rir(rt60, rng, drr_db=-3.0):
    n = int(rt60 * 1.2 * SR)
    t = np.arange(n) / SR
    tail = rng.standard_normal(n) * np.exp(-6.9 * t / rt60)
    # air absorption: later reflections lose highs
    tail = signal.sosfilt(signal.butter(1, 5000, "lowpass", fs=SR, output="sos"), tail)
    tail[: int(0.004 * SR)] = 0
    tail = tail / np.sqrt(np.sum(tail ** 2))
    h = tail * db(-drr_db)  # direct = 1, tail energy = 10^(-drr/20)
    h[0] += 1.0
    return h


def reverb(x, rir):
    return signal.fftconvolve(x, rir)[: len(x) + len(rir) // 2]


# ---- assembly -------------------------------------------------------------------------------

def to_int16(x):
    return np.clip(np.round(x * 32768), -32768, 32767).astype(np.int16)


def write(out, name, audio, spans, ref, meta):
    with wave.open(str(out / f"{name}.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(to_int16(audio).tobytes())
    (out / f"{name}.txt").write_text(" ".join(ref.split()) + "\n", encoding="utf-8")
    (out / f"{name}.speech.json").write_text(json.dumps([[round(a, 3), round(b, 3)] for a, b in spans]))
    meta["duration"] = round(len(audio) / SR, 2)
    meta["speech_spans"] = [[round(a, 3), round(b, 3)] for a, b in spans]
    return {name: meta}


class Pool:
    def __init__(self, items, rng):
        self.items, self.rng, self.i = items, rng, 0
        self.order = rng.permutation(len(items))

    def next(self):
        item = self.items[int(self.order[self.i % len(self.items)])]
        self.i += 1
        return item


def place(parts, total_secs, filler, rng, gain_steps=False, rir=None, speech_db=-30.0, noise_db=None):
    """parts: list of (start_sec, audio, text). Returns the clip, spans, reference."""
    n = int(total_secs * SR)
    bed = filler(n, rng)
    clip = bed.copy()
    spans, ref = [], []
    cur_gain = 0.0
    for start, a, text in parts:
        if gain_steps:
            cur_gain = float(rng.choice([-6, -3, 0, 3, 6]))
        x = reverb(a, rir) if rir is not None else a
        x = set_level(x, speech_db + cur_gain)
        i = int(start * SR)
        end = min(n, i + len(x))
        clip[i:end] += x[: end - i]
        spans.append((start, min(total_secs, start + len(a) / SR)))
        ref.append(text)
    return clip, spans, " ".join(ref)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--utterances", required=True, help="path<TAB>reference list (FLEURS)")
    ap.add_argument("--lang", default="vi")
    ap.add_argument("--en-wav")
    ap.add_argument("--en-lines")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--only", help="a family name")
    a = ap.parse_args()
    rng = np.random.default_rng(a.seed)
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)

    rows = [l.rstrip("\n").split("\t", 1) for l in Path(a.utterances).read_text(encoding="utf-8").splitlines() if "\t" in l]
    utts = []
    for path, text in rows:
        x = load(path)
        if 4.0 <= len(x) / SR <= 14.0:
            utts.append((x, text))
        if len(utts) >= 90:
            break
    pools = {a.lang: Pool(utts, rng)}
    if a.en_wav and a.en_lines:
        lines = json.loads(Path(a.en_lines).read_text())
        en = []
        for l in lines:
            if 4.0 <= l["end"] - l["start"] <= 14.0:
                en.append((load(a.en_wav, l["start"], l["end"]), l["text"]))
        pools["en"] = Pool(en, rng)
    langs = list(pools)
    meta_all = {}

    def clip_for(family, lang, name, build, meta):
        if a.only and a.only != family:
            return
        c, spans, ref = build()
        meta.update({"family": family, "lang": lang})
        meta_all.update(write(out, name, c, spans, ref, meta))

    for lang in langs:
        pool = pools[lang]

        # tail: every filler, two lengths of speech
        for fk, fn in FILLERS.items():
            for rep in range(2):
                def build(fn=fn, rep=rep):
                    u1, t1 = pool.next()
                    parts = [(0.5, u1, t1)]
                    if rep == 1:
                        u2, t2 = pool.next()
                        parts.append((0.5 + len(u1) / SR + 1.0, u2, t2))
                    # keep the speech inside the first 20 s
                    parts = [p for p in parts if p[0] + len(p[1]) / SR < 22]
                    return place(parts, 30, fn, rng)
                clip_for("tail", lang, f"tail_{lang}_{fk}_{rep}", build, {"filler": fk, "speech_db": -30})

        # long: 120 s with 105 s of filler
        for fk in ("tone45", "tone55", "trans50", "music45", "lsb"):
            def build(fn=FILLERS[fk]):
                u1, t1 = pool.next()
                u2, t2 = pool.next()
                parts = [(1.0, u1, t1), (1.0 + len(u1) / SR + 0.8, u2, t2)]
                return place(parts, 120, fn, rng)
            clip_for("long", lang, f"long_{lang}_{fk}", build, {"filler": fk, "speech_db": -30})

        # gaps: speech, gap, speech, gap, speech, gap (90 s)
        for fk in ("tone45", "tone55", "trans50"):
            for rep in range(2):
                def build(fn=FILLERS[fk]):
                    parts, t = [], 1.0
                    for gap in (22, 28, 24):
                        u, tx = pool.next()
                        parts.append((t, u, tx))
                        t += len(u) / SR + gap
                    return place(parts, 90, fn, rng, gain_steps=True)
                clip_for("gaps", lang, f"gaps_{lang}_{fk}_{rep}", build, {"filler": fk, "speech_db": -30, "gain_steps": True})

        # far field: reverberant speech at -35 dBFS, level steps, long gaps, noise -50
        for rt in (0.5, 0.9):
            for fk in ("tone45", "tone55"):
                def build(rt=rt, fn=FILLERS[fk]):
                    rir = make_rir(rt, rng)
                    parts, t = [], 1.0
                    for gap in (26, 30, 26):
                        u, tx = pool.next()
                        parts.append((t, u, tx))
                        t += len(u) / SR + gap
                    return place(parts, 100, fn, rng, gain_steps=True, rir=rir, speech_db=-35)
                clip_for("far", lang, f"far_{lang}_rt{int(rt*100)}_{fk}", build, {"filler": fk, "rt60": rt, "speech_db": -35, "gain_steps": True})

        # lead: silence first
        for fk in ("tone55", "trans50"):
            def build(fn=FILLERS[fk]):
                u, tx = pool.next()
                return place([(20.0, u, tx)], 20 + 12 + 25, fn, rng)
            clip_for("lead", lang, f"lead_{lang}_{fk}", build, {"filler": fk, "speech_db": -30})

        # quiet speaker over room tone, SNR 10 dB
        for rep in range(3):
            def build(fn=lambda n, r: room_tone(n, r, -55)):
                parts, t = [], 1.0
                for gap in (24, 30):
                    u, tx = pool.next()
                    parts.append((t, u, tx))
                    t += len(u) / SR + gap
                return place(parts, 75, fn, rng, speech_db=-45)
            clip_for("quiet", lang, f"quiet_{lang}_{rep}", build, {"filler": "tone55", "speech_db": -45})

        # end31: speech until just past one 30 s window, then a short tail
        for rep in range(2):
            def build(fn=lambda n, r: room_tone(n, r, -50)):
                parts, t = [], 0.5
                while t < 31:
                    u, tx = pool.next()
                    parts.append((t, u, tx))
                    t += len(u) / SR + 0.7
                total = t + 4.0
                return place(parts, total, fn, rng)
            clip_for("end31", lang, f"end31_{lang}_{rep}", build, {"filler": "tone50", "speech_db": -30})

    (out / "clips.json").write_text(json.dumps(meta_all, indent=1))
    tot = sum(m["duration"] for m in meta_all.values())
    print(f"{len(meta_all)} clips, {tot / 60:.1f} min in {out}")


if __name__ == "__main__":
    main()
