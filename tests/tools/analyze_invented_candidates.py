#!/usr/bin/env python3
"""Where do units with weak speech evidence sit? (0.2.7 plan, W2; counts only, no words.)

    python3 -I tests/tools/analyze_invented_candidates.py <signals.json> [<signals.json> ...]

`signals.json` is the output of `cargo run --release -p cm-ai --example unit_signals`: the units of
a transcript with their energy, Silero and speaker-engine shares, the piece plan and the quiet
spans. A unit is a CANDIDATE when it lasts 2 s or more, has 4 words or more, and at least two of
the three shares are below 0.25 (STRICT: all three present and below 0.25). Prints, per file, the
unit and candidate counts and where the candidates sit relative to the 300 s pieces: in the first
or last 30 s, the last 60 s, next to an isolated stretch (a gap of 5 s or more to the next word),
against the share of ALL units in the same place (so a place that merely holds many units does not
look special). No text is read or printed.
"""
import json, sys

WEAK = 0.25


def weak_count(u):
    vals = [u.get(k) for k in ("energy", "silero", "diar")]
    present = [v for v in vals if v is not None]
    return sum(1 for v in present if v < WEAK), len(present)


def classify(units, pieces):
    rows = []
    for i, u in enumerate(units):
        mid = (u["a"] + u["b"]) / 2
        pi = next((k for k, (a, b) in enumerate(pieces) if a <= mid < b), len(pieces) - 1)
        a, b = pieces[pi]
        gap_prev = u["a"] - units[i - 1]["b"] if i else 1e9
        gap_next = units[i + 1]["a"] - u["b"] if i + 1 < len(units) else 1e9
        rows.append({
            "piece": pi, "from_start": mid - a, "to_end": b - mid,
            "first30": mid - a < 30, "last30": b - mid < 30, "last60": b - mid < 60,
            "isolated": max(gap_prev, gap_next) >= 5.0,
            "first_piece": pi == 0, "last_piece": pi == len(pieces) - 1,
        })
    return rows


def main():
    for path in sys.argv[1:]:
        d = json.load(open(path))
        units, pieces = d["units"], [tuple(p) for p in d["pieces"]]
        rows = classify(units, pieces)
        cand, strict = [], []
        for i, u in enumerate(units):
            w, n = weak_count(u)
            if u["b"] - u["a"] >= 2.0 and u["words"] >= 4 and w >= 2:
                cand.append(i)
                if n == 3 and w == 3:
                    strict.append(i)
        hours = d["audio_secs"] / 3600
        print(f"== {path}")
        print(f"audio {d['audio_secs']/60:.1f} min, {len(pieces)} pieces, words {d['words']}, units {len(units)}, "
              f"quiet spans (3 s+, all three signals) {len(d['quiet_spans'])}")
        print(f"candidates {len(cand)} ({len(cand)/hours:.1f}/h), strict {len(strict)} ({len(strict)/hours:.1f}/h)")
        for name in ("first30", "last30", "last60", "isolated", "first_piece", "last_piece"):
            allc = sum(r[name] for r in rows)
            c = sum(rows[i][name] for i in cand)
            s = sum(rows[i][name] for i in strict)
            print(f"  {name:12s} all units {allc:4d} ({100*allc/max(1,len(rows)):4.1f}%)  candidates {c:3d} "
                  f"({100*c/max(1,len(cand)):4.1f}%)  strict {s:3d} ({100*s/max(1,len(strict)):4.1f}%)")
        if cand:
            ends = sorted(rows[i]["to_end"] for i in cand)
            print("  candidate distance to piece end (s): " + " ".join(f"{x:.0f}" for x in ends[:40]))


if __name__ == "__main__":
    main()
