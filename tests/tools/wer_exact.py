#!/usr/bin/env python3
"""WER-exact (case and punctuation folded, tone marks and 'đ' kept) of a transcript against a reference.

    python3 -I tests/tools/wer_exact.py --reference ref.txt --transcript transcript.json [--secs 300]

The reference may carry `Speaker N:` labels (ignored). Only transcript words that start before
`--secs` are scored (default: all). Prints one JSON line with the counts. Own implementation, so it
does not depend on the Rust `measure` module.
"""
import argparse, json, re, unicodedata


def words(text):
    text = unicodedata.normalize("NFC", text).lower()
    text = re.sub(r"speaker\s*\d+\s*:", " ", text)
    out = []
    for t in text.split():
        t = "".join(c for c in t if c.isalnum())
        if t:
            out.append(t)
    return out


def edit(ref, hyp):
    n, m = len(ref), len(hyp)
    prev = [(j, 0, 0, j) for j in range(m + 1)]  # cost, sub, del, ins
    for i in range(1, n + 1):
        cur = [(i, 0, i, 0)]
        for j in range(1, m + 1):
            if ref[i - 1] == hyp[j - 1]:
                c = prev[j - 1]
            else:
                p = prev[j - 1]
                c = (p[0] + 1, p[1] + 1, p[2], p[3])
            d = prev[j]
            d = (d[0] + 1, d[1], d[2] + 1, d[3])
            ins = cur[j - 1]
            ins = (ins[0] + 1, ins[1], ins[2], ins[3] + 1)
            cur.append(min(c, d, ins))
        prev = cur
    return prev[m]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reference", required=True)
    ap.add_argument("--transcript", required=True)
    ap.add_argument("--secs", type=float, default=1e9)
    a = ap.parse_args()
    ref = words(open(a.reference, encoding="utf-8").read())
    tr = json.load(open(a.transcript, encoding="utf-8"))
    hyp = []
    for w in tr["words"]:
        if w["start"] / 120000 < a.secs:
            hyp += words(w["text"])
    cost, sub, dele, ins = edit(ref, hyp)
    print(json.dumps({"ref_words": len(ref), "hyp_words": len(hyp), "wer_exact": round(cost / len(ref), 4),
                      "sub": sub, "del": dele, "ins": ins}))


if __name__ == "__main__":
    main()
