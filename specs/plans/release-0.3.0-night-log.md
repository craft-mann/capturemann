# Release 0.3.0 night log

Timestamps are Vietnam time (UTC+7), Nov 8-9, 2026. Written by the night orchestrator.

- 20:24 Start. Six worker branches: media (4 commits), packaging (4), desktop (7), platform (1), recording (0), camera (0). Laptop load average 18 at start (builds running).
- 20:40 Merged media, packaging (4 commits), platform (1) into main; Mac fmt, clippy, target_os, md links green; tests running.
- 20:40 Decision (coordinator relay): youtube-1080p preset stays VP9+AAC in MP4 labelled "VP9, not H.264" (YouTube accepts it); real WebM/Opus goes to the 0.3.1 plan and the release text says so. Todo: O10 note in specs/features/18-import.md and 08-export.md; vi.po string; cm-regress import,export on the laptop in 3 VA-API situations; shared refactors (copy_ladder, editing_copy, ExportContents.vp9) checked on the Mac with cm-regress --dev.
- 20:41 VA-API diagnostics question (why 0.2.9 said software) given to the recording agent.
- 21:15 Merged recording (4), platform, packaging, desktop (14) into main (6131cdd2). Camera conflicts with recording in recording/sys/linux.rs; camera agent told to merge main and resolve. Mac tests on first merge (media+packaging+platform): no failure lines seen but output was truncated, re-running with a failure filter on the full merge.
- 21:15 Decision: one shared Wayland block; desktop agent switches first (window screenshots), then recording agent (Wayland A/V gate), then lead regression; lead restores X11 auto-login at the end.
- 21:15 VA-API root cause (T490 offers CQP only; export/import built `-b:v` only) given to media agent: shared probe helper from recording's choose_backend.
- 21:35 Merged media final (VA-API CQP probe, T490 import/export 10/10 in 3 situations) and packaging final into main. Mac test run on the previous merge still going.
- 21:55 Merged camera (final), packaging (text-model check, release-build.sh), recording, desktop again; added zune-jpeg dev opt-level; cargo deny ok; i18n ok. Old Mac test run stopped; a new full run starts on this commit. Laptop clippy on the earlier merge was green; tests there pending.
- 21:55 Gap decisions for the release text: no live webcam bubble during a take on Linux (portal cannot exclude windows; camera recorded and composited at export); camera track VP9 software; Wayland window screenshot per desktop agent; tell recording agent about the trailing mfra in fragmented VP9 MP4 for kill -9 recovery.
- 22:20 Laptop cargo test on main: cm-media quick_export (11), caption_export (3), cm-daemon export_real (2) fail on Linux after the VA-API merge; media agent resumed to fix. Mac result pending. Quiet hours (22:00) notified to all workers; sound cases of the final regression will be 'not run: quiet hours'.
- 22:25 Wayland shell-saved-file window screenshot (desktop 9aa36f64) run on the T490 GNOME 46 Wayland with the main debug build: Return -> exit 0, 480x270 capture saved in the library; Esc -> exit 5 platform_rejected with the 'picker ended without an image' message (not exit 3 user_declined). Lock released.
- 22:40 Version bump commit made early (Cargo.toml + Cargo.lock 0.3.0) so builds can be warmed up; docs/CHANGELOG/README of the release come in later commits; final packages are built from the last code commit.
