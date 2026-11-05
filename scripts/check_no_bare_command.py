#!/usr/bin/env python3
"""Keeps console windows from flashing on Windows (every child program starts through cm-platform).

The Windows GUI builds own no console, so a console program they start (ffmpeg, powershell, ...)
opens a visible one unless it is started with CREATE_NO_WINDOW. `cm_platform::process::command(..)`
(or `hide_console`) sets the flag; this check fails on a `Command::new(` in non-test source code.
Code after a line-start `#[cfg(test)]`, files under tests/, examples/, benches/ and `cm-platform`'s
`process.rs` are free. A deliberate exception (the CLI's interactive `capturemann mcp`) is listed in
ALLOW with the reason.

    python3 scripts/check_no_bare_command.py
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# cm-regress is the release test driver: it links no product crate (so no cm_platform) and sets CREATE_NO_WINDOW itself.
FREE = re.compile(r"(^|/)(tests|examples|benches)/|^crates/cm-platform/src/process\.rs$|^crates/cm-regress/")
BARE = re.compile(r"\bCommand::new\(")
ALLOW = {
    # `capturemann mcp` runs cm-mcp with this terminal's stdio on purpose (the CLI owns the console).
    "crates/cm-cli/src/commands/settings.rs": "interactive cm-mcp launch with inherited stdio",
}


def main() -> int:
    bad = []
    for top in ("crates", "apps"):
        for path in sorted((ROOT / top).rglob("*.rs")):
            rel = path.relative_to(ROOT).as_posix()
            if FREE.search(rel) or rel in ALLOW:
                continue
            for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if line.startswith("#[cfg(test)]"):
                    break
                if BARE.search(line) and not line.lstrip().startswith("//"):
                    bad.append(f"{rel}:{n}: {line.strip()}")
    if bad:
        print("Command::new( without cm_platform::process::command (console window flash on Windows):")
        print("\n".join(bad))
        print("Use cm_platform::process::command(program) or hide_console(&mut cmd); see crates/cm-platform/src/process.rs.")
        return 1
    print("no bare Command::new( in non-test code")
    return 0


if __name__ == "__main__":
    sys.exit(main())
