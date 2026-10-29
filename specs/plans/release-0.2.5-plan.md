# Release 0.2.5 plan: automatic PII redaction, measured and published

Status: **draft** (Oct 29, 2026), nothing built. Requested by the maintainer after 0.2.4 was scoped: the
Auto-redact feature of the screenshot editor is liked, and it should become a headline feature: more kinds
of personal and secret data found, redaction that can run on its own, a public benchmark on a sample data
set, and promotion on GitHub and the website. Rules that apply: general techniques only, no one-image
fixes (open source); **nothing is called "verified" or "accurate" without a stored report**; all data in the
benchmark is synthetic (no real person's data, ever); on-device only, no network, no telemetry; no GPL;
every option has an MCP parameter and a CLI flag; UI text through `cm_i18n`. Builds on ANN-1 and ANN-2
(`specs/features/02-annotation.md`).

## What the app redacts today (0.2.3, checked in `crates/cm-render/src/ocr/detect.rs`)

Auto-redact (editor button, MCP `suggest_redactions`, CLI `capturemann annotate <id> --suggest-redactions`)
reads the text in a screenshot with the system's text recognizer (Apple Vision on macOS; nothing on
Windows yet), looks for the following in each recognized line, and **proposes** blur rectangles. Nothing is
applied until the user accepts; accepted proposals become blur ops (strength at least 16) in one undo step.

| Kind | What is found | Confidence |
|---|---|---|
| `email` | `local@domain.tld`, tld 2+ letters, with trailing punctuation and brackets handled | 0.95 |
| `token` | JWTs (`eyJ...`, three segments); well-known key shapes: `sk-`, `sk_live_`/`pk_live_`/`rk_live_`, `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`/`github_pat_`, `AIza`, `AKIA`/`ASIA` + 16, Slack `xox[baprs]`; any other 24+ character run of letters and digits mixed (at least 3 of each and 7 % digits), e.g. hex and base64 secrets | 0.95 known shapes, 0.6 generic |
| `ip_address` | dotted IPv4 (each part 0 to 255) | 0.5 |
| `phone` | 10 to 15 digits with at least two separators or a leading `+`; ISO dates and longer numbers are skipped | 0.5 |

How it works: a hit inside a line gets a rectangle for just that span (the word box split in proportion to
characters), padded and clipped; overlapping hits merge; the preview text is masked to the first and last
two characters. The detector is pure Rust over recognized lines, so it is tested with a scripted
recognizer; the real Vision recognizer ran once on one synthetic image.

**What it does not do (the gaps this release is about):**

- It has **no accuracy number**. No data set, no recall, no false-positive rate, no test of the whole chain
  (picture to OCR to detector to blur to exported file).
- Kinds that are common on screens but missing: payment card numbers, IBANs, national and tax ID numbers,
  street addresses, dates of birth, person names, private-key blocks, passwords and secrets written as
  `NAME=value`, URLs that carry credentials or tokens, IPv6, MAC addresses, user names inside file paths,
  faces, QR codes and barcodes.
- Phone and ID shapes are tuned for ASCII digits and a few formats; other scripts and local formats are
  untested (the users are partly Vietnamese speakers).
- It always waits for the user: there is no way to have it run on its own, and an agent that asks for a
  screenshot gets the unredacted picture.
- Images only; recorded video is not covered (a separate future feature).
- One recognizer (Apple Vision); a line split across two lines, or text too small or too low in contrast
  for the recognizer, is not found.

## The claim we want to be able to make, and the one we must not

Wanted, once the numbers exist: "CaptureMann finds and blurs the personal and secret data on a screenshot
on your own computer, without a cloud service, and here is how well it does on a public data set, and how
you can re-run the test." **Not allowed:** "removes all PII", "GDPR/HIPAA compliant", "100 %". The
interface and the website say that detection is automatic assistance, that the user is responsible for
checking, and that the numbers are for the data set described. Every number on the website links to the
stored report.

## Stage 0: the benchmark (first; every later stage is judged by it)

