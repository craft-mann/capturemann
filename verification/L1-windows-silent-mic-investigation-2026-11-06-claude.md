# Windows 0.2.8: "spoke Vietnamese, heard nothing on playback" (PARTIAL)

Status: PARTIAL. Code reading and fakes only. Not reproduced: the investigation ran on a Mac with no access
to the Windows lab laptop (no C:\lab). Nothing here is "verified".

## Findings from the code
- rec.mic (cm-regress) only checked the mic track length, not its level. Its pass says nothing about signal.
  It now also fails on digital silence.
- Sheet to track path: sheet mic device (None = default) -> `audio.mic_device` -> `MicConfig.device` ->
  `CpalSource::open_input`; same path as the CLI. Denoise defaults to on; the editor plays the denoised file.
- The 0.2.8 idle-gap change (dba11436) applies only when `gap_reason == "idle"` (loopback). The mic uses "dropout". Ruled out.
- RNNoise probe (synthetic voice + noise, 0.001 to 0.3 amplitude): output peak about 1.2x input at every level. Ruled out for synthetic signals; real speech not tested.
- Editor playback (`recedit/audio.rs`, `cm-audio/playback.rs`): decodes the manifest mic file through FFmpeg; failures show
  "No sound in the preview" notes; output is the default device. No defect found, unverified on Windows hardware.
- Gap found: a mic that delivers exact zeros (Windows privacy switch, muted device) raised the `mic_silent` warning
  only after 15 s, so a short take got no warning at all.

## Change
- A mic with no non-zero sample since the recording began warns after 4 s (15 s kept after sound was heard).
  Warning text no longer states a duration. Test: `r3_21_a_mic_stuck_on_digital_silence_is_reported_once`.

## Needs hardware (not done)
Record mic only through the app, then check the bundle mic file level (ffprobe/astats) and play the same file in another player.
