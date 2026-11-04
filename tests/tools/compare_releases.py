#!/usr/bin/env python3
"""Release comparison harness (0.2.6 vs 0.2.7). Decodes each row with each release's piece
pipeline, stores the transcripts, and scores them with `rescore` (folded and exact WER with
ins/del/sub, loops, dense repeats, sounds, coverage). Nothing is downloaded; private audio and
transcripts stay under --out, never in the repository.

  compare_releases.py run   --out DIR --rows ROWS.json [--only name,name] [--only-model turbo]
  compare_releases.py table --out DIR --rows ROWS.json

A row with "decode_as": "<other row>" reuses that row's transcripts and only scores a window of them ("from"/"until" seconds).
ROWS.json: {"models": {"turbo": ["large-v3-turbo", "/path/ggml-large-v3-turbo.bin"], ...},
 "bins": {"026": "/path/bench026", "027": "/path/asr_bench", "rescore": "/path/rescore"},
 "rows": [{"name": "..", "audio": "..wav", "reference": "..txt" or null, "language": "vi",
           "until": 300, "speech": "..json" or null, "runs": [["026", "turbo"], ["027", "turbo"], ["027", "large"]]}]}
Every decode is wrapped in `lockf /tmp/cm-gpu.lock` (one heavy process at a time on this Mac).
"""
import json, os, subprocess, sys

def sh(cmd, lock=True):
    if lock:
        cmd = ["lockf", "/tmp/cm-gpu.lock", "nice", "-n", "10"] + cmd
    return subprocess.run(cmd, capture_output=True, text=True)

def paths(out, row, rel, model):
    d = os.path.join(out, rel); os.makedirs(d, exist_ok=True)
    stem = os.path.join(d, f"{row.get('decode_as', row['name'])}__{model}")
    return stem + ".json", stem + ".time.json"

def run(cfg, out, only, only_model):
    for row in cfg["rows"]:
        if only and row["name"] not in only: continue
        if row.get("decode_as"): continue  # scored from another row's decode
        for rel, model in row["runs"]:
            if only_model and model != only_model: continue
            tr, tm = paths(out, row, rel, model)
            if os.path.exists(tm): continue
            mid, mpath = cfg["models"][model]
            if rel == "026":
                cmd = [cfg["bins"]["026"], "--audio", row["audio"], "--model", mpath, "--model-id", mid,
                       "--language", row["language"], "--out", tr]
            else:
                cmd = [cfg["bins"]["027"], "--audio", row["audio"], "--model", mpath, "--model-id", mid,
                       "--language", row["language"], "--lift", "--transcript-out", tr, "--label", row["name"]]
                cmd += ["--reference", row["reference"] or os.devnull]
            r = sh(cmd)
            line = (r.stdout.strip().splitlines() or ["{}"])[-1]
            try: json.loads(line)
            except Exception: print("FAILED", row["name"], rel, model, r.stderr[-300:]); continue
            open(tm, "w").write(line)
            print("done", row["name"], rel, model, flush=True)

def score(cfg, row, tr):
    cmd = [cfg["bins"]["rescore"], "--hyp", tr, "--language", row["language"]]
    if row.get("reference"): cmd += ["--reference", row["reference"]]
    if row.get("from"): cmd += ["--from", str(row["from"])]
    if row.get("until"): cmd += ["--until", str(row["until"])]
    if row.get("speech"): cmd += ["--speech", row["speech"]]
    return json.loads(sh(cmd, lock=False).stdout)

def table(cfg, out):
    for row in cfg["rows"]:
        print(f"\n## {row['name']}")
        for rel, model in row["runs"]:
            tr, tm = paths(out, row, rel, model)
            if not os.path.exists(tm): continue
            s = score(cfg, row, tr); t = json.load(open(tm))
            f, e = s.get("folded"), s.get("exact")
            cells = [f"{rel} {model}"]
            if f:
                cells.append(f"fold {100*f['wer']:.1f} ({f['sub']}/{f['del']}/{f['ins']})")
                cells.append(f"exact {100*e['wer']:.1f} ({e['sub']}/{e['del']}/{e['ins']})")
            cells.append(f"words {s['words']} loops {s['loop_runs']} dense {s['dense_repeat_words']} bare {s['bare_token_runs']} sounds {s['sounds']}")
            if "coverage" in s: cells.append(f"cov {s['coverage']:.3f} outside {s['outside_speech_words']}")
            cells.append(f"rtf {t.get('rtf', 0):.1f} rss {t.get('peak_rss_mb')}")
            print(" | ".join(cells))

if __name__ == "__main__":
    a = sys.argv[1:]
    def opt(n):
        return a[a.index(n) + 1] if n in a else None
    cfg = json.load(open(opt("--rows"))); out = opt("--out")
    if a[0] == "run":
        run(cfg, out, set((opt("--only") or "").split(",")) - {""}, opt("--only-model"))
    else:
        table(cfg, out)
