# RED-1 manual script: Auto-redact on real app screenshots (counts only)

The benchmark ([`verification/benchmarks/redaction-0.2.5-dev-2026-10-30.md`](../../verification/benchmarks/redaction-0.2.5-dev-2026-10-30.md)) is measured on synthetic screenshots made by this project, so it can flatter the detector. This script is the cross-check on **real** screenshots of real apps. It records **numbers only**. The screenshots are never stored, committed, attached to an issue or sent anywhere.

Needs: a Mac with the Apple Vision recognizer (macOS 13 or later), the app built from the commit under test (`make bundle-dev`), and 20 or more real screenshots of apps you actually use (a mail client, a bank or billing page, a CRM, a terminal or `.env` file, a settings page, a chat, a log viewer; screenshots from your own work, in the sizes and themes you use).

## Privacy notes

- Auto-redact runs on this Mac and sends nothing anywhere. Do not run any other tool on these screenshots.
- **Do not copy a screenshot, a crop of one, or any text read from one into the report, a commit, an issue or a chat.** The report holds counts and the names of kinds, nothing else. If a miss is worth reporting, describe its shape in general terms (for example "a card number printed in two columns") or make a synthetic reproduction with made-up data; that is what the issue template asks for.
- Delete the screenshots (and their library bundles) when you finish, or keep them in a library that is not backed up or shared.
- Use a separate library so your real one is untouched: `CAPTUREMANN_LIBRARY=<new folder> make run-bundle-dev`.

## Steps

1. **Pick the set.** Choose at least 20 screenshots. At least 5 should have **nothing sensitive** in them (they measure false alarms) and at least 10 should hold sensitive items of the kinds in the table in [`docs/user/redaction.md`](../../docs/user/redaction.md) (emails, tokens, cards, SSNs, routing or IBAN numbers, phone numbers, IP addresses, key blocks, secrets in URLs or `NAME=value` lines). Write down only the count of images.
2. **Open each in the editor and press Auto-redact.** Do not accept anything. For each image count, by looking at the screen:
   - **real**: sensitive items of the kinds Auto-redact claims to find that are visible and readable to you;
   - **found**: real items covered by a proposal (the dashed box covers the whole value);
   - **missed**: real items with no proposal or a box that leaves part of the value readable;
   - **false alarms**: proposals over something that is not sensitive.
   Count items of kinds that are off by default (`npi`) or not in the tool (names, addresses, dates) separately as "out of scope", and do not count them as missed.
3. **Check the blur.** Press Accept all on three images with several proposals and look at the result at 100 % and 200 %: is any proposed value still readable? Write down how many of the accepted items were still readable (the leak count). Undo with one step each.
4. **Check the masked preview.** In the proposal list, no row shows more than the first and last two characters of a value. Count rows that show more (should be 0).
5. **Add up.** Sum real, found, missed, false alarms and still-readable over all images, and the number of images with nothing sensitive and how many of them got any proposal.
6. **Delete the screenshots** and the test library.

## Where to write the numbers

Create `verification/RED-1-<date>-<who>.md` from the template in [`verification/README.md`](../../verification/README.md). Under "Steps" put the totals as a table with these rows only: images (and how many had nothing sensitive), real items, found, missed, false alarms, images with nothing sensitive that got a proposal, accepted items still readable, preview rows showing more than four characters, out-of-scope items seen. Add the per-kind counts of misses and false alarms if you can without naming content. Add one line: "No screenshots were stored." Then compare with the synthetic numbers in the 0.2.5 report and write the difference in words; do not average the two.

## Pass criteria

The run **passes** as a record (it can be cited as "verified on real screenshots, counts in the report") when all of these hold:

- Every step was done, no screenshot or text from one is in the repository, and the report lists the totals above.
- The preview never shows more than four characters of a value (step 4 is 0).
- Auto-redact applies nothing until you accept (steps 2 and 3).

Findings, not failures, to write up when they occur (they decide what to fix, not whether the run counts):

- found divided by real is clearly below the synthetic figure for the kind;
- images with nothing sensitive that got proposals exceed one in five (the false-alarm gate in the plan is one false alarm per five negative screenshots);
- any accepted item still readable after the blur.

Nothing in this script is a compliance test, and a pass does not mean a screenshot is free of personal data.
