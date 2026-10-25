# UI languages plan (draft, Oct 23, 2026)

Status: **built Oct 24, 2026 (tested with fakes; the owner used the Vietnamese UI and the fixes it led to are in); the Vietnamese draft awaits the owner's full review.** See section 8 for what was built and how it differs from this plan. Goal: the app UI can be shown in other languages,
**English first and default**, **Vietnamese next**, more later by the community. Scope is the **UI**, not the
content (captions, titles, descriptions, recordings).

---

## 1. What I found in the code (measured today)

| Fact | Where | Consequence |
|---|---|---|
| About **1,900 UI string literals** in `apps/desktop/src` (roughly: `ui` 1,200, `recording` 280, `launcher` 115, `ai` 80, the rest 150) in 76,000 lines; no localisation code, library or locale handling exists | grep of `.child("…")` and capitalised literals; no i18n crate in any `Cargo.toml` | Every string has to be touched once. The migration is mechanical but large; it should be done by area in parallel agents, like the redesign. The real number with `format!` strings is higher. |
| **The bundled fonts cannot draw Vietnamese.** Geist and Geist Mono (all weights, 645 and 648 glyphs) are missing **88 of the 104 Vietnamese letters**: `Ơ Ư ơ ư` and the whole block of stacked tone marks (`ạ ả ấ ầ ẩ ẫ ậ ắ ằ ẳ ẵ ặ ẹ…`). The Noto fallback fonts the plan once decided to bundle (docs/PLAN.md decisions) are not bundled | `apps/desktop/assets/fonts/*.ttf` read directly | A Vietnamese UI would mix Geist with system-fallback glyphs inside one word. A font decision is a prerequisite (section 4, S1). |
| **Text fields have no input-method (IME) composition.** `ui/text_field.rs` says "No IME composition yet", `text_buffer.rs` is char-based, not grapheme-based | file headers | Vietnamese is typed with Telex or VNI, which use marked (composing) text. Titles, search, descriptions and rename boxes would misbehave for Vietnamese users even if the labels were translated. This is not about the UI language, but Vietnamese users hit it on day one: a prerequisite (S2). |
| **Fixed sizes pinned by tests:** dock tiles (about 65 px), HUD bar (506 px), Record sheet (600), toast (440), dialog (520), inspector tab rail (54), 40 px controls | `theme.rs`, `recording/geometry.rs`, sheet and toast tests | Longer translations can clip. Vietnamese is about as long as English but its stacked marks need more line height than the tight 40 px controls give. A pseudo-locale stress pass is part of the plan. |
| **Strings built outside the app** and shown in it: the confirmation dialog's summary and detail rows come from `cm-daemon` (`Action::summary()`, details) and about **350 error/format sites** in `cm-daemon` produce messages that toasts and panels show | `crates/cm-daemon/src/action.rs` etc. | These must become structured (code + parameters) so the app can translate them. CLI, MCP and JSON-RPC stay English (machine interface; error codes stay stable). |
| **Native strings:** 6 native menu items (tray and menu bar), 66 literal accessibility labels (more built with `format!`), the macOS permission prompts in `Info.plist` | `platform.rs`, `packaging/macos/Info.plist` | The tray menu must be rebuilt when the language changes. Permission prompts are localised by macOS from `vi.lproj/InfoPlist.strings` and follow the **system** language, not the in-app setting. |
| **Formatting is English-specific:** `format_size` ("2.4 MB"), `day_label_at` ("Today", "Yesterday", "Oct 12"), mono timecodes | `library.rs` | Localise day names, month names and the decimal separator (Vietnamese uses a comma). Timecodes and unit symbols stay as they are. |
| About **126 test assertions** pin English UI text, and many more indirectly | grep | Tests run with the English catalogue pinned; they do not change. Vietnamese gets its own checks. |
| The settings pattern is ready: dotted keys such as `appearance.theme` | `settings.rs` | Language is one more key, `appearance.language`. |
| `deny.toml` already allows the licences of the usual i18n crates (MIT, Apache-2.0, MPL-2.0, Unicode-3.0) | `deny.toml` | No licence blocker. |

---

## 2. Scope

