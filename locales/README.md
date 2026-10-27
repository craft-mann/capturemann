# Locales: UI languages

The CaptureMann UI can be shown in other languages. English is the source and the default: the app stays
English until the user picks a language in Settings -> General -> Language. Scope is the UI only; recordings,
captions, titles and descriptions, the CLI, MCP, JSON-RPC, logs and error codes stay as they are, and the macOS
permission prompts are not translated.

| File | What it is |
|---|---|
| `capturemann.pot` | The template: every message in the code. **Generated**, never edited by hand. |
| `<code>.po` | One catalogue per language (`vi.po`), embedded in the binary. |
| `unwrapped-baseline.txt` | The ratchet, closed at 0: UI string literals not wrapped in `tr!`. A new one fails the tests. |

There is no `en.po`: the English text is the message id in the code, so English cannot go missing. A message with
no translation (or an empty one) shows the English, never a key.

## For developers: marking a string

Use the macros from the `cm-i18n` crate (`use cm_i18n::{tr, tr_ctx, trn};`). The text must be a string literal
(the extractor reads the source).

```rust
tr!("Copy link")                                   // a message
tr!("Delete {title}?", title = name)               // named placeholders
tr_ctx!("button", "Open")                          // context for a short or ambiguous word
trn!("{n} recording", "{n} recordings", count)     // plural; {n} is the count
```

They return a `String`. Rules (they exist so translators can do their job):

1. **One sentence per message.** Never build a sentence from pieces (`tr!("Delete") + " " + name`); word order
   differs between languages.
2. **Placeholders, not concatenation.** `tr!("Saved {n} files in {folder}", ...)`. A translation may reorder
   placeholders but must keep every one of them.
3. **Context for short words.** `Open`, `Save`, `System`, `Light` mean different things in different places: use
   `tr_ctx!("theme", "Light")` and the translator sees the context.
4. **No text in images or icons.** Anything a person reads goes through `tr!`.
5. **Plurals with `trn!`**, never `if n == 1`.
6. Do not translate product names (CaptureMann, YouTube, Google Drive, MCP, Whisper), keyboard glyphs, timecodes or
   unit symbols. Sizes, dates and day labels come from `cm_i18n::fmt` (`size`, `day_label`, `decimal`), which
   knows the decimal comma and the month names.
7. Strings that state a fact or confirm a destructive action (privacy statements, delete confirmations) are
   reviewed by the owner. Put `// TRANSLATORS: FACT: owner review required (why)` on the line above the
   `tr!` call (or above the `fn`, `match` or block that holds only such messages): the extractor carries
   the comment into `capturemann.pot` as `#. FACT: owner review required (why)`, and a `.po` made from the
   template keeps it. Any other `// TRANSLATORS: ...` comment is a note to the translator (what a
   placeholder is, how long the text may be) and travels the same way. A plain `//` line right under it
   continues the note.
8. A UI-looking literal that is not for people (an id, a log line) is marked `// i18n-ignore` on its line or the
   line above so the ratchet does not count it.

Native strings (tray, menus) are rebuilt when the language changes; see `platform::menu_text` and
`TrayHandle::relabel` in `apps/desktop`.

### Workflow

```sh
cargo run -p cm-i18n -- extract     # regenerate capturemann.pot after adding or changing tr! calls
cargo run -p cm-i18n -- check       # what CI checks: fresh template, sound .po files
cargo run -p cm-i18n -- coverage    # per-language percentages
cargo run -p cm-i18n -- unwrapped   # list the UI literals still to wrap (--count: only the number)
cargo run -p cm-i18n -- baseline    # write the count to unwrapped-baseline.txt (it is 0)
```

`cargo test -p cm-i18n` runs the same checks and fails when:

* `capturemann.pot` is stale (run `extract` and commit it),
* a `.po` has a msgid that is not in the template (stale), a duplicate msgid, a placeholder the English does not
  have, a missing placeholder, a wrong number of plural forms, or an empty plural form,
* a UI literal in `apps/desktop/src` is not wrapped (wrap the new string, or add `// i18n-ignore`). The
  count is 0 and stays 0.

The unwrapped count is a conservative heuristic (`cm_i18n::extract::unwrapped_in_source`): a string literal that
starts with a capital letter and is a direct argument of a call, a tuple or an array, in non-test code. It misses
strings that start with a lowercase letter or are built by `format!` from variables; review those by eye.

### Testing a layout

`CAPTUREMANN_PSEUDO_LOCALE=1 cargo run -p cm-desktop` shows every wrapped string accented, about 30 percent
longer and in brackets (`[Çöþý ļîñķ ~~~]`). Clipped text, hard-coded (plain) text and sentences glued from
pieces stand out. It is not offered in the UI and wins over the chosen language.

`cargo test -p cm-desktop --lib i18n_net_tests` is the same idea without a window: it builds the text of
about 450 label accessors (navigation, library, settings, permissions, recording sheet and HUD, editors, AI,
publish, destinations, toasts) in English and in the pseudo-locale and fails for every string that is the
same in both, because that string never reaches `tr!`. A product name or file format that must not change
goes into `ALLOWED` in that file; a new screen adds its accessors to `corpus()`.

