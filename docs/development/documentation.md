# Writing documentation

Where a new Markdown file goes, and the few rules that keep the documents easy to maintain.

## Where things go

| You are writing | Put it in | Name |
|---|---|---|
| A standard project file (README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, SUPPORT, CHANGELOG, LICENSE, NOTICE) | repository root | UPPERCASE, as GitHub expects |
| How to use the app | `docs/user/` | `kebab-case.md` |
| How to build, release or contribute | `docs/development/` | `kebab-case.md` |
| Status, plans, spike results, demo script | `docs/project/` | `kebab-case.md` |
| What a feature must do, with acceptance criteria | `specs/features/NN-area.md` | numbered, see `specs/README.md` |
| The approach for a larger feature | `specs/plans/` | `area-plan.md` |
| A hands-on test script | `tests/manual/` | the requirement ID, such as `REC-1.md` |
| The report of a manual run | `verification/` | per its README |
| A component's own notes (packaging, assets, icons) | next to the code, as `README.md` | `README.md` |

Every folder with several documents has an index (`docs/README.md`, `specs/README.md`,
`tests/manual/README.md`); add the new file to it.

## Rules

- **One topic per file**, with a single `#` title. Start with one or two sentences saying what the
  file is for and who reads it.
- **Relative links** between documents, never absolute paths or `https://github.com/.../blob/...`
  links to files in this repository. `python3 scripts/check_md_links.py` fails on a broken link and
  CI runs it, so a rename shows up in the pull request that makes it.
- **Link to a source of truth instead of copying it.** Status lives in `docs/project/status.md`,
  behavior in the feature specs, commands in `CLAUDE.md` and `docs/development/building.md`.
- **Dates and numbers that go stale** (test counts, tool counts) belong in `docs/project/status.md`
  or `reports/`, not in the README or the guides.
- **IDs are stable.** Requirement and acceptance-criteria IDs (`REC-2.AC3`) are never renumbered
  (see `specs/README.md`); cite them in tests and commits.
- **Wording about checks:** write "verified" only with a stored report in `verification/`; otherwise
  say "tested with fakes" or "verified on hardware (as reported)".
- **Translations** of the app live in `locales/`, not here (see `locales/README.md`).
