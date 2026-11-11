#!/usr/bin/env python3
"""Fails when the Claude plugin's version differs from the workspace version.

The plugin at plugins/capturemann/.claude-plugin/plugin.json pins `version`; a user who installed
it keeps that version until the number changes, so the release bump must reach it. This check
compares it with `[workspace.package] version` in Cargo.toml and, when the marketplace entry at
.claude-plugin/marketplace.json sets a version too, with that as well (plugin.json wins at install
time, so a different entry version is a mistake).

    python3 scripts/check_plugin_version.py          # exit 1 on a mismatch
    python3 scripts/check_plugin_version.py --fix    # write the workspace version into plugin.json

Not wired into CI yet (release 0.3.2 adds it next to check_target_os.py in the `docs` job).
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CARGO = ROOT / "Cargo.toml"
PLUGIN = ROOT / "plugins" / "capturemann" / ".claude-plugin" / "plugin.json"
MARKETPLACE = ROOT / ".claude-plugin" / "marketplace.json"


def workspace_version() -> str:
    text = CARGO.read_text(encoding="utf-8")
    section = text.split("[workspace.package]", 1)
    if len(section) < 2:
        sys.exit("Cargo.toml has no [workspace.package] section")
    body = section[1].split("\n[", 1)[0]
    m = re.search(r'^version\s*=\s*"([^"]+)"', body, re.M)
    if not m:
        sys.exit("Cargo.toml: no version in [workspace.package]")
    return m.group(1)


def main(argv: list[str]) -> int:
    fix = "--fix" in argv
    want = workspace_version()
    plugin = json.loads(PLUGIN.read_text(encoding="utf-8"))
    have = plugin.get("version")
    problems = []
    if have != want:
        problems.append(f"{PLUGIN.relative_to(ROOT)}: version {have!r}, workspace is {want!r}")
    if MARKETPLACE.exists():
        market = json.loads(MARKETPLACE.read_text(encoding="utf-8"))
        for entry in market.get("plugins", []):
            if entry.get("name") == plugin.get("name") and "version" in entry and entry["version"] != want:
                problems.append(
                    f"{MARKETPLACE.relative_to(ROOT)}: entry {entry['name']!r} has version "
                    f"{entry['version']!r}, workspace is {want!r}"
                )
    if problems and fix:
        plugin["version"] = want
        PLUGIN.write_text(json.dumps(plugin, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"wrote version {want} to {PLUGIN.relative_to(ROOT)}")
        return 0
    if problems:
        print("\n".join(problems))
        print("Bump the plugin with: python3 scripts/check_plugin_version.py --fix")
        return 1
    print(f"plugin version {have} matches the workspace")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
