# Auto-redact benchmark: 0.2.5 final run, dev and held-out (RED-1)

- **Date:** 2026-10-30
- **Who:** Claude (run for the maintainer)
- **Machine:** Apple M4 Pro, macOS 26.6.2; recognizer Apple Vision (`apple_vision`, accurate level)
- **Build:** commit ea4e771, release build of `redaction_bench`; the 0.2.3 detector for the "old" columns is `crates/cm-render/src/ocr/detect.rs` from `f819484` in a scratch copy, same harness
- **Set:** synthetic, `redaction_bench gen --seed 20261030` (300 images, 13 layouts; 9 layouts = 211 images in `dev`, 4 layouts = 89 images in `held_out`)
- **Kinds scored:** the default kinds (20 for 0.2.5, `npi` is off by default; 4 for 0.2.3). The earlier interim report scored all 21 kinds including `npi`.
- **Held-out:** read **once**, in this run, after every threshold and default was fixed on `dev`. No rule was changed after reading it.
- **Wording:** synthetic data made by this project, with the rules written for these formats: an upper bound for real screenshots, not a measure of real-world accuracy or compliance. A blurred screenshot is not proof that nothing sensitive remains.

## Summary (Apple Vision)

| | dev: 0.2.3 | dev: 0.2.5 | held-out: 0.2.3 | held-out: 0.2.5 |
|---|---|---|---|---|
| images (with nothing to find) | 211 (63) | 211 (63) | 89 (27) | 89 (27) |
| items scored | 1,367 | 2,275 | 436 | 1,410 |
| found, any kind | 92.2 % | 95.3 % | 94.5 % | 98.6 % |
| precision | 68.8 % | 92.0 % | 69.2 % | 94.4 % |
| false alarms per image | 2.725 | 0.417 | 2.056 | 0.831 |
| false alarms per image with nothing to find | 2.000 | 0.746 | 2.074 | 0.889 |
| leak rate (readable after the blur) | 4.2 % (58) | 2.1 % (47) | 3.4 % (15) | 0.5 % (7) |

Perfect text instead of Apple Vision (separates rule errors from recognizer errors): dev 0.2.3 96.2 % found at 69.6 % precision; dev 0.2.5 99.2 % at 98.5 %, 0.057 false alarms per image; held-out 0.2.3 96.3 % at 69.7 %; held-out 0.2.5 100 % at 95.9 %, 0.685 false alarms per image.

**Read it with care.**
- The old and new columns score different sets of items (0.2.3 knows 4 of the kinds), so "found" is not like for like; the 0.2.3 column cannot find the other kinds at all.
- Held-out recall is higher than dev recall: the four held-out layouts (chat, HR sheet, dev panel, insurance card) are easier for these rules. They contain **no private keys and no URLs with secrets**, so those two kinds are measured on dev only.
- **The false-alarm target is not met on either split.** The maintainer's target was below one false alarm per five screenshots with nothing to find (0.2); 0.2.5 shows 0.746 (dev) and 0.889 (held-out). The generic long-token rule (unchanged behaviour from 0.2.3, kept on by the maintainer's decision) is the main cause: token precision 57.4 % on dev and 31.9 % on held-out, where 56 of 57 full 40-character git hashes were proposed as tokens (6 of 6 on dev). Every other default kind stays at or below 0.15 false alarms per image with nothing to find (the highest is `secret_assignment` on held-out, 0.148).
- Private-key blocks with Apple Vision: 65.5 % of key lines covered, 15.5 % still readable after the blur (dev only).
- Not measured: real screenshots (`tests/manual/RED-1.md`), Windows and Linux (no recognizer there).

Commands:

```
redaction_bench gen --out <dir>                                          # seed 20261030
redaction_bench run --set <dir> --recognizer system --split dev|held_out
redaction_bench run --set <dir> --detector-only --split dev|held_out
```

---

# Run A: 0.2.5, dev, Apple Vision

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 211 (63) | 2275 | 95.3% (2168/2275) | 91.6% (2084/2275) | 2287 | 92.0% (2103/2287) | 0.417 | 0.746 | 2.1% (47/2275) |

