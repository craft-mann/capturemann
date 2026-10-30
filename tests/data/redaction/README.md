# Redaction benchmark set (RED-1)

A synthetic set of screenshots with exact ground truth, used to measure Auto-redact. It is built
on demand from a seeded generator; no image is committed or published.

## What it is

- PNG screenshots of invented application screens plus `manifest.json` (format: the types in
  `crates/cm-render/examples/redaction_bench/schema.rs`).
- 13 layouts: bank statement, insurance card page, patient intake form, mail client, chat, CRM
  table, HR spreadsheet, terminal, `.env` in an editor, browser developer panel, log viewer,
  settings page, checkout form. Each is drawn light and dark, at 1x and 2x, with text from 10 px
  up, and some with striped, dotted, checkered or gradient backgrounds.
- About 70 % of the pictures carry sensitive values (`truths`), about 30 % are negatives with
  look-alikes only. Positive pictures also carry look-alikes (`decoys`) such as order numbers,
  UUIDs, masked numbers and Luhn-failing 16-digit numbers.
- About a quarter are degraded (rescaled down and up, slight blur, JPEG at quality 60 to 85).
- About 5 % are Vietnamese (`language: "vi"`), for diacritics and phone formats.
- Whole layouts are assigned to `held_out` (four of thirteen); the rest are `dev`.
- Every drawn line and word is listed with its box, so a perfect recognizer can be simulated.

## It is not real data

Everything is synthetic and linked to no person. Values are made from published formats and
checksums over random digits, never from a record:

- cards: issuer prefixes, Luhn check digit (plus a few well-known public test numbers);
- SSN: issuable ranges (area 001 to 899 except 666, group 01 to 99, serial 0001 to 9999); ITIN
  9xx-7x/8x/9x; EIN: IRS campus prefixes; ABA: weighted checksum; IBAN: mod 97; NPI: Luhn with
  the 80840 prefix; DEA: checksum; MBI: CMS position rules; VIN: check digit; IMEI: Luhn;
  passport: letter plus 8 digits;
- phones: the 555-01xx block set aside for fiction (or 800 area code); e-mail on `example.com`,
  `example.org`, `example.net`, `.test`, `.invalid`; IPv4 in `192.0.2.0/24`, `198.51.100.0/24`,
  `203.0.113.0/24`; IPv6 in `2001:db8::/32`; MAC in the documentation block or locally
  administered; API keys, private-key blocks and secrets are random characters in a provider's
  shape; names and streets come from small invented lists;
- the Vietnamese phone numbers follow the local formats over random digits; a random number can
  coincide with an assigned one by chance, as can any format-only value.

The checksum code in the generator is independent of the detector.

## Build it

```text
cargo run -p cm-render --example redaction_bench --release -- gen --out <dir> [--seed N] [--count N]
```

Default seed `20261030`, default count 300. The same seed and count give the same manifest and
pictures. `--debug-boxes` outlines truths (red) and decoys (blue) for a visual check of the boxes;
do not use it for a real set. Output goes outside the repository (or into `out/` here, which is
git-ignored).

## Licence

The generated data is CC0 1.0. The generator code is Apache-2.0 like the rest of the repository.
