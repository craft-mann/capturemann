#!/usr/bin/env python3
"""Runs the PERF-1 benchmark matrix (release 0.2.4) and writes a Markdown table.

Each configuration is one process of `cargo run --release -p cm-ai --example perf` (peak memory is a
process high-water mark, so configurations must not share a process). Results are appended to a
JSONL file, so an interrupted run resumes and a finished one is not repeated.

    python3 -I tests/tools/perf_matrix.py --clips clips.json --models <models dir> --sherpa <dir> \\
        --out <dir> --suite models|threads|speakers|decode|all [--runs 3] [--max-busy 25]

`clips.json` (kept outside the repository, it names private recordings):
    {"s1": {"path": "/abs/file.wav", "lang": "vi", "note": "short, real"}, ...}
The report names clips by their id, length and language only. Needs a quiet machine: it waits
until the CPU is less than --max-busy percent in use by other work (the load average is not used: it
stays high on a Mac that runs a virtual machine) and records the busy percentage of every run.
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PERF = os.path.join(REPO, "target", "release", "examples", "perf")


def sh(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        return ""


def cpu_busy():
    """Percent of the CPU in use by everything else (two `top` samples, the second counts)."""
    out = sh("top", "-l", "2", "-n", "0", "-s", "1")
    line = [l for l in out.splitlines() if l.startswith("CPU usage")][-1]
    idle = float(line.split("sys,")[1].split("% idle")[0])
    return round(100 - idle, 1)


def machine():
    return {
        "chip": sh("sysctl", "-n", "machdep.cpu.brand_string"),
        "memory_gb": round(int(sh("sysctl", "-n", "hw.memsize") or 0) / 1e9),
        "cores": sh("sysctl", "-n", "hw.ncpu"),
        "perf_cores": sh("sysctl", "-n", "hw.perflevel0.physicalcpu"),
        "macos": sh("sysctl", "-n", "kern.osproductversion"),
        "power": sh("pmset", "-g", "batt").splitlines()[0] if sh("pmset", "-g", "batt") else "",
        "commit": sh("git", "-C", REPO, "rev-parse", "--short", "HEAD"),
        "busy_at_start": cpu_busy(),
    }


def configs(suite, clips, models, sherpa):
    out = []
    long_id = max(clips, key=lambda c: clips[c].get("secs", 0))
    mid_id = sorted(clips, key=lambda c: clips[c].get("secs", 0))[-2] if len(clips) > 1 else long_id
    short_id = min(clips, key=lambda c: clips[c].get("secs", 1e9))
    def tr(clip, model, extra=(), tag=""):
        m = os.path.join(models, f"ggml-{model}.bin")
        return {"task": "transcribe", "clip": clip, "model": model, "tag": tag,
                "args": ["--model", m, "--model-id", model, *extra]}
    if suite in ("models", "all"):
        for model in ("large-v3-turbo", "large-v3", "medium"):
            for c in (short_id, mid_id, long_id):
                out.append(tr(c, model))
    if suite in ("threads", "all"):
        for n in (4, 6, 8, 10, 14):
            out.append(tr(mid_id, "large-v3-turbo", ["--threads", str(n)], f"threads {n}"))
        out.append(tr(mid_id, "large-v3-turbo", ["--gpu", "off"], "gpu off"))
        out.append(tr(mid_id, "large-v3-turbo", ["--gpu", "off", "--threads", "10"], "gpu off, threads 10"))
    if suite in ("speakers", "all"):
        for c in (mid_id, long_id):
            out.append({"task": "speakers", "clip": c, "tag": "whole file", "args": ["--sherpa", sherpa]})
        for mins in (10, 15, 20, 30, 60):
            out.append({"task": "speakers", "clip": long_id, "tag": f"first {mins} min",
                        "args": ["--sherpa", sherpa, "--secs", str(mins * 60)]})
        out.append({"task": "speakers", "clip": long_id, "tag": "tiled to 150 min",
                    "args": ["--sherpa", sherpa, "--tile-to-secs", str(150 * 60)]})
    if suite in ("decode", "all"):
        for c in (mid_id, long_id):
            out.append({"task": "decode", "clip": c, "tag": "", "args": []})
        out.append({"task": "decode", "clip": long_id, "tag": "tiled to 150 min",
                    "args": ["--tile-to-secs", str(150 * 60)]})
    return out


def key(c):
    return json.dumps([c["task"], c["clip"], c.get("model"), c["tag"]])


def run_once(c, clips):
    clip = clips[c["clip"]]
    cmd = [PERF, c["task"], clip["path"], "--label", c["clip"], *c["args"]]
    if clip.get("lang") and c["task"] == "transcribe":
        cmd += ["--language", clip["lang"]]
    t = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        return {"error": p.stderr[-400:], "wall": time.time() - t}
    row = json.loads(p.stdout.strip().splitlines()[-1])
    row["wall"] = time.time() - t
    return row


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def fmt(x, nd=1):
    return "" if x is None else f"{x:.{nd}f}"


def report(rows, clips, mach):
    by = {}
    for r in rows:
        by.setdefault(r["key"], []).append(r)
    lines = ["# Performance, %s, %s" % (mach["chip"], time.strftime("%b %d, %Y")), "",
             "Machine: %s, %s GB, %s cores (%s performance), macOS %s, %s. Commit `%s`. "
             "Release build. Each row is the median of its runs (min to max shown for wall time); "
             "peak memory is the process high-water mark." % (
                 mach["chip"], mach["memory_gb"], mach["cores"], mach["perf_cores"], mach["macos"],
                 mach["power"], mach["commit"]), "",
             "Clips (ids only; the recordings are private and not published):", ""]
    for cid, c in sorted(clips.items()):
        lines.append("- `%s`: %s min, language %s, %s" % (cid, fmt(c.get("secs", 0) / 60), c.get("lang", "auto"), c.get("note", "")))
    for task, title, cols in (
        ("transcribe", "Transcription (Whisper, the app's piece-by-piece path)",
         ["clip", "model", "setting", "audio min", "runs", "wall s (min-max)", "x real time", "load s", "decode s", "peak MB", "words", "loops cut"]),
        ("speakers", "Speaker finding (speaker count unknown)",
         ["clip", "setting", "audio min", "runs", "wall s (min-max)", "x real time", "peak MB", "speakers"]),
        ("decode", "Decoding with FFmpeg to 16 kHz mono",
         ["clip", "setting", "audio min", "runs", "wall s (min-max)", "peak MB"]),
    ):
        sel = [(k, v) for k, v in by.items() if json.loads(k)[0] == task]
        if not sel:
            continue
        lines += ["", "## " + title, "", "| " + " | ".join(cols) + " |", "|" + "---|" * len(cols)]
        for k, v in sorted(sel, key=lambda kv: (json.loads(kv[0])[2] or "", clips[json.loads(kv[0])[1]].get("secs", 0), json.loads(kv[0])[3])):
            _, clip, model, tag = json.loads(k)
            ok = [r for r in v if "error" not in r]
            if not ok:
                lines.append("| %s | %s | %s | | failed: %s |" % (clip, model or "", tag, v[0].get("error", "")[:80].replace("|", "/")))
                continue
            secs = [r["decode_secs"] if task == "decode" else r["work_secs"] for r in ok]
            w = med(secs)
            audio = ok[0]["audio_secs"]
            rtf = audio / (med([r.get("transcribe_secs", r.get("speakers_secs", r.get("decode_secs"))) for r in ok]) or 1)
            common = [clip] + ([model] if task == "transcribe" else []) + [tag, fmt(audio / 60), str(len(ok)),
                      "%s (%s-%s)" % (fmt(w), fmt(min(secs)), fmt(max(secs)))]
            if task == "transcribe":
                row = common + [fmt(rtf), fmt(med([r["load_secs"] for r in ok]), 2), fmt(med([r["decode_secs"] for r in ok]), 1),
                                fmt(med([r["peak_rss_mb"] for r in ok]), 0), str(ok[0]["words"]), str(ok[0]["repeats_dropped"])]
            elif task == "speakers":
                row = common + [fmt(rtf), fmt(med([r["peak_rss_mb"] for r in ok]), 0), str(ok[0]["speakers"])]
            else:
                row = common + [fmt(med([r["peak_rss_mb"] for r in ok]), 0)]
            lines.append("| " + " | ".join(row) + " |")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--clips", required=True)
    ap.add_argument("--models", required=True)
    ap.add_argument("--sherpa", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--suite", default="all")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--long-runs", type=int, default=1, help="runs for inputs over 30 minutes")
    ap.add_argument("--max-busy", type=float, default=25.0, help="refuse to start above this percent CPU busy")
    ap.add_argument("--report-only", action="store_true")
    a = ap.parse_args()
    clips = json.load(open(a.clips))
    os.makedirs(a.out, exist_ok=True)
    jl = os.path.join(a.out, "runs.jsonl")
    rows = [json.loads(l) for l in open(jl)] if os.path.exists(jl) else []
    mach = machine()
    if not a.report_only:
        while cpu_busy() > a.max_busy:
            print("waiting for a quiet machine (CPU busy above %.0f%%)" % a.max_busy, flush=True)
            time.sleep(30)
        for c in configs(a.suite, clips, a.models, a.sherpa):
            n_have = sum(1 for r in rows if r["key"] == key(c))
            secs = clips[c["clip"]].get("secs", 0) * (150 / (clips[c["clip"]].get("secs", 1) / 60) if "tiled" in c["tag"] else 1)
            want = a.long_runs if (secs > 1800 or "tiled" in c["tag"]) else a.runs
            for i in range(n_have, want):
                load = cpu_busy()
                print("run", key(c), i + 1, "of", want, "busy %.0f%%" % load, flush=True)
                r = run_once(c, clips)
                r["key"] = key(c)
                r["busy_before"] = load
                rows.append(r)
                with open(jl, "a") as f:
                    f.write(json.dumps(r) + "\n")
    md = report(rows, clips, mach)
    open(os.path.join(a.out, "report.md"), "w").write(md)
    print(md)


main()