Readable before blurring: 85.1% (1937/2275); of those still readable after: 2.3% (44/1937).

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
| dea | 211 (63) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | n/a |
| mbi | 211 (63) | 10 | 80.0% (8/10) | 80.0% (8/10) | 8 | 100.0% (8/8) | 0.000 | 0.000 | 0.0% (0/10) |
| passport | 211 (63) | 50 | 98.0% (49/50) | 98.0% (49/50) | 51 | 96.1% (49/51) | 0.009 | 0.032 | 2.0% (1/50) |
| vin | 211 (63) | 11 | 90.9% (10/11) | 90.9% (10/11) | 10 | 100.0% (10/10) | 0.000 | 0.000 | 0.0% (0/11) |
| imei | 211 (63) | 13 | 100.0% (13/13) | 100.0% (13/13) | 13 | 100.0% (13/13) | 0.000 | 0.000 | 0.0% (0/13) |
| ip_address | 211 (63) | 333 | 96.1% (320/333) | 95.2% (317/333) | 317 | 100.0% (317/317) | 0.000 | 0.000 | 2.7% (9/333) |
| mac_address | 211 (63) | 28 | 92.9% (26/28) | 92.9% (26/28) | 26 | 100.0% (26/26) | 0.000 | 0.000 | 3.6% (1/28) |
| phone | 211 (63) | 447 | 94.4% (422/447) | 94.0% (420/447) | 455 | 94.1% (428/455) | 0.104 | 0.079 | 3.1% (14/447) |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: token 43, secret_assignment 28, ssn 19, aba 1, phone 5.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA | 211 (63) | 258 | 98.8% (255/258) | 97.7% (252/258) | - | - | - | - | 0.4% (1/258) |
| GLBA / state breach laws (credentials) | 211 (63) | 610 | 92.3% (563/610) | 79.8% (487/610) | - | - | - | - | 2.3% (14/610) |
| HIPAA 10 | 211 (63) | 147 | 98.6% (145/147) | 96.6% (142/147) | - | - | - | - | 0.0% (0/147) |
| HIPAA 11 | 211 (63) | 50 | 98.0% (49/50) | 98.0% (49/50) | - | - | - | - | 2.0% (1/50) |
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
| timestamp | 837 | 11 | 0.013 |
| tracking_number | 589 | 20 | 0.034 |
| uuid | 115 | 6 | 0.052 |
| version_string | 819 | 0 | 0.000 |
| vin_fail | 7 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 211 (63) | 2275 | 95.3% (2168/2275) | 91.6% (2084/2275) | 2287 | 92.0% (2103/2287) | 0.417 | 0.746 | 2.1% (47/2275) |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bank_statement | 26 (6) | 92 | 100.0% (92/92) | 100.0% (92/92) | 94 | 97.9% (92/94) | 0.077 | 0.167 | 0.0% (0/92) |
| checkout | 22 (9) | 75 | 100.0% (75/75) | 100.0% (75/75) | 75 | 100.0% (75/75) | 0.000 | 0.000 | 0.0% (0/75) |
| crm_table | 24 (8) | 754 | 95.0% (716/754) | 94.4% (712/754) | 780 | 92.3% (720/780) | 2.333 | 3.750 | 2.9% (22/754) |
| env_file | 22 (7) | 292 | 88.7% (259/292) | 74.3% (217/292) | 274 | 79.2% (217/274) | 0.182 | 0.286 | 4.8% (14/292) |
| log_viewer | 22 (6) | 296 | 97.0% (287/296) | 97.0% (287/296) | 298 | 98.0% (292/298) | 0.273 | 0.167 | 0.0% (0/296) |
| mail_client | 25 (6) | 325 | 96.9% (315/325) | 91.4% (297/325) | 326 | 91.7% (299/326) | 0.360 | 1.167 | 1.5% (5/325) |
| patient_intake | 25 (9) | 80 | 95.0% (76/80) | 95.0% (76/80) | 76 | 100.0% (76/76) | 0.000 | 0.000 | 2.5% (2/80) |
| settings_page | 24 (6) | 198 | 96.5% (191/198) | 88.9% (176/198) | 202 | 87.1% (176/202) | 0.417 | 0.833 | 2.0% (4/198) |
| terminal | 21 (6) | 163 | 96.3% (157/163) | 93.3% (152/163) | 162 | 96.3% (156/162) | 0.048 | 0.167 | 0.0% (0/163) |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 122 (34) | 1424 | 95.2% (1356/1424) | 92.4% (1316/1424) | 1422 | 93.2% (1325/1422) | 0.402 | 0.676 | 2.3% (33/1424) |
| light | 89 (29) | 851 | 95.4% (812/851) | 90.2% (768/851) | 865 | 89.9% (778/865) | 0.438 | 0.828 | 1.6% (14/851) |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 107 (32) | 1165 | 95.1% (1108/1165) | 91.5% (1066/1165) | 1178 | 90.7% (1069/1178) | 0.589 | 1.312 | 2.8% (33/1165) |
| 2x | 104 (31) | 1110 | 95.5% (1060/1110) | 91.7% (1018/1110) | 1109 | 93.2% (1034/1109) | 0.240 | 0.161 | 1.3% (14/1110) |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 47 (14) | 555 | 94.8% (526/555) | 90.6% (503/555) | 545 | 92.7% (505/545) | 0.319 | 0.786 | 2.5% (14/555) |
| b: 11-12 px | 109 (29) | 1321 | 96.1% (1269/1321) | 93.9% (1240/1321) | 1335 | 94.0% (1255/1335) | 0.431 | 0.759 | 1.7% (22/1321) |
| c: 13-14 px | 37 (16) | 273 | 92.3% (252/273) | 82.4% (225/273) | 281 | 80.8% (227/281) | 0.649 | 0.875 | 3.3% (9/273) |
| d: 15-18 px | 18 (4) | 126 | 96.0% (121/126) | 92.1% (116/126) | 126 | 92.1% (116/126) | 0.111 | 0.000 | 1.6% (2/126) |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 153 (45) | 1542 | 95.9% (1479/1542) | 92.9% (1433/1542) | 1557 | 93.1% (1450/1557) | 0.327 | 0.489 | 1.4% (21/1542) |
| degraded | 58 (18) | 733 | 94.0% (689/733) | 88.8% (651/733) | 730 | 89.5% (653/730) | 0.655 | 1.389 | 3.5% (26/733) |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 198 (63) | 2067 | 95.6% (1977/2067) | 91.9% (1900/2067) | 2093 | 91.7% (1919/2093) | 0.434 | 0.746 | 1.6% (33/2067) |
| vi | 13 (0) | 208 | 91.8% (191/208) | 88.5% (184/208) | 194 | 94.8% (184/194) | 0.154 | n/a | 6.7% (14/208) |

## Time per image

- Recognition: mean 157.2 ms, p95 252.6 ms (211 images)
- Detection: mean 0.2 ms, p95 0.4 ms (211 images)


---

# Run B: 0.2.5, held-out, Apple Vision

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 89 (27) | 1410 | 98.6% (1390/1410) | 97.9% (1381/1410) | 1460 | 94.4% (1378/1460) | 0.831 | 0.889 | 0.5% (7/1410) |

