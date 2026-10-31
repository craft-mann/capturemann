# Auto-redact benchmark: Linux, tesseract, dev split (RED-1)

- **Date:** 2026-10-31
- **Who:** Claude (agent), for the maintainer
- **Machine:** QEMU VM on an Apple Silicon Mac, Ubuntu 24.04 aarch64, 4 CPUs shared with other agents (so the timings below are rough); `tesseract 5.3.4` with leptonica 1.82.0 from the Ubuntu package `tesseract-ocr`, `eng` data only
- **Recognizer:** `tesseract` helper process (`crates/cm-render/src/ocr/tesseract.rs`), defaults: 3x up-scaling, page segmentation mode 6, inversion of mostly dark pictures
- **Set:** the same synthetic set as the macOS reports, `redaction_bench gen --seed 20261030` (300 images). **Only the `dev` split was read** (211 images). The `held_out` split was not read, so this report has no held-out figure and none of the choices made here (scale and mode) saw it.
- **Compare with:** `redaction-0.2.5-final-2026-10-30.md` (Apple Vision, dev): found 95.3 %, precision 92.0 %, 0.417 false alarms per image, leak rate 2.1 %. This run: found 84.5 %, precision 92.7 %, 0.431, leak rate 4.4 %. The leak rate re-reads the blurred picture with the same recognizer, so a weaker recognizer also reports a lower leak rate for the same blur: do not compare leak rates across recognizers.
- **Choice of scale and mode** (all `dev`, found any kind / precision): scale 1: 64.1 / 86.4; scale 2 with modes 11, 3, 6: 78.5 / 90.9, 78.8 / 91.4, 80.9 / 92.3; scale 3 with modes 11, 3, 6: 83.4 / 90.9, 82.5 / 91.5, **84.5 / 92.7**; scale 4 mode 6: 83.8 / 91.3. The first five runs used a line-ordering comparison that was later found to be able to panic on some pictures (fixed in `1c15aa7`); those five finished without hitting it, and the scale 3 mode 6 figure was reproduced after the fix (the final run below).
- **Wording:** synthetic data made by this project; an upper bound for real screenshots, not a measure of real-world accuracy or compliance. Weakest rows with this recognizer (found, any kind): MBI 30 % (3 of 10), VIN 64 %, passport 70 %, email 78 %, expiry and security code beside a card 77 %, `url_secret` 58 % with the right kind, the `checkout` layout 68 %; tokens: precision 67 % (the generic token rule, as on macOS). Per-kind and per-layout tables follow.

---

- Date: 2026-10-31
- Commit: 5adb8b7 (release build of `redaction_bench`, `--no-default-features`)
- Set: manifest version 1, seed 20261030, 300 images in the manifest
- Run: split `dev`: 211 images (63 negative), 2275 truths, 8152 decoys
- Recognizer: `tesseract`
- Kinds: email, token, private_key, secret_assignment, url_secret, card, card_extra, ssn, itin, ein, aba, iban, dea, mbi, passport, vin, imei, ip_address, mac_address, phone

> **All numbers here are synthetic.** The pictures come from a seeded generator with invented data (see `tests/data/redaction/`); they are not real screenshots and can flatter the detector. They are not a measure of any real-world compliance.
>
> **Thresholds are tuned on dev only; held_out is read once per release.**

## Definitions