1. **A synthetic screenshot set** with exact ground truth, made by a general generator, not by hand:
   - Fake PII from rule-based generators that cannot hit real people: reserved or documented ranges only
     (`example.com`/`.org`/`.net` and test domains for emails, `192.0.2.0/24`, `198.51.100.0/24`,
     `203.0.113.0/24` and `2001:db8::/32` for addresses, fictional phone ranges such as the 555-01xx
     block, payment-card numbers from public test numbers and Luhn-valid random ones with test prefixes,
     IBANs with valid checksums on fictional bank codes, key shapes built from random characters with the
     real prefixes, documented example keys, names from a small fictional list).
   - Rendered into many **screen layouts** by the generator (a mail client, a chat, a terminal with
     environment variables, a code editor with a `.env` file, a browser developer panel, a spreadsheet, a
     web form, a settings page, a log viewer, a PDF-like document, a table of records), in **light and dark**
     themes, at 1x and 2x, at several font sizes (down to 10 px), with several fonts, in **English and
     Vietnamese** (with diacritics) and a few other scripts the OCR supports, with some text on
     patterned and photographic backgrounds.
   - **Negative screenshots** with no PII but lookalikes: build hashes, UUIDs, version strings, timestamps,
     long identifiers, code, ISBNs, order numbers, coordinates, tables of numbers. They measure false alarms.
   - Output: PNG plus a JSON of ground truth (kind, exact text, box). Size target: about 600 images
     (about 400 with PII, 200 without) and under 150 MB so it can be downloaded; generator, seed and
     licence (CC0 for the data, Apache-2.0 for the code) in `tests/data/redaction/` (data fetched by a
     script or kept in a release asset, not in git history, to keep the repository small).
2. **The harness** (`cm-render` example `redaction_bench`, headless, takes a recognizer, so Windows can
   plug in later): for each image runs recognize, detect, blur and export, and reports
   - **detection recall per kind**: share of ground-truth boxes covered by a proposal (at least 90 % of
     the box);
   - **precision per kind** and **false alarms per negative image**;
   - **end-to-end leak rate**: after applying all proposals, OCR the exported image again; a ground-truth
     string that can still be read is a leak (this is the number that matters for privacy; it also catches
     blur strength that is too weak);
   - the split by layout, theme, scale, font size and language, so weak places are visible;
   - time per image (the cost of running it automatically).
   Fixed seed, so a run on the same machine gives the same table. A **held-out half** of the layouts is
   never used while tuning; its numbers are read once per release.
3. **Baseline report** `verification/benchmarks/redaction-0.2.3-<date>.md`: today's detector on the whole
   set, including the kinds it does not know (they show as misses, which is the honest starting point).
   Also a **detector-only** table with perfect text (no OCR) so detector errors and OCR errors can be told
   apart.

Done when: the harness runs from a clean checkout with one command, and the baseline is stored.

## Stage 1: more kinds, with checks that make them reliable (AI-free, general)

Each kind is added only with its benchmark rows (recall, precision, leak rate), and kept if precision on the
negative images stays acceptable (a target fixed after the baseline, for example below one false alarm per
five screenshots for the default kinds). Techniques, not lists of cases:

1. **Checksummed identifiers:** payment cards (13 to 19 digits, Luhn, with spaces or dashes), IBAN (mod 97),
   other numbers that carry a check digit. A checksum makes precision high without any list.