Readable before blurring: 96.3% (1358/1410); of those still readable after: 0.4% (6/1358).

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 89 (27) | 36 | 97.2% (35/36) | 97.2% (35/36) | 35 | 100.0% (35/35) | 0.000 | 0.000 | 0.0% (0/36) |
| token | 89 (27) | 37 | 89.2% (33/37) | 78.4% (29/37) | 94 | 31.9% (30/94) | 0.708 | 0.667 | 0.0% (0/37) |
| private_key | 89 (27) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | n/a |
| secret_assignment | 89 (27) | 20 | 95.0% (19/20) | 80.0% (16/20) | 25 | 64.0% (16/25) | 0.056 | 0.148 | 0.0% (0/20) |
| url_secret | 89 (27) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | n/a |
| card | 89 (27) | 28 | 100.0% (28/28) | 100.0% (28/28) | 28 | 100.0% (28/28) | 0.000 | 0.000 | 0.0% (0/28) |
| card_extra | 89 (27) | 42 | 92.9% (39/42) | 92.9% (39/42) | 39 | 100.0% (39/39) | 0.000 | 0.000 | 7.1% (3/42) |
| ssn | 89 (27) | 150 | 99.3% (149/150) | 99.3% (149/150) | 149 | 98.0% (146/149) | 0.034 | 0.000 | 0.7% (1/150) |
| itin | 89 (27) | 126 | 100.0% (126/126) | 100.0% (126/126) | 126 | 100.0% (126/126) | 0.000 | 0.000 | 0.0% (0/126) |
| ein | 89 (27) | 256 | 100.0% (256/256) | 100.0% (256/256) | 256 | 100.0% (256/256) | 0.000 | 0.000 | 0.0% (0/256) |
| aba | 89 (27) | 266 | 100.0% (266/266) | 100.0% (266/266) | 268 | 99.3% (266/268) | 0.022 | 0.074 | 0.0% (0/266) |
| iban | 89 (27) | 21 | 90.5% (19/21) | 90.5% (19/21) | 19 | 100.0% (19/19) | 0.000 | 0.000 | 0.0% (0/21) |
| dea | 89 (27) | 10 | 100.0% (10/10) | 90.0% (9/10) | 9 | 100.0% (9/9) | 0.000 | 0.000 | 0.0% (0/10) |
| mbi | 89 (27) | 16 | 93.8% (15/16) | 93.8% (15/16) | 15 | 100.0% (15/15) | 0.000 | 0.000 | 0.0% (0/16) |
| passport | 89 (27) | 22 | 100.0% (22/22) | 100.0% (22/22) | 23 | 95.7% (22/23) | 0.000 | 0.000 | 0.0% (0/22) |
| vin | 89 (27) | 10 | 90.0% (9/10) | 90.0% (9/10) | 9 | 100.0% (9/9) | 0.000 | 0.000 | 0.0% (0/10) |
| imei | 89 (27) | 4 | 75.0% (3/4) | 75.0% (3/4) | 3 | 100.0% (3/3) | 0.000 | 0.000 | 0.0% (0/4) |
| ip_address | 89 (27) | 52 | 92.3% (48/52) | 90.4% (47/52) | 47 | 100.0% (47/47) | 0.000 | 0.000 | 3.8% (2/52) |
| mac_address | 89 (27) | 3 | 66.7% (2/3) | 66.7% (2/3) | 2 | 100.0% (2/2) | 0.000 | 0.000 | 33.3% (1/3) |
| phone | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | 313 | 99.0% (310/313) | 0.011 | 0.000 | 0.0% (0/311) |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: token 1, secret_assignment 4, passport 1, phone 2.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA | 89 (27) | 613 | 99.5% (610/613) | 99.5% (610/613) | - | - | - | - | 0.2% (1/613) |
| GLBA / state breach laws (credentials) | 89 (27) | 57 | 91.2% (52/57) | 78.9% (45/57) | - | - | - | - | 0.0% (0/57) |
| HIPAA 10 | 89 (27) | 315 | 99.4% (313/315) | 99.4% (313/315) | - | - | - | - | 0.0% (0/315) |
| HIPAA 11 | 89 (27) | 32 | 100.0% (32/32) | 96.9% (31/32) | - | - | - | - | 0.0% (0/32) |
| HIPAA 12 | 89 (27) | 10 | 90.0% (9/10) | 90.0% (9/10) | - | - | - | - | 0.0% (0/10) |
| HIPAA 13 | 89 (27) | 7 | 71.4% (5/7) | 71.4% (5/7) | - | - | - | - | 14.3% (1/7) |
| HIPAA 15 | 89 (27) | 52 | 92.3% (48/52) | 90.4% (47/52) | - | - | - | - | 3.8% (2/52) |
| HIPAA 4, 5 | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | - | - | - | - | 0.0% (0/311) |
| HIPAA 6 | 89 (27) | 36 | 97.2% (35/36) | 97.2% (35/36) | - | - | - | - | 0.0% (0/36) |
| HIPAA 7 | 89 (27) | 276 | 99.6% (275/276) | 99.6% (275/276) | - | - | - | - | 0.4% (1/276) |
| HIPAA 9 | 89 (27) | 16 | 93.8% (15/16) | 93.8% (15/16) | - | - | - | - | 0.0% (0/16) |
| NIST SP 800-122 | 89 (27) | 532 | 99.8% (531/532) | 99.8% (531/532) | - | - | - | - | 0.2% (1/532) |
| PCI DSS | 89 (27) | 28 | 100.0% (28/28) | 100.0% (28/28) | - | - | - | - | 0.0% (0/28) |
| PCI DSS (sensitive authentication data) | 89 (27) | 42 | 92.9% (39/42) | 92.9% (39/42) | - | - | - | - | 7.1% (3/42) |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 6 | n/a |
| bare_date | 710 | 0 | 0.000 |
| build_number | 20 | 0 | 0.000 |
| coordinates | 18 | 0 | 0.000 |
| dollar_amount | 710 | 0 | 0.000 |
| epoch_ms | 91 | 0 | 0.000 |
| file_size | 668 | 0 | 0.000 |
| git_hash | 57 | 56 | 0.982 |
| iban_fail | 12 | 1 | 0.083 |
| invoice_number | 172 | 0 | 0.000 |
| lone_code | 25 | 0 | 0.000 |
| lone_expiry | 25 | 0 | 0.000 |
| luhn_fail_16_digits | 29 | 0 | 0.000 |
| masked_account | 496 | 0 | 0.000 |
| masked_card | 7 | 0 | 0.000 |
| masked_ssn | 57 | 0 | 0.000 |
| npi_fail | 4 | 0 | 0.000 |
| order_number | 349 | 0 | 0.000 |
| part_number | 374 | 0 | 0.000 |
| placeholder_secret | 66 | 5 | 0.076 |
| plain_url | 14 | 0 | 0.000 |
| short_hash | 2 | 0 | 0.000 |
| table_number | 76 | 2 | 0.026 |
| ten_digit_id | 95 | 0 | 0.000 |
| timestamp | 170 | 0 | 0.000 |
| tracking_number | 43 | 0 | 0.000 |
| uuid | 47 | 4 | 0.085 |
| version_string | 91 | 0 | 0.000 |
| vin_fail | 3 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| held_out | 89 (27) | 1410 | 98.6% (1390/1410) | 97.9% (1381/1410) | 1460 | 94.4% (1378/1460) | 0.831 | 0.889 | 0.5% (7/1410) |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| chat | 24 (6) | 172 | 95.9% (165/172) | 91.9% (158/172) | 168 | 94.0% (158/168) | 0.167 | 0.333 | 0.6% (1/172) |
| devtools | 22 (8) | 132 | 93.9% (124/132) | 93.2% (123/132) | 189 | 64.0% (121/189) | 3.045 | 2.500 | 1.5% (2/132) |
| hr_sheet | 22 (7) | 1004 | 99.9% (1003/1004) | 99.9% (1003/1004) | 1005 | 99.7% (1002/1005) | 0.136 | 0.286 | 0.1% (1/1004) |
| insurance_card | 21 (6) | 102 | 96.1% (98/102) | 95.1% (97/102) | 98 | 99.0% (97/98) | 0.000 | 0.000 | 2.9% (3/102) |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 48 (14) | 730 | 97.9% (715/730) | 97.0% (708/730) | 749 | 94.4% (707/749) | 0.750 | 0.929 | 0.5% (4/730) |
| light | 41 (13) | 680 | 99.3% (675/680) | 99.0% (673/680) | 711 | 94.4% (671/711) | 0.927 | 0.846 | 0.4% (3/680) |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 45 (12) | 796 | 97.9% (779/796) | 97.4% (775/796) | 817 | 94.6% (773/817) | 0.911 | 1.083 | 0.5% (4/796) |
| 2x | 44 (15) | 614 | 99.5% (611/614) | 98.7% (606/614) | 643 | 94.1% (605/643) | 0.750 | 0.733 | 0.5% (3/614) |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 29 (10) | 607 | 98.0% (595/607) | 97.7% (593/607) | 626 | 94.4% (591/626) | 1.138 | 1.300 | 0.3% (2/607) |
| b: 11-12 px | 30 (12) | 393 | 98.5% (387/393) | 97.2% (382/393) | 405 | 94.1% (381/405) | 0.667 | 0.833 | 0.8% (3/393) |
| c: 13-14 px | 21 (4) | 266 | 99.2% (264/266) | 98.9% (263/266) | 279 | 94.3% (263/279) | 0.714 | 0.250 | 0.8% (2/266) |
| d: 15-18 px | 9 (1) | 144 | 100.0% (144/144) | 99.3% (143/144) | 150 | 95.3% (143/150) | 0.667 | 0.000 | 0.0% (0/144) |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 63 (20) | 1064 | 99.3% (1057/1064) | 98.8% (1051/1064) | 1111 | 94.5% (1050/1111) | 0.873 | 1.100 | 0.5% (5/1064) |
| degraded | 26 (7) | 346 | 96.2% (333/346) | 95.4% (330/346) | 349 | 94.0% (328/349) | 0.731 | 0.286 | 0.6% (2/346) |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 87 (27) | 1387 | 98.8% (1371/1387) | 98.3% (1364/1387) | 1440 | 94.5% (1361/1440) | 0.839 | 0.889 | 0.5% (7/1387) |
| vi | 2 (0) | 23 | 82.6% (19/23) | 73.9% (17/23) | 20 | 85.0% (17/20) | 0.500 | n/a | 0.0% (0/23) |

