#!/usr/bin/env python3
"""Regenerate docs/reference/tools.json and check that cli.md and mcp.md cover everything.

The facts come from the built binaries, not from the source text:
  * the tool list, descriptions and schemas: `tools/list` over stdio from `cm-mcp`
    (no daemon is needed for that call);
  * the commands: `capturemann --help`, then `--help` of every subcommand, recursively;
  * the tool -> command map: the `CLI_COMMANDS` table in crates/cm-cli/src/lib.rs.

Usage (from the repository root, after
`cargo build -p cm-cli -p cm-mcp -p cm-daemon --no-default-features --locked`):

    python3 scripts/gen_agent_reference.py --write    # rewrite docs/reference/tools.json
    python3 scripts/gen_agent_reference.py --check    # fail if tools.json is stale or a tool or a command is missing from the .md files

--mcp-bin and --cli-bin default to $CARGO_TARGET_DIR/debug (or target/debug). Not part of CI.
"""
import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REF = os.path.join(ROOT, "docs", "reference")

# Tool -> area. Every tool must appear exactly once; the script fails on a new, unmapped tool.
AREAS = [
    ("discovery", "Discovery and diagnostics", ["list_targets", "list_devices", "get_diagnostics"]),
    ("settings", "Settings", ["get_settings", "set_settings"]),
    ("screenshot", "Screenshots", ["capture_screenshot", "get_capture", "recognize_text"]),
    ("annotate", "Annotate and undo", ["annotate", "suggest_redactions", "suggest_blueprint", "undo", "redo"]),
    ("boards_bulk", "Boards and bulk export", ["create_board", "export_images"]),
    ("library", "Library and import", ["list_library", "rename_capture", "delete_capture", "move_library", "import_media"]),
    ("recording", "Recording", ["start_recording", "pause_recording", "resume_recording", "set_mic_muted", "add_marker",
                                "stop_recording", "get_recording_status", "run_fps_selftest", "get_cursor_track"]),
    ("editing", "Editing a recording", ["trim", "cut_segments", "restore_segments", "split", "set_speed", "set_background",
                                        "set_audio_mix", "set_camera_layout", "set_zooms", "set_cursor_style", "set_reframe"]),
    ("export", "Export", ["export_video", "export_audio", "estimate_export"]),
    ("jobs", "Jobs", ["get_job", "list_jobs", "cancel_job", "retry_job"]),
    ("ai_models", "Speech and text models", ["list_models", "download_model", "import_model", "select_model", "remove_model",
                                             "list_text_models", "download_text_model", "select_text_model", "remove_text_model",
                                             "get_text_model", "test_text_model"]),
    ("transcript", "Transcript and captions", ["transcribe", "get_transcript", "get_captions", "set_captions", "replace_in_captions",
                                               "set_caption_options", "export_captions", "translate_captions",
                                               "normalize_transcript", "get_normalized_transcript"]),
    ("speakers", "Speakers", ["diarize", "set_speakers"]),
    ("suggestions", "Suggestions", ["suggest_zooms", "suggest_cuts"]),
    ("reports", "Reports", ["list_report_templates", "generate_report", "get_report", "export_report", "delete_report",
                            "edit_report", "check_report_item"]),
    ("guide", "Click Guide (Preview)", ["start_guide", "stop_guide", "get_guide", "set_guide", "export_guide"]),
    ("publish", "Destinations and publishing", ["list_destinations", "upload", "list_publish_accounts", "get_platform_rules",
                                                "publish_video"]),
]
# Tiers that depend on a parameter (specs/02, "A tier can depend on a parameter"; accessors.rs, long_shot_ops.rs).
CONFIRM_WHEN = {
    "import_media": "remove_originals is true and dry_run is not true",
    "capture_screenshot": "mode is \"long\" and scroll is \"auto\" (the default): the screen moves; scroll \"hand\" is refused for agents",
}


# CLI_COMMANDS names the command; these tools also need a flag to reach them.
CLI_FLAG_OVERRIDES = {"export_captions": "export --captions <srt|vtt|txt|md>"}


def default_bin(name):
    base = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target")
    return os.path.join(base, "debug", name)


def mcp_tools(binary):
    env = {"PATH": "/usr/bin:/bin", "CAPTUREMANN_SOCKET": os.path.join(os.environ.get("TMPDIR", "/tmp"), "cm-ref-no-daemon.sock")}
    p = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, env=env)

    def rpc(i, method, params=None):
        msg = {"jsonrpc": "2.0", "id": i, "method": method}
        if params is not None:
            msg["params"] = params
        p.stdin.write(json.dumps(msg) + "\n")
        p.stdin.flush()
        while True:
            r = json.loads(p.stdout.readline())
            if r.get("id") == i:
                return r["result"]

    try:
        init = rpc(1, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "refdocs", "version": "1"}})
        p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        p.stdin.flush()
        return init, rpc(2, "tools/list")["tools"]
    finally:
        p.kill()


def cli_table():
    src = open(os.path.join(ROOT, "crates", "cm-cli", "src", "lib.rs")).read()
    return dict(re.findall(r'\("([a-z_]+)", Some\("([^"]+)"\)\)', src))


