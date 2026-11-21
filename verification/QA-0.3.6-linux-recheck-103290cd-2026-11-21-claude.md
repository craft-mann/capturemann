# QA 0.3.6 Linux recheck of N-1 and N-2, commit 103290cd, PARTIAL

- **Date:** 2026-11-21. **Who:** Claude (QA agent), driven from the Mac. **Machine:** ThinkPad T490, Ubuntu 24.04, GNOME 46, one display (1920x1080).
- **Files under test:** unsigned candidate built on the laptop from `103290cd0cd274bde9bb8e4ddcd592393f7c9e73` (`packaging/linux/release-build.sh --src ~/wt/main --commit 103290cd --jobs 3`, log `~/wt/release-103b-x86_64.log`, no warning or error lines). `capturemann_0.3.6_amd64.deb` sha256 `e348ff101917c29ec06f76610027e16621331024cdeb6933316efdd3eb75d661`; tarball sha256 `70c62eee4a8e5352b19cb02fd4361ce06f3cd94b7bfecb6045f11592c53331ca` (not installed). Installed with `dpkg -i`, tested from `/opt/capturemann/lib/capturemann` (`capturemann 0.3.6`).
- **Build note:** the first run stopped in the sherpa step: `fetch-models.sh: 3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx: wrong size, expected 28281164 bytes` (a download from GitHub; the same pinned file sits in `~/wt/qa57-sherpa-cache`). Second run used `CM_SHERPA_SRC_DIR=$HOME/wt/qa57-sherpa-cache` (the cache of the last pass) and passed in 19 min. No code was changed. The sherpa download failure looks like a one-off network event (curl reported the full 26.9 MiB); not investigated further.
- **Isolation:** private `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET`, `CAPTUREMANN_SECRETS=file` (the four starter presets in `presets.json`; `cm-regress` makes its own per case). Default library `~/Videos/CaptureMann` and `~/.config/CaptureMann/settings.json`: before 44 files, tree hash `846c94fb...90ee`, settings sha256 `ea37a984...0191`; after the same values: **unchanged**. `~/Pictures` has the same 25 entries as before (the portal pictures I made were moved into my scratch folder and deleted).
- **Input and pictures:** X11 with `xdotool` and `import -window root`; Wayland with a scratch virtual tablet and keyboard through `/dev/uinput` (keys and Ctrl chords), the bar opened from the tray item (dbusmenu id 16, `gdbus ... Event 16 clicked`), pictures from the desktop portal. Selected pictures: `verification/QA-0.3.6-recheck-linux-shots/` (22 files, `x11-*` and `wl-*`).
- **Session switch:** `rec-session.sh` method (AccountsService, `systemctl restart gdm3`): X11 to Wayland and back, each needed a second run because the first run reports the old type. No reboot. The laptop is left on Ubuntu, **X11**, with the 103290cd `.deb` installed.

## Results

| Item | X11 | Wayland |
| --- | --- | --- |
| N-1: Ctrl+1 on the open bar picks the first preset (Bug report: Capture, Screenshot, Region), the library is not raised (`studio brought to front` not in the log for the whole N-1 run), keys 2 and 3 work right after | **PASS** (`x11-n1-ctrl2-3-4-1-then-key2.png`: Ctrl+2 Docs shot, Ctrl+3 Demo video, Ctrl+4 Meeting, Ctrl+1 Bug report, then key 2 switches to Video) | **PASS** (`wl-n1-ctrl3-ctrl1-key3.png`: Ctrl+3 Demo video, Ctrl+1 Bug report, key 3 Audio; bar stays on top) |
| N-1 with the picker open: Ctrl+1 picks Bug report and closes the picker, bar keeps the keys | **PASS** (`x11-n1-picker-open-ctrl1.png`) | **PASS** (`wl-n1-picker-open-ctrl1.png`, key 2 works after) |
| Ctrl+2, 3, 4 (control) | PASS | Ctrl+3 PASS (2 and 4 not repeated) |
| N-2: picker, "Save current as preset...", Esc returns to the picker, 3 of 3 | **PASS** x3 (`x11-n2-iter1..3-grid.png`) | **PASS** x3 (`wl-n2-iter1..3-grid.png`) |
| N-2: bar keeps the keyboard afterwards | **PASS** x3: Up moves the picker highlight, Esc closes the picker (bar stays), 3 then 2 then 1 change the kind, Tab shows the focus ring and tooltip, Esc closes the bar | **PASS** x3 (the same sequence; no Esc at the end) |
| N-7: Tab in the Edit preset sheet | **Unchanged**: typed "XY", Tab, "ZW" gave "Bug reportXYZW" in the Name field (`x11-n7-edit-sheet-tab.png`); nothing saved | not run |

Notes for whoever reads the pictures:
- With the picker open the kind keys 1/2/3 do nothing (control on X11: P then 3 left Screenshot selected; Esc closed the picker). So "keys work" after N-2 means after the second Esc closes the picker; the first Esc after the name screen leaves the picker open, and Up/Down moved its highlight, so the bar had the keyboard in both states.
- The picker opens with the active preset highlighted (No preset when none is active). A test that presses Down a fixed number of times from the top lands on "Edit presets...", which closes the bar and opens Settings; my first scripted runs did that, and the bar then logged `closed itself` followed by `activation requested (studio brought to front)`. That is the Edit presets row, not N-1. I made the runs deterministic by picking Bug report with Ctrl+1 before opening the picker.
- Wayland: the bar still opens at the top-left (N-4, not part of this recheck).

## Regression (`cm-regress`, installed `.deb`, X11, `--commit 103290cd`, standard tier)

57 passed, 0 failed, 7 skipped of 64 (the same counts as the 57f50732 pass). Report: `L1-linux-0.3.6-regression-standard-x11-lab-103290cd-deb-2026-11-21-claude.md` and `.json`. The `cm-regress` binary used is the one already on the laptop (`~/lab/cm-regress`, 18 Nov), not rebuilt.

## Not done

Wayland N-7, Wayland Ctrl+2 and Ctrl+4, full and hardware tiers, aarch64, `.deb` install case, N-3 to N-6 (not part of the request), anything from the "what I did not do" list of the 57f50732 report.

## Cleanup

Removed on the laptop: my private config, library and shots, helper scripts (`~/lab/rc103-*`), the session environment file, my git bundle in `~/wt`, the `cm-desktop` process; no scheduled tasks were made. Kept `~/wt` (build cache, `release-out/103290cd-x86_64`, logs). Left the 103290cd `.deb` installed, session X11.