## Time per image

- Recognition: mean 163.7 ms, p95 282.5 ms (89 images)
- Detection: mean 0.2 ms, p95 0.4 ms (89 images)


---

# Run C: 0.2.3, held-out, Apple Vision

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 89 (27) | 436 | 94.5% (412/436) | 94.3% (411/436) | 597 | 69.2% (413/597) | 2.056 | 2.074 | 3.4% (15/436) |

Readable before blurring: 92.2% (402/436); of those still readable after: 3.7% (15/402).

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 89 (27) | 36 | 97.2% (35/36) | 97.2% (35/36) | 35 | 100.0% (35/35) | 0.000 | 0.000 | 0.0% (0/36) |
| token | 89 (27) | 37 | 78.4% (29/37) | 78.4% (29/37) | 146 | 21.9% (32/146) | 1.281 | 1.519 | 2.7% (1/37) |
| ip_address | 89 (27) | 52 | 71.2% (37/52) | 69.2% (36/52) | 36 | 100.0% (36/36) | 0.000 | 0.000 | 26.9% (14/52) |
| phone | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | 380 | 81.6% (310/380) | 0.775 | 0.556 | 0.0% (0/311) |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: phone 1.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA / state breach laws (credentials) | 89 (27) | 37 | 78.4% (29/37) | 78.4% (29/37) | - | - | - | - | 2.7% (1/37) |
| HIPAA 15 | 89 (27) | 52 | 71.2% (37/52) | 69.2% (36/52) | - | - | - | - | 26.9% (14/52) |
| HIPAA 4, 5 | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | - | - | - | - | 0.0% (0/311) |
| HIPAA 6 | 89 (27) | 36 | 97.2% (35/36) | 97.2% (35/36) | - | - | - | - | 0.0% (0/36) |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 45 | n/a |
| bare_date | 710 | 0 | 0.000 |
| build_number | 20 | 0 | 0.000 |
| coordinates | 18 | 0 | 0.000 |
| dollar_amount | 710 | 0 | 0.000 |
| epoch_ms | 91 | 0 | 0.000 |
| file_size | 668 | 0 | 0.000 |
| git_hash | 57 | 56 | 0.982 |
| iban_fail | 12 | 1 | 0.083 |
| invoice_number | 172 | 0 | 0.000 |
| lone_code | 25 | 0 | 0.000 |
| lone_expiry | 25 | 0 | 0.000 |
| luhn_fail_16_digits | 29 | 23 | 0.793 |
| masked_account | 496 | 0 | 0.000 |
| masked_card | 7 | 0 | 0.000 |
| masked_ssn | 57 | 0 | 0.000 |
| npi_fail | 4 | 0 | 0.000 |
| order_number | 349 | 0 | 0.000 |
| part_number | 374 | 0 | 0.000 |
| placeholder_secret | 66 | 0 | 0.000 |
| plain_url | 14 | 0 | 0.000 |
| short_hash | 2 | 0 | 0.000 |
| table_number | 76 | 0 | 0.000 |
| ten_digit_id | 95 | 0 | 0.000 |
| timestamp | 170 | 0 | 0.000 |
| tracking_number | 43 | 11 | 0.256 |
| uuid | 47 | 47 | 1.000 |
| version_string | 91 | 0 | 0.000 |
| vin_fail | 3 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| held_out | 89 (27) | 436 | 94.5% (412/436) | 94.3% (411/436) | 597 | 69.2% (413/597) | 2.056 | 2.074 | 3.4% (15/436) |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| chat | 24 (6) | 65 | 83.1% (54/65) | 81.5% (53/65) | 94 | 58.5% (55/94) | 1.583 | 1.167 | 9.2% (6/65) |
| devtools | 22 (8) | 79 | 83.5% (66/79) | 83.5% (66/79) | 196 | 33.7% (66/196) | 5.909 | 5.625 | 11.4% (9/79) |
| hr_sheet | 22 (7) | 261 | 100.0% (261/261) | 100.0% (261/261) | 261 | 100.0% (261/261) | 0.000 | 0.000 | 0.0% (0/261) |
| insurance_card | 21 (6) | 31 | 100.0% (31/31) | 100.0% (31/31) | 46 | 67.4% (31/46) | 0.714 | 0.667 | 0.0% (0/31) |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 48 (14) | 225 | 95.1% (214/225) | 94.7% (213/225) | 309 | 68.9% (213/309) | 1.979 | 1.929 | 2.7% (6/225) |
| light | 41 (13) | 211 | 93.8% (198/211) | 93.8% (198/211) | 288 | 69.4% (200/288) | 2.146 | 2.231 | 4.3% (9/211) |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 45 (12) | 239 | 94.6% (226/239) | 94.1% (225/239) | 313 | 71.9% (225/313) | 1.933 | 2.167 | 2.9% (7/239) |
| 2x | 44 (15) | 197 | 94.4% (186/197) | 94.4% (186/197) | 284 | 66.2% (188/284) | 2.182 | 2.000 | 4.1% (8/197) |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 29 (10) | 181 | 91.2% (165/181) | 90.6% (164/181) | 237 | 69.2% (164/237) | 2.483 | 2.500 | 5.5% (10/181) |
| b: 11-12 px | 30 (12) | 125 | 96.0% (120/125) | 96.0% (120/125) | 170 | 71.2% (121/170) | 1.633 | 2.000 | 2.4% (3/125) |
| c: 13-14 px | 21 (4) | 85 | 97.6% (83/85) | 97.6% (83/85) | 129 | 65.1% (84/129) | 2.143 | 1.750 | 1.2% (1/85) |
| d: 15-18 px | 9 (1) | 45 | 97.8% (44/45) | 97.8% (44/45) | 61 | 72.1% (44/61) | 1.889 | 0.000 | 2.2% (1/45) |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 63 (20) | 330 | 96.1% (317/330) | 96.1% (317/330) | 460 | 69.3% (319/460) | 2.238 | 2.350 | 2.7% (9/330) |
| degraded | 26 (7) | 106 | 89.6% (95/106) | 88.7% (94/106) | 137 | 68.6% (94/137) | 1.615 | 1.286 | 5.7% (6/106) |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 87 (27) | 423 | 95.0% (402/423) | 95.0% (402/423) | 581 | 69.5% (404/581) | 2.034 | 2.074 | 3.3% (14/423) |
| vi | 2 (0) | 13 | 76.9% (10/13) | 69.2% (9/13) | 16 | 56.2% (9/16) | 3.000 | n/a | 7.7% (1/13) |

