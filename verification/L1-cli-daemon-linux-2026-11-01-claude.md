# L1 CLI and MCP without the app running (Linux VM, packaged files)

- **Date:** 2026-11-01
- **Who:** Claude (AI agent)
- **Machine:** Ubuntu 24.04 aarch64 VM (QEMU), headless GNOME Wayland session; no real desktop
- **Build:** release, stripped, from the tarball built by `packaging/linux/build-linux.sh`
- **Status:** PARTIAL

Finding: the first packages had no `cm-daemon`, so `capturemann diagnostics` and every other CLI command without the
app open answered `daemon_unavailable: cannot run cm-daemon`. `cm-mcp` answered `tools/list` but its tool calls would
have failed the same way. Fixed by packaging `cm-daemon` next to the CLI (the CLI looks for a sibling, then `PATH`).

| Step | Result | Note |
| --- | --- | --- |
| Before: `capturemann diagnostics` with no app running | FAIL | `daemon_unavailable`, no `cm-daemon` in the package |
| After: `scripts/testlab/smoke-linux.sh` on the new tarball (Wayland session, throw-away library) | PASS | diagnostics, targets (1 display), library list, settings get, `ai models`, MCP `tools/list` (75 tools) |
| `check-runtime.sh` covers `cm-daemon` | PASS | |
| Size cost | INFO | tarball +29 MB (104 to 133 MB), `.deb` +18 MB (77 to 95 MB), aarch64 |
| Real desktop (ThinkPad), Xorg and Wayland | NOT RUN | tomorrow |
