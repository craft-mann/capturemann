# L1 Linux 0.3.0 triage of the 17 failures of the 0.2.9 Linux hardware run (PARTIAL)

- **Source of the list:** `verification/L1-linux-0.2.9-regression-hardware-x11-hw-nosound-2026-11-06-claude.md` (35 passed, 17 failed, 25 skipped of 77 cases; Ubuntu 24.04, ThinkPad T490, Intel UHD 620, GNOME 46, X11).
- **Run that decides each outcome:** `L1-linux-0.3.0-regression-hardware-x11-x11-hw-nosound-2026-11-08-claude.md` (cm-regress hardware tier, `--allow screen,mic,camera`, no sound; the package files built from commit `5301d4b8` by `packaging/linux/release-build.sh`, extracted tarball): **58 passed, 0 failed, 20 skipped of 78 cases.**
- **Written by:** the night orchestrator, Nov 9, 2026. Status PARTIAL: one computer, X11 only for this table (Wayland results are in their own reports), no sound cases.

| # | Case (0.2.9 failure) | Outcome | Reason |
| --- | --- | --- | --- |
| 1 | `pkg.files` | **fixed, test expectation corrected** | The suite looked for `LICENSE` and `THIRD_PARTY_LICENSES` in the wrong place (Linux packages keep them under `share/doc/capturemann`), and the `.deb` really lacked `LICENSE` (now shipped). Passes. |
| 2 | `import.formats` | **fixed** | No hardware encoder was found (VA-API was never tried) and nothing else could prepare the editing copy. Now VA-API H.264 with a probed rate mode (the T490's iHD driver offers CQP only), software VP9 as the labelled fallback. Passes. |
| 3 | `import.rotated` | **fixed** | Same cause and fix as 2. |
| 4 | `import.vfr` | **fixed** | Same cause and fix as 2; the case accepts hardware H.264 or a labelled VP9 copy on Linux. |
| 5 | `export.quick` | **fixed** | The export used `-b:v` only, which the iHD driver refuses (CQP only), so it fell back to VP9. Now the probed mode is used; the T490 exports H.264 through VA-API. Passes. |
| 6 | `export.presets` | **fixed** | Same cause and fix as 5. (The `youtube-1080p` preset without a hardware encoder is VP9 + AAC in MP4, labelled; a real WebM is a 0.3.1 item.) |
| 7 | `mcp.policy_allow` | **fixed** | The failure was the portal dialog of the recording start waiting for a click. With the GNOME screen-cast service there is no dialog, and the daemon validates the display first. Passes on X11 (also `mcp.confirm_tier`). On a computer with no desktop session both cases now skip (a headless VM answers `platform_rejected`). Passes on the Mac. |
| 8 | `rec.screen_cursor` | **fixed** | "Killed by a signal": the harness timed out on the portal's Share Screen dialog. Recording now runs without it on GNOME; VA-API H.264, constant 30 fps. Passes. |
| 9 | `rec.region` | **fixed** | Region recording was `not_implemented` on Linux; built. Passes (exact size). |
| 10 | `rec.region_min` | **fixed** | Same; 18x18 and 20x20 record with the real encoder. Passes. |
| 11 | `rec.kill9_recovery` | **fixed** | Failed before the start (portal dialog). Now the next start recovers the bundle and loses under 3 s. Passes (also measured on Wayland: under 1 s lost). |
| 12 | `rec.mic` | **fixed** | "Killed by a signal", same cause as 8. Passes. |
| 13 | `rec.camera` | **fixed** | `cm-camera` had no Linux backend; a Video4Linux2 backend was written. Passes (1280x720, 30 fps, built-in webcam, lit room). |
| 14 | `linux.desktop_stays_up` | **test fixed** | "left programs running": the suite counted a zombie as a running program. Passes. |
| 15 | `linux.second_launch` | **test fixed** | Same cause as 14. Passes. |
| 16 | `linux.tray_icon` | **test fixed** | Same cause as 14. Passes. |
| 17 | `linux.vaapi_encoder` | **fixed** | The driver's CQP-only mode (see 5) made diagnostics report software encoding; fixed with the shared probe. The case itself now reads only the "video encoder" line (the camera line says "software VP9" by design). Passes. |

Lab limits that remain (not failures): see the skipped cases of the report (sound, second monitor, 4K external, soak, models, installers
in the hardware tier; the installer cases run in the full tier with `--installer`).
