# QA 0.3.5 pass 2, macOS, on the dev bundle built from 7cd4e31d (PARTIAL)

- **Date:** 2026-11-17. **Who:** Claude (QA agent, pass 2). **Machine:** Apple Silicon Mac, built-in display, macOS 26.x, screen unlocked, `caffeinate -d -i` on.
- **Build:** `make bundle-dev` of HEAD 7cd4e31d (version 0.3.5, bundle id `dev.capturemann.app.dev`), `codesign --verify --deep --strict` passed. Run from `target/bundle/CaptureMann-dev.app` with an isolated library, config dir, `CAPTUREMANN_SECRETS=file` and socket `/tmp/cmqa2.sock`. `~/Applications/CaptureMann-dev.app` (older, 17:25 build, not running) was refreshed with `ditto` and `lsregister -f` afterwards. `/Applications/CaptureMann.app` and the maintainer's library were not touched; no other process was stopped.
- **Method:** `capturemann` CLI, `cm-mcp` over stdio, captures of my own window. Keys (Tab, Return, Esc, Cmd+,) were posted to my own app's process with `CGEventPostToPid` to answer the Confirm card. A system-wide click injection (HID tap) was refused by the permission system, so **no pointer-driven UI work was done** (see Skipped). Pointer clicks posted to the process did not reach the app's controls (hover only).
- **Not done (no consent):** microphone, camera, system audio, any recording.
- **Privacy note:** the window list contains many private windows of the maintainer. I captured only my own windows by id. The BUG-9 error message prints the titles of other apps' windows to the caller; I do not copy them here. An "Open" file panel from a stray Return key of mine showed the Downloads listing once; I looked at it to find the Cancel button, deleted the picture and restarted my app instance.
- **Pictures:** `verification/QA-0.3.5-shots/macos-pass2-line-styles.png`, `macos-pass2-annotate-ops.png`, `macos-pass2-library-vi.png` (all my own window or images made from it).

## Regression (cm-regress, dev bundle, `--allow screen` only, no sound, mic or camera)

| Tier | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard | 51 | 1 | 12 of 64 | `L1-macos-0.3.5-dev-7cd4e31d-regression-standard-2026-11-17-claude.md` |
| full | 57 | 1 | 22 of 80 | `L1-macos-0.3.5-dev-7cd4e31d-regression-full-2026-11-17-claude.md` |