**In scope (translated):** sidebar, every page and panel, buttons, tooltips, accessibility labels, toasts, the
confirmation dialog (including the rows the daemon builds), error messages shown in the UI, the tray and menu bar,
the Record sheet and HUD, first-run and permissions screens, the macOS permission prompts (`InfoPlist.strings`),
date, number and size formatting.

**Not translated:** recorded content; transcripts and captions (their language is chosen separately for
Whisper); titles, descriptions and tags the user types; CLI output, MCP tool descriptions, JSON-RPC and error
codes (machine interfaces, English); logs; product names (CaptureMann, YouTube, Google Drive, MCP, Whisper);
keyboard glyphs (⌥⇧R); the docs and website (a later, separate task).

Design for more languages from the start: plural forms, named placeholders, text direction left-to-right now
but nothing that blocks right-to-left later, a font fallback for scripts Geist does not cover.

---

## 3. Approach (recommended)

**Mechanism: gettext-style catalogues with English as the source text** (`.po` files), through a small headless
crate `cm-i18n`.

- Code keeps English in place: `tr!("Copy link")`, `tr_ctx!("button", "Open")` (context for short ambiguous
  words), `trn!("{n} recording", "{n} recordings", n)` (plural), named placeholders
  `tr!("Delete {title}?", title = t)`. English is therefore the fallback by construction and the code stays
  readable; the English catalogue is generated, not hand-written.
- Catalogues are `locales/<code>.po` (and `capturemann.pot` as the template), embedded in the binary; a missing
  translation shows the English text, never a key.
- Why `.po` over Fluent or a key-based YAML: the lowest migration cost for 1,900 existing literals (wrap in
  place), the best tooling for volunteer translators (Poedit, Weblate, Crowdin, plain text, pull requests),
  and plural forms are in the standard. Fluent is richer for grammar but Vietnamese has no plural or gender, and the
  migration cost is higher. This is decision D1.
- **Locale negotiation:** setting `appearance.language` = `system` (default) | `en` | `vi` | ...; `system`
  reads the OS language (the `sys-locale` crate, MIT/Apache) and falls back to English when the language is not
  shipped. Switching is **live** (no restart): a global locale plus an epoch bump re-renders every view; the
  HUD, toast and confirm windows read the same global; the tray and menu bar are rebuilt.
- **Daemon strings:** `ConfirmationRequest` and error payloads carry a stable `code`/`kind` plus parameters
  next to the English `message`; the app translates from `code + params` and falls back to the English message.
  The CLI and MCP keep printing the English message. The activity log stays English.
- **Formatting:** a small `cm-i18n::fmt` module (sizes, dates, day labels, numbers) driven by a per-locale
  table (decimal separator, short month names, today/yesterday). No ICU dependency.
- **Extraction and checks (CI):** a tool scans `tr!` invocations and regenerates `capturemann.pot`; tests fail
  when a `.po` has a stale or missing msgid, mismatched placeholders, or an empty plural form; a coverage report
  prints per-language percentages; a ratchet test counts UI literals that are not wrapped and may only go down.
- **Pseudo-locale** (`en-XA`: accented, 30 % longer, bracketed) to find clipping, hard-coded strings and
  concatenated sentences, plus a Vietnamese rendering check with long stacked-mark text.
- **Rules for translatable strings:** one sentence per message (no concatenating fragments), placeholders instead
  of string joins, no text in images or icons, a context for every short word, a glossary for product terms.

---

## 4. Phases

