# Release 0.3.3: status board

Plan: `release-0.3.3-plan.md` (decisions final, section 5). Orchestrator log, local main only; nothing pushed, tagged or published.

| Track | Scope | Branch | State |
| --- | --- | --- | --- |
| Phase 0 | Specs ANN-4/6/7 (ANN-8 reserved), IMP-6, FR rows, annotate schema text; op JSON frozen; core types, validation, pass-through, `step_label`, render stubs, `cm_render::callout_box` stub | main (c286c007) | done, committed |
| A | Renderer (callout, spotlight, stamps, step labels), goldens, `callout_box`, mcp schema, CLI, fixture round trip | `track/a-core-render` | alive (Nov 12 20:56): renderer + callout_box committed (9aaa8e56); goldens, mcp schema and fixture test uncommitted, tests queued on the shared cargo lock; ETA about 1 h |
| B1 | Editor: palette, tools K/S/U, steps panel, stamps, icons, locales | `track/b1-editor` | alive: palette, callout, spotlight, steps committed (94115dec); stamps and tests in progress; ETA about 1.5 h |
| B2 | View bar, image strip | `track/b2-viewbar` | alive: view bar and strip committed (57533f2a); polishing tests; ETA about 1 h |
| C | Docs, regress case, manual scripts | merged | merged into main (b23d92a7); Trivy and cargo deny clean |
| D | Image import (IMP-6) | `track/d-import` | alive, 17 files dirty, nothing committed after 2 h; asked to commit WIP and rename the branch; ETA about 1.5 h |
| Release | Candidate files, release text, status docs, `cm-regress` on this Mac | main | not started |

Cut list (first to go): stamps (editor part), clipboard paste for images (stretch).

Note (20:56): all agents build into the shared main target dir, so cargo runs queue on one lock; this slows every track but keeps disk and load low.
