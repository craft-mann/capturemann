#!/usr/bin/env python3
"""Keeps `target_os` out of feature code (platform plan, Step 0.2).

OS-specific code belongs in `crates/cm-platform`, a crate's `sys/` folder, or an app's platform
module (`platform*.rs`). Tests and examples are free. Older code outside those places is listed in
`scripts/target_os_baseline.json` with the number of lines it has today; a file may not gain
lines or appear new, and the number must come down when it does (so the list only shrinks).

    python3 scripts/check_target_os.py            # check (CI)
    python3 scripts/check_target_os.py --update   # rewrite the baseline after a clean-up
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / "scripts" / "target_os_baseline.json"
FREE = re.compile(
    r"(^|/)(cm-platform|sys|tests|examples|benches)/|(^|/)platform[^/]*\.rs$|(^|/)sys\.rs$"
)


def counts():
    out = {}
    for top in ("crates", "apps"):
        for path in sorted((ROOT / top).rglob("*.rs")):
            rel = path.relative_to(ROOT).as_posix()
            if FREE.search(rel):
                continue
            n = sum(
                1
                for line in path.read_text(encoding="utf-8").splitlines()
                if "target_os" in line and not line.lstrip().startswith("//")
            )
            if n:
                out[rel] = n
    return out


def main():
    now = counts()
    if "--update" in sys.argv:
        BASELINE.write_text(json.dumps(now, indent=2, sort_keys=True) + "\n")
        print(f"wrote {len(now)} files")
        return 0
    base = json.loads(BASELINE.read_text())
    bad = []
    for f, n in sorted(now.items()):
        allowed = base.get(f, 0)
        if n > allowed:
            bad.append(f"{f}: {n} line(s) with target_os, allowed {allowed}")
    for f, allowed in sorted(base.items()):
        if now.get(f, 0) < allowed:
            bad.append(
                f"{f}: down to {now.get(f, 0)} from {allowed}; run with --update to lock it in"
            )
    if bad:
        print("target_os outside platform code:\n  " + "\n  ".join(bad))
        print(
            "\nPut the OS-specific part in crates/cm-platform, a sys/ module or a platform*.rs file."
        )
        return 1
    print(f"ok: {len(now)} older files still hold target_os ({sum(now.values())} lines)")
    return 0


sys.exit(main())
