# Auto-redact: rules, benchmark and recognizers

For contributors who add a kind of data to Auto-redact, run its benchmark, or add a text recognizer for another operating system. The behavior is specified in [`specs/features/17-redaction.md`](../../specs/features/17-redaction.md) (RED-1 to RED-3); the user chapter is [`docs/user/redaction.md`](../user/redaction.md); the plan with its decisions is [`specs/plans/release-0.2.5-plan.md`](../../specs/plans/release-0.2.5-plan.md).

Ground rules (from `CLAUDE.md` and the plan): **general techniques only** (a format, a checksum, a structure), never a phrase list or a fix for one picture; on-device, no network, no telemetry; no GPL code; every option has an MCP parameter and a CLI flag; nothing is called "verified", "accurate" or "compliant" without a stored report in `verification/`.

## Where things are

| What | Where |
|---|---|
| The kinds, their names, defaults, and the documentation data | `RedactionKind` in `crates/cm-render/src/ocr/detect.rs` |
| The scan: line rules, page rules, padding, merging | `crates/cm-render/src/ocr/detect.rs` |
| The rules, one file per group | `crates/cm-render/src/ocr/rules/`: `contact.rs`, `financial.rs`, `identity.rs`, `network.rs`, `secrets.rs` |
| The registry (`LINE_RULES`, `PAGE_RULES`) | `crates/cm-render/src/ocr/rules/mod.rs` |
| The recognizer trait and Apple Vision | `crates/cm-render/src/ocr/mod.rs`, `apple.rs` |
| The benchmark | `crates/cm-render/examples/redaction_bench/` (`gen/`, `harness/`, `schema.rs`) |
| Its data note | `tests/data/redaction/README.md` |
| The generated table check | `crates/cm-render/tests/redaction_docs.rs` |
| The MCP schema list of kinds (kept equal by a test) | `crates/cm-mcp/src/tools.rs`, `crates/cm-render/tests/redaction_kinds.rs` |

## How a rule is added

1. **Choose the group file** (or create one next to the others and declare it in `rules/mod.rs`). A rule is a pure function over the characters of recognized text: a **line rule** `fn(&[char]) -> Vec<Span>` sees one line; a **page rule** `fn(&[Flat], &[Located]) -> Vec<Located>` sees every line and what the line rules found (a private-key block over many lines, an expiry date beside a card number). No OCR engine, no I/O: it is tested with fixtures.
2. **Write the rule from a public source and cite it in a comment** on the function or constant: the standard that defines the format or the checksum (for example the SSA ranges for the SSN, GSMA TS.06 for the IMEI, the RFC for a URL). If you cannot cite a public source for the format, the rule does not belong here.
3. **Add the kind** to `RedactionKind` in `detect.rs`: the enum variant (the order is the priority when two suggestions overlap), `ALL`, `as_str`, `standards`, and the documentation methods `summary`, `check` and `group`. Add it to `DEFAULT` only if the benchmark supports it (below); otherwise leave a comment saying why it is off, as `npi` has.
4. **Register the rule** in `LINE_RULES` or `PAGE_RULES` in `rules/mod.rs`. The list order is the priority: the first rule to claim characters on a line keeps them. A rule runs only when one of its `kinds` is wanted.
5. **Write tests in the same file** (look at the `cards_accepted` / `cards_rejected` pairs in `financial.rs`): accepted forms, rejected near-misses (wrong checksum, masked, too long, part of a longer number, dates and versions), and one test through `suggest_redactions` with `TextLine::laid_out`. Cite the acceptance criterion in the test or comment when one applies.
6. **Keep the lists equal**: add the name to the `enum` lists of `suggest_redactions` in `crates/cm-mcp/src/tools.rs` (the `mcp_schema_lists_every_redaction_kind` test fails otherwise), and regenerate the tables (below).
7. **Teach the benchmark about it**: add a generator for the kind and its lookalikes (decoys) in `redaction_bench/gen/`, so the kind has recall, precision and false-alarm rows. A kind with no benchmark rows is not made a default.
8. **Run the benchmark and store a report** (below), and set the default from the numbers.

### The documentation tables are generated

The tables of kinds in `README.md` and `docs/user/redaction.md` sit between the markers `<!-- redaction-kinds:start -->` and `<!-- redaction-kinds:end -->` and are produced from `RedactionKind` (RED-3.AC1). The test `crates/cm-render/tests/redaction_docs.rs` regenerates the table and fails when either file differs. After you change a kind:

```sh
REDACTION_DOCS_UPDATE=1 cargo test -p cm-render --test redaction_docs
```

This rewrites the text between the markers in both files; review the diff and commit it. Never edit the table by hand.

## The benchmark

