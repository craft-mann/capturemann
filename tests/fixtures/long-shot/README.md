# Long Shot test fixtures (CAP-6)

The golden stitching tests need no screen. The generator is code, not files:
`cm_capture::long_shot::fake::SyntheticPage` (`crates/cm-capture/src/long_shot/fake.rs`).

* Every content row carries its own index in its first pixels, so a duplicated or missing row
  cannot pass: the check "every number appears once and in order" (AC1) is "the stitched image
  equals `page.expected()` row for row" (a row-hash and byte check; no OCR needed headless).
* `with_bands(header, footer)` adds a fixed header and footer (AC2).
* `with_jump(at, skip)` makes the content jump when a scroll reaches `at` (no overlap, AC3).
* `max_px` in the config makes a page longer than the limit (AC4).
* `.horizontal()` builds the same page on columns.
* `HandScript` replays the positions of a hand scroll (AC5, engine part).

Run: `cargo test -p cm-capture --lib long_shot` and `cargo test -p cm-daemon --no-default-features --test long_shot`.

## The real test page (manual and spike runs)

`long-page.html` is not written yet. When the spikes run (see
`docs/development/spikes-0.3.4-long-shot.md`) it should be a page 5 screen heights long with a
numbered line on every row (`Row 0001` ...), an optional fixed header and footer
(`?bands=1`), a jump (`?jump=1`: the list redraws with a gap after row 200), and a wheel script
(`?hand=1`: the page scrolls itself through its own JavaScript, for the Wayland case AC5).
The OCR check of AC1 reads the saved image with `recognize_text` and compares the numbers.

## `sticky-lazy.html` (BUG-6, no internet)

A tall page with a sticky masthead, a tab bar that sticks under it once the page has scrolled, and
numbered cards whose pictures load lazily: the shape of the YouTube channel and carchargeos.com pages
that ended after 2 frames on Windows (Edge). Open it in a browser (guest profile, `file:///...`) and
run Auto scroll on the window; on the lab laptop
`C:\lab\ls-site.ps1 -Url file:///C:/lab/sticky-lazy.html -Name sticky -W 1280 -H 900`.
