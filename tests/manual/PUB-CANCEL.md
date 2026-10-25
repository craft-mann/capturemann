# PUB-CANCEL manual script: Cancel for a running publish (Publish page)

Covers what the unit tests cannot: the button on the progress line, the real daemon's
`cancel_job`, and the timing. The state logic (idle, running, cancelling, finished while
cancelling, cancel error, two publishes) is tested in `apps/desktop/src/publish/cancel.rs`
(`cargo test -p cm-desktop --lib cancel`); the daemon side by
`pub2_a_running_publish_can_be_cancelled_and_leaves_no_record` in `crates/cm-daemon`.
No board draws the running state, so the control sits on the progress line in the page's
footer, where the progress and the result already appear.

Setup: a recording with an export, a connected YouTube account (see `PUB-1.md`), and a video big
enough to upload for a while (a few hundred MB, or throttle the network). Publish as Private so
nothing is exposed. This sends a video to YouTube: run it only when you mean to.

1. **The button.** Press Publish... and confirm in the dialog. The footer shows "Uploading to
   YouTube N%", a progress bar and a secondary "Cancel" button at the right of the line.
2. **No question.** Press Cancel. No dialog opens. The line and the button change at once to
   "Cancelling..." (the button is muted and cannot be pressed again).
3. **Stops.** Within a few seconds the page returns to the form (title and fields as you left
   them, Publish enabled) and a neutral toast "Publish cancelled" appears. Check the Published
   list: no entry for this video. Check YouTube Studio: no new video (or none that finished).
4. **Again.** Press Publish... again and let it finish: the result block with the link shows as
   usual.
5. **Finished while cancelling.** Use a small file so it ends fast; press Cancel in the last
   second. Expected: either "Publish cancelled" (form), or the normal result with the link and
   no cancel toast. Never a page stuck on "Cancelling...".
6. **Cancel fails.** Quit the daemon-side job some other way is not needed: run
   `capturemann jobs` during an upload, cancel it from the CLI first (`capturemann jobs cancel
   <id>`), then press Cancel on the page. Expected: the page returns to the form (or shows
   the error toast "Couldn't cancel the publish" with the reason) and is not stuck.
7. **Leave and return.** Start an upload, leave the page, come back: the progress and Cancel are
   shown again; Cancel works.
8. **Language.** In Tieng Viet the button reads "Huy", the pending state "Dang huy...", the
   toast "Da huy dang" (with accents; fuzzy draft awaiting review) and nothing is cut off.

Pass: every expected result holds. Record the run in `verification/` (template in its README).
