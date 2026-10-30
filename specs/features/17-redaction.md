# Redaction (RED)

Auto-redact proposes blur rectangles over personal data and secrets in a screenshot; the user reviews and accepts them. This file specifies what it finds (RED-2), how it is measured (RED-1) and how its documentation is kept equal to its rules (RED-3). It builds on ANN-2 (`features/02-annotation.md`), which keeps its IDs: the blur, the editor button, the proposal list and `suggest_redactions` are ANN-2. Plan, scope and maintainer decisions: `plans/release-0.2.5-plan.md`. Developer guide: `docs/development/redaction.md`; user chapter: `docs/user/redaction.md`.

Scope of 0.2.5: structure and checksum rules and secrets only (21 kinds below). Label-based kinds, addresses, dates, names, faces, QR codes, file-path names, layout handling, automatic apply, agent-side redaction and Windows are deferred. Wording rule: nothing here is called "verified", "accurate" or "compliant"; the numbers are in stored reports and are measured on synthetic data.

### RED-1 Benchmark of Auto-redact
**Priority** P1 · **Status** In progress (the generator, the harness and two stored reports exist: the 0.2.3 baseline and the 0.2.5 rules, both on the synthetic `dev` split and Apple Vision on an Apple M4 Pro; the `held_out` split has not been read; no run on real screenshots, `tests/manual/RED-1.md` has not been run; no Windows recognizer) · **Crates** cm-render (example `redaction_bench`) · **Depends on** ANN-2

**User story** — As the maintainer, I want a repeatable measurement of what Auto-redact finds, misses and wrongly proposes, so that defaults are chosen from numbers and nothing is claimed without a report.

**Description**
- `redaction_bench gen` builds a synthetic set from a seed (default 20261030, 300 images, 13 layouts, 4 layouts held out, light and dark, 1x and 2x, some degraded, a small Vietnamese set) with exact ground truth: kind, exact text, box and the standard identifiers each item maps to, plus lookalikes that must not be found. No real data; the images are built on demand and not committed (`tests/data/redaction/README.md`).
- `redaction_bench run` scores the detector: recall (a truth is found when a proposal covers at least 90 % of its box; any kind and right kind), precision and false alarms per image and per negative image, per kind and per standard; the end-to-end leak rate (apply every proposal, render, read again); splits by layout, theme, scale, font size, degraded and language; time per image; and a detector-only run with perfect text.
- Tuning uses the `dev` split only; `held_out` is read once per release.
- A default-on kind must stay below one false alarm per five negative screenshots, or it ships off by default (maintainer decision, plan "Decisions").

**Acceptance criteria**
- **AC1** Given the benchmark set, then the harness reports recall, precision, false alarms and end-to-end leak rate per identifier and per standard, and the report is stored in `verification/benchmarks/`.

**Edge cases and errors** — A recognizer that is unavailable: `run` without `--detector-only` stops with its error. `--split held_out` and `--split all` print a warning.

**Agent interface** — None (a developer tool: `cargo run -p cm-render --example redaction_bench`).

**Out of scope** — Real screenshots in the repository (the manual cross-check records counts only); other countries' standards; a Windows recognizer.

**Test notes** — Reports: `verification/benchmarks/redaction-0.2.3-baseline-2026-10-30.md`, `verification/benchmarks/redaction-0.2.5-dev-2026-10-30.md`. The harness and generator have their own unit tests in `crates/cm-render/examples/redaction_bench/` (for example `perfect_recall_and_precision`, `leak_follows_the_second_reading`, `generators_satisfy_their_checksums`, `decoys_do_not_pass_the_rules_they_imitate`, `same_seed_same_manifest`). Manual cross-check on real screenshots: `tests/manual/RED-1.md`.

