#!/usr/bin/env python3
"""Makes a synthetic English meeting with known words, speakers and times, from macOS text-to-speech
(M2 baselines, release 0.2.7). Nothing is downloaded and no recording is used; the sentences are
generated here from word lists, the voices are the system's.

    python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn4 --speakers 4 --minutes 4 [--seed 1]

Writes to <dir> (keep it outside the repository, the audio is large):
  <name>.wav          16 kHz mono 16-bit
  <name>.txt          the reference words in time order (read by `measure wer`)
  <name>.rttm.txt     `start duration speaker` per turn (read by `measure der`)
  <name>.lines.json   [{"start", "end", "speaker", "text"}] caption-like lines (report/translation bench)
  <name>.speech.json  [[start, end], ...] where speech is (for the coverage measure)

Synthetic speech is clean and one voice per person: WER and DER here are far better than on a real room
and say nothing about accuracy on real recordings. Needs `say` (macOS) and `ffmpeg`.
"""
import argparse, json, random, subprocess, sys, tempfile, wave
from pathlib import Path

VOICES = ["Daniel", "Karen", "Samantha", "Rishi", "Moira", "Tessa", "Fred", "Ralph"]
NAMES = ["Alice", "Brandon", "Chloe", "Dmitri", "Elena", "Farid", "Grace", "Hector"]
TOPICS = ["the billing page", "the mobile release", "the search index", "the onboarding flow",
          "the quarterly budget", "the support backlog", "the new dashboard", "the export feature",
          "the security review", "the customer survey", "the hiring plan", "the data migration"]
OPENERS = ["I think", "We agreed that", "My concern is that", "The latest numbers show that",
           "Last week", "As a reminder,", "To be honest,", "From my side,", "Quick update:"]
MIDDLES = ["{t} is behind schedule by {n} days", "{t} needs {n} more people", "{t} costs about {n} thousand dollars",
           "{t} will be ready on the {o} of next month", "{t} has {n} open issues", "the team finished {t} in {n} weeks",
           "customers asked about {t} {n} times", "{t} should wait until {a} is back", "{a} will own {t}",
           "we should ask {a} and {b} to review {t}"]
CLOSERS = ["and that worries me.", "so let us plan for it.", "which is better than before.", "but we can fix that.",
           "and nobody objected.", "so I will write it down.", "and I would like a decision today.", "which sounds reasonable."]


ONES = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
        "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"]
TENS = ["", "", "twenty", "thirty", "forty"]
ORD = {"one": "first", "two": "second", "three": "third", "five": "fifth", "eight": "eighth", "nine": "ninth",
       "twelve": "twelfth"}


def words(n):
    """Numbers are written out, so the reference does not depend on how a transcriber spells them."""
    if n < 20:
        return ONES[n]
    return TENS[n // 10] + ("" if n % 10 == 0 else "-" + ONES[n % 10])


def ordinal(n):
    w = words(n)
    head, sep, last = w.rpartition("-")
    if last in ORD:
        last = ORD[last]
    elif last.endswith("y"):
        last = last[:-1] + "ieth"
    else:
        last += "th"
    return head + sep + last


def sentence(rng, people):
    a, b = rng.sample(people, 2) if len(people) > 1 else (people[0], people[0])
    mid = rng.choice(MIDDLES).format(t=rng.choice(TOPICS), n=words(rng.randint(2, 40)), o=ordinal(rng.randint(2, 28)), a=a, b=b)
    return f"{rng.choice(OPENERS)} {mid} {rng.choice(CLOSERS)}"


def say(voice, text, rate, path):
    subprocess.run(["say", "-v", voice, "-r", str(rate), "-o", str(path), "--data-format=LEI16@16000", text], check=True)


def pcm(path):
    with wave.open(str(path)) as w:
        assert w.getframerate() == 16000 and w.getnchannels() == 1
        return w.readframes(w.getnframes())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--speakers", type=int, default=4)
    ap.add_argument("--minutes", type=float, default=4.0)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--rate", type=int, default=175, help="words per minute for say")
    a = ap.parse_args()
    if not 1 <= a.speakers <= len(VOICES):
        sys.exit(f"--speakers 1..{len(VOICES)}")
    rng = random.Random(a.seed)
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    voices = VOICES[: a.speakers]
    people = NAMES[: max(a.speakers, 2)]
    want = int(a.minutes * 60 * 16000)
    audio, lines, turns, speech = bytearray(), [], [], []
    last = None
    with tempfile.TemporaryDirectory() as tmp:
        n = 0
        while len(audio) // 2 < want:
            who = rng.choice([i for i in range(a.speakers) if i != last] or [0])
            last = who
            text = " ".join(sentence(rng, people) for _ in range(rng.randint(1, 3)))
            f = Path(tmp) / f"{n}.wav"
            n += 1
            # Each turn is one `say` call; a sentence boundary is a pause inside the voice.
            say(voices[who], text, a.rate, f)
            data = pcm(f)
            start = len(audio) / 2 / 16000
            audio += data
            end = len(audio) / 2 / 16000
            lines.append({"start": round(start, 3), "end": round(end, 3), "speaker": f"Speaker {who + 1}", "text": text})
            turns.append((start, end - start, f"S{who + 1}"))
            speech.append([round(start, 3), round(end, 3)])
            audio += b"\x00\x00" * int(16000 * rng.uniform(0.3, 1.6))
    wav = out / f"{a.name}.wav"
    with wave.open(str(wav), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(bytes(audio))
    (out / f"{a.name}.txt").write_text(" ".join(l["text"] for l in lines) + "\n")
    (out / f"{a.name}.rttm.txt").write_text("".join(f"{s:.3f} {d:.3f} {w}\n" for s, d, w in turns))
    (out / f"{a.name}.lines.json").write_text(json.dumps(lines, indent=1))
    (out / f"{a.name}.speech.json").write_text(json.dumps(speech))
    print(f"{wav}: {len(audio) / 2 / 16000:.1f} s, {len(lines)} turns, {a.speakers} voices, {sum(len(l['text'].split()) for l in lines)} words")


main()
