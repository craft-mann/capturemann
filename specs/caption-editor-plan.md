# Caption editor redesign (R3c) plan, Oct 24, 2026

Source: `design/redesign/screens/png/NewCaptions.png`, `screens/html/NewCaptions.html` and its text in
`design/redesign/CLAUDE.md` (row R3c). Goal (owner): make the caption editor friendlier so a user can fix what the
AI generated, line by line, quickly. **Approved by the owner on Oct 24, 2026:** the board, split at the cursor, the
placement of the displaced controls, the full-length captions lane, and the three extras below.

Code today: `apps/desktop/src/ui/recording_editor/ai_cards.rs` (`captions_panel`, `ai_select`, `ai_save`,
`ai_line_op`, `ai_add_line`, the `AiEditor` state), `ai/captions.rs` (`CaptionEditor`: `split` at the middle word,
`merge_with_next`, `remove`, `add_at`, `issues_of`, `line_at`, rules 42 chars x 2 rows, 1 to 7 s), `recedit/captions.rs`
(preview track), `ui/recording_editor/panels.rs` (inspector layout, `INSPECTOR_W` 400), `timeline.rs`.

## 1. Layout (board)

- Selecting the Captions tab puts the editor in **Captions mode**: the inspector grows to 600 px (never wider than the
  window leaves room for: keep at least a 360 px preview column; below that shrink the inspector down to its normal
  width, and set a minimum editor width of 1180 px if no minimum exists yet). The preview letterboxes (stays live and
  keeps its aspect). Every other tab returns to the normal inspector width.
- Panel header: icon tile/title "Captions", subtitle "{n} lines · click text to edit, click a time to jump there",
  right side "Line {i} of {n}" (mono) when a line is selected or under the playhead.
- **One row per caption**: start time and "→ end" time (mono, two lines, left, ~70 px), then the full text at 15 px,
  line height 1.55, wrapping naturally. Click the text to edit **in place** (the row's text becomes the multi-line
  field; no second copy). Click the time to seek the playhead there (no edit).
- The line under the playhead: 3 px white bar on the left edge, brighter text; the list auto-scrolls to keep it in
  view while playing or seeking, but never fights the user while they scroll or type (pause auto-follow for about
  2 s after a manual scroll, and never while a field has focus).
- While editing a row: Split, Merge with next and Remove sit in one slim row under the field, with a hint line
  "Split at the text cursor · Esc done". On other rows the same three actions appear as icon buttons on hover
  (tooltips; keyboard reachable) so the list stays dense.
- Footer: "Save a copy" label, SRT, VTT, TXT buttons and "Timestamps in TXT" switch, always visible.
- Captions lane on the timeline (below Mic, same label column): one block per line over the full recording length
  (the board's zoomed 0:00 to 0:30 view is a separate, later feature: `TODO(design)`), the text truncated, selection
  synced both ways (select a block = select and scroll to its row and seek; select a row = highlight its block),
  the active line (under the playhead) emphasised, lines with rule problems get the amber outline.
- Icons: use the kit's `merge`, `cm-split`, `trash-2`, `play`; sync any new icon the kit 3 `UiIcon` source adds.

## 2. Controls the board does not draw (kept, placement decided)

- Slim bar under the header: **Add at playhead** (plus) and **Save changes** (shown only when there are unsaved edits;
  shows "Saving…" while saving). Leaving an edited line still saves it, as today.
- Footer row above the save buttons: **Show captions in the preview** and **Include in exported video** as two small
  switch rows ("Display").
- Rule warnings: an amber `!` on the row and the issue messages under the field while editing; the count in the panel
  header ("{n} captions break the rules (42 characters a row, 2 rows, 1 to 7 s)") as an amber line under the subtitle.
- States: loading, no transcript yet (the Transcribe row), the note/hint line (`AiNote`), empty line "(empty)".

## 3. Behaviour changes (approved)

- **Split at the text cursor.** `CaptionEditor::split_at(i, char_index)`: splits the text at the caret (trimmed;
  both halves non-empty, otherwise not allowed), time divided in proportion to the characters like today. With the
  caret at the start or end, or when the row is not being edited, fall back to today's middle-word split. Tested with
  NFC and NFD Vietnamese, emoji, multi-line text. The caret is the field's cursor (`TextBuffer::cursor`, graphemes).
- **Keyboard flow** (when a caption field has focus; the recording editor's shortcuts stay disabled there):
  Enter commits the line and moves to the next line and starts editing it; Shift+Enter inserts a line break
  (the 2-row rule still applies); Cmd/Ctrl+Enter splits at the cursor; Up at the first row of the field and Down at the
  last move to the previous or next line (keeping the field in the same visual position when possible); Esc finishes
  editing (keeps the edit). Tab/Shift+Tab move between the row actions. Every action keeps its aria label.
- **Play this line**: a small play button on the selected row (and on hover) that plays from the line's start to its
  end, then pauses at the end. Reuse the editor's playback, no new media path; Esc or Space stops.
- **Find and replace**: a compact bar (Cmd/Ctrl+F while the Captions tab is active, also a search icon in the header):
  find field, replace field, "Replace" (this match), "Replace all", match count "{n} of {m}", case-insensitive by
  default with a "Match case" toggle, whole text of all lines, one undo step for Replace all. Matches are highlighted
  in the rows; the list keeps its order and scrolls to the current match. Vietnamese: compare without normalising away diacritics (a search for "dê" must not match
  "đê"); treat NFC and NFD as equal.

## 4. Quality bars

- English output of every untouched surface is unchanged; existing tests keep passing (mechanical edits only).
- All new strings through `tr!`/`tr_ctx!`/`trn!`; Vietnamese for the new strings added to `locales/vi.po` (draft,
  `#, fuzzy`, glossary in `locales/vi-glossary.md`); the layout guards (min_w, the width and pattern tests added by
  the overflow audit) pass.
- Undo/redo covers every caption change (text, split, merge, remove, add, replace all) as it does today.
- Accessibility: the list is a labelled list, each row has a label with time and text, the selected row announces
  editing, buttons have names, focus order follows reading order.
- Tested with fakes (pure logic in `ai/captions.rs` and a find module, models, the layout decisions); the manual
  script `tests/manual/CAP-EDIT.md` lists what only a person can check (Telex/VNI in the in-place field, caret
  split, auto-follow while playing, hover icons, the lane, narrow windows).
