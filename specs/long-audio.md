Treat a long file like a long recording: split it into chunks, process them in the background one at a time, save each result as soon as it's done, and show the transcript as it grows. Never load or transcribe 2–4 hours in one pass. Here's the pipeline I'd use.

**1. Decode once, then stream.**
- Convert the MP3 with your FFmpeg sidecar to a 16 kHz mono 16-bit WAV in the app's cache. That's about 115 MB per hour (2 h ≈ 230 MB, 4 h ≈ 460 MB).
- Read it in windows from disk instead of keeping all the audio in memory, so memory stays flat whatever the file length.
- Check free disk space first, and delete the temp WAV when the job finishes.

**2. Split on silence, not at fixed times.**
- Run voice activity detection (Silero VAD, already supported in whisper.cpp) across the file first. That gives you the speech regions and lets you skip long silences, which is where Whisper tends to invent text.
- Group speech into chunks of about 5–10 minutes, cut at pauses. Inside a chunk, whisper.cpp still works in its own 30-second windows.
- Keep a few seconds of overlap at each chunk edge and drop the duplicate words when merging, so no word is cut in half.

**3. Process chunk by chunk, with checkpoints.**
- One chunk at a time on the GPU (Metal, or CUDA/Vulkan on Windows and Linux). Running them in parallel doesn't speed up a single GPU and doubles memory.
- After each chunk, append its words with absolute timestamps to the bundle's transcript as JSON lines. If the app crashes, the laptop sleeps or the user cancels, the job resumes from the last finished chunk instead of starting over.
- Show progress and a time estimate measured from the first chunks, not guessed.
- Pause during a recording or an export, and optionally while on battery.

**4. Guard against Whisper's long-audio failure modes.**
- **Repetition loops**, where the same sentence repeats for minutes: don't feed each chunk the previous chunk's text, or limit it to a short glossary prompt. Use whisper.cpp's checks (no-speech threshold, compression-ratio and log-probability checks with temperature fallback) so stuck segments are retried.
- **Drifting language:** detect the language from the first few minutes, keep it fixed for the file, and let the user override it. Mixed Vietnamese and English still works, since the language setting doesn't block English words.
- **Low-confidence spans:** flag them for review in the transcript editor rather than hiding them.

**5. Model choice.**
- Default to **large-v3-turbo** for long files; it's several times faster than large-v3 with similar accuracy.
- Offer large-v3 (or PhoWhisper for Vietnamese) as a "more accurate, slower" choice.
- Measure speed on your Mac with a 30-minute sample before quoting times to users. For a 4-hour file the difference between models adds up to a long wait.

**6. Make it usable before it finishes.** The editor and transcript search can show finished chunks while the rest is still processing, so the user can start reading and searching the first hour right away.

**7. After the transcript: speakers and reports.**
- **Speaker labels:** diarization runs over the whole file as one pass, so speakers stay consistent across chunks (not "Speaker 2" meaning different people in hour 1 and hour 3). 4 hours at 16 kHz mono is manageable if read in windows.
- **Reports:** 4 hours is roughly 40,000–50,000 tokens, too much for one pass with a small model. Summarise each 15–20-minute block into notes with timestamps, then merge those notes into the final report with the same schema and citation checks (AI-7 chunking, AI-9). The citations still point to exact moments in the full recording.

This is the same pipeline your 15-minute segmented recordings would use. An imported file is just split into chunks after the fact, so imports and live recordings share one code path.

I can write this as a spec update to AI-1 (long-file transcription: chunking, checkpoints, partial results, the long-audio guards) with acceptance criteria. An example would be a 4-hour MP3 that transcribes with flat memory and resumes after the app is killed.