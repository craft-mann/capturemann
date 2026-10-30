# Auto-redact benchmark: the 0.2.5 rules (RED-1)

- **Date:** 2026-10-30
- **Who:** Claude (run for the maintainer)
- **Build:** commit 2d17724 (before the report commit), release build of `redaction_bench`
- **Machine:** Apple M4 Pro, macOS 26.6.2; recognizer Apple Vision (`apple_vision`, accurate level) where noted
- **Set:** synthetic, `redaction_bench gen --seed 20261030` (300 images, 13 layouts, 4 held out). **Only the `dev` split (211 images) was read; the `held_out` split has not been read.**
- **Wording:** these numbers are measured on synthetic data made by this project, with perfect knowledge of the formats the rules look for. They are an upper bound for real screenshots, not a measure of real-world accuracy or compliance. Nothing here is a verdict that a screenshot is free of personal data.

Commands:

```
cargo run -p cm-render --example redaction_bench --release --locked -- gen --out <dir>
cargo run -p cm-render --example redaction_bench --release --locked -- run --set <dir> --detector-only --split dev
cargo run -p cm-render --example redaction_bench --release --locked -- run --set <dir> --recognizer system --split dev
```

Kinds scored: the 20 default kinds plus `npi` (off by default, see below): all 21 in these tables.

## What the numbers say (dev split, Apple Vision)

| | 0.2.3 detector (4 kinds) | 0.2.5 rules (21 kinds) |
|---|---|---|
| truths scored | 1,367 | 2,288 |
| found, any kind | 92.2 % | 95.3 % |
| precision | 68.8 % | 90.9 % |
| false alarms per image / per negative image | 2.725 / 2.000 | 0.550 / 0.841 |
| end-to-end leak rate (readable text still readable after blur) | 4.2 % (58 of 1,367) | 2.0 % (46 of 2,288) |

Read it with care: the two columns score different truth sets, so the recall figures are not like for like
(on the 4 old kinds alone the new rules give: see the per-kind rows below for email, token, ip_address, phone).
With perfect text (run 2) the new rules find 99.2 % at 97.2 % precision; the gap to run 1 is the recognizer.

**Default kinds decided by these numbers.** `npi` is off by default (precision 30 %, 0.133 false alarms per image:
a 10-digit number passes its check digit one time in ten, and epoch timestamps look like it); name it to turn it
on. The generic long-token rule (`token`, 0.6 confidence, from 0.2.3) is above the false-alarm target of one per
five negative screenshots (0.49 per negative image, Apple Vision). It stays on because it is the 0.2.3 behaviour
(token precision 57.4 % here; 33.8 % for the old rule with perfect text, in the baseline report), but this is a
**decision for the maintainer** (options: keep, or split the generic rule into its own kind that is off by default).

## Known weak points seen in the numbers

- Private-key blocks: with Apple Vision 65.5 % of key lines are covered and 15.5 % stay readable after the blur
  (recognized lines of a base64 block are often merged or re-split, so the per-line boxes do not line up).
- `ssn` precision 56.8 %: 19 proposals sit inside another kind's truth (a same-shaped number of another kind);
  they are not counted as false alarms but the kind label is wrong.
- Bare ten-digit Vietnamese mobile numbers (`0934851497`) are not found (no separator, so they look like any
  ten-digit id). Plain-word passwords (`letmein`) are not proposed (a letters-only value is treated as a
  phrase, not a secret, by structure).
- Held-out layouts have not been read. Windows and Linux have no recognizer yet.

## Run 1: Apple Vision (end to end, with the leak pass)

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 211 (63) | 2288 | 95.3% (2180/2288) | 91.6% (2096/2288) | 2327 | 90.9% (2115/2327) | 0.550 | 0.841 | 2.0% (46/2288) |

