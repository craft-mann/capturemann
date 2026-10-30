# Release 0.2.5 plan: US PII coverage for Auto-redact, measured and published

Status: **draft, revision 3** (Oct 30, 2026), nothing built. **Revision 3 narrows the scope to structure and checksum rules (group A) and secrets (group B); see "Scope of 0.2.5".** Scope set by the maintainer: Auto-redact keeps
working as it does today (the user reviews proposals and accepts them; **no automatic apply, no agent-side
redaction in this release**). What changes is *what it finds*: extend the target information so the kinds of
personal data named by the main US standards are covered as far as they can be seen on a screenshot, prove
it with a benchmark on a sample data set, and then promote the feature. Rules: general techniques only, no
one-image fixes (open source); **nothing is called "verified", "accurate" or "compliant" without a stored
report**; all benchmark data is synthetic; on-device only, no network, no telemetry; no GPL; every option
has an MCP parameter and a CLI flag; UI text through `cm_i18n`. Builds on ANN-1 and ANN-2
(`specs/features/02-annotation.md`).

## Scope of 0.2.5 (revision 3, Oct 30, 2026)

The maintainer limited this release to two groups. Everything else in this document that is not in them is
**deferred** (kept below as the longer-term map, marked where it matters).

**In 0.2.5: group A, structure and checksum rules** (no label or word list needed):
payment card (Luhn, issuer prefix, length 13 to 19) and **card security data beside it (expiry date and CVV/CVC, found by position next to a detected card number, no label list; see Stage 1 item 1)**, SSN (`NNN-NN-NNNN` or spaced, issuable ranges), ITIN, EIN,
ABA routing number, IBAN, NPI, DEA number, Medicare Beneficiary Identifier, VIN, IMEI, US passport number, phone
and fax (NANP rules, `1-800`, extensions), email (have), IPv4 (have), IPv6, MAC address.

**In 0.2.5: group B, secrets and credentials:** the token rules we have; more provider key shapes as versioned
data; PEM private-key blocks; `NAME=value` and `"name": "value"` pairs with a secret-looking name; URLs with
credentials or token query strings; an entropy measure for generic secrets only if the benchmark shows it beats
the digits-ratio rule.

**Deferred (not built, not benchmarked, not promoted in 0.2.5):** every kind that needs a label or a word list
(dates of birth and other dates, medical record and member numbers, driver's licence and state ID, account
numbers, licence plates, stand-alone CVV and expiry fields with no card number on screen (label-based, agreed Oct 30), labelled serials, names next to a label), street address blocks and ZIP, home-folder
user names, person names by the language tagger, faces, QR codes and barcodes, small-text re-reading, and
layout handling for values split across lines. Consequences:
- The open decision on **label lists is moot** for this release (no label data is added).
- A bare nine-digit number is **not** proposed as an SSN; only the dashed or spaced form is (it has no label to
  confirm it). Same for cards: Luhn alone is not enough, the issuer prefix and length are required.
- Public wording must not claim coverage of HIPAA identifiers 1, 2, 3, 8 (MRN), 9 (member ID), 11 (licences),
  12 (plates), 16, 17 or 18; the mapping table says "not in 0.2.5" for them.
- The benchmark set needs ground truth and layouts only for A and B kinds; the negatives still matter most.

## What the app redacts today (0.2.3, `crates/cm-render/src/ocr/detect.rs`)

Auto-redact (editor button, MCP `suggest_redactions`, CLI `capturemann annotate <id> --suggest-redactions`)
reads the text in a screenshot with the system recognizer (Apple Vision on macOS only), finds the kinds
below, and **proposes** blur rectangles. Nothing is applied until the user accepts; accepted proposals
become blur ops (strength at least 16) in one undo step.

| Kind | Found today | Confidence |
|---|---|---|
| `email` | `local@domain.tld`, tld 2+ letters, punctuation and brackets handled | 0.95 |
| `token` | JWTs; key shapes `sk-`, `sk_live_`/`pk_live_`/`rk_live_`, `ghp_` and the other GitHub prefixes, `github_pat_`, `AIza`, `AKIA`/`ASIA`, Slack `xox*`; any other 24+ character mix of letters and digits | 0.95 known shapes, 0.6 generic |
| `ip_address` | dotted IPv4 | 0.5 |
| `phone` | 10 to 15 digits with at least two separators or a leading `+` | 0.5 |