2. **Secrets by structure:** more provider key shapes as *data* (a versioned file of prefix and length
   rules, with a test per rule; each rule cites its source), private-key blocks (`-----BEGIN ... KEY-----`
   through `END`, including the body lines), `NAME=value` and `"name": "value"` pairs where the name looks
   secret (the key rules of the job store's `scrub_secrets` are the model), URLs with credentials or
   query tokens, and an **entropy measure** for generic secrets that replaces the digits-ratio rule if the
   benchmark shows it is better.
3. **Network and device identifiers:** IPv6 (compressed forms), MAC addresses, with documented-range and
   private-range handling as an option.
4. **Contextual detection:** a value that follows a label ("Password", "SSN", "Date of birth", "Phone",
   "Địa chỉ", ...) is likely sensitive even without a recognisable shape. Labels live in the locale data
   (one list per language, translators can extend it, same mechanism as UI strings), and the match is
   by position (the value to the right of or below the label), not by a fixed screen. **Maintainer
   decision needed:** this is the one place where word lists are part of the design; it follows the rule
   "options and techniques, not one-case fixes" because the lists are per language and general, but say so
   if you disagree.
5. **Formats from other countries**, as data with tests: phone numbers by country (E.164 shapes and the
   national forms of the languages the app ships in, Vietnamese first), national ID shapes where a public
   format exists, postal codes only in context (a bare number is too ambiguous).
6. **User paths and names in text:** `/Users/<name>`, `C:\Users\<name>`, email-like display names. Only the
   name part is covered.
7. **Layout handling:** values broken across two lines; label and value in a table cell; text with letter
   spacing. Judged by the layout split of the benchmark.

Done when: each new kind has recall and precision stored, default kinds are chosen by the numbers, and the
README table of kinds is generated from the same rule data (so docs cannot drift from the code).

## Stage 2: platform recognizers for what rules cannot find (optional, decided by the numbers)

Only if Stage 1 leaves large gaps, and each behind the same trait as the text recognizer, on-device, with
its own benchmark rows:

1. **Person names and places** with the operating system's language tagging (macOS NaturalLanguage name
   tagger: nothing to ship; coverage per language is checked, not assumed). A model we ship would need a
   permissive licence, a SHA-256 pin and a size budget, and is a candidate only if the tagger is too weak.
2. **Faces** (Vision face rectangles) and **QR codes and barcodes** (Vision barcode request): kinds `face`,
   `code`, off by default, so a screenshot with a profile photo or a boarding pass is covered.
3. **Better text finding** for small or low-contrast text: run the recognizer on a 2x upscaled crop when the
   first pass finds little text in a busy area (measured against the font-size rows of the benchmark).

Rejected ideas are written down with their numbers. If no option is good enough, the release says "names
are not detected" in the documentation.

## Stage 3: running on its own (the "auto" part)

Off by default; a setting in Settings, Privacy (app-only keys, like the other `privacy.*` settings, which
`set_settings` cannot write):

1. **`privacy.auto_redact`** with three levels:
   - **Off** (today).
   - **Suggest:** when a screenshot is taken, detection runs in the background and the capture gets a badge
     ("3 items found") that opens the editor with the proposals shown; nothing changes in the image.
   - **Apply:** the proposals above a confidence threshold become blur ops automatically, recorded as one
     revision tagged `auto-redact` (so history shows what the app did), undoable in one step; the bundle
     keeps the original, as today, and the editor says so.
   A choice of **kinds** (a checklist), the threshold, and a **blur strength** (not below 16, with a
   pixelate option, because the benchmark leak rate decides which is safer).
2. **Gate before sharing:** with auto-redact on, Copy to clipboard, Save as, upload and publish of a
   screenshot use the redacted render; if detection has not finished, the action waits (up to a few
   seconds) and then asks. A confirmation line states "N items covered" with a link to the editor. Sharing
   the bundle folder itself shares the original; the share sheet warns about it.
3. **For AI agents:** an agent that calls `capture_screenshot` or any tool that returns an image gets the
   redacted render when `agents.redact_images` is on (an app-only key; default on once Apply works, and
   the first-run notice says so), so a language model never sees the secrets on the screen. Tools take an
   optional `redact` parameter that can only *tighten* the policy, never switch it off. This is the
   feature to promote to people who use agents.
4. **Same behaviour everywhere:** MCP `suggest_redactions` and a new `redact` mode on `annotate`, CLI
   `capturemann annotate <id> --redact [--kinds ...]`, and `capturemann capture screenshot --redact`;
   identical results, identical undo and confirmations. A redaction run reports the counts per kind and the
   time.