## Time per image

- Recognition: mean 166.3 ms, p95 268.3 ms (89 images)
- Detection: mean 0.1 ms, p95 0.1 ms (89 images)


---

# Run D: 0.2.5, dev, perfect text

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 211 (63) | 2275 | 99.2% (2257/2275) | 98.2% (2235/2275) | 2274 | 98.5% (2239/2274) | 0.057 | 0.095 | - |

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
| HIPAA 11 | 211 (63) | 50 | 100.0% (50/50) | 100.0% (50/50) | - | - | - | - | - |
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
| timestamp | 837 | 0 | 0.000 |
| tracking_number | 589 | 0 | 0.000 |
| uuid | 115 | 0 | 0.000 |
| version_string | 819 | 0 | 0.000 |
| vin_fail | 7 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 211 (63) | 2275 | 99.2% (2257/2275) | 98.2% (2235/2275) | 2274 | 98.5% (2239/2274) | 0.057 | 0.095 | - |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bank_statement | 26 (6) | 92 | 100.0% (92/92) | 100.0% (92/92) | 95 | 96.8% (92/95) | 0.115 | 0.333 | - |
| checkout | 22 (9) | 75 | 100.0% (75/75) | 100.0% (75/75) | 75 | 100.0% (75/75) | 0.000 | 0.000 | - |
| crm_table | 24 (8) | 754 | 98.7% (744/754) | 98.7% (744/754) | 744 | 100.0% (744/744) | 0.000 | 0.000 | - |
| env_file | 22 (7) | 292 | 99.7% (291/292) | 99.7% (291/292) | 292 | 100.0% (292/292) | 0.000 | 0.000 | - |
| log_viewer | 22 (6) | 296 | 100.0% (296/296) | 100.0% (296/296) | 296 | 100.0% (296/296) | 0.000 | 0.000 | - |
| mail_client | 25 (6) | 325 | 99.4% (323/325) | 98.5% (320/325) | 324 | 98.8% (320/324) | 0.040 | 0.000 | - |
| patient_intake | 25 (9) | 80 | 98.8% (79/80) | 98.8% (79/80) | 79 | 100.0% (79/79) | 0.000 | 0.000 | - |
| settings_page | 24 (6) | 198 | 99.5% (197/198) | 91.9% (182/198) | 206 | 88.3% (182/206) | 0.333 | 0.667 | - |
| terminal | 21 (6) | 163 | 98.2% (160/163) | 95.7% (156/163) | 163 | 97.5% (159/163) | 0.000 | 0.000 | - |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 122 (34) | 1424 | 98.9% (1409/1424) | 98.3% (1400/1424) | 1416 | 99.1% (1403/1416) | 0.025 | 0.029 | - |
| light | 89 (29) | 851 | 99.6% (848/851) | 98.1% (835/851) | 858 | 97.4% (836/858) | 0.101 | 0.172 | - |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 107 (32) | 1165 | 99.0% (1153/1165) | 97.9% (1141/1165) | 1162 | 98.2% (1141/1162) | 0.075 | 0.156 | - |
| 2x | 104 (31) | 1110 | 99.5% (1104/1110) | 98.6% (1094/1110) | 1112 | 98.7% (1098/1112) | 0.038 | 0.032 | - |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 47 (14) | 555 | 99.6% (553/555) | 98.7% (548/555) | 556 | 98.7% (549/556) | 0.043 | 0.071 | - |
| b: 11-12 px | 109 (29) | 1321 | 98.9% (1307/1321) | 98.1% (1296/1321) | 1316 | 98.7% (1299/1316) | 0.055 | 0.103 | - |
| c: 13-14 px | 37 (16) | 273 | 99.6% (272/273) | 98.2% (268/273) | 276 | 97.1% (268/276) | 0.081 | 0.125 | - |
| d: 15-18 px | 18 (4) | 126 | 99.2% (125/126) | 97.6% (123/126) | 126 | 97.6% (123/126) | 0.056 | 0.000 | - |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 153 (45) | 1542 | 99.2% (1529/1542) | 98.2% (1514/1542) | 1543 | 98.3% (1517/1543) | 0.072 | 0.111 | - |
| degraded | 58 (18) | 733 | 99.3% (728/733) | 98.4% (721/733) | 731 | 98.8% (722/731) | 0.017 | 0.056 | - |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 198 (63) | 2067 | 99.8% (2063/2067) | 98.8% (2043/2067) | 2079 | 98.5% (2047/2079) | 0.061 | 0.095 | - |
| vi | 13 (0) | 208 | 93.3% (194/208) | 92.3% (192/208) | 195 | 98.5% (192/195) | 0.000 | n/a | - |

