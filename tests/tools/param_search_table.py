#!/usr/bin/env python3
"""Summarise param_search JSONL: one row per cell, pooled WER-exact per group.
usage: param_search_table.py results.jsonl [--groups name=set,set;name=set]  (private rows: counts only)"""
import json, sys, collections

GROUPS = {
    "pat5": ["pat5"],
    "fleurs_vi": ["viu_a", "viu_b", "viu_c"],
    "en_syn4": ["syn4"],
    "noise": [f"noise{i}" for i in range(9)],
    "viu_h": ["viu_h"],
    "ami_b": ["ami_b"],
    "primock3": ["primock3"],
}
PRIVATE_SIGNAL = ["pat5", "nov2_10", "hop_10"]


def load(path):
    rows = [json.loads(l) for l in open(path) if l.startswith("{")]
    cells = collections.OrderedDict()
    for r in rows:
        cells.setdefault(r["cell"], {})[r["set"]] = r
    return cells


def pooled(sets, rs):
    got = [rs[s] for s in sets if s in rs and "wer_exact" in rs[s]]
    if not got:
        return None
    n = sum(r["reference_words"] for r in got)
    e = sum(r["ins"] + r["del"] + r["sub"] for r in got)
    f = sum(r["wer_fold"] * r["reference_words"] for r in got)
    return dict(exact=100 * e / n, fold=100 * f / n, ins=sum(r["ins"] for r in got), dele=sum(r["del"] for r in got), sub=sum(r["sub"] for r in got))


def main():
    cells = load(sys.argv[1])
    base = next(iter(cells.values()))
    names = list(GROUPS)
    print("cell | " + " | ".join(f"{g} exact/fold (i/d/s)" for g in names) + " | noise outside | loops | silent words (pat5/nov2/hop) | units/h nov2,hop | calls | rtf*")
    for c, rs in cells.items():
        out = [c]
        for g in names:
            p = pooled(GROUPS[g], rs)
            out.append("-" if p is None else f"{p['exact']:.1f}/{p['fold']:.1f} ({p['ins']}/{p['dele']}/{p['sub']})")
        out.append(str(sum(rs[s].get("outside_speech_words", 0) for s in GROUPS["noise"] if s in rs)))
        out.append(str(sum(r["loop_runs"] for r in rs.values())))
        out.append("/".join(str(rs[s]["silent_words"]) if s in rs and "silent_words" in rs[s] else "-" for s in PRIVATE_SIGNAL))
        out.append("/".join(f"{rs[s]['silent_units_per_hour']:.0f}" if s in rs and "silent_units_per_hour" in rs[s] else "-" for s in PRIVATE_SIGNAL[1:]))
        out.append(str(sum(r["whisper_calls"] for r in rs.values())))
        secs = sum(r["transcribe_secs"] for r in rs.values()); aud = sum(r["audio_secs"] for r in rs.values())
        out.append(f"{aud/secs:.1f}")
        print(" | ".join(out))


main()
