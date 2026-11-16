# QA 0.3.4 Boards look and Bulk export fixes (PARTIAL)

- **Date:** 2026-11-16
- **Who:** Claude (agent `boards`); nobody watched; no screen, no app run
- **Build:** worktree of `main` at `0e73b3c2` plus the uncommitted-then-committed changes of this branch (macOS dev machine, `--no-default-features`)
- **Method:** headless renders through `compose_board::render_board` (the ignored test `write_sample_boards`, synthetic 1800x1100 and 1900x1200 "screenshots"), PNGs looked at by eye; unit and integration tests with fakes. The Make a board and Export selected sheets were not opened.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Pair, English labels, title, captions, default options (light grey `#ECEDF0`, margin 40, radius 12, shadow) | PASS (by eye) | BEFORE and AFTER are white pills with a 2-3 px red `#DC2626` / green `#16A34A` border, bold letters, tilted -3 degrees, in the band above each picture (never on the screenshot); picture corners rounded and scaled with the size; one soft shadow per picture; title bold-looking and centred; captions centred under each picture; clearly roomier than the 0.3.4 build (margin 70, gap 42 at 1800 px sources) |
| Pair, Vietnamese words TRƯỚC / SAU and a Vietnamese title | PASS (by eye) | all diacritics (Ư, Ớ, Ơ, ƯỚC, Ó, Ả) drawn with the bundled fonts; the stamp widens with the word and shrinks if it would be wider than the picture |
| Pair on a dark background, no shadow | PASS (by eye) | stamps readable on dark; rounded corners |
| Grid of four with captions | PASS (by eye) | equal cells, centred captions, no shadow overlap |
| Spacing scale | PASS (test) | `spacing_grows_with_big_pictures_but_not_small_or_free_ones`: 300 px pictures keep the set gap and margin; 1200x800 gives 2x; 4000x3000 gives the cap of 3x; Free is never scaled |
| Stamps (test) | PASS | `pair_labels_are_coloured_stamps_in_the_band`: red and green pixels and a light pill in the band, for English and Vietnamese words, and the picture's own pixels untouched |
| Shadows do not darken the neighbour (test) | PASS | `a_shadow_never_darkens_the_neighbouring_picture` (all shadows are drawn before any picture) |
| Same bytes twice | PASS | `labels_numbers_captions_title_and_shadow_draw_something` compares two renders |
| Make a board button names why it is off | PASS (test only) | `board_disabled_reason`: fewer than 2, more than 12 (names the count), a recording or audio in the selection; shown as the tooltip of the disabled button; the sheet itself was not seen |
| Bulk: pattern with an unknown token or a stray brace | PASS (test) | the sheet blocks Export with a message before the job starts (it used to fail inside the job) |
| Bulk: a name that Windows keeps (CON, COM1, NUL.x) | PASS (test) | gets a `_` prefix on every system |
| Bulk: existing names never overwritten, cancel keeps written files, no partial file | PASS (existing tests re-run) | `tests/export_images.rs` 8/8 |
| `cargo test -p cm-render`, `-p cm-daemon`, `-p cm-desktop --lib` (1904), `layout_guard`, `cargo run -p cm-i18n -- check`, clippy 1.100 `-D warnings` on the three crates | PASS | see the hand-back |
| Sheet preview matches the output | not done | the canvas still draws its own stamps, has no shadow and no captions (audit P1-1); only its radius now follows the output |
| Windows and Linux | not run | |

## Verdict

PARTIAL