## Time per image

- Recognition: n/a (detector-only)
- Detection: mean 0.2 ms, p95 0.3 ms (211 images)


---

# Run E: 0.2.5, held-out, perfect text

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 89 (27) | 1410 | 100.0% (1410/1410) | 100.0% (1410/1410) | 1471 | 95.9% (1410/1471) | 0.685 | 0.778 | - |

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 89 (27) | 36 | 100.0% (36/36) | 100.0% (36/36) | 36 | 100.0% (36/36) | 0.000 | 0.000 | - |
| token | 89 (27) | 37 | 100.0% (37/37) | 100.0% (37/37) | 96 | 38.5% (37/96) | 0.663 | 0.704 | - |
| private_key | 89 (27) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | - |
| secret_assignment | 89 (27) | 20 | 100.0% (20/20) | 100.0% (20/20) | 20 | 100.0% (20/20) | 0.000 | 0.000 | - |
| url_secret | 89 (27) | 0 | n/a | n/a | 0 | n/a | 0.000 | 0.000 | - |
| card | 89 (27) | 28 | 100.0% (28/28) | 100.0% (28/28) | 28 | 100.0% (28/28) | 0.000 | 0.000 | - |
| card_extra | 89 (27) | 42 | 100.0% (42/42) | 100.0% (42/42) | 42 | 100.0% (42/42) | 0.000 | 0.000 | - |
| ssn | 89 (27) | 150 | 100.0% (150/150) | 100.0% (150/150) | 150 | 100.0% (150/150) | 0.000 | 0.000 | - |
| itin | 89 (27) | 126 | 100.0% (126/126) | 100.0% (126/126) | 126 | 100.0% (126/126) | 0.000 | 0.000 | - |
| ein | 89 (27) | 256 | 100.0% (256/256) | 100.0% (256/256) | 256 | 100.0% (256/256) | 0.000 | 0.000 | - |
| aba | 89 (27) | 266 | 100.0% (266/266) | 100.0% (266/266) | 268 | 99.3% (266/268) | 0.022 | 0.074 | - |
| iban | 89 (27) | 21 | 100.0% (21/21) | 100.0% (21/21) | 21 | 100.0% (21/21) | 0.000 | 0.000 | - |
| dea | 89 (27) | 10 | 100.0% (10/10) | 100.0% (10/10) | 10 | 100.0% (10/10) | 0.000 | 0.000 | - |
| mbi | 89 (27) | 16 | 100.0% (16/16) | 100.0% (16/16) | 16 | 100.0% (16/16) | 0.000 | 0.000 | - |
| passport | 89 (27) | 22 | 100.0% (22/22) | 100.0% (22/22) | 22 | 100.0% (22/22) | 0.000 | 0.000 | - |
| vin | 89 (27) | 10 | 100.0% (10/10) | 100.0% (10/10) | 10 | 100.0% (10/10) | 0.000 | 0.000 | - |
| imei | 89 (27) | 4 | 100.0% (4/4) | 100.0% (4/4) | 4 | 100.0% (4/4) | 0.000 | 0.000 | - |
| ip_address | 89 (27) | 52 | 100.0% (52/52) | 100.0% (52/52) | 52 | 100.0% (52/52) | 0.000 | 0.000 | - |
| mac_address | 89 (27) | 3 | 100.0% (3/3) | 100.0% (3/3) | 3 | 100.0% (3/3) | 0.000 | 0.000 | - |
| phone | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | 311 | 100.0% (311/311) | 0.000 | 0.000 | - |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: none.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA | 89 (27) | 613 | 100.0% (613/613) | 100.0% (613/613) | - | - | - | - | - |
| GLBA / state breach laws (credentials) | 89 (27) | 57 | 100.0% (57/57) | 100.0% (57/57) | - | - | - | - | - |
| HIPAA 10 | 89 (27) | 315 | 100.0% (315/315) | 100.0% (315/315) | - | - | - | - | - |
| HIPAA 11 | 89 (27) | 32 | 100.0% (32/32) | 100.0% (32/32) | - | - | - | - | - |
| HIPAA 12 | 89 (27) | 10 | 100.0% (10/10) | 100.0% (10/10) | - | - | - | - | - |
| HIPAA 13 | 89 (27) | 7 | 100.0% (7/7) | 100.0% (7/7) | - | - | - | - | - |
| HIPAA 15 | 89 (27) | 52 | 100.0% (52/52) | 100.0% (52/52) | - | - | - | - | - |
| HIPAA 4, 5 | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | - | - | - | - | - |
| HIPAA 6 | 89 (27) | 36 | 100.0% (36/36) | 100.0% (36/36) | - | - | - | - | - |
| HIPAA 7 | 89 (27) | 276 | 100.0% (276/276) | 100.0% (276/276) | - | - | - | - | - |
| HIPAA 9 | 89 (27) | 16 | 100.0% (16/16) | 100.0% (16/16) | - | - | - | - | - |
| NIST SP 800-122 | 89 (27) | 532 | 100.0% (532/532) | 100.0% (532/532) | - | - | - | - | - |
| PCI DSS | 89 (27) | 28 | 100.0% (28/28) | 100.0% (28/28) | - | - | - | - | - |
| PCI DSS (sensitive authentication data) | 89 (27) | 42 | 100.0% (42/42) | 100.0% (42/42) | - | - | - | - | - |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| bare_date | 710 | 0 | 0.000 |
| build_number | 20 | 0 | 0.000 |
| coordinates | 18 | 0 | 0.000 |
| dollar_amount | 710 | 0 | 0.000 |
| epoch_ms | 91 | 0 | 0.000 |
| file_size | 668 | 0 | 0.000 |
| git_hash | 57 | 57 | 1.000 |
| iban_fail | 12 | 2 | 0.167 |
| invoice_number | 172 | 0 | 0.000 |
| lone_code | 25 | 0 | 0.000 |
| lone_expiry | 25 | 0 | 0.000 |
| luhn_fail_16_digits | 29 | 0 | 0.000 |
| masked_account | 496 | 0 | 0.000 |
| masked_card | 7 | 0 | 0.000 |
| masked_ssn | 57 | 0 | 0.000 |
| npi_fail | 4 | 0 | 0.000 |
| order_number | 349 | 0 | 0.000 |
| part_number | 374 | 0 | 0.000 |
| placeholder_secret | 66 | 0 | 0.000 |
| plain_url | 14 | 0 | 0.000 |
| short_hash | 2 | 0 | 0.000 |
| table_number | 76 | 2 | 0.026 |
| ten_digit_id | 95 | 0 | 0.000 |
| timestamp | 170 | 0 | 0.000 |
| tracking_number | 43 | 0 | 0.000 |
| uuid | 47 | 0 | 0.000 |
| version_string | 91 | 0 | 0.000 |
| vin_fail | 3 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| held_out | 89 (27) | 1410 | 100.0% (1410/1410) | 100.0% (1410/1410) | 1471 | 95.9% (1410/1471) | 0.685 | 0.778 | - |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| chat | 24 (6) | 172 | 100.0% (172/172) | 100.0% (172/172) | 172 | 100.0% (172/172) | 0.000 | 0.000 | - |
| devtools | 22 (8) | 132 | 100.0% (132/132) | 100.0% (132/132) | 191 | 69.1% (132/191) | 2.682 | 2.375 | - |
| hr_sheet | 22 (7) | 1004 | 100.0% (1004/1004) | 100.0% (1004/1004) | 1006 | 99.8% (1004/1006) | 0.091 | 0.286 | - |
| insurance_card | 21 (6) | 102 | 100.0% (102/102) | 100.0% (102/102) | 102 | 100.0% (102/102) | 0.000 | 0.000 | - |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 48 (14) | 730 | 100.0% (730/730) | 100.0% (730/730) | 760 | 96.1% (730/760) | 0.625 | 0.786 | - |
| light | 41 (13) | 680 | 100.0% (680/680) | 100.0% (680/680) | 711 | 95.6% (680/711) | 0.756 | 0.769 | - |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 45 (12) | 796 | 100.0% (796/796) | 100.0% (796/796) | 827 | 96.3% (796/827) | 0.689 | 0.917 | - |
| 2x | 44 (15) | 614 | 100.0% (614/614) | 100.0% (614/614) | 644 | 95.3% (614/644) | 0.682 | 0.667 | - |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 29 (10) | 607 | 100.0% (607/607) | 100.0% (607/607) | 632 | 96.0% (607/632) | 0.862 | 1.100 | - |
| b: 11-12 px | 30 (12) | 393 | 100.0% (393/393) | 100.0% (393/393) | 410 | 95.9% (393/410) | 0.567 | 0.833 | - |
| c: 13-14 px | 21 (4) | 266 | 100.0% (266/266) | 100.0% (266/266) | 279 | 95.3% (266/279) | 0.619 | 0.000 | - |
| d: 15-18 px | 9 (1) | 144 | 100.0% (144/144) | 100.0% (144/144) | 150 | 96.0% (144/150) | 0.667 | 0.000 | - |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 63 (20) | 1064 | 100.0% (1064/1064) | 100.0% (1064/1064) | 1112 | 95.7% (1064/1112) | 0.762 | 1.000 | - |
| degraded | 26 (7) | 346 | 100.0% (346/346) | 100.0% (346/346) | 359 | 96.4% (346/359) | 0.500 | 0.143 | - |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 87 (27) | 1387 | 100.0% (1387/1387) | 100.0% (1387/1387) | 1448 | 95.8% (1387/1448) | 0.701 | 0.778 | - |
| vi | 2 (0) | 23 | 100.0% (23/23) | 100.0% (23/23) | 23 | 100.0% (23/23) | 0.000 | n/a | - |