Readable before blurring: 85.2% (1949/2288); of those still readable after: 2.3% (44/1949).

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 211 (63) | 426 | 97.4% (415/426) | 97.4% (415/426) | 417 | 99.8% (416/417) | 0.005 | 0.000 | 1.2% (5/426) |
| token | 211 (63) | 161 | 92.5% (149/161) | 75.2% (121/161) | 223 | 57.4% (128/223) | 0.246 | 0.492 | 0.0% (0/161) |
| private_key | 211 (63) | 84 | 65.5% (55/84) | 33.3% (28/84) | 28 | 100.0% (28/28) | 0.000 | 0.000 | 15.5% (13/84) |
| secret_assignment | 211 (63) | 286 | 99.3% (284/286) | 99.0% (283/286) | 314 | 90.1% (283/314) | 0.014 | 0.048 | 0.0% (0/286) |
| url_secret | 211 (63) | 79 | 94.9% (75/79) | 69.6% (55/79) | 57 | 100.0% (57/57) | 0.000 | 0.000 | 1.3% (1/79) |
| card | 211 (63) | 57 | 100.0% (57/57) | 100.0% (57/57) | 58 | 98.3% (57/58) | 0.005 | 0.016 | 0.0% (0/57) |
| card_extra | 211 (63) | 114 | 97.4% (111/114) | 97.4% (111/114) | 117 | 94.9% (111/117) | 0.028 | 0.063 | 2.6% (3/114) |
| ssn | 211 (63) | 25 | 100.0% (25/25) | 100.0% (25/25) | 44 | 56.8% (25/44) | 0.000 | 0.000 | 0.0% (0/25) |
| itin | 211 (63) | 36 | 100.0% (36/36) | 100.0% (36/36) | 36 | 100.0% (36/36) | 0.000 | 0.000 | 0.0% (0/36) |
| ein | 211 (63) | 25 | 100.0% (25/25) | 100.0% (25/25) | 25 | 100.0% (25/25) | 0.000 | 0.000 | 0.0% (0/25) |
| aba | 211 (63) | 48 | 100.0% (48/48) | 100.0% (48/48) | 50 | 96.0% (48/50) | 0.005 | 0.016 | 0.0% (0/48) |
| iban | 211 (63) | 42 | 95.2% (40/42) | 88.1% (37/42) | 38 | 100.0% (38/38) | 0.000 | 0.000 | 0.0% (0/42) |
| npi | 211 (63) | 13 | 92.3% (12/13) | 92.3% (12/13) | 40 | 30.0% (12/40) | 0.133 | 0.095 | 0.0% (0/13) |
| dea | 211 (63) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | n/a |
| mbi | 211 (63) | 10 | 80.0% (8/10) | 80.0% (8/10) | 8 | 100.0% (8/8) | 0.000 | 0.000 | 0.0% (0/10) |
| passport | 211 (63) | 50 | 98.0% (49/50) | 98.0% (49/50) | 51 | 96.1% (49/51) | 0.009 | 0.032 | 0.0% (0/50) |
| vin | 211 (63) | 11 | 90.9% (10/11) | 90.9% (10/11) | 10 | 100.0% (10/10) | 0.000 | 0.000 | 0.0% (0/11) |
| imei | 211 (63) | 13 | 100.0% (13/13) | 100.0% (13/13) | 13 | 100.0% (13/13) | 0.000 | 0.000 | 0.0% (0/13) |
| ip_address | 211 (63) | 333 | 96.1% (320/333) | 95.2% (317/333) | 317 | 100.0% (317/317) | 0.000 | 0.000 | 2.7% (9/333) |
| mac_address | 211 (63) | 28 | 92.9% (26/28) | 92.9% (26/28) | 26 | 100.0% (26/26) | 0.000 | 0.000 | 3.6% (1/28) |
| phone | 211 (63) | 447 | 94.4% (422/447) | 94.0% (420/447) | 455 | 94.1% (428/455) | 0.104 | 0.079 | 3.1% (14/447) |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: token 43, secret_assignment 28, ssn 19, aba 1, phone 5.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA | 211 (63) | 258 | 98.8% (255/258) | 97.7% (252/258) | - | - | - | - | 0.0% (0/258) |
| GLBA / state breach laws (credentials) | 211 (63) | 610 | 92.3% (563/610) | 79.8% (487/610) | - | - | - | - | 2.3% (14/610) |
| HIPAA 10 | 211 (63) | 147 | 98.6% (145/147) | 96.6% (142/147) | - | - | - | - | 0.0% (0/147) |
| HIPAA 11 | 211 (63) | 63 | 96.8% (61/63) | 96.8% (61/63) | - | - | - | - | 0.0% (0/63) |
| HIPAA 12 | 211 (63) | 11 | 90.9% (10/11) | 90.9% (10/11) | - | - | - | - | 0.0% (0/11) |
| HIPAA 13 | 211 (63) | 41 | 95.1% (39/41) | 95.1% (39/41) | - | - | - | - | 2.4% (1/41) |
| HIPAA 14 | 211 (63) | 79 | 94.9% (75/79) | 69.6% (55/79) | - | - | - | - | 1.3% (1/79) |
| HIPAA 15 | 211 (63) | 333 | 96.1% (320/333) | 95.2% (317/333) | - | - | - | - | 2.7% (9/333) |
| HIPAA 4, 5 | 211 (63) | 447 | 94.4% (422/447) | 94.0% (420/447) | - | - | - | - | 3.1% (14/447) |
| HIPAA 6 | 211 (63) | 426 | 97.4% (415/426) | 97.4% (415/426) | - | - | - | - | 1.2% (5/426) |
| HIPAA 7 | 211 (63) | 61 | 100.0% (61/61) | 100.0% (61/61) | - | - | - | - | 0.0% (0/61) |
| HIPAA 9 | 211 (63) | 10 | 80.0% (8/10) | 80.0% (8/10) | - | - | - | - | 0.0% (0/10) |
| NIST SP 800-122 | 211 (63) | 86 | 100.0% (86/86) | 100.0% (86/86) | - | - | - | - | 0.0% (0/86) |
| PCI DSS | 211 (63) | 57 | 100.0% (57/57) | 100.0% (57/57) | - | - | - | - | 0.0% (0/57) |
| PCI DSS (sensitive authentication data) | 211 (63) | 114 | 97.4% (111/114) | 97.4% (111/114) | - | - | - | - | 2.6% (3/114) |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 3 | n/a |
| bare_date | 1155 | 3 | 0.003 |
| build_number | 35 | 0 | 0.000 |
| coordinates | 157 | 0 | 0.000 |
| dollar_amount | 1836 | 1 | 0.001 |
| epoch_ms | 145 | 9 | 0.062 |
| git_hash | 6 | 6 | 1.000 |
| iban_fail | 17 | 1 | 0.059 |
| imei_fail | 3 | 0 | 0.000 |
| invoice_number | 141 | 0 | 0.000 |
| lone_code | 62 | 2 | 0.032 |
| lone_expiry | 62 | 2 | 0.032 |
| luhn_fail_16_digits | 62 | 1 | 0.016 |
| masked_account | 26 | 0 | 0.000 |
| masked_card | 17 | 2 | 0.118 |
| masked_ssn | 7 | 0 | 0.000 |
| npi_fail | 3 | 0 | 0.000 |
| order_number | 818 | 2 | 0.002 |
| part_number | 195 | 0 | 0.000 |
| placeholder_secret | 402 | 3 | 0.007 |
| plain_url | 250 | 4 | 0.016 |
| short_hash | 236 | 0 | 0.000 |
| table_number | 13 | 0 | 0.000 |
| ten_digit_id | 137 | 12 | 0.088 |
| timestamp | 837 | 39 | 0.047 |
| tracking_number | 589 | 20 | 0.034 |
| uuid | 115 | 6 | 0.052 |
| version_string | 819 | 0 | 0.000 |
| vin_fail | 7 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 211 (63) | 2288 | 95.3% (2180/2288) | 91.6% (2096/2288) | 2327 | 90.9% (2115/2327) | 0.550 | 0.841 | 2.0% (46/2288) |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bank_statement | 26 (6) | 92 | 100.0% (92/92) | 100.0% (92/92) | 94 | 97.9% (92/94) | 0.077 | 0.167 | 0.0% (0/92) |
| checkout | 22 (9) | 75 | 100.0% (75/75) | 100.0% (75/75) | 75 | 100.0% (75/75) | 0.000 | 0.000 | 0.0% (0/75) |
| crm_table | 24 (8) | 754 | 95.0% (716/754) | 94.4% (712/754) | 784 | 91.8% (720/784) | 2.500 | 4.125 | 2.9% (22/754) |
| env_file | 22 (7) | 292 | 88.7% (259/292) | 74.3% (217/292) | 274 | 79.2% (217/274) | 0.182 | 0.286 | 4.8% (14/292) |
| log_viewer | 22 (6) | 296 | 97.0% (287/296) | 97.0% (287/296) | 319 | 91.5% (292/319) | 1.227 | 0.500 | 0.0% (0/296) |
| mail_client | 25 (6) | 325 | 96.9% (315/325) | 91.4% (297/325) | 329 | 90.9% (299/329) | 0.480 | 1.333 | 1.5% (5/325) |
| patient_intake | 25 (9) | 93 | 94.6% (88/93) | 94.6% (88/93) | 88 | 100.0% (88/88) | 0.000 | 0.000 | 1.1% (1/93) |
| settings_page | 24 (6) | 198 | 96.5% (191/198) | 88.9% (176/198) | 202 | 87.1% (176/202) | 0.417 | 0.833 | 2.0% (4/198) |
| terminal | 21 (6) | 163 | 96.3% (157/163) | 93.3% (152/163) | 162 | 96.3% (156/162) | 0.048 | 0.167 | 0.0% (0/163) |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 122 (34) | 1434 | 95.2% (1365/1434) | 92.4% (1325/1434) | 1445 | 92.3% (1334/1445) | 0.516 | 0.824 | 2.2% (32/1434) |
| light | 89 (29) | 854 | 95.4% (815/854) | 90.3% (771/854) | 882 | 88.5% (781/882) | 0.596 | 0.862 | 1.6% (14/854) |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 107 (32) | 1175 | 95.1% (1117/1175) | 91.5% (1075/1175) | 1202 | 89.7% (1078/1202) | 0.729 | 1.469 | 2.7% (32/1175) |
| 2x | 104 (31) | 1113 | 95.5% (1063/1113) | 91.7% (1021/1113) | 1125 | 92.2% (1037/1125) | 0.365 | 0.194 | 1.3% (14/1113) |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 47 (14) | 557 | 94.6% (527/557) | 90.5% (504/557) | 557 | 90.8% (506/557) | 0.553 | 0.857 | 2.5% (14/557) |
| b: 11-12 px | 109 (29) | 1328 | 96.1% (1276/1328) | 93.9% (1247/1328) | 1357 | 93.0% (1262/1357) | 0.569 | 0.897 | 1.7% (22/1328) |
| c: 13-14 px | 37 (16) | 276 | 92.4% (255/276) | 82.6% (228/276) | 285 | 80.7% (230/285) | 0.676 | 0.938 | 2.9% (8/276) |
| d: 15-18 px | 18 (4) | 127 | 96.1% (122/127) | 92.1% (117/127) | 128 | 91.4% (117/128) | 0.167 | 0.000 | 1.6% (2/127) |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 153 (45) | 1549 | 95.9% (1486/1549) | 93.0% (1440/1549) | 1587 | 91.8% (1457/1587) | 0.477 | 0.556 | 1.3% (20/1549) |
| degraded | 58 (18) | 739 | 93.9% (694/739) | 88.8% (656/739) | 740 | 88.9% (658/740) | 0.741 | 1.556 | 3.5% (26/739) |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 198 (63) | 2077 | 95.6% (1986/2077) | 91.9% (1909/2077) | 2129 | 90.6% (1928/2129) | 0.571 | 0.841 | 1.6% (33/2077) |
| vi | 13 (0) | 211 | 91.9% (194/211) | 88.6% (187/211) | 198 | 94.4% (187/198) | 0.231 | n/a | 6.2% (13/211) |

