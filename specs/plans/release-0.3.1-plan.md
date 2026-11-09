# Release 0.3.1 plan: backlog that is not Linux-only

Status: **draft** (created Nov 8, 2026). Nothing built. Holds everything moved out of
[`release-0.3.0-plan.md`](release-0.3.0-plan.md), which is Linux-only. Platforms: Windows, macOS and Linux as each item
says; set the final scope after 0.3.0 ships.

Rules: same as the 0.3.0 plan (general techniques, no "verified" without a stored report, `cm-regress` per platform
before release, the maintainer pushes and signs).

## From the 0.2.8 backlog

1. **Windows: the Reveal button in the export toast opens the wrong folder** (window export of a recording). Leads:
   `ToastAction::RevealExport(id)` in `apps/desktop/src/toast.rs`, handled by `crate::ui::export_ui::reveal`
   (`apps/desktop/src/ui/app/toasts.rs`); check the path it opens (recording's exports folder versus the exported file,
   path separators, `explorer /select`). Add a regress case.
2. **Windows CI speed-up:** the Windows test job takes about 48 minutes (target 20) and `quick_export` alone 345 s. Same
   item: the flaky `exp4_ac4_cancel_deletes_partial_output` (`cm-media/tests/quick_export.rs`; cancel noticed after 0.44 s
   but the export took 31.8 s to wind down against a 20 s limit, CI run 42477256615, Nov 8, 2026): wait for the cancel
   signal instead of the clock, or give Windows CI a longer limit. Harden the NSIS install step in
   `windows-package.yml` with a retry (Chocolatey 5xx on Nov 7, 2026).
3. **Output-device chooser** in the editor (playback follows the system default; an external monitor with speakers can take
   it over).
4. **Spec decisions and small features:** `export_image` MCP tool needs a spec decision; the
   `capturemann://capture/{id}/cursor` MCP resource; macOS camera reconnect after replugging into another USB port.
5. **Wording:** the import summary messages in `import_ops` still say "Trash" on every platform (the capture-delete text
   uses `vocab::here(Term::Trash)` since 0.2.8); 86 Vietnamese entries are still fuzzy.
6. **Windows items not run for 0.2.8 and 0.2.9:** Windows 10, a mixed-DPI second monitor, NVIDIA and AMD encoders,
   SmartScreen on a real download, a phone A/V clap check, Intel Mac hardware; each a stored step or an explicit
   "not run" in the notes.

## From the 0.2.9 backlog

7. **MP3 and bare AAC import** (plan `mp3-import-plan.md`, asked for Nov 8, 2026): the bundled FFmpeg has no MP3 demuxer or
   decoder. Open decisions there: bare AAC (recommended yes), title from the ID3 tag, disk use. The question "0.3.1 or a
   0.2.10 for Windows and macOS" is now answered as 0.3.1 unless the maintainer pulls it forward.
8. **Disk use of audio recordings:** about 69 MB per minute on Windows (`online` preset, mic, denoised mic and system
   audio as 32-bit float) and 23 MB per minute on the Mac; a smaller stored format is a candidate (VOC-1 soak reports).
9. **Manual runs not done for 0.2.9:** `tests/manual/ANN-1-IME.md` (Telex and VNI), `tests/manual/HUD-1.md`, VOC-2
   screenshots; the HUD design questions the maintainer still has to answer.
10. **Windows details:** the file version field of `cm-desktop.exe` is empty.
11. **After 0.2.9:** winget for 0.2.9 (the 0.2.8 package, PR 472327, was still waiting), the website download links and the
    winget wording in the 0.2.8 notes.

## From the 0.3.0 triage

12. `mcp.policy_allow` failure (agents.tool_policy `allow` for `start_recording`), if the 0.3.0 triage finds it is not
    Linux-only.
13. Any other shared-code failure found by the Linux regression that is not Linux-only.

## Found during the 0.3.0 night work (Nov 8, 2026)

