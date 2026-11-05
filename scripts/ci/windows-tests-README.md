# CaptureMann Windows test programs

Built by the workflow `.github/workflows/windows-tests.yml` (`BUILD.txt` has the commit). These are **test
programs**, not a release: they were compiled on a hosted runner and are meant to be run on a real Windows machine
(the lab laptop) because they need a screen, a microphone, a camera or a hardware H.264 encoder. `PROGRAMS.txt`
lists every program in this folder. Program names are `<package>__<target>.exe`; `<package>__lib.exe` is a
library's unit tests; `example__*.exe` are the `cm-daemon` examples (release build).

The Visual C++ runtime DLLs next to the programs are copies from the runner (the programs link the dynamic C
runtime). Keep them in the same folder.

## Rules

- Hardware tests record your screen, camera or microphone: run them only when the owner of the machine asked, keep
  them short, and delete what they write. They are `#[ignore]` and print `SKIPPED` when a device is missing.
- Run them **one at a time**: add `--test-threads=1`. Add `--nocapture` to see what they print.
- Nothing here is "verified" until a report is stored in `verification/` (template in its README): name the
  machine, the Windows build, the commit in `BUILD.txt`, and paste the printed output.

## Where to run: ssh or the logged-in session

A command run over ssh lives in **session 0**, which has no desktop: it cannot capture the screen, list or
screenshot windows, open the camera or (usually) the microphone. Use `scripts/testlab/win-run-in-session.ps1` on the
laptop, which starts the program in the logged-in user's session and collects its output:

```powershell
C:\lab\win-run-in-session.ps1 -Exe C:\lab\tests\cm-capture__real_display.exe `
  -Arguments '--ignored --nocapture --test-threads=1' -OutFile C:\lab\out\real_display.txt
```

| Needs the desktop session (use `win-run-in-session.ps1`) | Fine over ssh (session 0) |
| --- | --- |
| everything that captures the screen or a window, the camera, the microphone, system audio, the cursor hook, the parity test (needs a GPU adapter) | `*__lib.exe` unit tests, `cm-media__*` tests, `cm-encode__mf_real.exe` (no window needed, the encoder is a Media Foundation transform; if it reports no hardware encoder over ssh, run it in the session too) |

## Commands

`<exe> --ignored --nocapture --test-threads=1 <test name>` runs one ignored test. Without a name it runs every
ignored test of that program.

| Program | Test name(s) | What it needs |
| --- | --- | --- |
| `cm-capture__real_display.exe` | `cap1_ac1_real_display_matches_reported_physical_size`, `cap1_ac2_real_region_has_requested_size`, `cap1_ac4_real_window_matches_bounds`, `cap3_real_list_targets_has_no_shell_windows`, `r4_6_display_screenshot_with_own_windows_excluded`, `cap1_windows_lists_and_captures_a_started_window` | session; the last one starts and closes Notepad |
| `cm-capture__real_window_thumbs.exe` | `every_listed_window_can_be_captured_for_a_thumbnail`, `window_thumbnail_of_the_first_three_windows` | session; open a few windows first |
| `cm-capture__record_real.exe` | (the macOS recording tests are `cfg(macos)`; the Windows ones, when present, are listed by `--list --ignored`) | session |
| `cm-encode__mf_real.exe` | `five_seconds_of_1080p30_make_a_valid_fragmented_mp4`, `a_size_no_encoder_accepts_is_the_unavailable_error` | a hardware H.264 encoder (the second needs none) |
| `cm-audio__hardware.exe` | `lists_devices_and_permission_state`, `mic_records_three_seconds_and_stamps_with_the_os_clock`, `monitor_shows_a_level`, `devices_open_without_streaming`, `system_audio_records_four_seconds` (play a tone), `system_audio_hears_a_chime_played_by_another_process` (**plays a sound** through the speakers) | microphone, loopback; session |
| `cm-camera__windows_camera.exe` | `lists_cameras_with_modes`, `default_camera_delivers_steady_stamped_frames` | a camera; session; the camera privacy switch on |
| `cm-render__ocr_real.exe` | `real_recognizer_finds_two_emails_and_a_key_on_a_synthetic_png` | set `CM_OCR_REAL=1`; an OCR language installed |
| `cm-desktop__parity.exe` | (no `--ignored`) `--nocapture` | a GPU adapter, and a **full** `ffmpeg` and `ffprobe` on `PATH` (for example `winget install Gyan.FFmpeg`; the bundled LGPL build lacks the `select` filter). Without them it prints SKIPPED. `CM_PARITY_KEEP=<dir>` keeps the frames |
| `cm-ai__real_model.exe` | `real_model_transcribes_speech` (prints a `SPEED` line), `real_model_cancel_works_and_a_normal_run_is_not_aborted` | `CM_WHISPER_MODEL` (a ggml file), `CM_WHISPER_AUDIO` (a 16 kHz mono 16-bit WAV is read directly; other formats need `ffmpeg` on `PATH`), optional `CM_WHISPER_REFERENCE`, `CM_WHISPER_LANGUAGE`, `CM_WHISPER_GPU=0`. Over ssh is fine (no desktop) |
| `cm-ai__real_diarize.exe` | `real_models_tell_speakers_apart` | `CM_SHERPA_MODELS` (folder with the two `.onnx` files), `CM_DIARIZE_AUDIO` (16 kHz mono 16-bit WAV), optional `CM_DIARIZE_SPEAKERS=2,3,auto`. Copies the models into a temp folder (no symlink on Windows) |

To list what a program holds: `<exe> --list` (all) or `<exe> --list --ignored`.

## End-to-end recording and fixtures

```powershell
# a synthetic recording (no screen, camera or microphone): for the editor, export and playback checks
.\example__make_fixture.exe --library C:\lab\fixture-lib