5. **Safety rules:** detection never uploads anything; it runs off the UI thread; a failure of the
   recognizer leaves the original unchanged and says so (it must never look as if a picture was checked
   when it was not: the badge shows "not checked"); the log (Part 1 of 0.2.4) records counts and kinds,
   never the text found.

Acceptance (new IDs `RED-1` to `RED-4` in a new feature file; ANN-2 keeps its IDs):
- **RED-1.AC1** Given the benchmark set, then the harness reports recall, precision and end-to-end leak rate
  per kind, stored in a report.
- **RED-2.AC1** Given a screenshot with a Luhn-valid card number and a PEM key block, then both are
  proposed (Stage 1), and an unchecked 16-digit order number is not.
- **RED-3.AC1** Given `privacy.auto_redact = apply`, when a screenshot is taken that contains an email,
  then the saved edits contain a blur over it tagged `auto-redact`, the original is unchanged and one undo
  removes it.
- **RED-3.AC2** Given the recognizer fails, then no blur is added, the capture shows "not checked", and
  Copy to clipboard asks before sharing.
- **RED-4.AC1** Given `agents.redact_images` on, when an agent calls `capture_screenshot`, then the
  returned image has the proposals blurred, and a call with `redact: false` does not change that.

## Stage 4: publish and promote (after the numbers are stored)

1. **README:** a "Private by design: automatic redaction" section: what is found (the generated table of
   kinds), a before and after image made from the synthetic set, the benchmark table with its date and the
   link to the report and to the data set, the one-line command to reproduce, and the honest limits.
2. **Website** (`craftmann.app`): a feature page with the same table, a short screen recording of Apply and
   of an agent receiving a redacted screenshot, and a "How we measured" page (data set, metrics, what a leak
   means, what is not covered). The wording check: every claim has a stored report; no "all", no
   compliance claims.
3. **The data set as a public asset:** a release asset or a data-hosting page with the generator, seed and
   licence, so others can test other tools too; an invitation to send new layouts and kinds (an issue
   template "redaction miss" that asks for a synthetic reproduction, never a real screenshot).
4. **Docs:** user guide chapter, `docs/development/redaction.md` (how a rule is added, how the benchmark is
   run), and the release notes.
5. **Comparison is optional:** a table against other tools is published only if they can be run offline on
   the same set with their licences allowing it; otherwise none. No claim about other products without
   numbers.

## Order and size (rough)

Stage 0 is about a week (the generator is the work). Stage 1 about a week, in the order of the numbers
(checksums and structured secrets first: high value, high precision). Stage 2 only if needed, about 3 days
per recognizer. Stage 3 about a week (settings, the background run, the gate, the agent policy, MCP and CLI).
Stage 4 about 3 days, after Stage 3. Core: Stages 0, 1 (items 1, 2, 3), 3 (items 1, 3, 4, 5) and 4;
everything else is stretch. Anything that does not meet its "done when" is dropped and written down.

Decisions needed from the maintainer: the contextual labels (Stage 1.4); whether Apply is allowed to be the
default for agents (Stage 3.3); the target for false alarms; whether faces and codes are in this release;
where the data set is hosted.

## Risks

- OCR quality is the ceiling: small text, low contrast, unusual fonts. The leak rate says it plainly, and the
  documentation does too.
- False alarms annoy users and teach them to turn it off; the default kinds are chosen by precision, and
  the generic-secret rule stays low-confidence unless the numbers say otherwise.
- A synthetic set can flatter the detector (it is made by us). The held-out layouts, the negatives and a
  small **manual run on screenshots of real apps made by the maintainer, never stored** (numbers only, in
  `tests/manual/`) are the cross-checks; the report says which numbers are synthetic.
- Privacy wording: a screenshot that was blurred by the app is not proof that nothing sensitive remains.
- Windows: the harness takes any recognizer, so Windows OCR can be benchmarked the day it exists.

## Not in 0.2.5

Video redaction, redaction of audio and transcripts (a related but separate feature: the transcript could
use the same text rules, noted for later), cloud OCR or AI models that send data away, and Windows.