14. **`mcp.policy_allow` passes on the Mac** (`cm-regress --dev --tier standard`, 34 passed, 0 failed, Nov 8): the 0.2.9 Linux failure is therefore Linux-only; see the 0.3.0 triage note in `verification/` for its outcome.
15. **Real WebM/Opus output for the `youtube-1080p` preset on Linux without a hardware encoder.** 0.3.0 keeps VP9 + AAC in MP4, labelled "VP9, not H.264" (YouTube accepts it); O10 said WebM.
16. **Shared code changed by the Linux work, no-op on macOS and Windows, to review there:** `copy_ladder` / `EncoderFamily::for_os` in `cm-media`, the `editing_copy` field in the import outcome, `ExportContents.vp9` in `apps/desktop`, `cm-media` now depends on `cm-encode`, `Caps::vaapi_cqp`, `editing_copy_encoders` returns a 3-tuple; the tray "Recent captures" list polls the library every 10 s in shared app code (`5900e4a5`); `MoveSource::Xi2Raw` in the cursor schema (the REC-7 spec list and the editor readers do not name it yet); `futures-lite` dev-dependency of `cm-desktop`.
17. **Camera track on Linux is software VP9**; wiring it to the shared VA-API probe (`cm_encode::ffmpeg_vp9::choose_backend`) is open.
18. **Intel-only stimulus-measured sync** was measured on Wayland; an X11 stimulus measurement and other GPUs (AMD, NVIDIA) are open for Linux and for Windows NVIDIA/AMD.
19. **Sidebar Record stays clickable during a recording on every OS.** The Linux UI pass noticed it; only the tray disables Record. A change that dims the sidebar Record row, the rail button and the two "Record audio" buttons while a recording runs was written on a 0.3.0 branch (`fa225b6d`) and reverted because it changes macOS and Windows too. Decide the behaviour for all three systems.
20. **Linux first-run intro note** (`permissions::intro_note`) still says "Your desktop asks when CaptureMann captures the screen", true for the portal backend only; GNOME's screen-cast service records without a dialog. It needs a new msgid and a vi.po entry (the Settings -> Permissions page has the true note).
21. **Recording editor timeline draws no thumbnails on any OS** (the Screen row is a plain bar; found in the Linux UI pass, not a decode problem). A feature, not a bug fix.
22. **Editor warning row cuts `source_note`** ("Decoded from the recording wi...") at 1180 px on every OS: wrap or ellipsis with a tooltip (`apps/desktop/src/recedit/source.rs`, `ui/recording_editor/panels.rs`).
23. **Event-driven PipeWire screen source on Linux** (late frames of still pictures) is the same bug class as the Windows WGC "54 % late" fix; the Linux fix could not be reproduced with a sparse-damage window on hardware (a repainting test clip was used), so a still-picture window recording on the T490 is open.

## Found during the 0.3.0 release (Nov 9, 2026)

24. **`cm-regress` smoke and standard on the rebuilt 0.3.0 files** (`6cdcb033`): the stored regression ran on the first build (`5301d4b8`).
25. **APT repository:** published first as the flat layout in a GitHub release tagged `apt`; move to S3 + CloudFront at `apt.<domain>` later (the users' `.sources` file then changes). The repository key (RSA 4096, signing) expires **Nov 8, 2031**: rotate before then (`linux-apt.md`, "Rotating").
26. **Windows CI:** `exp4_ac4_cancel_deletes_partial_output` failed again on the 0.3.0 push (cancel noticed after 0.42 s, export wound down after 31.4 s; passed on re-run); same fix as item 2.
27. **Two Mac `cm-desktop` render tests** (`recedit::render::source_errors_keep_the_last_picture`, `worker_is_latest_wins_and_stops`) fail inside this repo's own `target/` (also after `cargo clean -p zune-jpeg`) and pass in a clean external target directory and in CI; find what in the repo `target/` causes it.