# the real recording session (screen and cursor by default; add mic, camera, system only when asked)
.\example__record_e2e.exe --tracks screen,cursor --secs 10
.\example__record_e2e.exe --tracks screen,cursor,mic,system --secs 20 --pause-at 8 --pause-for 3 --fps 30
.\example__record_e2e.exe --kill9-test      # ends the recorder with a hard kill, then recovers the bundle
```

`record_e2e` never prompts for a permission and deletes its bundle unless `--keep`. The 10-minute 1080p30 gate and
the `taskkill /F` recovery step are in `tests/manual/REC-WIN.md`.

## Speech and speed of the whisper model (`cm-ai__real_model.exe`)

`scripts/testlab/win-make-speech.ps1` makes the audio with the voices Windows already has (no download): a
two-speaker English clip `speech2.wav` (16 kHz mono 16-bit), the same words as one voice, and `speech2.txt`, the
reference text.

```powershell
C:\lab\win-make-speech.ps1 -OutDir C:\lab\speech          # speech2.wav, speech1.wav, speech2.txt, speech1.txt
$env:CM_WHISPER_MODEL     = "$env:APPDATA\CaptureMann\ai\models\ggml-medium.bin"   # the app's medium model; spaces in a path are fine
$env:CM_WHISPER_AUDIO     = 'C:\lab\speech\speech2.wav'
$env:CM_WHISPER_REFERENCE = 'C:\lab\speech\speech2.txt'
C:\lab\tests\cm-ai__real_model.exe --ignored --nocapture --test-threads=1 real_model_transcribes_speech
```

It prints one line to quote, for example
`SPEED audio_s=62.3 wall_s=41.0 rtf=0.658 model=ggml-medium.bin threads=8 gpu_requested=true`
(`rtf` is wall time divided by audio time; below 1 is faster than real time), then the text and the word error rate.
The build is CPU only on Windows (no Metal); `CM_WHISPER_GPU=0` changes nothing there but is harmless. For the
speaker test: `$env:CM_DIARIZE_AUDIO='C:\lab\speech\speech2.wav'; $env:CM_SHERPA_MODELS='<folder>'`. Synthetic
voices are not a substitute for a real recording when judging accuracy; quote them as "synthetic speech".
