# Auto-redact benchmark: baseline of the 0.2.3 detector

- **Date:** 2026-10-30
- **Who:** Claude (run for the maintainer)
- **Build:** the 0.2.3 `detect.rs` (4 kinds: email, token, ip_address, phone) in a scratch copy of commit 2d17724, release build, with the benchmark harness of this release
- **Machine:** Apple M4 Pro, macOS 26.6.2; recognizer Apple Vision (`apple_vision`, accurate level) where noted
- **Set:** synthetic, `redaction_bench gen --seed 20261030` (300 images, 13 layouts, 4 held out). **Only the `dev` split (211 images) was read; the `held_out` split has not been read.**
- **Wording:** these numbers are measured on synthetic data made by this project, with perfect knowledge of the formats the rules look for. They are an upper bound for real screenshots, not a measure of real-world accuracy or compliance. Nothing here is a verdict that a screenshot is free of personal data.

Commands (the scratch copy replaces `crates/cm-render/src/ocr/detect.rs` with the file from `f819484` and removes `ocr/rules/`):

```
redaction_bench gen --out <dir>                                   # seed 20261030
redaction_bench run --set <dir> --detector-only --split dev       # perfect text (rule errors only)
redaction_bench run --set <dir> --recognizer system --split dev   # Apple Vision, with the leak pass
```

**Reading it.** The old detector knows 4 of the 21 kinds, so its tables score only the truths of those 4 kinds
(1,367 of 2,288 truths in the dev split). The other 921 truths (cards, SSNs, bank and health numbers, key
blocks, secret assignments, URLs with secrets, MAC addresses, and so on) are never proposed, so they are
misses by construction; the new report scores all 2,288.

## Run 1: Apple Vision (end to end, with the leak pass)

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 211 (63) | 1367 | 92.2% (1261/1367) | 91.9% (1256/1367) | 1858 | 68.8% (1278/1858) | 2.725 | 2.000 | 4.2% (58/1367) |