| Phase | Work | Effort | Parallel |
|---|---|---|---|
| **P0 Decisions and spikes** | **S1 fonts:** check whether the full upstream Geist covers Vietnamese; otherwise pick an OFL fallback with matching look (Inter is closest, Noto Sans is the safe choice), bundle it, make GPUI fall back per glyph, check line height for the stacked marks and clip risk in 40 px controls on a test sheet. **S2 IME:** implement input-method composition (marked text, commit, cancel) in `TextField` and the multi-line field through GPUI's input-handler interface, with grapheme-aware cursor and delete; test with the macOS Vietnamese Telex and VNI input sources by hand. **S3** pseudo-locale and layout stress harness. | M each | 3 agents |
| **P1 Foundation** | `cm-i18n` crate (catalogue loader, negotiation, `tr!`/`tr_ctx!`/`trn!`, formatting, tests), the `appearance.language` setting and a Language row in Settings → General, live switching, the English catalogue generated, the extraction tool and CI checks, the pseudo-locale. English output byte-identical to today | M to L | 1 agent |
| **P2 Wrap the UI strings** | Wrap the roughly 1,900 literals by area (sidebar and nav, library and details, publish and published, AI and agents, editors, recording surfaces and toasts, settings, launcher), refactor concatenated sentences into whole sentences with placeholders, replace the English-specific formatters, localise accessibility labels. English snapshots unchanged | XL (about 7 agents like the redesign) | yes, by area |
| **P3 Strings from the daemon** | Structured confirmation requests and errors, app-side translation by code, fallback to English; CLI and MCP unchanged and tested | M | 1 agent |
| **P4 Vietnamese** | Glossary (record, capture, library, export, publish, destination, agent...), a first draft of the catalogue, **owner review**, then an in-app pass with screenshots of every screen in Vietnamese and layout fixes | M plus your review time | after P2 |
| **P5 Native** | Tray and menu bar in the chosen language and rebuilt on change, `vi.lproj/InfoPlist.strings` and `CFBundleLocalizations`, a note that macOS permission prompts follow the system language | S | with P2 |
| **P6 Contributor workflow** | A translator guide in `CONTRIBUTING.md`, `locales/README.md`, the status report, and how to add a language (copy `capturemann.pot`, translate, add to the language list, add a font if the script needs one) | S | with P4 |

Order: P0 (S1, S2 can start now, they are independent of the rest), then P1, then P2 and P3 and P5 in parallel,
then P4 and P6. Roughly five agent waves, with P4 gated by your review.

---

## 5. Decisions

**Owner answers (Oct 23, 2026):** D2 English first (the app starts in English and changes only when the user picks a language; no automatic switch to the system language); D4 accepted (check upstream Geist, else Inter); D5 the owner reviews the Vietnamese; D7 IME first; macOS permission prompts are **not** translated (drop `InfoPlist.strings` from P5). D1, D3, D6, D8 stand as recommended.

| # | Question | Recommendation |
|---|---|---|
| D1 | Mechanism | `.po` catalogues with English as the source text (section 3). The alternative is Fluent. |
| D2 (answered) | **Default language.** "English is native and default": always English until the user picks, or follow the system language when we ship it? | Follow the system when it is a shipped language, otherwise English, with the choice always visible in Settings → General. If you want strictly English until chosen, `appearance.language` simply defaults to `en`. |
| D3 | Daemon-built texts (confirmation dialogs, errors) | Translate them in the app from code and parameters; CLI and MCP stay English. |
| D4 | Fonts for Vietnamese | Decide after spike S1: full upstream Geist if it covers Vietnamese, otherwise Inter as a bundled fallback (OFL, same licence family as Geist). |
| D5 | Who translates Vietnamese, and who reviews | I draft with a glossary, you review (you read Vietnamese); later languages come from community pull requests reviewed by a native speaker. |
| D6 | Formats | Localise day names, month names and the decimal separator (comma for Vietnamese); keep timecodes, units and shortcut glyphs as they are. |
| D7 | Priority of IME (S2) | Do it first: Vietnamese users cannot type titles or search without it, whatever the UI language is. |
| D8 | First languages to ship besides English | Vietnamese only for v0.1; the structure supports more. |

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| Vietnamese letters drawn in a different font inside one word | S1 before any translation work; a rendering test sheet with all 104 letters in every weight. |
| Vietnamese cannot be typed (no IME composition) | S2; manual test with Telex and VNI; keep a note in the release checklist. |
| Clipped diacritics or text overflowing fixed-size controls | Pseudo-locale and Vietnamese stress pass; raise line heights in compact controls; update the pinned layout constants and their tests together. |
| Concatenated sentences and word-order assumptions in English code | Rule "one sentence per message", reviewed in P2; the pseudo-locale exposes the rest. |
| Mixed-language screens while the migration is partial | English fallback is automatic; a coverage report and the ratchet test show what is left. |
| Translators change meaning (destructive confirmations, privacy facts) | Strings that state facts or confirm destructive actions are marked in the catalogue (`#. FACT`) and reviewed by the owner; the Privacy page copy stays the true statements. |
| Tests pinned to English | Tests run with the English catalogue; a separate small suite checks Vietnamese for placeholders and plurals. |
| macOS permission prompts follow the system language, not the app setting | Localise `InfoPlist.strings` anyway; document the limitation. |
| Contributor churn on the catalogue (merge conflicts in `.po`) | One file per language; the generated `.pot` is not hand-edited; CI regenerates and checks it. |