## Time per image

- Recognition: n/a (detector-only)
- Detection: mean 0.2 ms, p95 0.3 ms (89 images)


---

# Run F: 0.2.3, held-out, perfect text

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 89 (27) | 436 | 96.3% (420/436) | 96.3% (420/436) | 603 | 69.7% (420/603) | 2.056 | 2.111 | - |

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 89 (27) | 36 | 100.0% (36/36) | 100.0% (36/36) | 36 | 100.0% (36/36) | 0.000 | 0.000 | - |
| token | 89 (27) | 37 | 97.3% (36/37) | 97.3% (36/37) | 150 | 24.0% (36/150) | 1.281 | 1.556 | - |
| ip_address | 89 (27) | 52 | 71.2% (37/52) | 71.2% (37/52) | 37 | 100.0% (37/37) | 0.000 | 0.000 | - |
| phone | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | 380 | 81.8% (311/380) | 0.775 | 0.556 | - |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: none.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA / state breach laws (credentials) | 89 (27) | 37 | 97.3% (36/37) | 97.3% (36/37) | - | - | - | - | - |
| HIPAA 15 | 89 (27) | 52 | 71.2% (37/52) | 71.2% (37/52) | - | - | - | - | - |
| HIPAA 4, 5 | 89 (27) | 311 | 100.0% (311/311) | 100.0% (311/311) | - | - | - | - | - |
| HIPAA 6 | 89 (27) | 36 | 100.0% (36/36) | 100.0% (36/36) | - | - | - | - | - |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 43 | n/a |
| bare_date | 710 | 0 | 0.000 |
| build_number | 20 | 0 | 0.000 |
| coordinates | 18 | 0 | 0.000 |
| dollar_amount | 710 | 0 | 0.000 |
| epoch_ms | 91 | 0 | 0.000 |
| file_size | 668 | 0 | 0.000 |
| git_hash | 57 | 57 | 1.000 |
| iban_fail | 12 | 2 | 0.167 |
| invoice_number | 172 | 0 | 0.000 |
| lone_code | 25 | 0 | 0.000 |
| lone_expiry | 25 | 0 | 0.000 |
| luhn_fail_16_digits | 29 | 23 | 0.793 |
| masked_account | 496 | 0 | 0.000 |
| masked_card | 7 | 0 | 0.000 |
| masked_ssn | 57 | 0 | 0.000 |
| npi_fail | 4 | 0 | 0.000 |
| order_number | 349 | 0 | 0.000 |
| part_number | 374 | 0 | 0.000 |
| placeholder_secret | 66 | 0 | 0.000 |
| plain_url | 14 | 0 | 0.000 |
| short_hash | 2 | 0 | 0.000 |
| table_number | 76 | 0 | 0.000 |
| ten_digit_id | 95 | 0 | 0.000 |
| timestamp | 170 | 0 | 0.000 |
| tracking_number | 43 | 11 | 0.256 |
| uuid | 47 | 47 | 1.000 |
| version_string | 91 | 0 | 0.000 |
| vin_fail | 3 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| held_out | 89 (27) | 436 | 96.3% (420/436) | 96.3% (420/436) | 603 | 69.7% (420/603) | 2.056 | 2.111 | - |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| chat | 24 (6) | 65 | 89.2% (58/65) | 89.2% (58/65) | 95 | 61.1% (58/95) | 1.542 | 1.167 | - |
| devtools | 22 (8) | 79 | 88.6% (70/79) | 88.6% (70/79) | 201 | 34.8% (70/201) | 5.955 | 5.750 | - |
| hr_sheet | 22 (7) | 261 | 100.0% (261/261) | 100.0% (261/261) | 261 | 100.0% (261/261) | 0.000 | 0.000 | - |
| insurance_card | 21 (6) | 31 | 100.0% (31/31) | 100.0% (31/31) | 46 | 67.4% (31/46) | 0.714 | 0.667 | - |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 48 (14) | 225 | 96.9% (218/225) | 96.9% (218/225) | 312 | 69.9% (218/312) | 1.958 | 1.929 | - |
| light | 41 (13) | 211 | 95.7% (202/211) | 95.7% (202/211) | 291 | 69.4% (202/291) | 2.171 | 2.308 | - |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 45 (12) | 239 | 96.7% (231/239) | 96.7% (231/239) | 317 | 72.9% (231/317) | 1.911 | 2.167 | - |
| 2x | 44 (15) | 197 | 95.9% (189/197) | 95.9% (189/197) | 286 | 66.1% (189/286) | 2.205 | 2.067 | - |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 29 (10) | 181 | 93.9% (170/181) | 93.9% (170/181) | 241 | 70.5% (170/241) | 2.448 | 2.500 | - |
| b: 11-12 px | 30 (12) | 125 | 97.6% (122/125) | 97.6% (122/125) | 172 | 70.9% (122/172) | 1.667 | 2.083 | - |
| c: 13-14 px | 21 (4) | 85 | 98.8% (84/85) | 98.8% (84/85) | 129 | 65.1% (84/129) | 2.143 | 1.750 | - |
| d: 15-18 px | 9 (1) | 45 | 97.8% (44/45) | 97.8% (44/45) | 61 | 72.1% (44/61) | 1.889 | 0.000 | - |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 63 (20) | 330 | 97.3% (321/330) | 97.3% (321/330) | 463 | 69.3% (321/463) | 2.254 | 2.400 | - |
| degraded | 26 (7) | 106 | 93.4% (99/106) | 93.4% (99/106) | 140 | 70.7% (99/140) | 1.577 | 1.286 | - |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 87 (27) | 423 | 96.7% (409/423) | 96.7% (409/423) | 587 | 69.7% (409/587) | 2.046 | 2.111 | - |
| vi | 2 (0) | 13 | 84.6% (11/13) | 84.6% (11/13) | 16 | 68.8% (11/16) | 2.500 | n/a | - |

## Time per image

- Recognition: n/a (detector-only)
- Detection: mean 0.1 ms, p95 0.2 ms (89 images)

