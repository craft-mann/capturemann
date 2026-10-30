#!/usr/bin/env python3
"""Makes longer stand-ins of a transcribed recording for the text-model benchmark (PERF-1).

    python3 -I tests/tools/text_bench_tile.py <library dir> <bundle.cmrec name> 60 150

Copies the bundle once per length and repeats its transcript, speaker turns and duration end to end
until the length is reached, under a new capture id. Then rebuild the index (JSON-RPC `rescan_library`).
The text repeats every source-length, so the result measures time and memory, never quality.
"""
import json, os, random, shutil, subprocess, sys

lib, name, *lengths = sys.argv[1:]
src = os.path.join(lib, name)
for mins in map(int, lengths):
    dst = os.path.join(lib, f"Tiled {mins} min.cmrec")
    shutil.rmtree(dst, ignore_errors=True)
    subprocess.run(["cp", "-cR", src, dst], check=True)  # APFS clone: no copy of the audio
    m = json.load(open(f"{dst}/manifest.json"))
    one, want = m["duration"], mins * 60 * 120000
    n = -(-want // one)

    def tile(items):
        out = []
        for i in range(n):
            for it in items:
                x = dict(it)
                x["start"] += i * one
                x["end"] += i * one
                if x["end"] <= want:
                    out.append(x)
        return out

    t = json.load(open(f"{dst}/transcript.json"))
    t["words"] = tile(t["words"])
    json.dump(t, open(f"{dst}/transcript.json", "w"))
    d = json.load(open(f"{dst}/diarization.json"))
    d["segments"] = tile(d["segments"])
    json.dump(d, open(f"{dst}/diarization.json", "w"))
    m.update(duration=want, title=f"Tiled {mins} min", speaker_segments=d["segments"],
             edits=[{"in": 0, "out": want, "speed": 1.0}],
             id="rec_" + "".join(random.choice("0123456789ABCDEFGHJKMNPQRSTVWXYZ") for _ in range(16)))
    json.dump(m, open(f"{dst}/manifest.json", "w"))
    print(dst, len(t["words"]), "words", m["id"])
