# Release 0.3.6: Quick Capture (plan, draft, Nov 17, 2026)

Status: **scope only; nothing built.** Quick Capture was first added to 0.3.5 and moved to 0.3.6 by the maintainer on Nov 17, 2026, so 0.3.5 stays light (`specs/plans/release-0.3.5-plan.md`).

## 1. FEATURE: Quick Capture (QC-1, QC-3, QC-4, QC-5)

- **Spec:** `specs/features/21-quick-capture.md` (Phase 0 text is written, with acceptance criteria; Preview label). FR rows are in `specs/00-product-spec.md`.
- **Design pack:** `specs/capturemann-0.3.6-quick-capture/` (local, git-ignored): boards Q036Brief, Q036Panel, Q036After, Q036Presets, Q036Scope. The spec file wins where they differ.
- **Not in this release:** QC-2 (one smart crosshair) and QC-6 (pinned presets in the dock).
- **MCP:** one new read tool, `list_presets`, so the count goes from 89 to 90; the assertion in `crates/cm-mcp/tests/stdio.rs` and `docs/reference/` change with it.

**Decisions already made (Nov 17, 2026), all written into the spec:**

| Point | Decision |
| --- | --- |
| Shortcut | **⌥⇧C** (Ctrl+Alt+Shift+C on Windows and Linux) opens the panel; the Launcher keeps ⌥⇧Space. |
| Tray and menu bar | The native menu stays; **Quick Capture…** is its first item. The panel opens under the icon after that. |
| Panel options and Settings | The option tiles are remembered per kind and apply only to captures started from the panel or a preset; dock tiles, hotkeys and the tray keep using Settings; neither overwrites the other. |
| Preview | Quick Capture carries the Preview label and a Send feedback button, no Settings switch. |
| Starter presets | Ship Bug report, Docs shot, Demo video and Meeting, editable and deletable. |
| Scope | QC-1, QC-3, QC-4 and QC-5 (including `list_presets` and the `preset` argument for agents and the CLI). |

## 2. Also to plan for 0.3.6

The bugs and checks moved out of 0.3.5 (section 8 of the 0.3.5 plan), and whatever the QA agent passes and users report for 0.3.4 and 0.3.5.

## 3. To write before the build

Tracks, order and exit criteria (as in the 0.3.5 plan: Phase 0 done, parallel tracks on disjoint files, one full gate, QA agent passes on the candidates, `cm-regress` on all three systems). Open points to check when the build starts: "Long Shot extends this area" and "Record exactly this area" from the after-capture bar start a capture from a region that already exists, which is more than adding a surface.