def cli_commands(binary):
    """Every command path from `--help`, in help order: ['targets', 'targets list', ...]."""
    out = []

    def walk(path):
        text = subprocess.run([binary, *path, "--help"], capture_output=True, text=True).stdout
        m = re.search(r"^Commands:\n(.*?)(?:\n\n|\nOptions:)", text, re.S | re.M)
        if not m:
            return
        for line in m.group(1).split("\n"):
            mm = re.match(r"^  (\S+)\s", line)
            if mm and mm.group(1) != "help":
                out.append(" ".join(path + [mm.group(1)]))
                walk(path + [mm.group(1)])

    walk([])
    return out


def build(mcp_bin):
    init, tools = mcp_tools(mcp_bin)
    cli = cli_table()
    area_of = {}
    for aid, title, names in AREAS:
        for n in names:
            if n in area_of:
                sys.exit(f"{n} listed twice in AREAS")
            area_of[n] = (aid, title)
    names = [t["name"] for t in tools]
    missing = [n for n in names if n not in area_of]
    if missing:
        sys.exit(f"tools without an area (add them to AREAS): {missing}")
    gone = [n for n in area_of if n not in names]
    if gone:
        sys.exit(f"AREAS names tools the server does not list: {gone}")
    pos = {n: (i, j) for i, (_, _, ns) in enumerate(AREAS) for j, n in enumerate(ns)}
    rows = []
    for t in sorted(tools, key=lambda t: pos[t["name"]]):
        d = t["description"]
        ann = t.get("annotations", {})
        confirm = "Tier: Confirm" in d
        read_only = bool(ann.get("readOnlyHint"))
        if read_only:
            policy = "locked_allow"
        elif confirm and t["name"] == "start_recording":
            policy = "default_ask"
        elif confirm:
            policy = "locked_ask"
        else:
            policy = "default_allow"
        spec = re.search(r" Spec: (.*?)\.(?: \(preview\)|$)", d)
        meta = t.get("_meta") or {}
        pparams = [k for k, v in t["inputSchema"].get("properties", {}).items() if "x-stage" in json.dumps(v)]
        rows.append({
            "name": t["name"],
            "area": area_of[t["name"]][0],
            "areaTitle": area_of[t["name"]][1],
            "tier": "confirm" if confirm else "free",
            "policy": policy,
            "readOnly": read_only,
            "destructive": bool(ann.get("destructiveHint")),
            "acceptsRequestId": "request_id" in t["inputSchema"].get("properties", {}),
            "confirmWhen": CONFIRM_WHEN.get(t["name"]),
            "preview": bool(meta.get("stage") == "preview"),
            "previewParams": pparams,
            "spec": spec.group(1) if spec else None,
            "cli": CLI_FLAG_OVERRIDES.get(t["name"], cli.get(t["name"])),
            "description": d,
            "inputSchema": t["inputSchema"],
            "outputSchema": t.get("outputSchema"),
        })
    return {
        "server": init["serverInfo"],
        "toolCount": len(rows),
        "policyValues": {
            "locked_allow": "read-only: always allowed, no setting",
            "default_allow": "runs at once; the user can set it to Ask first",
            "default_ask": "asks first; the user can set it to Allow (start_recording only)",
            "locked_ask": "always asks first; cannot be changed",
        },
        "generatedBy": "python3 scripts/gen_agent_reference.py --write",
        "tools": rows,
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mcp-bin", default=default_bin("cm-mcp"))
    ap.add_argument("--cli-bin", default=default_bin("capturemann"))
    ap.add_argument("--write", action="store_true", help="write docs/reference/tools.json")
    ap.add_argument("--check", action="store_true", help="check tools.json and that the .md files name every tool and command")
    a = ap.parse_args()
    data = build(a.mcp_bin)
    text = json.dumps(data, indent=2, ensure_ascii=False) + "\n"
    path = os.path.join(REF, "tools.json")
    if a.write:
        os.makedirs(REF, exist_ok=True)
        open(path, "w").write(text)
        print(f"wrote {path} ({data['toolCount']} tools)")
    if a.check:
        bad = []
        if not os.path.exists(path) or open(path).read() != text:
            bad.append("tools.json is stale: run with --write")
        mcp_md = open(os.path.join(REF, "mcp.md")).read()
        cli_md = open(os.path.join(REF, "cli.md")).read()
        for t in data["tools"]:
            if f"`{t['name']}`" not in mcp_md:
                bad.append(f"mcp.md does not mention tool `{t['name']}`")
        if f"{data['toolCount']} tools" not in mcp_md:
            bad.append(f"mcp.md does not say '{data['toolCount']} tools'")
        cmds = cli_commands(a.cli_bin)
        for c in cmds:
            if f"`capturemann {c}`" not in cli_md:
                bad.append(f"cli.md does not mention command `capturemann {c}`")
        for n in ("--json", "--socket", "--no-start", "--request-id"):
            if n not in cli_md:
                bad.append(f"cli.md does not mention the global flag {n}")
        if bad:
            print("\n".join(bad))
            sys.exit(1)
        print(f"ok: {data['toolCount']} tools and {len(cmds)} commands are covered")
    if not (a.write or a.check):
        print(text)


if __name__ == "__main__":
    main()
