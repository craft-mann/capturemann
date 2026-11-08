#!/usr/bin/env python3
"""A/V sync of a recording made in front of the rec-live-av.sh clip (flash and beep together every 2 s).

  rec-av-analyze.py <bundle.cmrec> [track ...]     tracks default to the mic and system tracks of the manifest
  rec-av-analyze.py --ref <video file> <audio file>   an independent reference recording (ffmpeg x11grab + pulse
                                                      monitor): timestamps are the files' own, audio starts at 0

For each audio track: flash frame time minus beep onset (ms, negative = picture first), over the whole
recording and per minute, so a drift shows. Also prints the video frame count and duration and the audio
duration. Needs ffmpeg and ffprobe.
"""
import json, statistics as st, struct, subprocess, sys

R = 120000.0


def flashes_of(video):
    fr = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=pts_time", "-of", "csv=p=0", video], capture_output=True, text=True).stdout.split()
    pts = [float(x.strip(",")) for x in fr if x.strip(",")]
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", video, "-vf", "scale=1:1:flags=area,format=gray", "-f", "rawvideo", "-"], capture_output=True).stdout
    out, prev = [], 0
    for i, b in enumerate(raw):
        if b > 128 and prev <= 128 and i < len(pts):
            out.append(pts[i])
        prev = b
    print(f"video: {len(pts)} frames, {pts[-1] - pts[0]:.2f} s, first pts {pts[0]:.3f}, flashes {len(out)}")
    return out


def report(name, path, rate, start, flashes):
    a = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-f", "f32le", "-ac", "1", "-ar", str(rate), "-"], capture_output=True).stdout
    n = len(a) // 4
    x = struct.unpack("<%df" % n, a[: n * 4])
    peak = max((abs(v) for v in x), default=0) or 1
    thr = 0.3 * peak
    onsets, quiet = [], 0
    for i, v in enumerate(x):
        if abs(v) > thr:
            if quiet > rate * 0.5:
                onsets.append(start + i / rate)
            quiet = 0
        else:
            quiet += 1
    diffs = []
    for f in flashes:
        best = min(onsets, key=lambda o: abs(o - f), default=None)
        if best is not None and abs(best - f) < 0.5:
            diffs.append((f, (f - best) * 1000))
    print(f"\n{name}: {n / rate:.2f} s of audio, peak {peak:.3f}, onsets {len(onsets)}, matched {len(diffs)}")
    if not diffs:
        return
    v = [d for _, d in diffs]
    print("  flash minus beep onset (ms): mean %.1f median %.1f sd %.1f min %.1f max %.1f" % (st.mean(v), st.median(v), st.pstdev(v), min(v), max(v)))
    per = {}
    for f, d in diffs:
        per.setdefault(int(f // 60), []).append(d)
    print("  per minute median:", {k: round(st.median(w), 1) for k, w in sorted(per.items())})


if sys.argv[1] == "--ref":
    report("reference", sys.argv[3], 48000, 0.0, flashes_of(sys.argv[2]))
    sys.exit(0)

d = sys.argv[1]
T = json.load(open(f"{d}/manifest.json"))["tracks"]
print("tracks:", {k: {kk: v[kk] for kk in v if kk in ("start", "end", "file_start", "rate", "channels", "file")} for k, v in T.items()})
fl = flashes_of(f"{d}/screen.mp4")
for name in sys.argv[2:] or [k for k in T if k in ("mic", "system")]:
    t = T[name]
    report(name, f"{d}/{t.get('file') or name + '.wav'}", t["rate"], t.get("file_start", t["start"]) / R, fl)
