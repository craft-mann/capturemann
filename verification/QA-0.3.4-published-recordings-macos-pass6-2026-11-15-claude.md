# QA 0.3.4 Published recordings (PUB-2, S22) and B1, macOS pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 6). Final dev bundle of main `0bcac6a3`, isolated library `/tmp/cmq6/lib`.
- **Fixture (no network):** five synthetic 4 s recordings from `make_fixture`; manifest hand-edited with `publishes[]` records: "Pub processed" (`processed`, public, no warning), "Pub warned" (`privacy_locked`, requested public, warning "YouTube kept this video private."), "Pub waiting" (`processed`, unlisted) plus a paused upload job with `pause_reason auth_expired` in `jobs.sqlite`; "Delete me B1" (no publish record, same kind of paused job); "Plain rec". Pictures: `QA-0.3.4-shots/macos-pass6-published-*.png` (fixtures only).

| Case | Result | Evidence |
| --- | --- | --- |
| Recordings chips | PASS (accessibility text) | Imported 0, Uploaded 0, **Published 3**, **Needs attention 2** (warned + waiting); the Boards chip is not on the Recordings page |
| Published chip view | PASS (seen) | 3 cards, sorted "Date published"; meta line "Published Yesterday 17:00 · 205 KB"; status lines "Upload waiting for sign-in" (amber, warning icon), "YouTube kept it private" (amber), "Live on YouTube" (green dot). Heading "All captures  3 of 15 · published" |
| Needs attention chip | PASS | the 2 cards; the Published chip stays ringed at the same time, so two chips look selected (UX note) |
| Card pill | PARTIAL | on the Recordings page the thumbnail pill appears only for "Upload waiting for sign-in"; published recordings have no "Published" pill there (the S22 pill text is in the Published view's meta line). Confirm against the design |
| Card menu (published) | PASS (seen) | Open, Open on YouTube, Copy YouTube link, Reveal in Finder, Rename…, Delete… |
| Card menu (waiting upload) | PASS (seen) | amber note "The upload is paused, not lost. Delete and rename still work; deleting cancels the paused upload." then Reconnect, Cancel upload, then the usual items |
| Details: header pill | PASS | "Published · YouTube" next to the folder, Export…, Publish…, Edit |
| Details: Published card | PASS (seen) | YouTube, account "QA channel · Yesterday 17:00", privacy pill "Public", link row (copy, open), "Live on YouTube", text "Publish… in the header sends it to another platform. Visibility is changed on the platform."; also a "Published 1 · YouTube" stat tile |
| Details of the warned record | PASS with two notes | pill "Private (you asked for Public)" in amber; the warning appears **twice** (amber line "YouTube kept it private" and a boxed "YouTube kept this video private."); the account label is cut ("QA channel · Y…") by the long pill. The header pill is the same "Published · YouTube" for a warned record |
| Sidebar | PASS | Recordings 4/5, Audio, Screenshots, Click Guides; **no Published item** |
| **B1: delete with a paused upload** | PASS | menu note read; Delete… opens "Delete this capture? It is removed from your library and moved to the Trash." [Cancel, Delete capture]; after confirming the card is gone, the job is `cancelled` in `jobs.sqlite`, the other job is untouched. The capture folder went to the user's Trash (`~/.Trash/Delete me B1.cmrec`, my fixture; left there, I was told to remove only `/tmp/cmq6`). The dialog does not mention the paused upload (the menu does) |
| Reconnect, Cancel upload, Published chip with a real YouTube | NOT RUN | no network, no account |

## Verdict

PARTIAL (fixture only, no real publish). All listed S22 surfaces were seen with the fixture; B1 delete works.