## Time per image

- Recognition: mean 170.2 ms, p95 286.3 ms (211 images)
- Detection: mean 0.2 ms, p95 0.4 ms (211 images)


---

## Run 2: perfect text (detector only)

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 211 (63) | 2288 | 99.2% (2270/2288) | 98.3% (2248/2288) | 2317 | 97.2% (2252/2317) | 0.199 | 0.206 | - |

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 211 (63) | 426 | 100.0% (426/426) | 100.0% (426/426) | 426 | 100.0% (426/426) | 0.000 | 0.000 | - |
| token | 211 (63) | 161 | 100.0% (161/161) | 100.0% (161/161) | 172 | 93.6% (161/172) | 0.038 | 0.079 | - |
| private_key | 211 (63) | 84 | 100.0% (84/84) | 100.0% (84/84) | 84 | 100.0% (84/84) | 0.000 | 0.000 | - |
| secret_assignment | 211 (63) | 286 | 100.0% (286/286) | 100.0% (286/286) | 286 | 100.0% (286/286) | 0.000 | 0.000 | - |
| url_secret | 211 (63) | 79 | 94.9% (75/79) | 70.9% (56/79) | 60 | 100.0% (60/60) | 0.000 | 0.000 | - |
| card | 211 (63) | 57 | 100.0% (57/57) | 100.0% (57/57) | 57 | 100.0% (57/57) | 0.000 | 0.000 | - |
| card_extra | 211 (63) | 114 | 100.0% (114/114) | 100.0% (114/114) | 116 | 98.3% (114/116) | 0.009 | 0.000 | - |
| ssn | 211 (63) | 25 | 100.0% (25/25) | 100.0% (25/25) | 44 | 56.8% (25/44) | 0.000 | 0.000 | - |
| itin | 211 (63) | 36 | 100.0% (36/36) | 100.0% (36/36) | 36 | 100.0% (36/36) | 0.000 | 0.000 | - |
| ein | 211 (63) | 25 | 100.0% (25/25) | 100.0% (25/25) | 25 | 100.0% (25/25) | 0.000 | 0.000 | - |
| aba | 211 (63) | 48 | 100.0% (48/48) | 100.0% (48/48) | 49 | 98.0% (48/49) | 0.000 | 0.000 | - |
| iban | 211 (63) | 42 | 100.0% (42/42) | 92.9% (39/42) | 39 | 100.0% (39/39) | 0.000 | 0.000 | - |
| npi | 211 (63) | 13 | 100.0% (13/13) | 100.0% (13/13) | 43 | 30.2% (13/43) | 0.142 | 0.111 | - |
| dea | 211 (63) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | - |
| mbi | 211 (63) | 10 | 100.0% (10/10) | 100.0% (10/10) | 10 | 100.0% (10/10) | 0.000 | 0.000 | - |
| passport | 211 (63) | 50 | 100.0% (50/50) | 100.0% (50/50) | 50 | 100.0% (50/50) | 0.000 | 0.000 | - |
| vin | 211 (63) | 11 | 100.0% (11/11) | 100.0% (11/11) | 11 | 100.0% (11/11) | 0.000 | 0.000 | - |
| imei | 211 (63) | 13 | 100.0% (13/13) | 100.0% (13/13) | 13 | 100.0% (13/13) | 0.000 | 0.000 | - |
| ip_address | 211 (63) | 333 | 100.0% (333/333) | 100.0% (333/333) | 333 | 100.0% (333/333) | 0.000 | 0.000 | - |
| mac_address | 211 (63) | 28 | 100.0% (28/28) | 100.0% (28/28) | 28 | 100.0% (28/28) | 0.000 | 0.000 | - |
| phone | 211 (63) | 447 | 96.9% (433/447) | 96.9% (433/447) | 435 | 99.5% (433/435) | 0.009 | 0.016 | - |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: token 3, ssn 19, aba 1.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA | 211 (63) | 258 | 100.0% (258/258) | 98.8% (255/258) | - | - | - | - | - |
| GLBA / state breach laws (credentials) | 211 (63) | 610 | 99.3% (606/610) | 96.2% (587/610) | - | - | - | - | - |
| HIPAA 10 | 211 (63) | 147 | 100.0% (147/147) | 98.0% (144/147) | - | - | - | - | - |
| HIPAA 11 | 211 (63) | 63 | 100.0% (63/63) | 100.0% (63/63) | - | - | - | - | - |
| HIPAA 12 | 211 (63) | 11 | 100.0% (11/11) | 100.0% (11/11) | - | - | - | - | - |
| HIPAA 13 | 211 (63) | 41 | 100.0% (41/41) | 100.0% (41/41) | - | - | - | - | - |
| HIPAA 14 | 211 (63) | 79 | 94.9% (75/79) | 70.9% (56/79) | - | - | - | - | - |
| HIPAA 15 | 211 (63) | 333 | 100.0% (333/333) | 100.0% (333/333) | - | - | - | - | - |
| HIPAA 4, 5 | 211 (63) | 447 | 96.9% (433/447) | 96.9% (433/447) | - | - | - | - | - |
| HIPAA 6 | 211 (63) | 426 | 100.0% (426/426) | 100.0% (426/426) | - | - | - | - | - |
| HIPAA 7 | 211 (63) | 61 | 100.0% (61/61) | 100.0% (61/61) | - | - | - | - | - |
| HIPAA 9 | 211 (63) | 10 | 100.0% (10/10) | 100.0% (10/10) | - | - | - | - | - |
| NIST SP 800-122 | 211 (63) | 86 | 100.0% (86/86) | 100.0% (86/86) | - | - | - | - | - |
| PCI DSS | 211 (63) | 57 | 100.0% (57/57) | 100.0% (57/57) | - | - | - | - | - |
| PCI DSS (sensitive authentication data) | 211 (63) | 114 | 100.0% (114/114) | 100.0% (114/114) | - | - | - | - | - |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 1 | n/a |
| bare_date | 1155 | 2 | 0.002 |
| build_number | 35 | 0 | 0.000 |
| coordinates | 157 | 0 | 0.000 |
| dollar_amount | 1836 | 0 | 0.000 |
| epoch_ms | 145 | 0 | 0.000 |
| git_hash | 6 | 6 | 1.000 |
| iban_fail | 17 | 2 | 0.118 |
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
| placeholder_secret | 402 | 0 | 0.000 |
| plain_url | 250 | 0 | 0.000 |
| short_hash | 236 | 0 | 0.000 |
| table_number | 13 | 0 | 0.000 |
| ten_digit_id | 137 | 0 | 0.000 |
| timestamp | 837 | 30 | 0.036 |
| tracking_number | 589 | 0 | 0.000 |
| uuid | 115 | 0 | 0.000 |
| version_string | 819 | 0 | 0.000 |
| vin_fail | 7 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 211 (63) | 2288 | 99.2% (2270/2288) | 98.3% (2248/2288) | 2317 | 97.2% (2252/2317) | 0.199 | 0.206 | - |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bank_statement | 26 (6) | 92 | 100.0% (92/92) | 100.0% (92/92) | 95 | 96.8% (92/95) | 0.115 | 0.333 | - |
| checkout | 22 (9) | 75 | 100.0% (75/75) | 100.0% (75/75) | 75 | 100.0% (75/75) | 0.000 | 0.000 | - |
| crm_table | 24 (8) | 754 | 98.7% (744/754) | 98.7% (744/754) | 750 | 99.2% (744/750) | 0.250 | 0.500 | - |
| env_file | 22 (7) | 292 | 99.7% (291/292) | 99.7% (291/292) | 292 | 100.0% (292/292) | 0.000 | 0.000 | - |
| log_viewer | 22 (6) | 296 | 100.0% (296/296) | 100.0% (296/296) | 317 | 93.4% (296/317) | 0.955 | 0.333 | - |
| mail_client | 25 (6) | 325 | 99.4% (323/325) | 98.5% (320/325) | 327 | 97.9% (320/327) | 0.160 | 0.167 | - |
| patient_intake | 25 (9) | 93 | 98.9% (92/93) | 98.9% (92/93) | 92 | 100.0% (92/92) | 0.000 | 0.000 | - |
| settings_page | 24 (6) | 198 | 99.5% (197/198) | 91.9% (182/198) | 206 | 88.3% (182/206) | 0.333 | 0.667 | - |
| terminal | 21 (6) | 163 | 98.2% (160/163) | 95.7% (156/163) | 163 | 97.5% (159/163) | 0.000 | 0.000 | - |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 122 (34) | 1434 | 99.0% (1419/1434) | 98.3% (1410/1434) | 1442 | 98.0% (1413/1442) | 0.156 | 0.206 | - |
| light | 89 (29) | 854 | 99.6% (851/854) | 98.1% (838/854) | 875 | 95.9% (839/875) | 0.258 | 0.207 | - |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 107 (32) | 1175 | 99.0% (1163/1175) | 98.0% (1151/1175) | 1189 | 96.8% (1151/1189) | 0.234 | 0.344 | - |
| 2x | 104 (31) | 1113 | 99.5% (1107/1113) | 98.6% (1097/1113) | 1128 | 97.6% (1101/1128) | 0.163 | 0.065 | - |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 47 (14) | 557 | 99.6% (555/557) | 98.7% (550/557) | 569 | 96.8% (551/569) | 0.277 | 0.143 | - |
| b: 11-12 px | 109 (29) | 1328 | 98.9% (1314/1328) | 98.1% (1303/1328) | 1338 | 97.6% (1306/1338) | 0.193 | 0.241 | - |
| c: 13-14 px | 37 (16) | 276 | 99.6% (275/276) | 98.2% (271/276) | 282 | 96.1% (271/282) | 0.162 | 0.250 | - |
| d: 15-18 px | 18 (4) | 127 | 99.2% (126/127) | 97.6% (124/127) | 128 | 96.9% (124/128) | 0.111 | 0.000 | - |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 153 (45) | 1549 | 99.2% (1536/1549) | 98.2% (1521/1549) | 1573 | 96.9% (1524/1573) | 0.222 | 0.178 | - |
| degraded | 58 (18) | 739 | 99.3% (734/739) | 98.4% (727/739) | 744 | 97.8% (728/744) | 0.138 | 0.278 | - |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 198 (63) | 2077 | 99.8% (2073/2077) | 98.8% (2053/2077) | 2118 | 97.1% (2057/2118) | 0.207 | 0.206 | - |
| vi | 13 (0) | 211 | 93.4% (197/211) | 92.4% (195/211) | 199 | 98.0% (195/199) | 0.077 | n/a | - |

## Time per image

- Recognition: n/a (detector-only)
- Detection: mean 0.2 ms, p95 0.4 ms (211 images)

