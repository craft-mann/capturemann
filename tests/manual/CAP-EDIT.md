# CAP-EDIT manual script: the caption editor (redesign R3c)

Covers what only a person can check in the Captions tab of the recording editor: typing Vietnamese
in the in-place field, splitting at the caret, the Enter flow, find and replace, playing one line,
the list following the playhead, the hover icons, the lane on the timeline, narrow windows, the
light theme and the pseudo-locale. The pure logic is covered by `cargo test -p cm-desktop --lib
caption` (split at the caret with NFC, NFD and emoji, find and replace, line navigation, the
follow rules, the lane geometry, the Captions-mode width rule) and the layout guards by
`cargo test -p cm-desktop --test layout_guard` and `--lib layout_width_tests`. The plan is
[`specs/plans/caption-editor-plan.md`](../../specs/plans/caption-editor-plan.md), the board
`design/redesign/screens/png/NewCaptions.png`. Status: built, tested with fakes, **never seen by
eye** before this script is run (see `design/redesign/STATUS-captions.md`).

Nothing is recorded and no microphone is used. Make a recording with a transcript from the
synthetic fixture (`cargo run -p cm-daemon --example make_fixture -- --library <dir>
--secs 40`), point a second instance at it (`CAPTUREMANN_LIBRARY=<dir>`), and transcribe it from
the AI page with a downloaded model (or use a recording of yours that already has captions). A
Vietnamese input method (macOS: Vietnamese Telex and Vietnamese VNI under Keyboard, Input
Sources) is needed for steps 2 and 3. Report in `verification/` (template in its README).

1. **Captions mode.** Open the editor, click the Captions tab. Expected: the inspector grows to
   about 600 px, the preview shrinks but stays live and keeps its aspect (black bars at the sides
   or above and below), the panel header says "{n} lines · click text to edit, click a time to
   jump there" and, on the right, "Line {i} of {n}" once a line is selected or under the
   playhead. Click another tab: the inspector goes back to its normal 400 px. Return to Captions.
2. **Telex typing in place.** Click the text of a row. Expected: the row's text becomes a framed
   multi-line field (no second copy of the text), the caret is where you clicked, the playhead
   jumped to the line's start, the preview shows the line. With the Vietnamese Telex source
   type "chaof" (becomes "chào"), "Viejt Nam" ("Việt Nam"): the underlined draft is replaced key
   by key, the word is never clipped, long text wraps onto a second row of the field.
3. **VNI typing.** Switch to Vietnamese VNI and type "a1", "o6", "d9" (á, ô, đ) in the field.
   Expected: the same as step 2. Press Backspace over a letter with two marks: the whole letter
   goes, not half of it.
4. **Split at the caret.** Click inside a long caption, between two words, and press **Split**
   (the button under the field). Expected: two rows, the text divided exactly at the caret (trimmed), the times divided in proportion to
   the characters, the second half is now being edited with the caret at its start. Click in the
   middle of a word and press **Cmd/Ctrl+Enter**: the word is cut there (allowed). Put the caret at
   the very start or end of a line and split: it falls back to the middle word. Try a caption with
   an accented letter and an emoji next to the caret: the caret never splits a letter from its
   accent or an emoji.
5. **The Enter flow.** In a line press **Enter**: the line is saved, the next line opens with the
   caret at its start and the preview shows it. **Shift+Enter** adds a line break inside the
   caption (two rows are fine, a third row shows the amber rule message). **Up** on the first row
   of a field moves to the end of the previous line, **Down** on the last row to the next line,
   close to the same column. **Esc** closes the field, keeps the edit, saves it and gives the
   keyboard back: the editor's single letter shortcuts (Space, S, I, O, Z) work again. While a
   field has focus, typing Space or S types a space or an S and does not play or split the
   timeline.
6. **Tab order.** Tab from the list: each row is a stop (Enter opens it), the selected row adds
   its time button, Play, Split, Merge, Remove; while editing, the three buttons under the field
   come next. Every one has a name in the screen reader and a tooltip.
7. **Hover icons.** Hover a row that is not being edited. Expected: a play, split, merge and
   trash icon appear at its right (no merge on the last row), the list stays dense when not
   hovering. Merge with the next row: one row, the text joined, from the start of the first to the
   end of the second. Remove a row: it goes. Tab onto an icon with the keyboard: it appears too.
8. **Undo.** After edits, press Enter or **Save changes** (it shows only while there are unsaved
   edits), then Cmd/Ctrl+Z in the editor (focus outside the field) and Shift+Cmd/Ctrl+Z. Expected:
   each save is one step; the list and the preview follow. Do this after a text edit, a split, a
   merge, a remove, an Add at playhead and a Replace all (step 10).
9. **Rule warnings.** Make a caption 50 characters long on one row, or 3 rows, or 0.5 s long.
   Expected: an amber triangle on the row, the messages under the field while editing, and an
   amber line under the panel subtitle "{n} captions break the rules (...)".
10. **Find and replace.** Press Cmd/Ctrl+F (also from inside a field) or click the search icon.
    Type a word: matches are marked in the rows, "{n} of {m}" counts them, Enter or the arrows go
    to the next or previous match and the list scrolls to it (the order does not change).
    Vietnamese: search "dê" in a text that has both "dê" and "đê": only "dê" matches; "e" does
    not match "ê"; "chao" does not match "chào"; a search typed in the Vietnamese input method
    finds text stored in either form. **Match case** makes "Tiếp" differ from "tiếp". Type a
    replacement, press **Replace** (this match, saved at once) and **Replace all** (the note says
    how many places changed): undo reverts Replace all in **one** step. Esc closes the bar.