A hit inside a line gets a rectangle for just that span, padded and clipped; overlapping hits merge; the
preview is masked to the first and last two characters. It has **no accuracy number** (no data set, no
recall or false-alarm rate, no test of the whole chain from picture to exported file).

## The target: US standards, mapped to what can appear on a screen

"The US PII standard" is not one list, so the plan names the ones it follows and the mapping is published:

1. **NIST SP 800-122** (the federal definition of PII: name, SSN, date and place of birth, mother's maiden
   name, biometric records, and linked information such as financial, medical, educational, employment).
2. **HIPAA Safe Harbor** (45 CFR 164.514(b)(2)): the 18 identifiers. It is the most concrete list, and
   healthcare, support and compliance teams use it for screenshots.
3. **PCI DSS**: primary account numbers, and sensitive authentication data shown beside them.
4. **GLBA / state breach-notification laws** (e.g. California, New York): name with SSN, driver's licence
   or state ID, account or card number with its access code, medical and health insurance data, biometric
   data, and credentials (user name or email with password).
5. **Secrets and credentials** (what the app already covers, extended): not PII in law, but the second
   thing people blur.

| Identifier (source) | Visible on a screenshot as | Plan |
|---|---|---|
| Names (HIPAA 1, NIST) | any text | Stage 2 (platform tagger), plus label context in Stage 1 |
| Street address, city, county, ZIP (HIPAA 2) | address lines, forms | Stage 1: address block shapes; ZIP and ZIP+4 in context |
| Dates tied to a person: birth, admission, discharge, death (HIPAA 3) | `DOB: 01/02/1980` | Stage 1: dates in context of a label |
| Phone, fax (HIPAA 4, 5) | numbers | have phone; add US NANP rules (area code and exchange cannot start 0 or 1), `1-800`, extensions, labels `fax` |
| Email (HIPAA 6) | text | have |
| **SSN** (HIPAA 7, NIST, GLBA) | `123-45-6789`, `123 45 6789`, nine digits in context | Stage 1: structural rules (no 000, 666, 900-999 area; no 00 group; no 0000 serial), masked forms (`***-**-6789` is not a leak), ITIN (9xx-7x/8x) |
| Medical record number, health plan beneficiary number (HIPAA 8, 9) | labelled IDs; Medicare Beneficiary Identifier | Stage 1: MBI format (11 characters with the CMS position rules), NPI (10 digits, Luhn with the 80840 prefix), DEA number (checksum), labelled MRN or member IDs by context |
| Account numbers (HIPAA 10, GLBA, PCI) | bank, card, loan | Stage 1: **payment card (Luhn, issuer prefixes, 13 to 19 digits)**, **ABA routing (checksum)**, IBAN (mod 97), account numbers in context |
| Certificate and licence numbers (HIPAA 11, GLBA) | driver's licence, state ID, passport, professional licence | Stage 1: US passport (9 digits, or the letter-and-8-digits form), EIN (`NN-NNNNNNN` with valid prefixes), driver's licence only in context of a label because state formats overlap ordinary text (a table of the state shapes as data, used with a label) |
| Vehicle identifiers and plates (HIPAA 12) | VIN, licence plate | Stage 1: **VIN (17 characters, check digit)**; plates only in context |
| Device identifiers and serial numbers (HIPAA 13) | IMEI, serials | Stage 1: IMEI (Luhn), MAC addresses, labelled serials |
| URLs (HIPAA 14) | links | Stage 1: URLs with credentials or tokens in the query; personal-profile URLs are left to the user |
| IP addresses (HIPAA 15) | v4 and v6 | have v4; add IPv6 |
| Biometric identifiers, full-face photographs (HIPAA 16, 17) | fingerprints, photos | Stage 2: faces (Vision), off by default |
| Any other unique number or code (HIPAA 18) | QR codes, barcodes, ticket numbers | Stage 2: QR and barcodes |
| Credentials (GLBA, state laws) | `password: ...`, `NAME=value`, private keys | Stage 1: `NAME=value` and `"name": "value"` secrets, PEM private-key blocks, URLs with credentials |
| Names inside file paths (home folders) | `/Users/jane`, `C:\Users\Jane` | Stage 1: the name part |

