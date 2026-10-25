# I18N-IME: input-method composition in the text fields

Covers I18N S2 (`specs/i18n-plan.md`): Vietnamese (Telex, VNI), Japanese and Chinese input in every
text field, plus grapheme-aware caret and delete. Status: **not run** (built and tested with fakes;
the pure logic is in `apps/desktop/src/text_buffer.rs`, the wiring in `apps/desktop/src/ui/text_field.rs`).
Report a run in `verification/` (see its README).

No recording, camera or microphone is used. Needs macOS and the bundled app (`make bundle`, then open
it; ask the agent to rebuild, do not run it over a running copy).

## Setup

1. System Settings, Keyboard, Text Input, Input Sources, Edit, `+`: add **Vietnamese** with **Telex**
   and **VNI**, **Japanese** (Hiragana), and **Chinese, Simplified** (Pinyin). Keep **ABC**.
2. Switch sources with the menu bar flag or Ctrl+Space (or Fn).
3. Fields to test (each one with every step marked "all fields"):
   - Library search box (Library page, one line, search glyph).
   - Rename box (library item menu, Rename: starts with the title selected).
   - Publish sheet: title (one line) and **description** (multi-line, YouTube).
   - Settings, Destinations: a setup field (client id), and the secret field (shows bullets).
   - Recording editor, AI tab: the caption text field (needs a recording with a transcript).

Notation: `[Telex]` types the keys as written. `=>` is the text the field must show. Underlined means
the composing (marked) text. Press the keys one at a time, not pasted.

## A. Telex (Vietnamese - Telex source), all fields

| # | Keys | Expected while typing | Expected after the final key |
|---|---|---|---|
| A1 | `a` `s` | `a` underlined, then `á` underlined | `á` |
| A2 | `a` `s` Space | `á` committed, a space after | `á ` (no underline) |
| A3 | `d` `d` | `d`, then `đ` | `đ` |
| A4 | `u` `o` `w` | `u`, `uo`, `ươ` | `ươ` |
| A5 | `V` `i` `e` `e` `t` `j` Space | `Vi`, then the draft `ê`, `êt`, `ệt` underlined | `Việt ` |
| A6 | `t` `o` `o` `i` Space `y` `e` `e` `u` Space `V` `i` `e` `e` `t` `j` Space `N` `a` `m` | | `tôi yêu Việt Nam` |
| A8 | `h` `o` `a` `f` Space | `hoà` | `hoà ` |
| A9 | `m` `o` `w` `i` | `mơ`, `mơi` | `mơi` |
| A10 | `a` `s` `z` (undo the tone) | `á`, then `a` | `a` |

Pass: no letter is doubled (`aá`, `áá`), no raw Latin letter remains in front of the composed one,
the caret stays at the end of the draft, the draft is underlined while composing and not after the
space, and `Changed` is reported (the library list filters live while composing in the search box).

## B. VNI (Vietnamese - VNI source), all fields

| # | Keys | Expected |
|---|---|---|
| B1 | `a` `1` | `á` |
| B2 | `d` `9` | `đ` |
| B3 | `u` `7` `o` `7` | `ươ` |
| B4 | `V` `i` `e` `6` `t` `5` Space | `Việt ` |
| B5 | `T` `o` `6` `i` Space `y` `e` `6` `u` | `Tôi yêu` |

## C. Cancel, commit and focus

1. **Escape with a draft** (one-line field, Telex): type `a` `s` (draft `á`), press Escape. macOS
   normally resolves the draft itself; record what you see. Acceptable: the draft is committed or
   removed, the field is **not** left (a dialog or the sheet stays open), and the next Escape leaves
   as before.
2. **Escape without a draft**: Escape closes the rename box / clears the search / closes the sheet as
   it did before this change.
3. **Enter with a draft** (rename box): type `a` `s`, Enter. Expected: the draft is committed (`á`) and
   the rename is submitted once with `á` (not `a`, not `aá`).
4. **Enter in the description** (multi-line, Telex): `V` `i` `e` `e` `t` `j`, Enter. Expected: the
   draft is committed and a new line starts only if the OS passes the Enter on (macOS takes the first
   Enter to commit; a second Enter adds the line). Note which.
5. **Click elsewhere while composing**: type `a` `s`, click another field. Expected: `á` stays in the
   first field as plain (not underlined) text, nothing is lost or doubled.