The one failure in both: `guide.edit_export`. The Markdown export now goes to a temp folder outside the trusted folders (BUG-3), so the headless daemon answers `user_declined / no_confirmer`. This is the intended new behaviour, and the suite case needs updating (export into the case's library folder, or expect the confirmation). Not a product defect. Skips: cases not implemented for macOS (`shot.window`, `redact.window_text`, Windows/Linux cases), `ai.transcribe`, `ai.speakers` (no model flags passed), `installer.dmg` (no installer given), `regress.ffmpeg_missing_wording` (system ffmpeg cannot be hidden), hardware-tier cases (not run). The full tier ran screen and cursor recording, region recording, minimal region and kill -9 recovery: all PASS.

## What I ran (QA pass 2)

| Item | Result | Evidence |
| --- | --- | --- |
| Tool count, MCP list | PASS | 89 tools; `create_board` and `start_guide` descriptions end "(preview) Part of ... a Preview feature" |
| `capturemann --help` marks board | PASS | `board ... [preview]` and `guide ... [preview]` |
| BUG-1 CLI-started guide shows a bar | PASS (partly seen) | `guide start --window <id>`, Confirm answered with Tab+Return: a 440x58 layer-101 window of the app appeared for the whole recording and went away a few seconds after `guide stop`. The bar is hidden from screen capture so I did not read its text ("Started by agent", Stop) or see the sidebar running state; those need a human look. Stop with 0 steps: "No step was taken, so no guide was kept." |
| BUG-2 Markdown names | PASS | `guide-2026-11-17-183850.md` and `images-2026-11-17-183850/step-01.png`, links in the .md point to that folder; a second export 7 s later made a second pair, nothing overwritten |
| BUG-2 0-step guide refused | PASS | `invalid_input: the guide has no step to export` for markdown and html (steps deleted with `guide set`, then `undo`) |
| BUG-2 Open folder on the toast | NOT SEEN | toast not visible to capture |
| BUG-3 existing file refused | PASS | `export_guide --format html --dest <existing file>` gives `invalid_input ... never writes over a file`; file content unchanged (also with a Confirm answered) |
| BUG-3 outside trusted folders asks | PASS | export to `/tmp/...`: no output after 5 s; Return declined: `user_declined`, reason `ask_first`, nothing written |
| BUG-3 inside the library runs at once | PASS | export into `<library>/exports` finished without a card |
| BUG-4 title from the accessibility name | NOT RUN | needs real clicks (see Skipped). Markdown escaping part: PASS, a title with emphasis stars and Markdown link syntax around the word page was written with every special character backslash-escaped, in the heading and in the image alt (the exact strings were edited out of this line so the link checker does not read them as a link) |
| BUG-5 moved window, other-window click | NOT RUN | needs real clicks |
| BUG-6 | skipped (Windows) | |
| BUG-7 board sheet hint | code and catalogue only | `board_sheet.rs` has "use the arrows to change the order"; `vi.po` "dùng mũi tên để đổi thứ tự"; the sheet itself was not opened (needs the pointer) |
| BUG-8 / About button | unit test only | `cargo test -p cm-desktop --lib feedback_url`: 2 passed. URL: `https://github.com/craft-mann/capturemann/issues/new?template=feedback.yml&version=0.3.5&title=CaptureMann%200.3.5%20general%20feedback` (template `feedback.yml` exists with fields version, feature, feedback). Version and title only. The Settings > About page was not opened by pointer; button code at `ui/settings/general.rs:345`; `Send feedback` has a Vietnamese entry (marked fuzzy in `vi.po`) |
| BUG-9 ambiguous window | PASS | `capture screenshot --window mann` (4 windows match) gives exit 2, `invalid_input`, `data.reason = ambiguous_target`, `matches` list with id, app, title and a message that says to pass the id; a word that equals the app name exactly (`capturemann`) still picks that window; a unique partial and an unknown text work as before (`not_found`). Over MCP `capture_screenshot` takes `window_id` only (no title text), so the title case is CLI-only there |
| BUG-10 transcript on a synthetic file | PASS (what is observable) | 1.5 s tone bursts, 3 s and 2.5 s of exact zeros; imported WAV, `ai transcribe` with `medium` (copied into the isolated config, removed after), `--language en --wait`: no error, 13 words, times in session ticks (bursts at 0, 540k and 1,020k ticks = 0 s, 4.5 s, 8.5 s, right), no text in the zero spans. Polish: the last word of each piece ("beep.") has start equal to end (zero length), seen 3 times on this synthetic file |
| BUG-11 | skipped (needs a long recording and the text model) | |
| ANN-12 styles | PASS (seen) | arrow, line, rect, ellipse with `dashed`, `dotted` and no style in one call: dashes and round dots as in the plan, arrow head solid, no-style ops solid (`macos-pass2-line-styles.png`). `style: "wavy"` and `"Dashed"` give `invalid_input` naming `ops[0]` with the allowed values. Schema lists `style` on the four ops. Editor three-button control: not driven (pointer) |
| Boards Preview | partly | `capturemann --help` and MCP marked; the Make a board sheet's Preview label not opened by pointer (code wires `PreviewFeature::Boards` at `board_sheet.rs:962,1117`). Boards created by the CLI: pair (5032x2133) and grid (5032x3480); `board` with one id is refused |
| Pass 1 areas: library | PASS (seen) | counts, chips (Boards 2, Click Guides 1, Imported 1), sort row |
| Delete confirmation | covered in pass 1; the Confirm card for guide export (Tab+Return accepts, Return declines) behaved the same |
| Annotate (retype with Vietnamese, rect, callout, step upper, stamp, text) | PASS (seen) | `macos-pass2-annotate-ops.png`, diacritics complete |
| Copy text (`text`) | PASS | text of the Settings page in reading order |
| Import | PASS | audio WAV import (job done); images pass 1 |
| Bulk export (3 shots, longest 800, JPEG) | PASS | 3 JPEG files written into the library exports folder |
| Long Shot, Click Guide on a real click | NOT RUN | needs pointer; the CLI-started guide had 0 steps because posted clicks are not seen by the click listener |
| Vietnamese UI spot check | PASS with one polish | set `appearance.language=vi` in the isolated settings and restarted (`macos-pass2-library-vi.png`): sidebar, chips (Đã nhập, Bảng ảnh, Hướng dẫn thao tác), sort row, "Hôm nay" all Vietnamese. Polish F5 (below) |

## Findings

| Id | Class | Finding | Steps | Expected | Seen | Reproduced | Known? |
| --- | --- | --- | --- | --- | --- | --- | --- |
| G1 | test suite (not product) | `guide.edit_export` fails on 0.3.5 | `cm-regress --tier standard` | export into a trusted folder or expect the card | `user_declined / no_confirmer` for a temp-folder dest (BUG-3 works as designed) | 2 of 2 runs (standard, full) | new |
| G2 | polish | Vietnamese sidebar tile "Hướng dẫn thao tác" wraps to two lines and the "Xem trước" pill touches the label and icon | UI language Tiếng Việt, look at the Long Shot and Click Guide tiles | one line or room for the pill | wrapped, pill overlaps the icon row | 2 passes (pass 1 F5 and now) | known (F5, D28) |
| G3 | polish | A transcript piece ends with a zero-length word ("beep." start = end) | transcribe the synthetic tone file | end after start | start = end for the last word of each of 3 pieces | 3 times in one run | new, may be older than 0.3.5 |
| G4 | polish | The ambiguous-target message and `matches` list titles of other apps' windows to any caller | `capture screenshot --window mann` | acceptable for the CLI/MCP user, but an agent sees private window titles | titles of 4 windows returned | once | by design of BUG-9; consider whether MCP callers should see titles |
| G5 | note | `capture_screenshot` over MCP has no window-title argument, so BUG-9 is CLI-only for screenshots | schema of `capture_screenshot` | plan says "same argument over MCP" | only `target.window_id` | once | check the plan wording |

No P1 (safety or data loss) finding. BUG-1, BUG-2, BUG-3, BUG-9, ANN-12 and the tool count behaved as specified in what I could run.

## Skipped and why

- Everything that needs a pointer or real clicks in an app window: BUG-4 title from the accessibility name, BUG-5 window move and click on another window, Settings > About look and the About button click, the Make a board sheet look, the editor line-style control, Long Shot and Click Guide on a page, Retype and the editor tools by pointer. System-wide click injection was refused by the permission system; pointer events posted to the process did not reach the controls. A human or a session with that permission should run these.
- BUG-1 text of the bar and the sidebar running state, BUG-2 toast Open folder (not capturable).
- BUG-6 (Windows only), BUG-11 (long recording).
- Hardware tier, microphone, camera, system audio, any recording by me (no consent), second monitor, Intel Mac.

## Cleanup

App instance stopped, my `caffeinate` stopped, `/tmp/cmqa2` (library, config, exports, the copied medium model, tools) and the socket removed, build and regression logs removed. I also removed one old leftover regression output folder from an earlier run on Nov 7 (`cm-regress-mac-20261107...` in the user temp dir) while cleaning; it held only suite temp output.

## Verdict

PARTIAL. 0 P1, 1 suite item (G1), 4 polish and notes.
