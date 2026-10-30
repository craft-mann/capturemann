# Auto-redact: find personal data before you share a screenshot

How to use Auto-redact in the screenshot editor, from the command line and from an agent, which kinds of data it looks for, and what its limits are. Written for people who share screenshots. The specification is [`specs/features/17-redaction.md`](../../specs/features/17-redaction.md); the measured numbers are in the stored reports linked at the end.

## What it does

Auto-redact reads the text in a screenshot with the text recognizer of your computer (Apple Vision on macOS), looks for data that has a recognizable format, and **proposes** blur rectangles. It does not apply anything on its own. You look at each proposal, accept the ones you want, and the accepted ones become ordinary blur operations in one undo step. It runs on your computer and sends nothing anywhere.

It works from **formats and checksums** (a number that passes the Luhn check and starts like a card number, a nine-digit routing number with a valid checksum, a `-----BEGIN PRIVATE KEY-----` block). It does not understand words, so it does not find names, street addresses or dates.

## In the editor

1. Open a screenshot from the library (the annotation editor).
2. Press **Auto-redact** under the canvas. The button is hidden on a computer with no text recognizer (Windows and Linux today).
3. Dashed boxes appear over what was found, and the inspector lists them: the kind and a **masked preview** (the first and last two characters only, or stars for short values). Nothing is blurred yet.
4. **Accept** one proposal, **Accept all**, or **Dismiss** to forget the proposals. Accepting adds blur operations (strength at least 16), all in one undo step, so Undo takes them back together.
5. Look at the result before you export or share. A box can be a little too wide or too narrow on proportional fonts, and text the recognizer could not read gets no box at all.

The editor looks for the default kinds (below). To choose kinds, open **What to look for** in the Auto-redact card: switches per group, one chip per kind, **Defaults** to reset and **Search again** to run with your choice. The choice is kept until the app closes.

## From the command line

```sh
capturemann annotate <id> --suggest-redactions          # print proposals for the default kinds
capturemann annotate <id> --redact-emails               # email addresses only
```

Both only print the proposals (rectangles in pixels of the original image, kind, confidence, masked preview). Accepting is a separate call: pass the rectangles you accept to `capturemann annotate <id> --ops` as `blur` ops.

Choose kinds with `--kinds` (comma separated or repeated), for example `capturemann annotate <id> --suggest-redactions --kinds ssn,card`. A `kinds` list replaces the default list, it does not add to it. `--kinds` without `--suggest-redactions` is an error.

## From an agent (MCP)

```json
{ "tool": "suggest_redactions", "arguments": { "capture_id": "<id>", "kinds": ["card", "ssn"] } }
```

`suggest_redactions` is a read-only, free-tier tool: it returns `{ rects: [{ rect, kind, confidence, text_preview }] }` and applies nothing; the preview is masked. `kinds` is optional; leave it out for the default kinds. Accepting is the `annotate` tool with `blur` ops (one call, one undo step). The tool fails with `not_implemented` when the computer has no text recognizer. This release does not apply redactions on behalf of an agent and does not redact screenshots returned to an agent.

## Which kinds, and how to turn `npi` on

These are all the kinds. The table is generated from the rule data and a test fails when it differs from the code; to change it, see [`docs/development/redaction.md`](../development/redaction.md).

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

The "Standards that name it" column maps each kind to public lists that mention that kind of data (NIST SP 800-122, HIPAA Safe Harbor identifiers, PCI DSS, GLBA and state breach-notification laws). **It is a mapping of kinds, not a compliance claim. CaptureMann is not a compliance tool, and using it does not make a screenshot compliant with any of them.**

**`npi` is off by default.** A ten-digit number passes its check digit about one time in ten, and timestamps look like it: on the synthetic dev split only 30 % of its proposals were real (see the 0.2.5 report below). Turn it on with its chip in **What to look for**, with `--kinds npi,...` on the command line or `"kinds": ["npi", ...]` in the MCP tool (a `kinds` list replaces the default list, it does not add to it).

## Reading the preview and the confidence

- The preview is **masked on purpose**, so a list of proposals does not repeat the secret: `ja...om` for a long value, stars for a short one.
- The confidence is how sure the rule is, scaled by how sure the recognizer was about the text. A value that passes a checksum (a card, a routing number) gets a higher number than a shape without one (a long mixed token at 0.6, a phone number of a non-US shape at 0.5). It orders the list; it is not a probability.
- A rectangle covers just the found text, padded a little. Two proposals that mostly overlap are merged and show the kind with the higher priority.
- A wrong kind label can happen when two kinds have the same shape (a nine-digit number that fits both an SSN and an ITIN range). The text is still covered.

## Review before accepting

Treat the list as a help, not an answer. **Everything the recognizer cannot read stays visible**, and false alarms exist. After accepting, look at the blurred picture. A blurred screenshot is not proof that nothing sensitive remains: names, addresses, faces, dates, file paths and anything the rules do not know are not touched.

## Limits

- Formats and checksums only: no names, addresses or dates, and no label-based kinds. A card security code or expiry date is found only beside a detected card number.
- Text that is too small, too faint or in an unusual font may not be read; a private-key block can be partly missed because the recognizer merges and re-splits its lines.
- Bare ten-digit Vietnamese mobile numbers (`0934851497`) are not found, and a password made only of letters (`letmein`) is not proposed.
- No Windows or Linux recognizer exists yet.
- All measurements below are on synthetic screenshots, from the dev split only.

## Numbers

Apple Vision, synthetic dev split of 211 images, 2026-10-30: the 0.2.5 rules found 95.3 % of 2,288 items at 90.9 % precision with a leak rate of 2.0 % (the 0.2.3 detector, which knew 4 kinds, scored 1,367 items: 92.2 %, 68.8 %, 4.2 %). These are upper bounds, not real-world accuracy. Reports: [0.2.3 baseline](../../verification/benchmarks/redaction-0.2.3-baseline-2026-10-30.md), [0.2.5 rules](../../verification/benchmarks/redaction-0.2.5-dev-2026-10-30.md).
