# Spikes S1 and S6 for Long Shot (0.3.4): state and plan

Written by Track C. Nothing here was run against a real screen or with real input injection;
that needs the maintainer's consent (it scrolls a real application).

## What is done headless (tested with fakes)

* The stitching engine, settle detection and the Auto and Hand loops
  (`crates/cm-capture/src/long_shot/`), CAP-6.AC1 to AC5 on synthetic frame sequences.
* A macOS scroller (`crates/cm-capture/src/sys/long_scroll.rs`): `CGEventPost` of pixel-unit
  scroll-wheel events (400 px per event at most), and `AXIsProcessTrusted`. Compiles; **not run**.
* Windows `SendInput` and X11 XTest: **not written**; `auto_scroll_support()` answers `NotBuilt`
  there, so Auto scroll is refused (`not_implemented`) and Hand scroll works wherever frames can
  be taken. Wayland answers `NoInjection` (Hand scroll only).

## S1: Auto scroll and settle detection on each system

Question: does Auto scroll (80 % of the scrollable content per step, the first step 50 %, then
wait for two equal frames 50 ms apart, 150 ms at most) give AC1 on three apps per system?

Needs the lab (a person at the screen, because the test scrolls real apps):

1. macOS (this Mac, Accessibility granted to `CaptureMann-dev`): Chrome with `long-page.html`, a
   native list (Finder list view with a few hundred files), Numbers or Excel. Run
   `capturemann capture screenshot --long --window <id>`.
   Measure per app: pixels moved by one 400 px event (it differs per app: acceleration, smooth
   scrolling), whether the settle limit of 150 ms is enough for smooth scrolling (Chrome ends its
   animation after about 250 ms: expect "not settled" frames; the stitcher still joins them when
   the frames are equal, otherwise it stops with "did not line up"), and the end detection.
   If one event does not move the pixels asked, the hint (used only to break ties) is wrong but
   the join is still found by the pixels, so the result is correct.
2. Windows 11 (ThinkPad T490): write `SendInput` with `MOUSEEVENTF_WHEEL` and `MOUSEEVENTF_HWHEEL`
   (`WHEEL_DELTA` 120 per notch; a notch is about 3 lines, so convert px to notches and measure),
   in `sys/long_scroll.rs`. Same three kinds of app.
3. Linux X11 (T490, GNOME on Xorg): XTest `fake_input` with buttons 4 to 7 (x11rb needs its
   `xtest` feature), the pointer must be over the area. Same three kinds of app.

Pass: AC1 on 3 apps per system. Fail: ship Hand scroll only on that system (the code already
does that through `auto_scroll_support`).

Open points the spike must settle: the pointer has to be over the area for a wheel event to reach
it (the daemon does not move the pointer today); whether `Window` targets capture only the
window (the area is then the whole window including fixed bars, which the band detection keeps
once); a window that resizes during the run (the engine stops with "did not line up").

## S6: frame source on Wayland (GNOME and KDE)

Question: can Long Shot (Hand scroll) take frames repeatedly without a prompt each time?

Code that exists: the recorder already keeps one PipeWire ScreenCast session open
(`sys/pipewire_linux.rs`, `sys/mutter_cast.rs`). What is missing is a `FrameSource` over that
session (take the latest frame and crop it to the area). That is a small change but needs a
Wayland desktop to test, so it is not written.

Needs the lab: GNOME 46 Wayland (T490) and a KDE Plasma session. Take 30 frames in 60 s and count
prompts. Pass: one prompt. Fail: Long Shot is shown disabled with the reason (REC-5 banner rule).
Until then the daemon uses `Capturer::capture`, which on Wayland asks the portal each time, so
Long Shot on Wayland must be treated as untested.

## Not part of the spikes, still open

* Track C3 (Nov 14, 2026): the overlay (Long Shot mode in a bottom bar, window highlight, Auto and
  Hand scroll, direction, Start, the Accessibility card), the HUD and side preview in the desktop
  app, the launcher command, menu item, hotkey row (no default) and `source: "long_shot"` with the
  frame count in the manifest are built and tested with pure models and fakes. Nothing of it was
  seen on a real screen when this was written; `tests/manual/CAP-6.md` steps 10 to 21 are the first
  run. The HUD and the preview are separate windows of the desktop app (the overlay process ends
  when the area is picked), excluded from capture, and on Windows non-activating
  (`WS_EX_NOACTIVATE`), so the scrolled window keeps the focus. The preview picture comes from a
  pull action, `long_shot_state` (counts and a small PNG, never logged), not from the bus event.