11. **Play this line.** Select a row and press its play button (also on hover). Expected: the
    playhead goes to the line's start, plays to its end, and pauses there. Esc or Space while it
    plays stops it. Click the play button of the playing line: it stops.
12. **Follow while playing.** Press Space on the preview and watch the list: the line under the
    playhead has a white bar on the left and brighter text, and the list scrolls to keep it in
    view. Scroll the list by hand while playing: it does not fight you for about 2 seconds, and
    follows again after that when the playhead enters the next line. Click into a field while
    playing: playback stops and the list never moves on its own while you type. Drag the
    playhead on the timeline over a far line: the list scrolls to it.
13. **Click a time.** Click "0:08.2": the playhead jumps there, the row is selected (a bar and a
    background), nothing opens for editing.
14. **The lane.** Under Mic there is a "Captions" lane with one block per line over the whole
    recording, the text cut to each block, the same label column as the tracks. The line under
    the playhead is bright (white with dark text), a line with a rule problem has an amber outline,
    the selected one has a 2 px frame. Click a block: the row is selected, the list scrolls to it,
    the playhead moves there. Select a row: its block is framed. The board's "Timeline zoomed to
    0:00-0:30" view is **not built** (`TODO(design)`): the lane shows the whole recording.
15. **Footer.** "Display": the preview switch and "Include in exported video" work and persist
    (reopen the editor). "Save a copy" SRT, VTT, TXT write the files (toast with the path) and
    the "Timestamps in TXT" switch changes the TXT. All stay visible however long the list is.
16. **Narrow window.** Drag the window from 1440 down to 720 px wide with the Captions tab open.
    Expected: the inspector is 600 px down to a window of about 1090 px, then gives up width
    before the preview does (the preview keeps 360 px) until the inspector is back at its normal
    width; below about 800 px the preview shrinks below 360 px (the window's own minimum is 720).
    The find bar and the footer wrap onto more rows, nothing runs out of the card.
17. **Light theme.** Settings, General, Theme: Light. Repeat steps 1, 7, 10 and 14: the active
    row, the white bar (dark on light), the find marks (current match inverted), the amber
    outlines and the field frame are all readable.
18. **Pseudo-locale.** `CAPTUREMANN_PSEUDO_LOCALE=1 cargo run -p cm-desktop` (set `CAPTUREMANN_LIBRARY`
    as above) and Vietnamese (Settings, General, Language). Expected: every label wraps or is cut
    with an ellipsis (the lane label, the find buttons), never overflows the card, the header's
    "Line {i} of {n}" and the hint "Split at the text cursor · Esc done" wrap under the buttons.
19. **Empty and missing states.** A recording with no transcript shows the "Transcribe" row as
    before; one with a transcript but no lines shows "There are no caption lines yet"; the
    loading state shows "Loading...". Add at playhead inside a caption shows the amber hint, in a
    gap opens a new empty line for editing.

Pass criteria: every expected result above. Note anything that looks different from the board
and say whether the board or the build is right.

## AI-6a (Oct 24, 2026): glossary, doubtful words, kept edits, replace_in_captions

Built and tested with fakes; **never seen by eye**. Two items need more than a fixture: the glossary
priming needs a real Whisper model run (a scripted transcriber only proves the prompt is passed),
and the doubtful underlines need a real transcript (the fixtures give every word a confidence of
0.9).

20. **Glossary page.** AI page, "Glossary" section under the transcribe switch. Add "Google Drive"
    often heard as "Logo Drive, Gu gồ Drai"; Enter saves, Esc closes the form, Edit and Remove on
    the row work, a repeated term (any case) shows "That term is already in the glossary.". Quit
    and reopen: the list is still there (setting `ai.glossary`). With a long Vietnamese UI the
    rows and the form wrap, nothing leaves the card.
21. **Glossary in a real transcript (real model).** Add a term that Whisper usually gets wrong
    (a product name, a person's name) and list the spelling you hear as a wrong spelling. Record
    10 s saying it, transcribe with `small` or better. Expected: the caption line shows the term
    as written in the glossary; with a term that is only a hint (no wrong spellings) check by
    eye whether the model spells it better than without the glossary (it is a hint, not a
    guarantee). A word that is a Vietnamese "dê" is not turned into "đê" and the other way round.
22. **Doubtful words (real transcript).** Open Captions on a recording with a real transcript.
    Some words have a wavy underline in the amber warning tone (not red) in the rows. Edit one
    underlined word: its underline goes away while you type. The slim bar under the header shows
    up and down arrows and "{n} doubtful" on the same row as Add at playhead; no extra header row.
23. **Jumping.** Click the down arrow: the line is selected, the playhead moves to where the word
    is said, the caret is at the word, the list scrolls to it. Up goes back, both wrap around.
    In a caption field, Cmd+Alt+Down / Cmd+Alt+Up (Ctrl+Alt on Windows) do the same.
24. **Keep edits on re-generate.** Edit two lines of a caption, then re-generate with the same
    model. Expected: the toast says how many edits were kept; the lines keep your text; undo
    brings the previous captions back in one step. Re-generate with another model or language so
    some words change: the edits on those lines are dropped and the toast counts them. The
    Re-generate panel says "your caption edits are kept where the same words are heard again".
25. **Agent replace.** `capturemann ai captions replace <id> --find "Logo Drive" --replace
    "Google Drive"`; the editor (open) shows the new text after it reloads, `capturemann undo
    <id>` brings it back in one step, and `transcript.json` is unchanged.