---

## 7. Definition of done

- Language can be changed in Settings → General (System, English, Vietnamese), live, and persists.
- With English selected, every screen is byte-identical to today (golden text snapshots).
- With Vietnamese selected, every screen, toast, dialog, tray item and accessibility label is Vietnamese or
  deliberately untranslated (brand names, shortcuts); the owner has reviewed every screen in screenshots; no
  clipped or fallback-font glyphs; Vietnamese can be typed in every text field with Telex and VNI.
- CI fails on a stale or incomplete catalogue, mismatched placeholders, or a new unwrapped UI literal.
- `CONTRIBUTING.md` explains how to add a language in under an hour.

---

## 8. As built (Oct 24, 2026)

| Phase | Result |
|---|---|
| S1 fonts | Upstream Geist and Geist Mono 1.8.2 cover every Vietnamese letter, so no second font and no fallback code; the app and `design/fonts/` use the new files (the kit's variable `.woff2` files for its HTML screens are still 1.2). `apps/desktop/tests/font_coverage.rs` fails if an embedded face lacks a Vietnamese letter. English metrics differ slightly (advance widths 1 to 4 %, taller default line height). |
| S2 IME | Text fields implement GPUI's input-handler interface (composition, candidate window position, marked text underline) and edit by grapheme cluster. Manual script `tests/manual/I18N-IME.md`; the owner has typed Vietnamese with it. Open: no undo in fields, Windows uses the IMM-style path. |
| P1 foundation | `crates/cm-i18n`: own `.po` parser, `tr!`, `tr_ctx!`, `trn!`, plural rules for en and vi, `fmt` (sizes, decimals, day and month names), `Lang` (`En`, `Vi`, pseudo-locale `EnXa`), a global language plus an epoch, extraction and checks (`cargo run -p cm-i18n -- extract | check | unwrapped | baseline | coverage`), the ratchet at 0, `// TRANSLATORS:` comments carried into the template, `locales/README.md`. Setting `appearance.language` (default `en`, app-only; no "System" choice: D2). |
| P2 wrapping | About 1,830 messages in `apps/desktop` and `apps/overlay`; contexts for short words; plurals through `trn!`; display text rendered at paint or rebuilt on a language switch; sticky toasts re-word through `ToastSpec::live`; the region overlay gets the language through `CAPTUREMANN_LANG`; the integration sweep routed daemon and provider errors through `daemon_text`. |
| P3 daemon | `ConfirmationRequest` and the event carry `kind` and `params` (additive); `apps/desktop/src/daemon_text.rs` builds the dialog text and the error text with an English fallback; errors use an id table in `cm_core::message` instead of a payload field (the wire format and exit codes are unchanged). Not converted: job failure reasons from FFmpeg or providers, recording warnings, publish validation messages, the delete dialog's remote-copy rows (see `design/i18n-STATUS-p3-daemon.md`). |
| P4 Vietnamese | `locales/vi.po`: 1,833 messages, every entry `#, fuzzy`, 137 sentences marked `# FACT: owner review required`, glossary `locales/vi-glossary.md`, review sheet `locales/vi-review.md`. The owner reviewed the glossary choices ("Quay" for Record, bản ghi and bản chụp) and the UI by eye; the full catalogue review is still open. |
| P5 native | Tray and menu bar relabel on a switch. macOS permission prompts are not translated (owner decision). |
| P6 contributors | `CONTRIBUTING.md` section "Translate the app", `locales/README.md`. |
| Layout | The overflow audit (about 36 fixes, helpers `label_fill`, `shrink_text`, `ellipsis_text`), `tests/layout_guard.rs` and the font-metric width tests (`design/i18n-STATUS-overflow.md`); nothing overflows in Vietnamese by measurement (it is usually shorter than English); the pseudo-locale overflows a few controls (stress result). |

Open items: the owner's full review of the Vietnamese; no layout pass with the pseudo-locale by eye; events whose text is still made English at the time (listed in the status files); Windows IME path; a typed code for "FFmpeg not found" (today a prefix match on the job reason); more languages from contributors.