Text that is kept in a view's state (an error under a field, a note after a sign-in, a sticky toast) must be
stored as a reason and worded when drawn, so it follows a language switch: use an enum with a `text()`
method (see `daemon_text::ErrorNote`, `ui/settings/problem.rs`) and, for a toast, `ToastSpec::live(|| ..)`,
which the app words again when the language changes.

In tests, `cm_i18n::with_language(Lang::Vi, || ...)` sets the language for the current thread only, so tests
run in parallel. Tests that pin English text need nothing: the default is English.

### Layout: long translations

GPUI measures a text item with no wrapping when its parent asks for the minimum size, so a flex item that
holds text is never narrower than its whole text unless it says so. A longer translation then runs out of
its card, tile or pill. The rule for every row (`.flex()` without `.flex_col()`) that holds text:

* A text child that takes the rest of the row: `components::label_fill(text)` (`flex_1` and `min_w(0)`), or
  `.flex_1().min_w(px(0.))` on your own element. A text child that keeps its width while it fits:
  `components::shrink_text(text)`. A single line in a fixed-height control that must not grow:
  `components::ellipsis_text(text)` (and the full text as the aria label or a tooltip).
* Icons, dots and keycaps are `flex_none` (`icon`, `dot`, `keycap` already are).
* A control with a fixed or capped width is listed in `src/layout_width_tests.rs`: add a `Case` with the room
  its label has (worked out from the sizes in the code, noted in the case) and where the labels come from.
  `cargo test -p cm-desktop --lib layout_width_tests` measures them in the bundled Geist fonts in English,
  Vietnamese and the pseudo-locale and fails when English or Vietnamese does not fit. The fix is, in this order:
  let the text wrap or cut it (above), a shorter wording (ask the translator), and only then a new pinned
  constant, changed together with the test that pins it. The sizes that are estimated from character counts
  (toast, confirmation dialog and Record sheet heights, the HUD agent chip) have their own checks in the same
  file.
* `LAYOUT_WIDTH_REPORT=<file> cargo test -p cm-desktop --lib layout_width_tests` writes the table of
  `design/i18n-STATUS-overflow.md`.

`cargo test -p cm-desktop --test layout_guard` reads the sources (no window) and fails for a `.flex_1()` child
of a row that holds text and has no `min_w`, `truncate` or `overflow_hidden`, and for a bare text child of a
row that has other children and does not shrink or clip. When the code is right as it is (a content-sized
button, a badge over a thumbnail, a word that is never translated), add a line to
`apps/desktop/tests/layout_guard_allow.txt`: `RULE|path|snippet  # why`, where the snippet is the text the
failing test prints. A line that matches nothing fails the test, so the list cannot go stale.

## For translators: adding a language

1. Copy `capturemann.pot` to `locales/<code>.po` (a two-letter code such as `de`, `fr`; `vi.po` is the model).
   Set the header: `Language: <code>` and the `Plural-Forms` line of your language
   (<https://www.gnu.org/software/gettext/manual/html_node/Plural-forms.html>).
2. Translate with any `.po` editor (Poedit, Weblate, a text editor). Fill `msgstr`; for plurals fill every
   `msgstr[N]`. Keep every `{placeholder}` exactly as written (you may move it). Leave a `msgstr` empty when you
   are not sure: the English shows. Mark a guess `#, fuzzy` (it is shown, and counted as a draft).
3. Keep the language's own name out of translation: the picker shows each language by its own name.
4. A developer adds the language to the code: a variant of `cm_i18n::Lang` (`lang.rs`: code, native name,
   plural rule), the `include_str!` line in `catalogue_source`/`catalogue` in `lib.rs`, and the date and number
   table in `fmt.rs` (decimal separator, short months, today and yesterday). If the script is not covered by
   the bundled Geist fonts (Vietnamese was), a font fallback is needed too (`apps/desktop/assets/fonts`).
5. Run `cargo run -p cm-i18n -- check` and `-- coverage`, then open a pull request. A native speaker reviews it.

A language is shipped when the owner or a native reviewer has gone through the app in it. Until then the file
carries a header comment saying which entries are still drafts. `vi.po` (Oct 27, 2026): the strings added for
0.2.0 (386 messages not in v0.1.0's catalogue) were reviewed by the maintainer, a native reader, and passed,
so their `#, fuzzy` flag was removed (385 flags; one of them was not flagged); the older entries keep `#, fuzzy`
until the full catalogue is reviewed (`cargo run -p cm-i18n -- coverage`: 2575 of 2576 translated, 2183 draft).

## Plural rules

The rules of the shipped languages are hard-coded in `cm_i18n::Lang` (English: one form when `n == 1`;
Vietnamese: a single form) instead of evaluating the `Plural-Forms` expression. A test checks that every `.po`
header says the same, so a mistake in the header is caught.