Readable before blurring: 89.3% (1221/1367); of those still readable after: 4.5% (55/1221).

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 211 (63) | 426 | 97.4% (415/426) | 97.4% (415/426) | 449 | 92.7% (416/449) | 0.156 | 0.000 | 1.2% (5/426) |
| token | 211 (63) | 161 | 79.5% (128/161) | 79.5% (128/161) | 462 | 30.5% (141/462) | 1.521 | 1.048 | 3.1% (5/161) |
| ip_address | 211 (63) | 333 | 88.6% (295/333) | 87.7% (292/333) | 292 | 100.0% (292/292) | 0.000 | 0.000 | 10.2% (34/333) |
| phone | 211 (63) | 447 | 94.6% (423/447) | 94.2% (421/447) | 655 | 65.5% (429/655) | 1.047 | 0.952 | 3.1% (14/447) |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: phone 5.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA / state breach laws (credentials) | 211 (63) | 161 | 79.5% (128/161) | 79.5% (128/161) | - | - | - | - | 3.1% (5/161) |
| HIPAA 15 | 211 (63) | 333 | 88.6% (295/333) | 87.7% (292/333) | - | - | - | - | 10.2% (34/333) |
| HIPAA 4, 5 | 211 (63) | 447 | 94.6% (423/447) | 94.2% (421/447) | - | - | - | - | 3.1% (14/447) |
| HIPAA 6 | 211 (63) | 426 | 97.4% (415/426) | 97.4% (415/426) | - | - | - | - | 1.2% (5/426) |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 248 | n/a |
| bare_date | 1155 | 3 | 0.003 |
| build_number | 35 | 0 | 0.000 |
| coordinates | 157 | 0 | 0.000 |
| dollar_amount | 1836 | 10 | 0.005 |
| epoch_ms | 145 | 9 | 0.062 |
| git_hash | 6 | 6 | 1.000 |
| iban_fail | 17 | 1 | 0.059 |
| imei_fail | 3 | 0 | 0.000 |
| invoice_number | 141 | 0 | 0.000 |
| lone_code | 62 | 0 | 0.000 |
| lone_expiry | 62 | 0 | 0.000 |
| luhn_fail_16_digits | 62 | 47 | 0.758 |
| masked_account | 26 | 0 | 0.000 |
| masked_card | 17 | 1 | 0.059 |
| masked_ssn | 7 | 0 | 0.000 |
| npi_fail | 3 | 0 | 0.000 |
| order_number | 818 | 1 | 0.001 |
| part_number | 195 | 0 | 0.000 |
| placeholder_secret | 402 | 0 | 0.000 |
| plain_url | 250 | 3 | 0.012 |
| short_hash | 236 | 0 | 0.000 |
| table_number | 13 | 0 | 0.000 |
| ten_digit_id | 137 | 11 | 0.080 |
| timestamp | 837 | 12 | 0.014 |
| tracking_number | 589 | 108 | 0.183 |
| uuid | 115 | 115 | 1.000 |
| version_string | 819 | 0 | 0.000 |
| vin_fail | 7 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 211 (63) | 1367 | 92.2% (1261/1367) | 91.9% (1256/1367) | 1858 | 68.8% (1278/1858) | 2.725 | 2.000 | 4.2% (58/1367) |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bank_statement | 26 (6) | 32 | 100.0% (32/32) | 100.0% (32/32) | 87 | 36.8% (32/87) | 2.115 | 1.833 | 0.0% (0/32) |
| checkout | 22 (9) | 21 | 100.0% (21/21) | 100.0% (21/21) | 52 | 40.4% (21/52) | 1.409 | 1.556 | 0.0% (0/21) |
| crm_table | 24 (8) | 754 | 94.8% (715/754) | 94.3% (711/754) | 801 | 89.8% (719/801) | 3.250 | 5.250 | 3.1% (23/754) |
| env_file | 22 (7) | 34 | 91.2% (31/34) | 91.2% (31/34) | 159 | 20.1% (32/159) | 5.773 | 0.000 | 0.0% (0/34) |
| log_viewer | 22 (6) | 237 | 89.9% (213/237) | 89.9% (213/237) | 377 | 57.8% (218/377) | 7.227 | 5.167 | 5.1% (12/237) |
| mail_client | 25 (6) | 131 | 83.2% (109/131) | 82.4% (108/131) | 172 | 66.9% (115/172) | 2.240 | 1.333 | 7.6% (10/131) |
| patient_intake | 25 (9) | 33 | 97.0% (32/33) | 97.0% (32/33) | 32 | 100.0% (32/32) | 0.000 | 0.000 | 3.0% (1/33) |
| settings_page | 24 (6) | 66 | 92.4% (61/66) | 92.4% (61/66) | 88 | 69.3% (61/88) | 1.125 | 2.333 | 6.1% (4/66) |
| terminal | 21 (6) | 59 | 79.7% (47/59) | 79.7% (47/59) | 90 | 53.3% (48/90) | 2.000 | 1.000 | 13.6% (8/59) |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 122 (34) | 905 | 92.9% (841/905) | 92.5% (837/905) | 1179 | 71.8% (847/1179) | 2.689 | 1.971 | 4.6% (42/905) |
| light | 89 (29) | 462 | 90.9% (420/462) | 90.7% (419/462) | 679 | 63.5% (431/679) | 2.775 | 2.034 | 3.5% (16/462) |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 107 (32) | 722 | 92.8% (670/722) | 92.2% (666/722) | 944 | 71.2% (672/944) | 2.514 | 2.188 | 4.7% (34/722) |
| 2x | 104 (31) | 645 | 91.6% (591/645) | 91.5% (590/645) | 914 | 66.3% (606/914) | 2.942 | 1.806 | 3.7% (24/645) |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 47 (14) | 292 | 89.4% (261/292) | 89.0% (260/292) | 408 | 64.5% (263/408) | 3.064 | 1.786 | 6.2% (18/292) |
| b: 11-12 px | 109 (29) | 893 | 92.9% (830/893) | 92.6% (827/893) | 1157 | 72.8% (842/1157) | 2.853 | 2.414 | 3.8% (34/893) |
| c: 13-14 px | 37 (16) | 134 | 95.5% (128/134) | 94.8% (127/134) | 226 | 57.1% (129/226) | 2.622 | 1.688 | 1.5% (2/134) |
| d: 15-18 px | 18 (4) | 48 | 87.5% (42/48) | 87.5% (42/48) | 67 | 65.7% (44/67) | 1.278 | 1.000 | 8.3% (4/48) |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 153 (45) | 889 | 92.2% (820/889) | 92.2% (820/889) | 1268 | 66.1% (838/1268) | 2.804 | 1.867 | 4.2% (37/889) |
| degraded | 58 (18) | 478 | 92.3% (441/478) | 91.2% (436/478) | 590 | 74.6% (440/590) | 2.517 | 2.333 | 4.4% (21/478) |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 198 (63) | 1221 | 92.9% (1134/1221) | 92.5% (1129/1221) | 1720 | 66.8% (1149/1720) | 2.859 | 2.000 | 3.6% (44/1221) |
| vi | 13 (0) | 146 | 87.0% (127/146) | 87.0% (127/146) | 138 | 93.5% (129/138) | 0.692 | n/a | 9.6% (14/146) |

