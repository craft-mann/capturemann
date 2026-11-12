# Release 0.3.3: status board

Plan: `release-0.3.3-plan.md` (decisions final, section 5). Orchestrator log, local main only; nothing pushed, tagged or published.

| Track | Scope | Branch | State |
| --- | --- | --- | --- |
| Phase 0 | Specs ANN-4/6/7 (ANN-8 reserved), IMP-6, FR rows, annotate schema text; op JSON frozen; core types, validation, pass-through, `step_label`, render stubs, `cm_render::callout_box` stub | main (c286c007) | done, committed |
| A | Renderer (callout, spotlight, stamps, step labels), goldens, `callout_box`, mcp schema, CLI, fixture round trip | `track/a-core-render` | agent running |
| B1 | Editor: palette, tools K/S/U, steps panel, stamps, icons, locales | `track/b1-editor` | agent running |
| B2 | View bar, image strip | `track/b2-viewbar` | agent running |
| C | Docs, regress case, manual scripts | `track/c-docs` | agent running |
| D | Image import (IMP-6) | `track/d-import` | agent running |
| Release | Candidate files, release text, status docs, `cm-regress` on this Mac | main | not started |

Cut list (first to go): stamps (editor part), clipboard paste for images (stretch).