- **Found (recall):** a truth box is found when the union of the proposals' rectangles covers at least 90 % of its area. *Any kind* uses all proposals; *right kind* only proposals of the truth's own kind. Truths of kinds outside the selected kinds are left out.
- **Precision (per kind):** the share of proposals of that kind with at least 50 % of their area inside the truth boxes of the same kind, each grown by `max(15 % of its height, 2 px)` (the detector's own padding). A proposal that is inside a truth of another kind is counted as *wrong kind*: not a hit for its kind, and not a false alarm.
- **False alarm:** a proposal that is not a true positive and either covers at least 30 % of a decoy box (counted under that decoy's `what`; the decoy covered most wins) or touches nothing listed (`(no decoy)`). Reported per image overall and per negative image (an image whose manifest lists no truth).
- **Leak rate (end-to-end):** every proposal is blurred with strength 16 (the editor's minimum, `MIN_STRENGTH`) by the same blur op the editor writes, the recognizer reads the blurred picture, and a truth is *leaked* when its letters and digits (case-folded, other characters ignored, at least 3) appear in the text of the lines touching its box grown by its own height. Truths shorter than 3 letters/digits are skipped. Only the system recognizer; the rate is over all truths of the selected kinds, so a missed truth that was readable counts. A truth not readable before blurring cannot leak; *readable before* is shown separately.
- **Per-standard roll-up:** a truth counts under each standard it lists, so rows overlap; recall and leak only (proposals do not carry a standard).
- **Time:** per image, recognition and detection separately; p95 is nearest-rank.

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 211 (63) | 2275 | 84.5% (1923/2275) | 82.3% (1873/2275) | 2111 | 92.7% (1957/2111) | 0.431 | 0.571 | 4.4% (100/2275) |

Readable before blurring: 74.7% (1699/2275); of those still readable after: 5.2% (88/1699).

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 211 (63) | 426 | 78.2% (333/426) | 78.2% (333/426) | 340 | 99.4% (338/340) | 0.009 | 0.000 | 6.8% (29/426) |
| token | 211 (63) | 161 | 83.2% (134/161) | 74.5% (120/161) | 198 | 67.2% (133/198) | 0.246 | 0.349 | 0.6% (1/161) |
| private_key | 211 (63) | 84 | 73.8% (62/84) | 67.9% (57/84) | 66 | 89.4% (59/66) | 0.033 | 0.000 | 8.3% (7/84) |
| secret_assignment | 211 (63) | 286 | 94.1% (269/286) | 93.7% (268/286) | 302 | 92.4% (279/302) | 0.024 | 0.063 | 0.3% (1/286) |
| url_secret | 211 (63) | 79 | 81.0% (64/79) | 58.2% (46/79) | 49 | 98.0% (48/49) | 0.005 | 0.000 | 5.1% (4/79) |
| card | 211 (63) | 57 | 86.0% (49/57) | 86.0% (49/57) | 55 | 100.0% (55/55) | 0.000 | 0.000 | 0.0% (0/57) |
| card_extra | 211 (63) | 114 | 77.2% (88/114) | 77.2% (88/114) | 93 | 97.8% (91/93) | 0.009 | 0.000 | 20.2% (23/114) |
| ssn | 211 (63) | 25 | 100.0% (25/25) | 100.0% (25/25) | 43 | 58.1% (25/43) | 0.000 | 0.000 | 0.0% (0/25) |
| itin | 211 (63) | 36 | 83.3% (30/36) | 83.3% (30/36) | 31 | 100.0% (31/31) | 0.000 | 0.000 | 5.6% (2/36) |
| ein | 211 (63) | 25 | 92.0% (23/25) | 92.0% (23/25) | 25 | 100.0% (25/25) | 0.000 | 0.000 | 0.0% (0/25) |
| aba | 211 (63) | 48 | 93.8% (45/48) | 93.8% (45/48) | 46 | 100.0% (46/46) | 0.000 | 0.000 | 2.1% (1/48) |
| iban | 211 (63) | 42 | 81.0% (34/42) | 78.6% (33/42) | 36 | 100.0% (36/36) | 0.000 | 0.000 | 0.0% (0/42) |
| dea | 211 (63) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | n/a |
| mbi | 211 (63) | 10 | 30.0% (3/10) | 30.0% (3/10) | 3 | 100.0% (3/3) | 0.000 | 0.000 | 0.0% (0/10) |
| passport | 211 (63) | 50 | 70.0% (35/50) | 70.0% (35/50) | 38 | 100.0% (38/38) | 0.000 | 0.000 | 0.0% (0/50) |
| vin | 211 (63) | 11 | 63.6% (7/11) | 63.6% (7/11) | 7 | 100.0% (7/7) | 0.000 | 0.000 | 0.0% (0/11) |
| imei | 211 (63) | 13 | 76.9% (10/13) | 76.9% (10/13) | 13 | 100.0% (13/13) | 0.000 | 0.000 | 0.0% (0/13) |
| ip_address | 211 (63) | 333 | 90.1% (300/333) | 87.1% (290/333) | 298 | 100.0% (298/298) | 0.000 | 0.000 | 4.2% (14/333) |
| mac_address | 211 (63) | 28 | 71.4% (20/28) | 71.4% (20/28) | 24 | 100.0% (24/24) | 0.000 | 0.000 | 3.6% (1/28) |
| phone | 211 (63) | 447 | 87.7% (392/447) | 87.5% (391/447) | 444 | 91.9% (408/444) | 0.104 | 0.159 | 3.8% (17/447) |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: token 13, secret_assignment 18, ssn 18, phone 14.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA | 211 (63) | 258 | 84.5% (218/258) | 84.1% (217/258) | - | - | - | - | 1.2% (3/258) |
| GLBA / state breach laws (credentials) | 211 (63) | 610 | 86.7% (529/610) | 80.5% (491/610) | - | - | - | - | 2.1% (13/610) |
| HIPAA 10 | 211 (63) | 147 | 87.1% (128/147) | 86.4% (127/147) | - | - | - | - | 0.7% (1/147) |
| HIPAA 11 | 211 (63) | 50 | 70.0% (35/50) | 70.0% (35/50) | - | - | - | - | 0.0% (0/50) |
| HIPAA 12 | 211 (63) | 11 | 63.6% (7/11) | 63.6% (7/11) | - | - | - | - | 0.0% (0/11) |
| HIPAA 13 | 211 (63) | 41 | 73.2% (30/41) | 73.2% (30/41) | - | - | - | - | 2.4% (1/41) |
| HIPAA 14 | 211 (63) | 79 | 81.0% (64/79) | 58.2% (46/79) | - | - | - | - | 5.1% (4/79) |
| HIPAA 15 | 211 (63) | 333 | 90.1% (300/333) | 87.1% (290/333) | - | - | - | - | 4.2% (14/333) |
| HIPAA 4, 5 | 211 (63) | 447 | 87.7% (392/447) | 87.5% (391/447) | - | - | - | - | 3.8% (17/447) |
| HIPAA 6 | 211 (63) | 426 | 78.2% (333/426) | 78.2% (333/426) | - | - | - | - | 6.8% (29/426) |
| HIPAA 7 | 211 (63) | 61 | 90.2% (55/61) | 90.2% (55/61) | - | - | - | - | 3.3% (2/61) |
| HIPAA 9 | 211 (63) | 10 | 30.0% (3/10) | 30.0% (3/10) | - | - | - | - | 0.0% (0/10) |
| NIST SP 800-122 | 211 (63) | 86 | 90.7% (78/86) | 90.7% (78/86) | - | - | - | - | 2.3% (2/86) |
| PCI DSS | 211 (63) | 57 | 86.0% (49/57) | 86.0% (49/57) | - | - | - | - | 0.0% (0/57) |
| PCI DSS (sensitive authentication data) | 211 (63) | 114 | 77.2% (88/114) | 77.2% (88/114) | - | - | - | - | 20.2% (23/114) |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 11 | n/a |
| bare_date | 1155 | 7 | 0.006 |
| build_number | 35 | 0 | 0.000 |
| coordinates | 157 | 0 | 0.000 |
| dollar_amount | 1836 | 0 | 0.000 |
| epoch_ms | 145 | 4 | 0.028 |
| git_hash | 6 | 5 | 0.833 |
| iban_fail | 17 | 0 | 0.000 |
| imei_fail | 3 | 0 | 0.000 |
| invoice_number | 141 | 0 | 0.000 |
| lone_code | 62 | 0 | 0.000 |
| lone_expiry | 62 | 0 | 0.000 |
| luhn_fail_16_digits | 62 | 0 | 0.000 |
| masked_account | 26 | 0 | 0.000 |
| masked_card | 17 | 1 | 0.059 |
| masked_ssn | 7 | 0 | 0.000 |
| npi_fail | 3 | 0 | 0.000 |
| order_number | 818 | 0 | 0.000 |
| part_number | 195 | 0 | 0.000 |
| placeholder_secret | 402 | 5 | 0.012 |
| plain_url | 250 | 3 | 0.012 |
| short_hash | 236 | 0 | 0.000 |
| table_number | 13 | 0 | 0.000 |
| ten_digit_id | 137 | 6 | 0.044 |
| timestamp | 837 | 4 | 0.005 |
| tracking_number | 589 | 17 | 0.029 |
| uuid | 115 | 28 | 0.243 |
| version_string | 819 | 0 | 0.000 |
| vin_fail | 7 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 211 (63) | 2275 | 84.5% (1923/2275) | 82.3% (1873/2275) | 2111 | 92.7% (1957/2111) | 0.431 | 0.571 | 4.4% (100/2275) |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bank_statement | 26 (6) | 92 | 93.5% (86/92) | 93.5% (86/92) | 91 | 98.9% (90/91) | 0.038 | 0.167 | 2.2% (2/92) |
| checkout | 22 (9) | 75 | 68.0% (51/75) | 68.0% (51/75) | 57 | 100.0% (57/57) | 0.000 | 0.000 | 24.0% (18/75) |
| crm_table | 24 (8) | 754 | 82.5% (622/754) | 81.2% (612/754) | 663 | 92.9% (616/663) | 1.583 | 2.500 | 6.4% (48/754) |
| env_file | 22 (7) | 292 | 84.9% (248/292) | 80.5% (235/292) | 277 | 88.1% (244/277) | 0.545 | 0.143 | 3.8% (11/292) |
| log_viewer | 22 (6) | 296 | 90.2% (267/296) | 90.2% (267/296) | 314 | 91.1% (286/314) | 1.273 | 1.000 | 0.3% (1/296) |
| mail_client | 25 (6) | 325 | 84.3% (274/325) | 81.2% (264/325) | 295 | 93.9% (277/295) | 0.160 | 0.667 | 3.7% (12/325) |
| patient_intake | 25 (9) | 80 | 83.8% (67/80) | 83.8% (67/80) | 67 | 100.0% (67/67) | 0.000 | 0.000 | 2.5% (2/80) |
| settings_page | 24 (6) | 198 | 80.8% (160/198) | 74.2% (147/198) | 190 | 88.4% (168/190) | 0.292 | 0.500 | 2.0% (4/198) |
| terminal | 21 (6) | 163 | 90.8% (148/163) | 88.3% (144/163) | 157 | 96.8% (152/157) | 0.048 | 0.167 | 1.2% (2/163) |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 122 (34) | 1424 | 85.0% (1211/1424) | 82.9% (1180/1424) | 1320 | 93.4% (1233/1320) | 0.410 | 0.618 | 4.6% (66/1424) |
| light | 89 (29) | 851 | 83.7% (712/851) | 81.4% (693/851) | 791 | 91.5% (724/791) | 0.461 | 0.517 | 4.0% (34/851) |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 107 (32) | 1165 | 82.2% (958/1165) | 79.7% (928/1165) | 1041 | 90.9% (946/1041) | 0.579 | 1.000 | 5.3% (62/1165) |
| 2x | 104 (31) | 1110 | 86.9% (965/1110) | 85.1% (945/1110) | 1070 | 94.5% (1011/1070) | 0.279 | 0.129 | 3.4% (38/1110) |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 47 (14) | 555 | 80.4% (446/555) | 77.1% (428/555) | 486 | 91.6% (445/486) | 0.404 | 0.286 | 4.0% (22/555) |
| b: 11-12 px | 109 (29) | 1321 | 87.7% (1159/1321) | 86.2% (1139/1321) | 1249 | 93.8% (1171/1249) | 0.477 | 0.793 | 4.6% (61/1321) |
| c: 13-14 px | 37 (16) | 273 | 79.9% (218/273) | 76.9% (210/273) | 255 | 89.4% (228/255) | 0.486 | 0.562 | 4.8% (13/273) |
| d: 15-18 px | 18 (4) | 126 | 79.4% (100/126) | 76.2% (96/126) | 121 | 93.4% (113/121) | 0.111 | 0.000 | 3.2% (4/126) |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 153 (45) | 1542 | 88.5% (1365/1542) | 86.7% (1337/1542) | 1514 | 93.5% (1415/1514) | 0.405 | 0.467 | 3.5% (54/1542) |
| degraded | 58 (18) | 733 | 76.1% (558/733) | 73.1% (536/733) | 597 | 90.8% (542/597) | 0.500 | 0.833 | 6.3% (46/733) |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 198 (63) | 2067 | 84.5% (1747/2067) | 82.4% (1703/2067) | 1932 | 92.5% (1787/1932) | 0.449 | 0.571 | 3.8% (79/2067) |
| vi | 13 (0) | 208 | 84.6% (176/208) | 81.7% (170/208) | 179 | 95.0% (170/179) | 0.154 | n/a | 10.1% (21/208) |

## Time per image

- Recognition: mean 956.1 ms, p95 1878.4 ms (211 images)
- Detection: mean 0.2 ms, p95 0.3 ms (211 images)