## Time per image

- Recognition: mean 155.7 ms, p95 248.2 ms (211 images)
- Detection: mean 0.1 ms, p95 0.1 ms (211 images)


---

## Run 2: perfect text (detector only)

## Overall

| scope | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| all | 211 (63) | 1367 | 96.2% (1315/1367) | 96.2% (1315/1367) | 1890 | 69.6% (1315/1890) | 2.725 | 1.698 | - |

## Per kind

| kind | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| email | 211 (63) | 426 | 100.0% (426/426) | 100.0% (426/426) | 461 | 92.4% (426/461) | 0.166 | 0.000 | - |
| token | 211 (63) | 161 | 93.8% (151/161) | 93.8% (151/161) | 447 | 33.8% (151/447) | 1.403 | 0.635 | - |
| ip_address | 211 (63) | 333 | 91.3% (304/333) | 91.3% (304/333) | 304 | 100.0% (304/304) | 0.000 | 0.000 | - |
| phone | 211 (63) | 447 | 97.1% (434/447) | 97.1% (434/447) | 678 | 64.0% (434/678) | 1.156 | 1.063 | - |

Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: none.

## Per standard (recall and leak)

| standard | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| GLBA / state breach laws (credentials) | 211 (63) | 161 | 93.8% (151/161) | 93.8% (151/161) | - | - | - | - | - |
| HIPAA 15 | 211 (63) | 333 | 91.3% (304/333) | 91.3% (304/333) | - | - | - | - | - |
| HIPAA 4, 5 | 211 (63) | 447 | 97.1% (434/447) | 97.1% (434/447) | - | - | - | - | - |
| HIPAA 6 | 211 (63) | 426 | 100.0% (426/426) | 100.0% (426/426) | - | - | - | - | - |

## False alarms by lookalike

| decoy `what` | decoys in set | false alarms | hit rate |
|---|---:|---:|---:|
| (no decoy) | 0 | 262 | n/a |
| bare_date | 1155 | 2 | 0.002 |
| build_number | 35 | 0 | 0.000 |
| coordinates | 157 | 0 | 0.000 |
| dollar_amount | 1836 | 67 | 0.036 |
| epoch_ms | 145 | 0 | 0.000 |
| git_hash | 6 | 6 | 1.000 |
| iban_fail | 17 | 2 | 0.118 |
| imei_fail | 3 | 0 | 0.000 |
| invoice_number | 141 | 0 | 0.000 |
| lone_code | 62 | 0 | 0.000 |
| lone_expiry | 62 | 0 | 0.000 |
| luhn_fail_16_digits | 62 | 47 | 0.758 |
| masked_account | 26 | 0 | 0.000 |
| masked_card | 17 | 0 | 0.000 |
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
| tracking_number | 589 | 74 | 0.126 |
| uuid | 115 | 115 | 1.000 |
| version_string | 819 | 0 | 0.000 |
| vin_fail | 7 | 0 | 0.000 |

