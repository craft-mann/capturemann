# FIRST-RUN-0.2 manual script: a clean Mac, the 0.2.0 app, no models

Covers what only a person on a clean machine can see: that the release app installs, that the voice
features work from nothing but the app and the downloads started on the AI page, and that models can
be removed again. The logic behind each step is tested with fakes (`cargo test -p cm-ai`,
`cm-daemon`, `cm-desktop`); the packaged app, the real models and the real engines have not been run
by anyone as a whole before this script. Report in `verification/` (template in its README); say
"verified" only with one.

Privacy and network. The only network traffic allowed is the install itself (GitHub or Homebrew) and
the model downloads you start on the AI page (Hugging Face). Watch with Little Snitch or `nettop`:
before step 4 nothing should leave the Mac, and after the downloads, a report, a translation or
speakers must show traffic only to `127.0.0.1`. Record only yourself, or people who agreed; delete
the test recordings at the end. Do not use your daily library.

Setup:

1. A Mac (Apple Silicon, macOS 13+) that has never had CaptureMann: no `/Applications/CaptureMann.app`,
   no CaptureMann folder in `~/Library/Application Support`, no models. A second user account on your
   own Mac is enough. Note the memory (a 16 GB and a 32 GB or more Mac differ in step 5), the macOS
   version and the free disk (about 25 GB if you try the 26B model).
2. The 0.2.0 DMG (`CaptureMann-v0.2.0-macos-arm64.dmg`; the app is about 181 MB) or
   `brew install --cask craft-mann/tap/capturemann`. Check the `.sha256` file first
   (`shasum -a 256 -c ...`). The Homebrew tap is updated only after the release is public.

Steps:

1. **Install and open.** Drag the app to Applications (or use brew) and open it. Expected: no
   Gatekeeper warning beyond "downloaded from the internet"; the permissions screen appears; nothing
   is requested until you press a button. `spctl --assess --type execute --verbose
   /Applications/CaptureMann.app` says accepted, Notarized Developer ID. Grant Microphone only.
2. **Record audio.** Record sheet, Audio, the Online meeting or In the room preset, microphone. Speak
   about a minute of English (two people if you can: the second voice makes step 7 meaningful), answer
   "How many people?", add a marker with M, stop. Expected: the recording is under Audio in the
   library; opening it shows the audio editor (there is no details page; **Reveal in Finder** is in
   the top bar).
3. **Transcript offer, no model.** Expected: with no speech model the app does not transcribe by
   itself and does not hide that: it offers to set one up and points to the AI page. Nothing is
   downloaded before you press Download.
4. **Download the speech model.** AI page. Expected: **Large v3 Turbo** carries the Recommended tag;
   its size and licence are shown before you confirm; progress; then it shows as installed. Transcribe
   the recording: the Transcript tab fills in. If "Find speakers after each transcript" is on
   (Settings > AI, the default) the Speakers lane fills in by itself afterwards (step 7).
5. **Download the text model.** AI page, Text model. Expected: **Gemma 4 E2B** carries Recommended.
   - A model that needs more memory than this Mac has free shows a warning.
   - **Gemma 4 26B-A4B** (about 15 GB) is listed only on a Mac with 32 GB of memory or more. On a
     16 GB Mac check it is absent. On a 32 GB Mac check its size, memory and licence are shown; the
     download is optional (if you do it, run step 6 once with it and write the time and quality down).
   Download Gemma 4 E2B. Expected: nothing to choose, it is in use when it arrives; Test says
   "Replied in N s", "Structured output works" and "only talks to 127.0.0.1".
6. **Report.** Report tab, Meeting minutes (Summary for one voice), Generate. Expected: the bar moves
   with the model's words, the report appears with times that jump to the moment. The "Write a
   report?" toast after a transcript works too (`AI-9.md` steps 15 to 17 cover the view).
7. **Speakers, automatic.** Expected: after the transcript, with no button pressed, the Speakers tab
   lists speakers and the lane shows blocks. No setup and no download: the two speaker models are
   inside the app (`Contents/Resources/Models/speaker/` holds two `.onnx` files, about 36 MB).
   Repeat once with Wi-Fi off on a new recording: it still works. A recording with "How many
   people?" answered uses that number. Do not write down an accuracy: it has been judged on one
   meeting only.
8. **Translation with the built-in model.** Transcript tab, Translate..., choose Vietnamese (or
   another language). Expected: with the text model in use it runs and the translation appears in
   the language switcher, traffic only to loopback. Translation to English needs the `large-v3`
   model (the turbo model cannot translate, and the message says so).
9. **Remove.** AI page: Remove the models you downloaded. Expected: the row goes back to Download
   and the file is gone from disk (check the folder the page names; the free space returns by about
   the size you downloaded). Removing the text model in use turns the text model off; a report then
   says where to set one up and sends no request.
10. **Quit and reopen.** Expected: models still listed as installed or removed as you left them, and
    the recording, transcript, report, speakers and translation are intact.
11. **Clean up.** Delete the test recordings and quit.

Pass: every expected result above, the first network traffic is the one you started, and the voice
features need nothing but the app and the downloads. Not covered here: Intel Macs, long recordings
(`VOC-1.md`), report quality (`AI-9.md` review set), speaker accuracy (`AI-8.md`).