### RED-2 Kinds of personal data and secrets
**Priority** P1 · **Status** In progress (21 kinds built and tested with fakes in `cm-render` (rules, `suggest_redactions`) and through daemon, MCP and CLI; benchmark numbers stored for macOS, synthetic dev split only; the editor panel has a kind chooser and the CLI has `--kinds` (both tested with fakes, the editor chooser not seen by eye); no run on real screenshots; Apple Vision only, no Windows or Linux recognizer) · **Crates** cm-render, cm-daemon, cm-mcp, cm-cli, apps/desktop · **Depends on** ANN-2, RED-1

**User story** — As someone who shares screenshots, I want Auto-redact to find the payment cards, ID numbers, health and bank identifiers and credentials on the screen, so that I can blur them in one review instead of hunting for them.

**Description**
- The kinds are found by structure and checksum (Luhn, issuer prefix, mod 97, check digits, issuable ranges, position rules), not by label words. Rules are data with a test each and cite their public source in a comment (`crates/cm-render/src/ocr/rules/`).
- `card_extra`: an expiry (`MM/YY`, `MM/YYYY`, valid month) or a 3 or 4 digit code (4 for American Express) on the same line as, or on the line directly before or after, a detected card number. A lone `MM/YY` or 3-digit number elsewhere is never proposed; a code or expiry with no card number on the screen is a known miss.
- Masked forms (`***-**-6789`, `**** 1111`) are skipped.
- `kinds` limits the search to the named kinds; with none named the default kinds are used. `npi` is not a default (precision 30 % on the dev split, [report](../../verification/benchmarks/redaction-0.2.5-dev-2026-10-30.md)).
- Nothing is applied by the detector; the user accepts proposals (ANN-2).

**The kinds** (generated from the rule data; RED-3.AC1):

<!-- redaction-kinds:start -->
| Kind | Group | What it finds | Standards that name it | On by default | Check used |
|---|---|---|---|---|---|
| `email` | Contact | Email addresses (`local@domain.tld`). | HIPAA 6 | yes | Shape only |
| `token` | Credentials and secrets | API keys, JWTs and other long random-looking strings: known provider shapes, or a mix of 24 or more letters and digits (lower confidence). | GLBA / state breach laws (credentials) | yes | Known prefix and length, or letter-and-digit mix |
| `private_key` | Credentials and secrets | PEM private-key blocks, every line from the BEGIN line to the END line. | GLBA / state breach laws (credentials) | yes | BEGIN and END lines |
| `secret_assignment` | Credentials and secrets | The value in `NAME=value` or `"name": "value"` pairs whose name looks secret (password, token, key and similar). | GLBA / state breach laws (credentials) | yes | Secret-looking name, value is not a placeholder |
| `url_secret` | Credentials and secrets | A password inside a URL, or a secret value in a URL query string. | HIPAA 14; GLBA / state breach laws (credentials) | yes | Userinfo password or secret-looking parameter name |
| `card` | Payment cards | Payment card numbers of 13 to 19 digits; masked numbers are skipped. | PCI DSS; HIPAA 10; GLBA | yes | Luhn + issuer prefix + length |
| `card_extra` | Payment cards | An expiry date or security code beside a detected card number (found by position, no label needed). | PCI DSS (sensitive authentication data) | yes | Valid month, or 3 or 4 digits, beside a detected card |
| `ssn` | Identity and tax numbers | US Social Security numbers written `NNN-NN-NNNN` or with spaces; masked forms and bare nine-digit numbers are skipped. | NIST SP 800-122; HIPAA 7; GLBA | yes | Issuable ranges (no 000, 666 or 9xx area, no 00 group, no 0000 serial) |
| `itin` | Identity and tax numbers | US Individual Taxpayer Identification Numbers (`9NN-NN-NNNN` with an ITIN group). | NIST SP 800-122; HIPAA 7; GLBA | yes | ITIN group ranges |
| `ein` | Identity and tax numbers | US Employer Identification Numbers (`NN-NNNNNNN`). | NIST SP 800-122 | yes | Valid prefix |
| `aba` | Bank accounts | US bank routing numbers (nine digits). | HIPAA 10; GLBA | yes | Checksum + prefix range |
| `iban` | Bank accounts | International bank account numbers, solid or in groups of four. | HIPAA 10; GLBA | yes | Mod 97 |
| `npi` | Health identifiers | US National Provider Identifiers (ten digits). | HIPAA 11 | no | Luhn with the 80840 prefix |
| `dea` | Health identifiers | US DEA registration numbers (two letters, seven digits). | HIPAA 11 | yes | Check digit |
| `mbi` | Health identifiers | US Medicare Beneficiary Identifiers (11 characters). | HIPAA 9 | yes | Position rules for each character |
| `passport` | Identity and tax numbers | US passport book numbers written as a letter and eight digits. | HIPAA 11; GLBA | yes | Letter + eight digits |
| `vin` | Devices and networks | Vehicle identification numbers (17 characters). | HIPAA 12 | yes | Check digit at position 9 |
| `imei` | Devices and networks | Mobile device IMEI numbers (15 digits). | HIPAA 13 | yes | Luhn |
| `ip_address` | Devices and networks | IPv4 and IPv6 addresses; loopback and unspecified addresses are skipped. | HIPAA 15 | yes | Valid address syntax |
| `mac_address` | Devices and networks | MAC addresses (six hex pairs). | HIPAA 13 | yes | Six hex pairs, one separator style |
| `phone` | Contact | Phone and fax numbers of 10 to 15 digits with separators or a leading `+`, extensions included. | HIPAA 4, 5 | yes | NANP rules for US numbers; separators or `+` required |
<!-- redaction-kinds:end -->

