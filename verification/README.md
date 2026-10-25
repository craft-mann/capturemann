# Verification reports

One file per manual run, so a status can say "verified" with something to point at. A report is
the filled-in template below, named `<script id>-<date>-<who>.md` (for example
`EDT-1-2026-10-23-owner.md`), committed with the change it covers. Do not store recordings of your
screen here; screenshots of the fixture or of the app window are fine.

The scripts are in `tests/manual/` (`M4-GATE.md` lists the M4 ones). The words used in specs:

* "tested with fakes": automated, with scripted sources.
* "verified on hardware (as reported)": a person ran it and said so, nothing stored.
* "verified": a report in this folder exists and every step passed.

## Template

```markdown
# <script id> report

- **Date:** YYYY-MM-DD
- **Who:** name
- **Machine:** model, chip, macOS version, display(s)
- **Build:** git commit (`git rev-parse --short HEAD`), debug or release, bundle or `cargo run`
- **FFmpeg:** `ffmpeg -version | head -1`
- **Fixture:** `make_fixture` flags used, capture id

## Steps

| Step | Result (PASS / FAIL / not built / skipped) | Notes (numbers, what you saw or heard) |
| --- | --- | --- |
| 1 | | |

## Acceptance criteria

| AC | Result | Evidence (step, file, screenshot) |
| --- | --- | --- |

## Problems found

- (what, how to reproduce, expected vs actual; attach a screenshot or the exported file name)

## Verdict

PASS / FAIL / PARTIAL (which steps wait for which feature)
```
