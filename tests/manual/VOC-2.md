# VOC-2 manual script: the audio editor and Export audio (boards V4 and V7)

Covers what only a person can check: how the audio editor looks and feels (the card, the lanes,
the overview strip, the Transcript tab in wide mode), playing and seeking by ear, the keys, the
Export audio sheet and the files it writes. The pure logic is covered by `cargo test -p
cm-desktop --lib voice` and `audio_export` (turns, markers, waveform columns, lanes, keys, sheet
choices and size estimate), the headless editor over a real daemon by `--lib audio_tests`, the
layout by `--lib layout_width_tests`, and the export itself by the golden tests of
`cm-media` (`cargo test -p cm-media --test audio_export`, `cm-daemon --test export_audio`).
Behaviour: `specs/features/15-voice.md` (VOC-2). Boards: `design/redesign/screens/png/VoiceEditor.png`
and `VoiceExport.png`. Status: built, tested with fakes, **never seen by eye** before this script
is run.

Nothing here records the screen or the camera. A microphone is not needed either: use the
synthetic fixture for a recording with a mic and a system track, a pause and a cursor path
(`cargo run -p cm-daemon --example make_fixture -- --library <dir> --secs 60`), or any audio
recording of yours. Point a second instance at it (`CAPTUREMANN_LIBRARY=<dir>`
`CAPTUREMANN_CONFIG_DIR=<dir2>`; never your own library). For the transcript steps transcribe it
from the AI page with a downloaded model; for the speaker steps wait for the automatic Find speakers after the transcript (Settings > AI,
on by default; the two speaker models are inside the app, nothing to download), or press Find
speakers in the Speakers tab, or run `capturemann ai speakers <id>`; a recording made with "How many
people?" answered on the Record sheet uses that number. Open a recording of
`kind: "audio"` (a recording made with the Audio mode of the Record sheet, ⌥⇧V). Report in
`verification/` (template in its README). Delete anything you exported.

1. **The page.** Open the recording. Expected: header with "Audio" over the title, a chip with
   the length, **Reveal in Finder** (it replaced Recording details: an audio recording has no details
   page), Upload and **Export audio...** (the primary, light button), no Publish. Press Reveal in
   Finder: Finder shows the `.cmrec` folder. Opening a recording from the Audio view of the library
   goes straight to this editor, never to a details page.
   Under it the "now speaking" card, the timeline card, and the inspector on the **Transcript**
   tab about 560 to 600 px wide. The inspector rail has six tabs in this order: Speed,
   Transcript, Speakers, Report, Suggestions, Output.
2. **Now speaking.** Press Space. Expected: the card shows the speaker (avatar in the speaker's
   colour, name, the time the line began), the current line at 20 px with the word being said
   marked, the time readout `m:ss.f / m:ss.f`, and the key hints (Space, J K L, M, ⌫). Pause in
   a silence: the card keeps the last line. A recording without a transcript says so.
3. **Lanes.** Expected: a ruler, the **Markers** lane (white flags with a bookmark and the
   label), the **Speakers** lane (coloured blocks; the colours are `#8FB0FF #7FD6C2 #F2B880
   #C9A7F5` and then repeat, never red, green or amber; empty without speakers), one waveform per
   track (Mic, System) with the played part brighter than the rest, and the playhead. Scroll
   with the wheel to pan, Cmd/Ctrl and the wheel to zoom (or the toolbar buttons): the ruler,
   flags, blocks and waves follow.
4. **Seeking.** Click on a lane: the playhead moves. Drag on the ruler or a lane: a range is
   selected (blue box). Click a marker flag: the playhead goes to the marker.
5. **Keys.** Space plays and pauses. **J** jumps back 5 s, **K** pauses, **L** plays and, while
   playing, jumps ahead 5 s (it is not the video editor's loop). **M** adds a marker at the
   playhead: a flag appears within a second; ⌘Z removes it. Record with a pause first if you can
   and check a marker set after the pause lands on the same sound (pauses are removed from the
   recorded clock the marker uses).
6. **Cuts.** Select a range and press Delete: it is hatched grey on the lanes and the playing
   skips it (the Length in the Output tab shrinks). R restores. I and O set the in and out points
   (the ends are dimmed).
7. **Suggested cuts.** Suggestions tab: only **Suggest cuts** (no zooms). Press it. Expected: the
   proposals are hatched amber on the lanes with "Suggested cut: {what} at {time}" under them;
   Accept all makes them cuts in one undo step.
8. **Overview.** The strip under the lanes shows the whole recording, a box for what the timeline
   shows, a tick for each marker and the playhead. Click it: the view centres there.
9. **Transcript tab.** Expected: the title, "{language} · {n} speakers · click a time to jump
   there", the language switcher (the original with "original", then each translation with
   "translated") and **Translate...**. Turns show the avatar, the name (a speaker nobody renamed
   is "Speaker N" in its colour), a mono time chip, and "overlap" in an amber chip where people
   talked at once; markers appear as dividers with their time between turns. The line under the
   playhead is highlighted and the list follows it while playing, and leaves you alone for two
   seconds after you scroll. Click a time chip or a line: the playhead goes there.
10. **Translate.** Press Translate..., choose a language. Expected: the chooser closes, a chip
    "{language} ..." appears while it runs, and "Translation ready: {language}" arrives as a
    toast with the new language in the switcher. With the turbo model installed and no text model
    the call fails with the daemon's own message in a toast (nothing is hidden). Switch to the
    translation: the lines change, the turns follow the speakers' times.
11. **Footer.** TXT, SRT and VTT save a file in the recording's `exports/` and a toast says
    where (with Show in Finder). With a translation shown the file is in that language. **Speaker
    names** off: the SRT has no names. With no speakers the switch is dimmed.
12. **Export audio.** Press **Export audio...** (or Output > Export audio...). Expected: a sheet
    580 px wide over a dimmed editor: format cards (M4A chosen), tracks (Mix chosen; a track the
    recording does not have is dimmed), "Even out the loudness" on for M4A and off for WAV until
    you flip it, the tags line with the title and date, "Also save a copy to", and the footer with
    "{time} after cuts · about {size}". Choosing Mic only changes the estimate. Esc or the scrim
    or Cancel closes it.
13. **The export.** Choose a copy folder (the system picker) and press **Export M4A**. Expected:
    the sheet closes, a progress toast "Exporting audio..." and then "Audio exported" with the file
    name, size and `-16 LUFS`, **Show in Finder** (the file in `exports/`) and **Upload**. The
    copy is in your folder (never overwriting). Play the file: the length is the length after the
    cuts (±20 ms), Mic only has no system audio, a WAV is 48 kHz 24-bit. Upload with no destination
    configured shows the daemon's message in a toast.
14. **Export audio only on a screen recording.** Open a screen recording that has a microphone:
    Output tab > **Export audio only**, and the details page's Exports card > **Export audio
    only**. Expected: the same sheet over the editor, the same cuts, no video encoded.
15. **Screen recordings.** Their rail has eleven tabs: the Speakers and Report tabs are there
    with a short "coming here" note, nothing else changed.
16. **Narrow and light.** Resize the window to its minimum: the inspector shrinks before the
    left column does and nothing is cut off. Switch the theme to light and back: the lanes,
    chips and the sheet stay readable. `CAPTUREMANN_PSEUDO_LOCALE=1`: no label is clipped.
17. **Vietnamese.** Switch the language: the tab tooltips, the lanes' names, the sheet and the toasts
    are in Vietnamese and fit.

Pass: every expected result above, the keys work with the focus on the editor, nothing is
recorded or sent anywhere except the Upload you ask for.