The "Standards that name it" column maps kinds to public lists; it is not a compliance claim.

**Acceptance criteria**
- **AC1** Given a screenshot with a Luhn-valid card number, a dashed SSN (no label needed) and a PEM key block, then three proposals are made; given a 16-digit order number that fails Luhn and a bare nine-digit part number, then none is.
- **AC2** Given a masked SSN (`***-**-6789`) or a card shown as `**** 1111`, then nothing is proposed for the masked part.
- **AC3** Given `kinds: ["ssn"]`, then only SSN proposals are returned (MCP and CLI the same).
- **AC4** Given a Luhn-valid card number with `12/27` and `123` on the same line, then expiry and code are proposed as `card_extra`; given `12/27` and `123` with no card number on the screen, then nothing is.

**Edge cases and errors** — No recognizer: `not_implemented`, and the editor button is hidden. An unknown kind name: the MCP input schema lists the allowed names (an enum); the daemon rejects a name it does not know with `invalid_input` (`kinds[i]: must be one of ...`), and the CLI passes that message through. Overlapping hits: the first rule in the priority order keeps the characters; a page span that fully covers line spans replaces them, a partial overlap is dropped.

**Agent interface** — MCP `suggest_redactions { capture_id, kinds? }` (Free, read-only, never applied; `kinds` enum = the 21 names). CLI `capturemann annotate <id> --suggest-redactions [--kinds a,b]` (`--kinds` alone is `invalid_input`).

**Out of scope** — Names, street addresses, dates, driver's licences and other label-based kinds; faces; QR codes; automatic apply; redaction of screenshots returned to agents; video, audio and transcript redaction; other countries' standards; Windows and Linux recognizers.

