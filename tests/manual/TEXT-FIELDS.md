# TEXT-FIELDS: the shared text field look (0.3.3)

Spec: `specs/capturemann-text-fields/text-field-spec.md` (local design pack), tokens in `apps/desktop/src/ui/field_style.rs`.
Run on each system with a release candidate; note the system, the theme and what you saw in `verification/TEXT-FIELDS-<date>-<who>.md`.

1. **Rename** a capture (library menu): the Name label, `Enter rename · Esc cancel` under the field, the whole name selected, Rename disabled
   while the name is empty or unchanged, a blue frame and ring on the field. Type a name the app refuses: the reason shows under the field in red.
2. **Search** the library: type text, the clear button appears only with text; Esc clears, a second Esc leaves the field.
3. **Selection and caret:** double-click a word, triple-click all, Shift+arrows extend; the caret is 2 px and blinks when idle; click in another
   window: the selection turns grey and the ring goes.
4. **Vietnamese Telex and VNI** in a field and on the canvas (`tests/manual/I18N-IME.md`): the composing underline is blue and thin.
5. **Text on a screenshot** (Text tool T, Callout K): the new box has an outline that shows on a light and on a dark picture, "Type text" until the first
   key; select it: handles and the "Double-click or Enter to edit" hint; hover another box: a dashed outline.
6. **Steps list** (tool N): click a label to edit (whole label selected), Enter saves and opens the next step, Esc restores, Alt+Up and Alt+Down move
   the step, Delete removes it (Undo brings it back); the selected badge has a blue ring and nothing is orange.
7. **Caption line** in a recording: hover says "click to edit"; editing shows the blue frame, `Enter save · Esc cancel`; Esc restores the old text.
8. **Light theme:** repeat 1, 3 and 5.
