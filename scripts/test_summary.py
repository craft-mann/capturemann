#!/usr/bin/env python3
"""Turns the output of `cargo test --workspace` into a Markdown table, one row per crate.

Usage: test_summary.py <cargo-test.log> <out.md> <meta.json-or-empty>
Reads only the `Running ...` / `Doc-tests ...` headers and the `test result:` lines, so it works on
stable Rust with no extra tools. A crate's row adds up its unit tests, integration tests and doctests.
"""
import json, re, sys, collections

log, out = sys.argv[1], sys.argv[2]
meta = json.load(open(sys.argv[3])) if len(sys.argv) > 3 and sys.argv[3] else {}

run = re.compile(r"^\s+Running .*\(target/[^/]+/deps/([A-Za-z0-9_]+)-[0-9a-f]+\)")
doc = re.compile(r"^\s+Doc-tests (\S+)")
res = re.compile(r"^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*finished in ([\d.]+)s")
rows = collections.OrderedDict()
cur = None
for line in open(log, errors="replace"):
    m = run.match(line) or doc.match(line)
    if m:
        name = m.group(1)
        # Integration-test binaries are named after the test file; the crate is what Cargo printed
        # in `Compiling`/`Running` order, so keep the last crate-looking owner when it is not one.
        cur = name
        continue
    m = res.match(line)
    if m and cur:
        ok, p, f, i, secs = m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4)), float(m.group(5))
        r = rows.setdefault(cur, [0, 0, 0, 0.0])
        r[0] += p; r[1] += f; r[2] += i; r[3] += secs

tp = sum(r[0] for r in rows.values()); tf = sum(r[1] for r in rows.values()); ti = sum(r[2] for r in rows.values())
lines = ["# Test report", ""]
for k in ("commit", "date", "rustc", "host"):
    if meta.get(k):
        lines.append(f"- **{k}**: {meta[k]}")
lines += [
    f"- **result**: {'PASS' if tf == 0 else 'FAIL'}: {tp} passed, {tf} failed, {ti} ignored",
    "",
    "Ignored tests need real hardware (screen, camera, microphone), a real Whisper model or a real",
    "account, so they never run in CI. Run them by hand: see `CLAUDE.md` and `tests/manual/`.",
    "",
    "| Test binary | Passed | Failed | Ignored | Seconds |",
    "|---|---:|---:|---:|---:|",
]
for name, (p, f, i, s) in rows.items():
    lines.append(f"| `{name}` | {p} | {f} | {i} | {s:.1f} |")
lines.append(f"| **Total** | **{tp}** | **{tf}** | **{ti}** | |")
open(out, "w").write("\n".join(lines) + "\n")
print(f"{tp} passed, {tf} failed, {ti} ignored", file=sys.stderr)
sys.exit(1 if tf else 0)