**Test notes** — Tested with fakes (scripted `TextLine`s, no OCR engine), in `crates/cm-render/src/ocr/`:
- AC1: `cards_accepted` and `cards_rejected` (`rules/financial.rs`); `red2_through_suggest_redactions` (`rules/identity.rs`: a dashed SSN is proposed and a bare nine-digit part number is not); `suggest_redactions_finds_secrets_on_a_page` and `private_key_block_covers_every_line` (`rules/secrets.rs`). No single test puts the card, the SSN and the key block on one page.
- AC2: `through_suggest_redactions` (`rules/financial.rs`: a masked card gives nothing); `red2_through_suggest_redactions` (`rules/identity.rs`: a masked SSN gives nothing); `ssn_rejects_unissuable_and_shapes`.
- AC3: `kinds_filter_limits_what_is_found` (`detect.rs`) and `red2_through_suggest_redactions` (`rules/identity.rs`: `kinds: [ssn]` returns only the SSN). MCP schema: `mcp_schema_lists_every_redaction_kind` (`tests/redaction_kinds.rs`). CLI: `ann_2_ac4_annotate_suggest_redactions_is_read_only` (`crates/cm-cli/tests/cli.rs`) covers `--kinds` (single, comma, repeated, unknown name, alone). The editor chooser is tested with fakes (`apps/desktop/src/editor/redact.rs`) and not seen by eye.
- AC4: `extras_same_and_neighbour_lines`, `extras_need_a_card_and_adjacency` and `through_suggest_redactions` (`rules/financial.rs`).
- Other rules: `ssn_accepts_separated_forms`, `itin_ranges`, `ein_accepts_valid_prefix_only`, `npi_checksum`, `dea_checksum`, `mbi_positions`, `passport_letter_and_eight_digits`, `vin_check_digit`, `imei_luhn`, `negatives_across_rules` (`rules/identity.rs`); `ibans`, `abas` (`rules/financial.rs`); `ipv6_forms_are_found`, `mac_forms_are_found` (`rules/network.rs`); `nanp_forms_get_higher_confidence` (`rules/contact.rs`); `provider_shapes_are_found`, `assignments_in_common_syntaxes`, `url_query_secrets_are_the_values` (`rules/secrets.rs`).
- Through the clients: `ann_2_ac4_suggest_redactions_proposes_and_applies_nothing` (`crates/cm-mcp/tests/mcp.rs`), `ann_2_ac4_annotate_suggest_redactions_is_read_only` (`crates/cm-cli/tests/cli.rs`).
- Benchmark rows per kind: the 0.2.5 report. Manual: `tests/manual/ANN-2.md`, `tests/manual/RED-1.md`.

### RED-3 Documentation generated from the rule data
**Priority** P1 · **Status** In progress (the generated tables in `README.md`, `docs/user/redaction.md` and this file are checked by `crates/cm-render/tests/redaction_docs.rs`, tested; the website pages, the before-and-after image and the issue template of plan Stage 3 are not built) · **Crates** cm-render · **Depends on** RED-1, RED-2

**User story** — As a reader of the README, I want the list of kinds to be the list the code uses, so that the documentation cannot promise something the app does not do.

**Description**
- `RedactionKind` carries the documentation data: name, one-line summary, group, the standards that name it, whether it is on by default, and the check used. The tables between `<!-- redaction-kinds:start -->` and `<!-- redaction-kinds:end -->` are generated from it.
- The README section "Find personal data before you share a screenshot" holds the table, the benchmark table with its date and links to the two stored reports (each figure labelled synthetic, dev split, Apple Vision), the commands to reproduce, and the limits. Standards are named only in the mapping table, next to the line that this is not a compliance claim.

**Acceptance criteria**
- **AC1** The README table and the rule data list the same kinds (a test compares them).

**Edge cases and errors** — A table that differs fails the test with the command that regenerates it: `REDACTION_DOCS_UPDATE=1 cargo test -p cm-render --test redaction_docs`.

**Agent interface** — None.

**Out of scope** — The website, a before-and-after image, the issue template "redaction miss" (plan Stage 3 items 2 and 3).

**Test notes** — `readme_table_matches_the_rule_data`, `user_guide_table_matches_the_rule_data`, `spec_table_matches_the_rule_data`, `every_kind_has_documentation_data` in `crates/cm-render/tests/redaction_docs.rs`.