`redaction_bench` is a `cm-render` example. It is headless and needs the OS text recognizer only for `run` without `--detector-only`.

```sh
# Build the synthetic set (seed 20261030 and 300 images by default; no network)
cargo run -p cm-render --example redaction_bench --release --locked -- gen --out <dir> [--seed N] [--count N]

# Perfect text: rule errors only, no recognizer
cargo run -p cm-render --example redaction_bench --release --locked -- run --set <dir> --detector-only --split dev

# The system recognizer end to end, with the leak pass (blur, export, read again)
cargo run -p cm-render --example redaction_bench --release --locked -- run --set <dir> --recognizer system --split dev [--report <file.md>] [--kinds a,b]
```

**How the set is built.** `gen` draws screenshots of 13 layouts (bank statement, checkout, CRM table, `.env` file, log viewer, mail client, patient intake, settings page, terminal and others) in light and dark, at 1x and 2x, with text down to about 10 px, some images degraded (blur, JPEG, rescaling) and a small Vietnamese set. Every image carries exact ground truth in `manifest.json`: for each item its kind, exact text, box and the standard identifiers it maps to; for each lookalike (a `decoy`: order number, UUID, hash string, Luhn failure, masked SSN, and so on) what it imitates. Negative images hold only lookalikes and measure false alarms. **All data is synthetic**: numbers are made to satisfy the published format and checksum rules, are unlinked to any person, and use reserved ranges (555-01xx phone numbers, `example.*` domains, documentation IP ranges); nothing is drawn from a real record. Details and the licence note: `tests/data/redaction/README.md`. The same seed gives the same set. The images are built on demand and are never committed.

**The dev / held_out rule.** Each image is tagged `dev` or `held_out`; whole layouts are held out (4 of 13 in the first set), so a rule cannot be tuned to a layout it has seen. **Tune only on `dev`.** Thresholds, defaults and rule changes are decided from `dev` numbers. The `held_out` split is read **once per release**: `--split held_out` and `--split all` print a warning for a reason. Reading it repeatedly and then changing rules turns it into a second dev split and ends its use as a check. The first reports (0.2.3 baseline and 0.2.5) read `dev` only.

**What is scored.** A truth is found when a proposal covers at least 90 % of its box (any kind, and separately the right kind). Per kind and per standard: recall, precision, false alarms per image and per negative image, and the end-to-end **leak rate** (apply every proposal as a blur, render, read the result with the recognizer again; a truth still readable is a leak). Also splits by layout, theme, scale, font size, degraded or not, and language, and the time per image.

**Default kinds are chosen by the numbers.** The plan's gate: a default-on kind must stay below one false alarm per five negative screenshots (0.2 per negative image); a kind above it ships off by default. `npi` is off for that reason in 0.2.5 (the 0.2.5 report has the numbers).

### The report format

`--report <file.md>` writes Markdown: a header (date, who, build, machine, set, wording), then for each run the tables Overall, Per kind, Per standard, False alarms by lookalike, By split, By layout, By theme, By scale, By font size, By degraded, By language and Time per image. Store reports in `verification/benchmarks/` as `redaction-<release>-<split>-<date>.md` and keep the **wording line**: the numbers are synthetic, measured with perfect knowledge of the formats the rules look for, an upper bound for real screenshots and not a verdict about compliance. Say which split was read and which recognizer ran. Every number quoted in the README or the user guide must come from a stored report and link to it.

## How to add a recognizer for Windows (or Linux)

The detector never sees pixels; it sees lines of text with boxes. A new platform only needs a [`TextRecognizer`](../../crates/cm-render/src/ocr/mod.rs):

1. Implement `TextRecognizer` (`name`, `is_available`, `recognize(&RgbaImage) -> Result<Vec<TextLine>, CmError>`): lines in reading order, each with a box and, where the API gives them, word boxes, in the image's own pixels. Return `not_implemented` when unavailable. The Windows candidate is `Windows.Media.Ocr`.
2. Return it from `system_recognizer()` on that platform (today it returns the `Unavailable` stub there, which is why the Auto-redact button is hidden).
3. Run the benchmark with `--recognizer system` on that platform and store the report next to the macOS one. The harness compares recognizers on the same set, so the leak rate and the per-font-size rows show where the new recognizer is weaker.
4. Hardware or OS-service tests are `#[ignore]` and print `SKIPPED` without the service; keep the default tests on a scripted fake.

## Before you open a pull request

`cargo fmt --all -- --check`, `cargo clippy -p cm-render --all-targets --locked -- -D warnings`, `cargo test -p cm-render --locked`, and the manual cross-check [`tests/manual/RED-1.md`](../../tests/manual/RED-1.md) when a rule change is meant to help real screenshots.
