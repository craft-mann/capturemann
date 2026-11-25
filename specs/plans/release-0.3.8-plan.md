# Release 0.3.8: long audio made safe and measured, and a new audio editor (plan)

Status: **draft, written Nov 25, 2026 after the release of 0.3.7, for the maintainer's review.** The maintainer's direction of the same day: 0.3.8 to 0.4.0 focus on the audio module, features and UI/UX. The roadmap is `specs/plans/audio-roadmap-0.3.8-0.4.0.md` (proposal): **0.3.8** makes long audio safe and measured and brings the audio editor to the level of the screenshot editor (this plan; the design is the canvas page "0.3.8 Audio", boards A0 to A4, https://claude.ai/artifact/FiiEkZwwjNHzs14DB6Q6CJ), **0.3.9** makes it faster and cleaner (section 4), **0.4.0** adds the live and meeting features (section 4). Everything in 0.3.8 is a precondition of the later two: without flat memory and a quality gate their changes cannot be judged. Every item states the end state and names where it was found; numbers marked *target* need a stored report before the release text may use them. Reports are in `verification/` (`*-0.3.7-*`). Order of work is at the end.

## 1. Audio module (roadmap section 3)

Specs: `specs/features/05-audio.md`, `15-voice.md` (VOC-1, VOC-2), `07-ai.md` (AI-1, AI-8). Each item becomes acceptance criteria there before it is built.

### A1. Flat memory for long audio ("windowed audio", carried since 0.3.2)

- Transcribing and finding speakers decode the audio by window, never the whole file: at most one piece plus its margin (300 s, plus or minus 20 s) is held, and the piece cutter finds the quietest 100 ms from a small level index built in one streaming pass.
- Speakers always run by window (`speaker_engine/windowed.rs`, today used over 20 minutes), whatever the length, with the voice-print linking that keeps "Speaker 2" the same in hour 1 and hour 3. The whole-file run stays only as a measured comparison.
- The transcript and the speaker job never run at full size together: they run in sequence or share one decoded window.
- Windows are stitched without changes at the seams: no repeated or dropped word at a boundary and no speaker split in two at a boundary (the window overlap is raised, not the clustering threshold retuned; the 0.2.7 review measured 17 % to 36 % whole-versus-windowed disagreement as a stitching problem).
- Cancel works at any point and frees the memory at once.
- *Target:* a peak under 3 GB for a 4-hour recording with speakers on a 16 GB computer, with WER and DER within 0.5 points of today's on the existing long-audio reports (the 0.3.2 figure to beat is 11.7 GB for the 4 h 14 min Vietnamese meeting; the transcript alone was 4.3 GB). The benchmark is re-run with speakers on and its report stored.
- A long job shows its stage, percent, time left and memory in use, with a Cancel that works; partial results stay visible while it runs.

### A2. An audio quality gate in `cm-regress`

- Public, reproducible sets, with licences checked before use: English (the held-out clips already used from PriMock57, CC BY 4.0, plus AMI meeting excerpts, CC BY 4.0); Vietnamese (FLEURS vi_vn, CC BY 4.0, made far-field with room impulse responses, room noise and overlapping speakers, a synthetic mix as the review brief asked); hallucination probes (room-only clips, muted stretches and music beds, expected result no words).
- Reported per release and per system: WER, DER, the speaker-count error, words invented per hour of non-speech, time against real time, and peak memory. The reports go to `verification/benchmarks/`.
- The gate fails when a number is worse than the last release by more than a set margin, per language, so a change that helps English cannot silently hurt Vietnamese (the 0.2.7 speaker retune did).
- The maintainer's real meetings stay private: the gate reads a local folder of reference transcripts when one exists and never stores its content.
- The first run gives the first Vietnamese DER and the speaker-count error on the Vietnamese meeting (7 to 8 found for 6); these are measured in 0.3.8 and improved in 0.3.9.

### A3. One hallucination rule for every language (closes D4)

- Signals already computed are combined: the Silero speech share (`silero.rs`), the level against the room floor, Whisper's no-speech probability, average log-probability and words per second. Room-only audio has no speech reference, so the Silero share is the reference: a line over a stretch where Silero finds less than X % speech and the level is within Y dB of the floor is dropped or written as `[noise]`. X and Y are measured on the A2 probes and written into `specs/features/07-ai.md` with their numbers.
- The Vietnamese-only limit of the AC12 rule is removed once A2 shows English loses no real words (*target:* no more than 0.2 % of real words removed), on English and one more language.
- No phrase lists and no rule fitted to one recording (project rule). The known limit stays in the release text if no rule passes.

### A4. Spec and status hygiene

- AUD-1, AUD-2, AUD-3 and AUD-6 state the Windows and Linux facts as shipped (WASAPI process loopback, PipeWire, the 0.2.8 and 0.3.0 hardware runs); VOC-3 moves from Draft to a real spec (0.4.0, C1).
- "Windowed audio" is defined in the spec (A1), so the item stops being carried over without a definition.
- `specs/features/09-library.md` describes the 0.3.7 sidebar and filter bar (SB-1 to SB-7 of `release-0.3.7-plan.md`).

### A5. The live-mode hardware gate

- The 20-minute live-mode check runs on macOS (Apple Silicon and an Intel Mac), Windows (the ThinkPad) and Linux (X11 and Wayland), measuring CPU, memory, dropped live pieces and the time from Stop to the final transcript. Reports are stored. No user-facing change in 0.3.8; it unblocks 0.4.0 C1. (Intel Mac hardware: the maintainer provides it, otherwise the report says it was not run.)

### A6. The audio editor redesign (design page "0.3.8 Audio", boards A038)

Specs: `15-voice.md` (VOC-2), `07-ai.md`; the design page is the reference for layout and states, and the requirements below are what is built. The audio editor keeps every function it has; this is layout, states and consistency. Agents keep the same tools (`transcribe`, `diarize`, `set_speakers`, `suggest_cuts`, `cut_segments`); naming a speaker in the app and `set_speakers` write the same names.

- **AU-1 Transcript first (P1).** The transcript is the main column. The one-line caption card that repeats the transcript is gone. One player row at the bottom holds play, the time, the tools, zoom and the shortcuts button, above the tracks; the empty space under the timeline is gone.
- **AU-2 Turns, times and read-along (P1).** Turns are built from whole sentences: a new turn starts only when the speaker changes or after a pause longer than 2 s. Every turn shows its start time (click a time to jump there). While playing, the current turn has the speaker's colour on its edge, played words are bright, the current word is marked and the rest is dimmer; the view follows playback ("Following playback") until the user scrolls, and a click on a word plays from there.
- **AU-3 Speaker names and colours (P1).** Six calm colours in order (blue, green, amber, pink, violet, sky), one per speaker on the avatar, name, transcript edge and Speakers lane; the colour can be changed when naming. "Name this speaker" renames every turn of that speaker at once and says how many turns it renames; the Speakers panel lists each speaker's time and share. The Mic lane is labelled with the person's name (for example "Du Fei Lian's mic"), not the device name.
- **AU-4 Pause review (P1).** Suggested cuts are shaded on the waveform. "Review N pauses" in the player row opens a review bar: Pause i of N with its length and time, Hear it (Space, with a second either side), Keep (Tab, moves on), Cut (Enter), Cut all N, and a threshold of 1, 1.5, 2 or 3 s whose count updates. The timeline zooms to each pause in turn. After the review the bar says how many were cut and kept and the new length, with Undo (⌘Z); cuts are joined with a short crossfade and the transcript times move with them. Filler words ("um", "uh") in the same bar are later.
- **AU-5 Times, shortcuts, red (P1).** One time format everywhere: m:ss (0:52, 6:10), m:ss.d only in the player clock, h:mm:ss past an hour, no leading zero, and a range never ends past the total. One chord style (as 0.3.7); toolbar icons show their key in the tooltip, the full list is behind the keyboard button and "?", and nothing is shown twice. Red means recording only; selection uses the neutral highlight, as in the screenshot editor. Space, J K L, M, I, O and Backspace keep their meaning, and the Speakers, Mic and System lanes keep their order.
- **AU-6 Panel and menus (P2).** Panel tabs are labelled (Speakers, Minutes, Ask AI; markers are listed under Speakers with their times). Downloads are one **Save as** menu (TXT, SRT, VTT); **Re-generate** lives in the more menu and asks first, saying what will be replaced (fixed words and speaker names are kept). One surface per area with thin dividers, no cards inside cards.
- **AU-7 Misheard words and find and replace (P2).** Words with low confidence have a dotted underline; clicking one opens a small editor with Play, the word, **Fix this one** and **Fix all N**; fixes go into the transcript, SRT and VTT and never change the audio. Find and replace (⌘F, Ctrl+F) shows the match count, steps through matches and has Replace all.
- **AU-8 Suggested title (P3).** A small AI suggest button sits next to the title box (as YouTube offers). It runs only when pressed: the local text model suggests a title, shown for the user to accept or edit; nothing is renamed or generated on its own. The suggestion is made on this computer and uses the transcript when there is one; without a text model the button says so and offers the AI page.
- **Decided (maintainer, Nov 25, 2026).** Recordings with speech (video) get the same transcript-first layout later, with the video editor upgrade after 0.4.0; editing text to cut audio (delete words, the audio follows) is later; Minutes and Ask AI stay separate tabs; the Mic lane shows the person's name; the suggested title is a button, not automatic (AU-8).
- **States and text.** Every state on the design page (playing, misheard word, naming, find and replace, pause review, after the review) exists and has a test of its text and layout; every new string is translatable (English and Vietnamese) and holds under the pseudo-locale; every new control is reachable by keyboard and has an accessible name.

## 2. Needs from the maintainer

- **A2 data:** a yes for AMI excerpts and synthetic far-field FLEURS mixes, and a 5-minute hand-corrected stretch of a real Vietnamese meeting for a private check, kept out of the repository.
- **A1 and the last-word check:** consent for real recordings on the lab computers.
- **A5:** an Intel Mac for the live check.

## 3. Fixes

The "Known open items after 0.3.7" of `docs/development/release-day-0.3.7.md`, one item each, then the findings of the public check.

### 3a. Transcripts

- **Invented words over room-only noise (D4).** Closed by A3 (section 1).
- **The last word of a piece.** The check that every word has an end after its start runs on a long real recording on all three systems, and any piece that fails it is fixed in the engine. It needs the maintainer's consent for a real recording. (0.3.5 G3.)

### 3b. Quick Capture

- **Wayland placement.** On a Wayland session the bar opens centred on the display and the after-capture bar under the captured area wherever the portal or the compositor allows it; where nothing can place them the spec says where they open (today the top-left on GNOME, Linux limits and AC12). Checked on GNOME Wayland on the ThinkPad. (0.3.7 not done; Linux QA N-3.)

### 3c. Windows

- **Long Shot first step.** The first scroll step of Long Shot on Windows is not lost. The scroll-back that recovers it stays. (Windows F-9.)
- **Edge second tab (P-3).** A maximised Edge window with a second tab no longer ends Long Shot after 2 frames. It is tried again with the traces kept (`refused-NN.txt`, `trace.txt`); if it does not reproduce, the item is closed with the traces named in the report. (P-3, not reproduced in 0.3.7.)

### 3d. Library and the daemon

- **`list_library` kind `guide` with status.** `list_library` with kind `guide` and a status filter returns the same captures in the daemon, the MCP tool and the CLI as the library page shows for Click Guides with that status chip; a test covers the three. (Open since 0.3.7.)

### 3e. Translations

- **Vietnamese.** The 16 strings still untranslated (3660 of 3676 translated, `cargo run -p cm-i18n -- coverage`) are translated, and the draft strings (375) are listed for the maintainer's review. `cm-i18n check` passes.

### 3f. The regression suite (`cm-regress`)

- **`installer.silent` baseline.** The Start-menu check compares the entries after the uninstall with the entries before the install and flags only entries that appeared with the install. A shortcut that existed before the run (a lab dev install's `CaptureMann (dev).lnk`) is never reported as left behind. (Windows public check, Nov 25, 2026: the case failed on that shortcut and passed once it was moved away.)
- **Screen Recording not granted (macOS).** When the package under test has no Screen Recording permission, the cases that need it skip with a message naming the permission, instead of failing with `permission_missing`, and the report header says so. (macOS public check, Nov 25, 2026: 9 failures on an app copy in `/tmp`.)

### 3g. Release process and packaging

- **APT listing.** `apt list -a capturemann` on an amd64 computer does not list arm64 rows, or `linux-apt.md` says why it does (the sources file names both architectures). (0.3.6 QA, cosmetic.)
- **winget.** The pull requests 472327, 477195 and 483031 are open and no 0.3.7 pull request exists. This is an account matter; the state is recorded in the release text and the README once the moderators answer.
- **Public check on the DMGs.** The macOS regression of the release is run on the notarized DMG's app copy with a Screen Recording grant, as in 0.3.6; a `brew install` test runs in a throwaway location. (0.3.7 not run.)

## 4. After 0.3.8 (roadmap sections 4 and 5; not in this release)

**0.3.9, faster and cleaner audio:** GPU Whisper on Windows and Linux through Vulkan with a processor fallback, a setting that shows the device and a switch to force the processor, and flash attention and a quantised Turbo measured on the A2 set (B1); echo removal from the microphone track using the system track as the reference, as a post-process that leaves the masters alone (B2); compact FLAC masters after Stop (B3); speaker turns aligned to words and clustering with an estimated speaker count, adopted only where it wins on the A2 gate for both English and Vietnamese (B4); voice clean-up on export (B5).

**0.4.0, meetings and live:** live transcript as Preview (C1), recording one app's audio (C2), noticing when a meeting starts (C3), remembered voices as opt-in biometric data (C4), asking the library (C5), a slot for the CraftMann Vietnamese meeting model (C6).

## 5. Not in 0.3.8

Everything in section 4, new features beyond audio, and the 0.3.7 "Not run" list that needs a system the lab does not have: Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA and AMD encoders, KDE.

## 6. Order of work and exit

A2 starts first: it produces the baseline every other item is measured against. A1 follows, then A3 (its X and Y come from A2's probes). The fixes of section 3, A4 and A5 run in parallel on disjoint files; the Windows items (3c) and A5's Windows run need the ThinkPad on Windows, Wayland (3b) and A5's Linux run need it on Ubuntu. During development only fmt, clippy and the tests of the touched crates run. After development the maintainer tries the build by hand; every item is fixed or answered with a line in the release text. Then the full gate runs once on the final tree (fmt, clippy, tests, `cargo deny`, Trivy), the Windows package workflow, the Linux and macOS builds, and `cm-regress` on the final files of each system, stored in `verification/`.