**Honest limits written into the docs:** this follows the *kinds* of data the standards name. It does not
make a screenshot "HIPAA Safe Harbor compliant" (that is a legal determination about a whole data set and
what else is known), it does not remove text the recognizer cannot read, and it does not know that a name is
sensitive unless a tagger or a label says so. The interface and the website call it assistance that needs a
review. No compliance claims, and every number links to its stored report.

## Stage 0: the benchmark (first; every later stage is judged by it)

1. **A synthetic US screenshot set** with exact ground truth, from a general generator:
   - Fake data from rule-based generators that cannot point at real people: random numbers that satisfy the
     published *format* and *checksum* rules and are unlinked to any person (SSN shapes within the issuable
     ranges, Luhn card numbers and the public test numbers, ABA numbers with a valid checksum, NPI, DEA,
     VIN, IMEI, EIN, MBI), phone numbers in the fictional 555-01xx block and other ranges set aside for
     fiction, emails on `example.*` and reserved test domains, addresses built from invented street names
     with real ZIP code *shapes*, names from a fictional list, IPs in the documentation ranges
     (`192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`, `2001:db8::/32`), key shapes from random characters.
     The generator and its documentation say that it does not draw on any real record.
   - Rendered into many **screen layouts**: a patient portal and an intake form, a bank statement, an
     insurance card page, a mail client, a chat, a CRM record table, an HR spreadsheet, a terminal with
     environment variables, a `.env` file in an editor, a browser developer panel, a log viewer, a
     scanned-document look, a settings page; **light and dark**, 1x and 2x, font sizes down to 10 px,
     several fonts and some patterned backgrounds. English only for the US target, plus a small Vietnamese
     set for the diacritics and phone-format checks that the app's own users need.
   - **Negative screenshots** with lookalikes: nine-digit order numbers and part numbers, tracking numbers,
     UUIDs, git hashes, version strings, timestamps, ISBNs, 16-digit identifiers that fail Luhn,
     coordinates, dollar amounts, tables of numbers, dates without a label. They measure false alarms.
   - Output: PNG plus ground-truth JSON (kind, standard and identifier it maps to, exact text, box). About
     700 images (about 500 with PII, 200 negatives), under 200 MB. Generator, seed and licence (CC0 data,
     Apache-2.0 code) in `tests/data/redaction/`; the images are a release asset or built on demand, not
     committed.
2. **The harness** (`cm-render` example `redaction_bench`, headless, recognizer passed in so Windows can plug in):
   - **recall per identifier** (a ground-truth box counts when a proposal covers at least 90 % of it) and
     a roll-up **per standard** (NIST list, the 18 HIPAA identifiers, PCI, GLBA/state), so the website can
     show which identifiers are covered and how well;
   - **precision per kind** and **false alarms per negative image**;
   - **end-to-end leak rate**: apply every proposal, export, OCR the result again; a ground-truth string that
     is still readable is a leak (this is the number that matters for privacy; it also tests blur strength);
   - splits by layout, theme, scale, font size and language; time per image;
   - a **detector-only** run with perfect text, so rule errors and OCR errors can be told apart.
   Fixed seed; a **held-out half of the layouts** is read once per release.
3. **Baseline report** `verification/benchmarks/redaction-0.2.3-<date>.md`: today's detector on the whole
   set. Identifiers it does not know show as misses; that is the honest starting point.

Done when: one command builds the set and runs the harness from a clean checkout, and the baseline is stored.

## Stage 1: new kinds (AI-free, general techniques; revision 3: items 1 and 2 only)

Each kind arrives with its benchmark rows and is kept as a default only if the false-alarm rate on the
negatives stays low (target set after the baseline, for example below one false alarm per five screenshots
for the default set). Kinds that rely on context are proposed with lower confidence and labelled so the
user can see why. Techniques:

1. **Structure and checksum rules**, which give high precision without lists: payment cards (Luhn plus
   issuer prefixes and lengths; masked forms are skipped; **kind `card_extra`: an expiry (`MM/YY`, `MM/YYYY`, valid month) or a 3 or 4 digit code (4 for American Express) on the same line as, or the line directly after or before, a detected card number, or in the same card-shaped group, so no label word is needed; a lone `MM/YY` or 3-digit number elsewhere is never proposed; a CVV or expiry with no card number on the screen is a known miss and is written in the limits**), ABA routing numbers, IBAN, NPI, DEA, VIN (check
   digit), IMEI (Luhn), EIN prefixes, ITIN ranges, SSN issuable ranges, MBI position rules, NANP phone rules.
   Rules are data with a test each, and each cites its public source in a comment.
2. **Secrets by structure:** more provider key shapes as versioned data, private-key blocks through their
   end line, `NAME=value` and `"name": "value"` pairs with a secret-looking name (the key rules of the job
   store's `scrub_secrets` are the model), URLs with credentials or token query strings, an **entropy**
   measure for generic secrets if the benchmark shows it beats the digits-ratio rule, IPv6, MAC addresses.
3. **[Deferred, not in 0.2.5]** **Contextual detection:** a value that follows or sits under a label ("SSN", "Date of birth", "Member
   ID", "MRN", "Driver's license", "Account number", "Password", "Address", "ZIP"...) is likely sensitive.
   The label list is **data, per language** (English first, in the locale files so translators can extend
   it) and the match is by position (right of or below the label), not by a fixed screen. Used for dates of
   birth, medical and member numbers, driver's licence, account numbers, and address lines.
   **Maintainer decision needed:** this is the one place where word lists are part of the design. It fits the
   rule "techniques and options, not one-case fixes" because the lists are general and per language, but
   confirm.
4. **[Deferred, not in 0.2.5]** **Address blocks:** street line (number, street name, suffix such as St, Ave, Blvd, Apt, Suite),
   city, two-letter state, ZIP or ZIP+4, found as a pattern across one or two lines.
5. **[Deferred, not in 0.2.5]** **Dates in context:** only dates next to a label that ties them to a person (birth, admission, discharge,
   death); a bare date is never proposed.
6. **[Deferred, not in 0.2.5]** **File-path names:** the user-name part of home-folder paths.
7. **[Deferred, not in 0.2.5]** **Layout handling:** values broken across two lines, label and value in table cells, text with letter
   spacing; judged by the layout split of the benchmark.

Done when: recall, precision and leak rate are stored for each new kind, the default kinds are chosen by the
numbers, the kind names exist in MCP `suggest_redactions { kinds }` and the CLI `--kinds`, the Auto-redact
panel can switch kinds on and off, and the README table of kinds is **generated from the same rule data**
(so documentation cannot drift from the code).

## Stage 2: names, faces and codes through the operating system (deferred out of 0.2.5)

Not built in 0.2.5. Kept as the map for a later release; only where Stage 1 leaves a large gap, each behind the recognizer trait, on-device, with benchmark rows:

1. **Person names** with the operating system's language tagging (macOS NaturalLanguage name tagger: nothing
   to ship; its quality is measured, not assumed). A shipped model needs a permissive licence, a SHA-256 pin
   and a size budget, and is considered only if the tagger is too weak. Names are the biggest gap in
   HIPAA identifier 1; if they cannot be done well the docs say "names are found only next to a label".
2. **Faces** (Vision face rectangles; HIPAA 17) and **QR codes and barcodes** (Vision barcode request; HIPAA
   18): kinds `face` and `code`, off by default.
3. **Small or low-contrast text:** run the recognizer again on an upscaled crop where the first pass finds
   little text in a busy area; judged on the font-size rows.

Rejected ideas are written down with their numbers.

## Stage 3: publish and promote (after the numbers are stored)

1. **README:** a section "Find personal data before you share a screenshot": the generated table of
   identifiers and which US standard names each, the benchmark table with date and link to the report and
   data set, a before-and-after image from the synthetic set, the command to reproduce, and the limits.
2. **Website:** a feature page with the same table and a short screen recording of Auto-redact, and a "How we
   measured" page (data set, metrics, what a leak means, what is not covered, and "not a compliance tool").
   Wording check: every claim has a stored report; no "all", no "compliant", no "guaranteed".
3. **The data set as a public asset:** generator, seed and licence, so anyone can test other tools too, and an
   issue template "redaction miss" that asks for a synthetic reproduction and never a real screenshot.
4. **Docs:** user guide chapter, `docs/development/redaction.md` (how a rule is added, how the benchmark runs),
   release notes. Optional comparison with other tools only if they run offline on the same set and their
   licences allow it; otherwise none.

Acceptance (new IDs `RED-1` to `RED-3` in a feature file; ANN-2 keeps its IDs):
- **RED-1.AC1** Given the benchmark set, then the harness reports recall, precision, false alarms and
  end-to-end leak rate per identifier and per standard, and the report is stored.
- **RED-2.AC1** Given a screenshot with a Luhn-valid card number, a dashed SSN (no label needed) and a PEM key block,
  then three proposals are made; given a 16-digit order number that fails Luhn and a bare nine-digit part
  number, then none is.
- **RED-2.AC4** Given a Luhn-valid card number with `12/27` and `123` on the same line, then expiry and code are proposed as `card_extra`; given `12/27` and `123` with no card number on the screen, then nothing is.
- **RED-2.AC2** Given a masked SSN (`***-**-6789`) or a card shown as `**** 1111`, then nothing is proposed
  for the masked part.
- **RED-2.AC3** Given `kinds: ["ssn"]`, then only SSN proposals are returned (MCP and CLI the same).
- **RED-3.AC1** The README table and the rule data list the same kinds (a test compares them).

## Order and size (rough)

Stage 0 about a week (the generator is the work). Stage 1 about a week, in the order of value and precision:
checksummed identifiers (cards, SSN, ABA, NPI, VIN), structured secrets, then context and addresses. Stage 2
only if needed, about 3 days per recognizer. Stage 3 about 3 days. Revision 3 core: Stage 0, Stage 1 items 1 and 2, and
Stage 3 (so Stage 1 shrinks to a few days); everything else is deferred. Anything that does not meet its "done when" is dropped and written down.

Decisions (maintainer, Oct 30, 2026):
- **Release focus:** 0.2.5 is redaction only. Nothing else (including the issue 16 follow-up) is scheduled into it; if
  a 0.2.4 problem needs a fix it ships as its own 0.2.4.x.
- **False-alarm gate:** default-on kinds must stay below one false alarm per five negative screenshots; the
  number is re-checked against the baseline and any change is recorded here. Kinds above it ship off by default.
- **Data set:** built on demand from a seeded generator; no images are published or committed.
- **Website wording:** lead with the formats found (payment card and security data, SSN and tax IDs, health and
  bank identifiers, credentials); name NIST SP 800-122, HIPAA Safe Harbor and PCI DSS only in the mapping table,
  with the no-compliance line beside it.
- **Manual cross-check:** the maintainer runs Auto-redact on real app screenshots near the end; numbers only
  in `tests/manual/`, images never stored.
- **Benchmark design (adopted on my recommendation, say if you object):** degraded images (JPEG, rescaling,
  blur) with the leak rate reported for clean and degraded separately; a Rust-only renderer so one command
  works from a clean checkout; numeric recall and leak gates per default-on kind, set after the baseline;
  thresholds tuned on the dev half of the layouts only; a first set of about 300 images, grown later.

## Risks

- OCR quality is the ceiling (small text, low contrast, unusual fonts). The leak rate says so plainly.
- False alarms teach users to ignore the feature; defaults are chosen by precision, context kinds carry
  lower confidence, and the user stays in control of kinds.
- A synthetic set made by us can flatter the detector. The held-out layouts, the negatives, and a small
  **manual run on real app screenshots made by the maintainer, never stored** (numbers only, in
  `tests/manual/`) are the cross-checks; the report says which numbers are synthetic.
- Legal wording: a blurred screenshot is not proof that nothing sensitive remains.
- Windows: the harness takes any recognizer, so Windows OCR can be benchmarked the day it exists.

## Not in 0.2.5

Everything listed as deferred in "Scope of 0.2.5", automatic apply, redaction of screenshots returned to agents, video, audio and transcript redaction, cloud
OCR or AI models that send data away, other countries' standards (EU/GDPR, Vietnam, and so on; the rule data
is built so they can be added), and Windows.
