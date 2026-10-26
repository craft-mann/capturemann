#!/usr/bin/env python3
"""Checks that every relative Markdown link points at a file tracked by git.

Usage: check_md_links.py            (run from anywhere inside the repository)
Skips URLs, mailto:, pure #anchors and fenced code blocks. Anchors after a path are not checked.
Exit status 1 when a link is broken, so CI can gate on it.
"""
import os, re, subprocess, sys

root = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip()
tracked = set(subprocess.run(["git", "ls-files"], cwd=root, capture_output=True, text=True, check=True).stdout.split("\n"))
dirs = {os.path.dirname(p) for p in tracked}
dirs |= {os.path.dirname(d) for d in dirs if d}
link = re.compile(r"(?<!\!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)|!\[[^\]]*\]\(([^)\s]+)\)")
bad = 0
for f in sorted(p for p in tracked if p.endswith(".md")):
    fence = False
    for n, line in enumerate(open(os.path.join(root, f), errors="replace"), 1):
        if line.lstrip().startswith("```"):
            fence = not fence
            continue
        if fence:
            continue
        for m in link.finditer(line):
            t = (m.group(1) or m.group(2)).split("#")[0]
            if not t or re.match(r"^[a-z][a-z0-9+.-]*:", t) or t.startswith("<"):
                continue
            t = t.split("?")[0]
            p = os.path.normpath(os.path.join(os.path.dirname(f), t)) if not t.startswith("/") else t.lstrip("/")
            if p in tracked or p.rstrip("/") in dirs or p == "":
                continue
            print(f"{f}:{n}: broken link -> {t}")
            bad += 1
print(f"{bad} broken link(s)", file=sys.stderr)
sys.exit(1 if bad else 0)
