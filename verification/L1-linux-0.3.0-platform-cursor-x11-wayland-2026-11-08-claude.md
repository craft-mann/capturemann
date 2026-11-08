# L1 Linux 0.3.0 platform: cursor track (REC-7), experiment and result

Result: PARTIAL.

- Machine: ThinkPad T490, Ubuntu 24.04, x86_64, GNOME 46. Two runs: an X11 session (first) and a Wayland session (later, after another agent switched sessions).
- Build: branch `track/linux-platform`, debug. Date 2026-11-08. Run by the Claude agent. Test: `crates/cm-cursor/tests/x11_real.rs` (`--ignored`), pointer driven by `xdotool` on the lab desktop.

## X11 (first run, before the 16 ms position poll was added)

Recorded by `CursorRecorder` on the XInput2 raw-event source: `Down Left (600,400)` and `Up` at the click point, `Scroll dy=-1` for wheel button 5, and a cursor image (`Cursor`, hotspot [4,4], scale 1). **Moves made by `xdotool mousemove` were not recorded**: that tool warps the pointer (`XWarpPointer`), which produces no XInput2 raw motion event. A 16 ms `QueryPointer` poll was added for warps (remote desktops, games do the same); that fix was **not re-run on X11** because the session was no longer X11 when the lab became free. Real hardware mouse motion was not tested (nobody in front of the laptop).

## Wayland (second run)

`preflight()` answers clicks and scroll `Unavailable` with a `not_implemented` warning that says why; starting the recorder returns `not_implemented` and leaves no `cursor.jsonl` behind. Not tried: positions from the ScreenCast portal's metadata cursor mode (cursor position/bitmap in the PipeWire stream). That belongs to the screen stream owned by cm-capture; the cursor crate has no access to it. The product therefore says there is no cursor track on Wayland (EDT-4 click effects and AI-3 auto-zoom need it).

## Not run

X11 moves after the poll fix, hardware mouse, modifiers on click, a recording alongside a screen track, a multi-monitor root-coordinate target.