6. **Tab** while composing: same as 5.
7. **Cmd+A / Cmd+C / Cmd+V / Cmd+Z** (ABC and Telex): Cmd+A selects all, Cmd+C copies, Cmd+V
   pastes, as before (there is no undo in these fields; Cmd+Z does nothing, as before).

## D. Selection and paste with a draft

1. Type `hello`, Cmd+A (all selected), then in Telex `a` `s`. Expected: `hello` is replaced, the
   field shows only `á` (underlined while composing).
2. Rename box (title preselected): type `V` `i` `e` `e` `t` `j`. Expected: the old title is gone,
   `Việt` remains.
3. Type `ab`, move the caret between `a` and `b` (Left), then `a` `s`. Expected: `a` + draft `á` + `b` = `aáb`, only the middle letter underlined, caret right after it.
4. **Paste during a draft**: type `a` `s` (draft `á`), press Cmd+V with `PASTE` on the clipboard.
   Expected: `áPASTE`, no underline, nothing lost. (macOS may commit the draft first; either way the
   draft must survive.)
5. Click inside a long single-line text while a draft is active: the draft is committed at that
   place, the caret goes to the click.

## E. Caret, Backspace and Delete over combining marks (any source)

Use text typed as separate characters. Easiest: in the multi-line description paste `e` + `U+0323` +
`U+0302` (the decomposed `ệ`; copy it from this line: `ệ` written as e, dot below, circumflex, i.e.
"ệ"). If you cannot paste NFD text, type it in a text editor that does not normalize and
copy it.

1. Decomposed `ệ`: Left moves over the whole letter in one press, Backspace deletes the whole letter in
   one press, Delete likewise.
2. Precomposed `ệ` (typed with Telex): same: one Left, one Backspace.
3. Emoji `👨‍👩‍👧` (family, ZWJ sequence), `🇻🇳` (flag), `👍🏽` (skin tone): each is one Left, one
   Backspace.
4. Backspace in a draft (Telex): type `a` `s`, Backspace. Expected: the draft is removed (macOS ends
   the composition), nothing else is deleted. With text before it (`xyz` then `a` `s` Backspace) `xyz`
   stays intact.

## F. Japanese and Chinese sanity

1. Japanese (Hiragana): `k` `a` `n` `j` `i` Space. Expected: the draft `かんじ` is underlined, the
   candidate window opens under the **draft** (not at the window corner), Space converts to a kanji
   candidate list, Enter commits `漢字`. Arrow keys inside the candidate list move the choice, they
   do not move the caret.
2. Chinese (Pinyin): `n` `i` `h` `a` `o`. Expected: draft `ni hao`, candidates, `1` selects `你好`
   (committed, no underline).
3. Press Escape during the draft: the draft goes away, the field stays open.
4. Candidate window position, one line: in the search box and in the description (multi-line), also
   after the description has wrapped to two lines and scrolled. Expected: the window sits right under
   the line of the draft, follows it when the draft wraps, and never covers the text being typed.
   In the description, put the caret on line 2 and type; the window must be under line 2.
5. Shift or Caps Lock toggles ABC/Kana: plain Latin typing still produces plain text.

## G. Plain typing is unchanged (ABC source)

1. Type `Hello, world 123`; Backspace, Delete, Home, End, Left, Right behave as before.
2. Hold a letter key (press and hold): macOS accent popup may appear (`é` `è`...). Choosing an accent
   replaces the letter; or, if you disabled the popup, the letter repeats.
3. Option+E then `e` (dead key) gives `é`; Option+U then `u` gives `ü`.
4. Multi-line: Up/Down/Home/End, Enter adds a line, Cmd+Enter submits, as before.
5. The library keyboard shortcuts and the recording editor shortcuts (space, `j`, `k`, `l`, `s` ...)
   still work when no field is focused, and **do not fire** while the caption field has focus (type
   `s` and a space in it: no split, no play/pause).
6. The secret field (bullets): typing shows one bullet per letter; typing `a` `s` shows one bullet
   for the draft. (macOS may suggest the IME candidates in a secret field: note it, see the status
   report.)

## Record

For each row note: macOS version, input source, field, pass or fail, and what appeared on a fail
(screenshot described in text). Failures worth special attention: doubled letters (`aá`), a draft
that stays underlined after Space, text lost on blur, the candidate window at the screen corner.
