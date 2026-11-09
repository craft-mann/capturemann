# EXP-5 manual script: the Export popover (size, format, estimate)

Covers what only eyes and hands can check: the popover's look and place, the keyboard, the
narrow-window dialog, the four looks of the size block, queueing, Vietnamese text. The rules
(defaults, the size list, the GIF limits, stale replies, the request sent) are unit tested without
a display (`cargo test -p cm-desktop --lib export_flow`); the numbers behind the estimate are
`cm-media` and `cm-daemon` tests (EXP-5.AC1 to AC9). **Not run yet: the popover was built without
a display and has never been seen.** Say "verified" only with a report in `verification/`.

Setup: a library of its own (`CAPTUREMANN_LIBRARY=<dir>`), the synthetic recording from
[`M4-GATE.md`](M4-GATE.md): `cargo run -p cm-daemon --example make_fixture -- --library <dir>
--secs 192` (3:12, one over 30 s) and a second one with `--secs 24`. Nothing is recorded; no
sound is played. Use the bundle (`make bundle-dev`), not `target/debug`, if FFmpeg must be the
bundled one.

Only the maintainer can judge by eye: items 1, 2, 5 (the amber look), 8 and 9 (layout and text
fit). The rest can be done from the keyboard.

1. **Button and shortcut.** Open the 3:12 recording's page (Library, click it). In the header
   the button reads `Export…` (a download icon), between the folder button and Publish…. Click
   it: a 460 px popover appears at the top right under the toolbar, nothing on the page moves.
   Close it (Cancel, Esc, a click outside). With the page showing, press **Cmd+E** (macOS) or
   **Ctrl+E** (Windows, Linux): the same popover opens. In the editor (Edit button) both header
   Export buttons and the shortcut do the same, and the tooltip shows the keycaps. Pass: opens on
   MP4 and Recording size every time, even after an export (nothing is remembered); no split
   button, no arrow.
2. **Defaults and the estimate (MP4).** Header line: the title and "3:12 after cuts". Format cards
   MP4 (chosen), WebM, GIF with their lines; Size list with real pixels on the right (Recording
   size first, then 1440p, 1080p, 720p, 480p, only steps below the recording) and the hint "Never
   larger than the recording"; the size block shows "Estimated size", an "Estimate" tag, "Up to
   N MB" and "MP4 · H.264 · W × H. The real size is shown after export." Choose 720p, then WebM:
   the number changes the moment you click, with no spinner and no flicker of an old value under
   the wrong format. Footer: "Runs in the background", Cancel, "Export MP4" / "Export WebM".
3. **Estimate against the file.** Export MP4 at 720p; when it ends, the toast reads
   "Exported · NNN MB" and under it "estimate: up to NNN MB"; the real size is at or below the
   estimate. Repeat with WebM. The Exports list shows only the real size.
4. **Queueing.** Start an export of the 3:12 recording; while it runs the header button reads
   "Exporting 42%" (with a spinner icon), and clicking it still opens the popover. Choose another
   option and press Export: a second job queues (no error, popover closes). Open it again and
   press Export with the same options: no duplicate toast (the equal request is reused). Two
   sizes of the same recording replace the first file: nothing is said about it besides the
   Exports list.
5. **GIF.** Choose GIF on the 3:12 recording: the Size list is replaced by the Width control
   (480, 640, **800**, 1024), the clip line "Whole recording · 3:12 (30 s at most)", the "Keep the
   file under [10] MB" checkbox, and the size block turns **amber**: "Can't export as GIF",
   "GIF is for clips of 30 s or less. This one is 3:12.", "Set In and Out on the timeline to pick
   a short part, or choose MP4 or WebM." The Export GIF button is dimmed, does nothing when
   pressed, and its tooltip says the same text. Switch back to MP4: the amber block goes and
   Export works. On the 24 s recording (or set In and Out in the editor so the clip is 24 s, then
   open the popover from the editor): GIF shows the grey look "Size known after export" and
   "GIF size can't be estimated. The real size is shown after export."; tick the checkbox and type
   25: "... It will stay under 25 MB: width, then frame rate, step down to fit." In the editor with
   In and Out set the header says "0:24 between In and Out" and the clip line "In 0:42 → Out
   1:06 · 24 s (30 s at most)". Clear the number: the box shows a warning outline and Export
   waits. Export it: the toast names "GIF, 800 × 500" and the real size with no estimate line.
   A GIF that cannot fit (set 1 MB on a busy clip) fails with "Can't fit under 1 MB at 480 px and
   10 fps." (job failure toast).
6. **Narrow window.** Resize the window under 900 px wide: the popover becomes a centred dialog
   over a dimmed window with a close x, same content, nothing clipped (the body scrolls when the
   window is short). Over 900 px it is the top-right popover again.
7. **Keyboard.** Tab reaches every card, size row, width segment, the checkbox, the MB field,
   Cancel and Export, each with a visible focus ring; Esc closes (also from the MB field);
   Enter in the MB field exports.
8. **Vietnamese and the pseudo-locale.** Settings, General, Language: Tiếng Việt. Open the
   popover: "Xuất", "Định dạng", "Kích thước", "Không lớn hơn bản ghi", "Dung lượng ước tính" tag
   "Ước tính", "Tối đa N MB" (decimal comma in "1,2 GB"), the MP4/WebM/GIF/H.264/VP9/In/Out/MB
   words stay English, the amber text wraps in three lines, nothing is cut. Then
   `CAPTUREMANN_PSEUDO_LOCALE=1`: same check at about 30 % longer text; the cards and lines wrap,
   the footer wraps its buttons under "Runs in the background" instead of overlapping.
9. **Linux without a hardware H.264 encoder** (a VM or a machine without VA-API): with MP4
   chosen a blue info note (not amber) sits under the format cards: "This computer has no H.264
   video encoder, so MP4 files use VP9 video. ...", the size block's detail line says
   "MP4 · VP9 · ...", the card still says MP4; WebM hides the note. Not run on any machine yet.
10. **Failure and edges.** With the file's recording deleted in the background, or FFmpeg
    missing, the estimate block shows the daemon's text in grey and Export shows the refusal in
    the popover (an amber line above the footer) instead of closing. An audio recording has no
    Export… popover (Cmd/Ctrl+E does nothing there; Export audio is in its editor).

Pass: every step as written; record deviations in a report in `verification/` (template in its
README).
