# Release 0.3.7: Quick Capture on Linux and Windows, transcripts, test suite (plan)

Status: **draft, written Nov 22, 2026 after the release of 0.3.6.** The plan states final requirements only; each item names where it was found. Reports are in `verification/` (`*-0.3.6-*`). The order of work is at the end.

## 1. Quick Capture

Spec: `specs/features/21-quick-capture.md`.

- **X11 drag.** On X11 the bar can be moved by dragging its background. The window is an override-redirect window, so the window manager never moves it; the app moves it itself with the X server. (Linux final QA N-3.)
- **Wayland placement.** On a Wayland session the bar and the after-capture bar open where the user expects: the bar centred on the display and the after-capture bar under the captured area, as far as the portal or the compositor allows. Where nothing can place them, the spec says where they open. (N-4, N-5.)
- **Raised library window.** On Wayland a click on the library window does not hide the open bar; the bar stays above it or the spec says why it cannot. (N-6.)
- **Tab in the preset sheet.** Tab moves from the Name field to the next field of the Edit preset sheet on X11. (N-7.)
- **Open position on Windows.** The bar opens at the pointer without moving by 5 px on its first resize. (Windows F-4.)
- **Picker above the bar.** When the picker opens above the bar on Windows, the window rectangle does not jump to the top of the display; only the picker area changes. (Windows F-3.)
- **Keys with the picker open.** The spec states what plain 1, 2, 3 and Tab do while the picker is open (they do nothing today; only Ctrl+1 to Ctrl+4 and the arrows are specified), and the bar does it. (Windows F-2.)
- **The strip under the bar.** The empty strip under the bar (room for tooltips) does not take clicks meant for the windows below it; checked on macOS, Windows and Linux. (macOS K5.)
- **Log noise.** Closing the bar writes no "window not found" error line, and opening the recording bar writes no "RefCell already borrowed" error line. (macOS K3, Windows F-8.)
- **Boards intro.** The Boards intro card appears while its sheet is open and closes with it, or the plan of 0.3.6 (P-7) is reworded to say it appears after the sheet. (Windows F-7.)

## 2. Transcripts

- **Invented words over room-only noise (D4).** A 9-second Vietnamese clip that holds only room noise (about -60 dBFS) no longer gets an invented sign-off. The guard needs a measured rule for audio that has no speech reference; the rule is written into `specs/features/07-ai.md` with its numbers, and it is only adopted if it loses no quiet real speech on the silence and quiet-speech sets (`silence_probe`, `quiet_silence`, `outro_guard`). If no such rule exists, the limit stays and the user guide and the release text say so. (0.3.6 Known limits; Windows W-8.)
- **The last word of a piece.** The check that every word has an end after its start is run on a long real recording on all three systems. (0.3.5 G3.)

## 3. Messages and errors

- **FFmpeg not found.** The message that FFmpeg was not found names the right way to install it on each system: Homebrew only on macOS, the package manager on Linux, the bundled copy on Windows. The text lives in `crates/cm-core/src/message.rs` (lines 138 and 164) and `crates/cm-media/src/tools.rs` (line 38) and is shared today. Case `regress.ffmpeg_missing_wording` passes in a clean Linux root. (Linux aarch64 public check, issue 1.)
- **Ambiguous window error.** The error for an ambiguous window title lists no titles of windows of other applications beyond what is needed to choose by id. (0.3.5 G4.)

## 4. Windows

- **Long Shot.** The first scroll step of Long Shot on Windows is not lost. The scroll-back that recovers it stays. The maximised Edge window with a second tab that once ended after 2 frames (P-3) is tried again with the traces kept (`refused-NN.txt`, `trace.txt`). (Windows F-9, P-3.)
- **Licence list.** The Windows package carries the third-party licence list (open since 0.3.4).

## 5. The regression suite (`cm-regress`)

- `desktop.hotkey_overlay` skips when another program holds the global shortcut, by looking at the hotkey holder and not at a process name. (Windows 0.3.6 lab pass.)
- `ai.transcribe` takes its time limit from `--timeout-scale`; the fixed 600 s limit failed once on a busy laptop. (Windows lab pass.)
- `desktop.second_launch` does not count a copy that macOS runs from a translocated location as a second instance. (macOS public check.)
- The Windows rail and the other overlay windows have an accessibility name test that does not depend on a screen capture, because the overlay windows are excluded from capture.

## 6. Carried over

In-app update (Velopack spikes done, `specs/plans/release-0.3.3-plan.md`), windowed audio, WebP and HEIC image import, a headless daemon on macOS, Click Guide on Linux, and a winget package that gets accepted (pull requests 472327, 477195 and 483031 are open).

## 7. Order of work and exit

The transcript rule (section 2) starts first because it needs measurements. The Quick Capture items (section 1) and the messages (section 3) run in parallel on disjoint files; the Windows items (section 4) need the ThinkPad on Windows. During development only fmt, clippy and the tests of the touched crates run. After development the maintainer tries the build by hand; every item is fixed or answered. Only then the full gate runs, once, on the final tree, followed by QA passes and `cm-regress` on all three systems with reports in `verification/`, the push and the release. The speaker model is fetched from the mirror release `models-sherpa-2026-11-21` (never Latest); the CI jobs of Windows keep the cache and ffmpeg pins of `release-day-0.3.6.md`.
