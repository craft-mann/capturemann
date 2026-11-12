# Release 0.3.3: status board

Plan: `release-0.3.3-plan.md` (decisions final, section 5). Orchestrator log, local main only; nothing pushed, tagged or published.

## Status at 22:45 (Nov 12)

All five tracks have delivered; all agents have finished (none alive, none blocked). Integration is done on branch `release/0.3.3` (worktree `.claude/worktrees/agent-aeae7820b95d4cd94`), because the harness pinned the orchestrator to that worktree for a while; main (099fa2f4: Phase 0, C, B2) will be fast-forwarded to it once the gate is green.

| Track | Scope | State |
| --- | --- | --- |
| Phase 0 | specs, frozen op JSON, core types, pass-through | merged on main |
| A | renderer, `callout_box`, mcp schema, fixture | merged into release/0.3.3 (69f06a04) |
| B1 | palette, callout, spotlight, steps panel, stamps | merged into release/0.3.3 (b942ce00); stamps NOT cut |
| B2 | view bar, image strip | merged on main (099fa2f4) |
| C | docs, regress cases, manual scripts, release text drafts | merged on main (b23d92a7) |
| D | image import IMP-6 | is the base of release/0.3.3 (clipboard paste cut as stretch) |
| Gate | fmt, clippy workspace -D warnings | clippy green after two small fixes (22:40); workspace tests running |

Next: workspace tests, cargo deny, Trivy, layout_width_tests, then ff main, one push, Windows candidate and regress (full tier) on the ThinkPad, macOS regress, release material.

Cause of the earlier slowness: all agents shared one cargo target dir, so their cargo runs queued on one lock (0 percent CPU); the private-target clones were too slow and were dropped.

