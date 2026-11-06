# ANN-1-IME manual script: Vietnamese (Telex, VNI) in the annotation Text tool

Covers ANN-1.AC7 and AC8 (`specs/features/02-annotation.md`). The logic is tested with fakes
(`cargo test -p cm-desktop --lib editor::`, `cargo test -p cm-render --test fallback_fonts`); a real
input method can only be tried by hand. Run on Windows 11 and on macOS. Linux (IBus, Fcitx) is not
tested in 0.2.9. Report a run in `verification/` (see its README). No camera, microphone or screen
recording is needed: open any existing screenshot (or take one of a made-up window).

## Setup

- Windows: Settings, Time and language, Language and region, add Vietnamese, and add the keyboard
  Vietnamese Telex (and Vietnamese VNI). Switch with Win+Space.
- macOS: System Settings, Keyboard, Text Input, Input Sources, add Vietnamese with Telex and VNI.
  Switch with Ctrl+Space.

## Steps

1. Open a screenshot in the editor, press T (Text tool), click on the image. Expected: a caret; the
   panel shows size 24 and a white colour. (AC8)
2. Switch to Vietnamese Telex. Type, one key at a time: `T` `i` `e` `e` `n` `g` `s` Space `V` `i` `e`
   `e` `t` `j` Space `c` `o` `s` Space `d` `a` `a` `u` `s`. Expected while typing: the letters being
   composed are underlined or marked; after the last key: `Tiếng Việt có dấu`. No doubled letters
   (`Tiêếng`) and no raw letter before an accented one.
3. Press Esc. Expected: the text is placed, no outline or shadow around it.
4. Click the text with Select (double click) to edit it, press End, Backspace once. Expected: one
   character is removed (the last letter), not a whole word.
5. Switch to Vietnamese VNI. Place a new text and type, as VNI digits:
   `T` `i` `e` `6` `n` `g` `1` Space `V` `i` `e` `6` `t` `5`. Expected: `Tiếng Việt`.
6. Switch back to the English keyboard and type `Hello 123`. Expected: it appears normally (plain
   typing still works), Enter starts a new line, Esc finishes.
7. With no text box open, shortcuts still switch tools (T text, V select). While a text box is open, typing `v` or `t`
   inserts the letter and does not change the tool; Cmd/Ctrl+V pastes.
8. Export or Save as PNG. Open the PNG in an image viewer. Expected: the same Vietnamese text, every
   accent visible, no empty boxes.
9. Change the colour of one existing text and the size to 40: only that text changes. Reopen an
   older screenshot that already has text: it keeps its colour, size and outline. (AC8)
10. Place a text on a white screenshot: it is white on white and cannot be seen unless the colour is
    changed. Expected and accepted: there is no automatic outline (maintainer decision, Nov 6, 2026).

Pass: steps 2, 5 and 8 show the exact phrase. Report any doubled or raw letter with the input source
name and the operating system.