## By split

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 211 (63) | 1367 | 96.2% (1315/1367) | 96.2% (1315/1367) | 1890 | 69.6% (1315/1890) | 2.725 | 1.698 | - |

## By layout

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bank_statement | 26 (6) | 32 | 100.0% (32/32) | 100.0% (32/32) | 88 | 36.4% (32/88) | 2.154 | 2.000 | - |
| checkout | 22 (9) | 21 | 100.0% (21/21) | 100.0% (21/21) | 52 | 40.4% (21/52) | 1.409 | 1.556 | - |
| crm_table | 24 (8) | 754 | 98.5% (743/754) | 98.5% (743/754) | 810 | 91.7% (743/810) | 2.792 | 3.125 | - |
| env_file | 22 (7) | 34 | 91.2% (31/34) | 91.2% (31/34) | 165 | 18.8% (31/165) | 6.091 | 0.000 | - |
| log_viewer | 22 (6) | 237 | 94.1% (223/237) | 94.1% (223/237) | 384 | 58.1% (223/384) | 7.318 | 5.000 | - |
| mail_client | 25 (6) | 131 | 93.1% (122/131) | 93.1% (122/131) | 177 | 68.9% (122/177) | 2.200 | 1.167 | - |
| patient_intake | 25 (9) | 33 | 97.0% (32/33) | 97.0% (32/33) | 32 | 100.0% (32/32) | 0.000 | 0.000 | - |
| settings_page | 24 (6) | 66 | 92.4% (61/66) | 92.4% (61/66) | 87 | 70.1% (61/87) | 1.083 | 2.167 | - |
| terminal | 21 (6) | 59 | 84.7% (50/59) | 84.7% (50/59) | 95 | 52.6% (50/95) | 2.143 | 1.000 | - |

## By theme

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dark | 122 (34) | 905 | 96.4% (872/905) | 96.4% (872/905) | 1201 | 72.6% (872/1201) | 2.697 | 1.647 | - |
| light | 89 (29) | 462 | 95.9% (443/462) | 95.9% (443/462) | 689 | 64.3% (443/689) | 2.764 | 1.759 | - |

## By scale

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1x | 107 (32) | 722 | 96.5% (697/722) | 96.5% (697/722) | 956 | 72.9% (697/956) | 2.421 | 1.625 | - |
| 2x | 104 (31) | 645 | 95.8% (618/645) | 95.8% (618/645) | 934 | 66.2% (618/934) | 3.038 | 1.774 | - |

## By font size (smallest text, logical px)

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| a: <= 10 px | 47 (14) | 292 | 95.5% (279/292) | 95.5% (279/292) | 428 | 65.2% (279/428) | 3.170 | 1.714 | - |
| b: 11-12 px | 109 (29) | 893 | 96.4% (861/893) | 96.4% (861/893) | 1174 | 73.3% (861/1174) | 2.872 | 2.000 | - |
| c: 13-14 px | 37 (16) | 134 | 97.8% (131/134) | 97.8% (131/134) | 220 | 59.5% (131/220) | 2.405 | 1.312 | - |
| d: 15-18 px | 18 (4) | 48 | 91.7% (44/48) | 91.7% (44/48) | 68 | 64.7% (44/68) | 1.333 | 1.000 | - |

## By degraded

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| clean | 153 (45) | 889 | 95.3% (847/889) | 95.3% (847/889) | 1279 | 66.2% (847/1279) | 2.824 | 1.689 | - |
| degraded | 58 (18) | 478 | 97.9% (468/478) | 97.9% (468/478) | 611 | 76.6% (468/611) | 2.466 | 1.722 | - |

## By language

| bucket | images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en | 198 (63) | 1221 | 96.9% (1183/1221) | 96.9% (1183/1221) | 1745 | 67.8% (1183/1745) | 2.838 | 1.698 | - |
| vi | 13 (0) | 146 | 90.4% (132/146) | 90.4% (132/146) | 145 | 91.0% (132/145) | 1.000 | n/a | - |

## Time per image

- Recognition: n/a (detector-only)
- Detection: mean 0.0 ms, p95 0.1 ms (211 images)

