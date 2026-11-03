//! Transcribing a recording while it is still being written (M2h): the pieces of the batch path
//! ([`super::plan`]) are cut and read as soon as the audio on disk makes the cut certain, so at Stop
//! only the last piece is left.
//!
//! A piece boundary of the batch plan depends on the total length only through two rules: audio up
//! to [`LONG_AUDIO_SECS`] is one piece, and a piece is cut while more than one and a half pieces
//! remain. A piece is therefore cut here only once those rules are certain whatever the final length
//! is (more than [`LONG_AUDIO_SECS`] on disk for the first, more than one and a half pieces beyond
//! the start for the next ones); the cut itself looks at the audio within [`SEARCH_SECS`] of the
//! target, which is on disk by then. So the pieces are the same ones the batch path would make from
//! the finished file, they go through the same [`Job::piece`] (loop checks, repair, gap fill), and the
//! words are the same word for word when the engine is deterministic.
//!
//! The caller owns the audio: it passes the samples written so far to [`LiveTranscription::advance`]
//! (any number of times) and the finished file to [`LiveTranscription::finish`]. Discarding a
//! recording is dropping the value. A crash is [`LiveTranscription::checkpoint`], saved after every
//! piece, and [`LiveTranscription::resume`].

use cm_core::{CmError, MediaTime};
use serde::{Deserialize, Serialize};

use super::{
    collapse_repeats, is_degenerate, is_unsettled, plan_cut, samples_of, Job, OtherAudio,
    PiecesReport, LONG_AUDIO_SECS,
};
use crate::transcribe::{Transcriber, Transcript, Word, TRANSCRIPT_VERSION};

/// Where a live run stood after a finished piece.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveCheckpoint {
    /// Samples of the finished pieces (the next piece starts here).
    pub from: usize,
    pub words: Vec<Word>,
    pub language: Option<String>,
    pub first: Option<(String, String, Option<String>)>,
    pub report: PiecesReport,
    #[serde(default)]
    pub sounds: Vec<crate::transcribe::Sound>,
}

/// The state of a transcription that follows a growing file.
pub struct LiveTranscription {
    base: MediaTime,
    asked: Option<String>,
    prompt: Option<String>,
    from: usize,
    words: Vec<Word>,
    language: Option<String>,
    first: Option<(String, String, Option<String>)>,
    report: PiecesReport,
    sounds: Vec<crate::transcribe::Sound>,
    high: f32,
}

impl LiveTranscription {
    /// `start` is the file time of the first sample (as `Pcm16k::start`); `language` and `prompt`
    /// as for [`super::transcribe_pieces`].
    pub fn new(start: MediaTime, language: Option<&str>, prompt: Option<&str>) -> Self {
        let asked = language
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("auto"))
            .map(str::to_owned);
        LiveTranscription {
            base: start,
            language: asked.clone(),
            asked,
            prompt: prompt.map(str::to_owned),
            from: 0,
            words: Vec::new(),
            first: None,
            report: PiecesReport::default(),
            sounds: Vec::new(),
            high: 0.0,
        }
    }

    /// Goes on from a saved state (the same audio file, the same options).
    pub fn resume(
        start: MediaTime,
        language: Option<&str>,
        prompt: Option<&str>,
        cp: LiveCheckpoint,
    ) -> Self {
        let mut me = Self::new(start, language, prompt);
        me.from = cp.from;
        me.words = cp.words;
        me.first = cp.first;
        me.report = cp.report;
        me.sounds = cp.sounds;
        if is_unsettled(me.asked.as_deref()) {
            me.language = cp.language;
        }
        me
    }

    pub fn checkpoint(&self) -> LiveCheckpoint {
        LiveCheckpoint {
            from: self.from,
            words: self.words.clone(),
            language: self.language.clone(),
            first: self.first.clone(),
            report: self.report,
            sounds: self.sounds.clone(),
        }
    }

    /// The state as a batch [`super::Checkpoint`] of the finished file `samples`: the job that runs
    /// at Stop (`transcribe_pieces_resumable`) continues from it and reads only the pieces that are
    /// left. `None` when the finished pieces are not a prefix of the batch plan of `samples` (the
    /// file is not the one that was followed), in which case the job starts from the first piece.
    pub fn to_batch_checkpoint(&self, samples: &[f32]) -> Option<super::Checkpoint> {
        let plan = super::plan(samples);
        let done = if self.from == 0 {
            0
        } else {
            plan.iter().position(|r| r.start == self.from)?
        };
        Some(super::Checkpoint {
            total_samples: samples.len(),
            pieces: plan.len(),
            done,
            words: self.words.clone(),
            language: self.language.clone(),
            first: self.first.clone(),
            split: self.report.split,
            repeats_dropped: self.report.repeats_dropped,
            other_audio: self.report.other_audio,
            other_audio_kept: self.report.other_audio_kept,
            broken_reread: self.report.broken_reread,
            broken_dropped: self.report.broken_dropped,
            gaps_reread: self.report.gaps_reread,
            gaps_filled: self.report.gaps_filled,
            sounds: self.sounds.clone(),
        })
    }

    /// Samples of the pieces finished so far.
    pub fn done_samples(&self) -> usize {
        self.from
    }

    /// The next piece that is certain whatever the final length: `samples` is what is on disk.
    fn next_certain(&self, samples: &[f32]) -> Option<std::ops::Range<usize>> {
        if self.from == 0 && samples.len() <= samples_of(LONG_AUDIO_SECS) {
            return None;
        }
        plan_cut(samples, self.from).map(|cut| self.from..cut)
    }

    /// Reads every piece that the audio written so far makes certain. Returns how many it finished.
    /// `progress` is called with 0.0 to 1.0 inside a piece and returns `false` to stop (hold or
    /// cancel); the piece is then not finished and the next call reads it again.
    pub fn advance(
        &mut self,
        transcriber: &dyn Transcriber,
        samples: &[f32],
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<usize, CmError> {
        let mut done = 0;
        while let Some(range) = self.next_certain(samples) {
            self.read_piece(transcriber, samples, range.clone(), None, &mut *progress)?;
            self.from = range.end;
            done += 1;
        }
        Ok(done)
    }

    fn read_piece<'p>(
        &mut self,
        transcriber: &dyn Transcriber,
        samples: &[f32],
        range: std::ops::Range<usize>,
        other: OtherAudio<'p>,
        progress: &'p mut dyn FnMut(f32) -> bool,
    ) -> Result<(), CmError> {
        let prompt = self.prompt.clone();
        let mut job = Job {
            transcriber,
            samples,
            other,
            other_samples: None,
            base: self.base,
            prompt: prompt.as_deref(),
            language: self.language.take(),
            first: self.first.take(),
            progress,
            high: self.high,
            report: self.report,
            sounds: std::mem::take(&mut self.sounds),
            segs: Vec::new(),
            gate_ev: None,
            last_read_segs: Vec::new(),
        };
        let r = job.piece(range, 0);
        // Give the state back whether the piece finished or was stopped.
        self.language = job.language;
        self.first = job.first;
        self.high = job.high;
        self.report = job.report;
        self.sounds = job.sounds;
        self.words.extend(r?);
        Ok(())
    }

    /// The finished file: reads what is left (the batch plan's remaining pieces, the last one up to
    /// one and a half pieces long) and returns the transcript the batch path would have made.
    pub fn finish<'p>(
        mut self,
        transcriber: &dyn Transcriber,
        samples: &[f32],
        other: OtherAudio<'p>,
        progress: &'p mut dyn FnMut(f32) -> bool,
    ) -> Result<(Transcript, PiecesReport), CmError> {
        let prompt = self.prompt.clone();
        let mut job = Job {
            transcriber,
            samples,
            other,
            other_samples: None,
            base: self.base,
            prompt: prompt.as_deref(),
            language: self.language.take(),
            first: self.first.take(),
            progress,
            high: self.high,
            report: self.report,
            sounds: std::mem::take(&mut self.sounds),
            segs: Vec::new(),
            gate_ev: None,
            last_read_segs: Vec::new(),
        };
        // Further cuts while more than one and a half pieces remain, then the tail as one piece.
        let mut from = self.from;
        let mut words = std::mem::take(&mut self.words);
        loop {
            let range = match plan_cut(samples, from) {
                Some(cut) if from > 0 || samples.len() > samples_of(LONG_AUDIO_SECS) => from..cut,
                _ => from..samples.len(),
            };
            let last = range.end == samples.len();
            from = range.end;
            words.extend(job.piece(range, 0)?);
            if last {
                break;
            }
        }
        let _ = (job.progress)(1.0);
        let (mut report, first, language, sounds) =
            (job.report, job.first, job.language, job.sounds);
        if is_degenerate(&words) {
            let (kept, dropped) = collapse_repeats(words);
            words = kept;
            report.repeats_dropped += dropped;
        }
        let (first_language, model, note) = first.unwrap_or_default();
        let note = if words.is_empty() { note } else { None };
        Ok((
            Transcript {
                version: TRANSCRIPT_VERSION,
                language: language.unwrap_or(first_language),
                model,
                words,
                note,
                sounds,
                segments: Vec::new(),
            },
            report,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::SAMPLE_RATE;
    use crate::pieces::transcribe_pieces;
    use crate::transcribe::{media_secs, Pcm16k};

    const RATE: usize = SAMPLE_RATE as usize;

    /// A deterministic engine: one word per second of the audio it is given, its text a checksum
    /// of that second, so two runs give the same words only if they were given the same ranges.
    struct Checksum;

    impl Transcriber for Checksum {
        fn transcribe(
            &self,
            audio: Pcm16k<'_>,
            _language: Option<&str>,
            progress: &mut dyn FnMut(f32) -> bool,
        ) -> Result<Transcript, CmError> {
            if !progress(1.0) {
                return Err(crate::transcribe::cancelled());
            }
            let words = audio
                .samples
                .chunks(RATE)
                .enumerate()
                .map(|(i, c)| {
                    let sum: f64 = c.iter().map(|s| f64::from(s.abs())).sum();
                    Word {
                        text: format!("w{}", (sum * 1000.0) as u64),
                        start: audio.start + media_secs(i as f64),
                        end: audio.start + media_secs(i as f64 + 0.5),
                        confidence: 0.9,
                    }
                })
                .collect();
            Ok(Transcript {
                version: TRANSCRIPT_VERSION,
                language: "en".into(),
                model: "checksum".into(),
                words,
                note: None,
                sounds: Vec::new(),
                segments: Vec::new(),
            })
        }
    }

    /// Varied audio with quiet spots near multiples of 300 s so the cuts are not at the targets.
    fn audio(secs: usize) -> Vec<f32> {
        let mut x: u32 = 99;
        let mut v: Vec<f32> = (0..secs * RATE)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                0.2 * ((x >> 9) as f32 / (1 << 22) as f32 - 1.0)
            })
            .collect();
        for centre in [297usize, 611, 893, 1214, 1497, 1803, 2111] {
            if centre + 1 < secs {
                for s in &mut v[centre * RATE..(centre + 1) * RATE] {
                    *s *= 0.001;
                }
            }
        }
        v
    }

    fn batch(samples: &[f32]) -> Transcript {
        let audio = Pcm16k {
            samples,
            start: MediaTime::ZERO,
        };
        transcribe_pieces(&Checksum, audio, Some("en"), None, &mut |_| true)
            .unwrap()
            .0
    }

    /// The file grows in chunks of `step` seconds; the live run advances after each and finishes
    /// at `total`.
    fn live(samples: &[f32], step: usize) -> (Transcript, usize) {
        let mut run = LiveTranscription::new(MediaTime::ZERO, Some("en"), None);
        let mut during = 0;
        let mut at = 0;
        while at + step * RATE < samples.len() {
            at += step * RATE;
            during += run
                .advance(&Checksum, &samples[..at], &mut |_| true)
                .unwrap();
        }
        (
            run.finish(&Checksum, samples, None, &mut |_| true)
                .unwrap()
                .0,
            during,
        )
    }

    /// M2h: the words of a live run equal the batch result word for word, at lengths below, at and
    /// above the piece sizes, however the file grows.
    #[test]
    fn live_words_equal_the_batch_words() {
        for secs in [15, 590, 601, 760, 1500, 2300] {
            let x = audio(secs);
            let want = batch(&x);
            for step in [7, 61] {
                let (got, during) = live(&x, step);
                assert_eq!(got.words, want.words, "{secs} s, steps of {step} s");
                assert_eq!(got.language, want.language);
                // Pieces were finished before the end once the audio was long enough.
                if secs >= 760 {
                    assert!(
                        during >= 1,
                        "{secs} s: {during} pieces finished while recording"
                    );
                }
            }
        }
    }

    /// A piece is not read until the cut is certain (more than 600 s for the first, then more than
    /// one and a half pieces beyond it).
    #[test]
    fn nothing_is_cut_before_the_plan_is_certain() {
        let x = audio(1000);
        let mut run = LiveTranscription::new(MediaTime::ZERO, Some("en"), None);
        assert_eq!(
            run.advance(&Checksum, &x[..600 * RATE], &mut |_| true)
                .unwrap(),
            0
        );
        assert_eq!(
            run.advance(&Checksum, &x[..601 * RATE], &mut |_| true)
                .unwrap(),
            1
        );
        let cut = run.done_samples();
        assert!((290 * RATE..320 * RATE).contains(&cut), "{cut}");
        // The next one needs more than 450 s beyond the cut.
        assert_eq!(
            run.advance(&Checksum, &x[..cut + 450 * RATE], &mut |_| true)
                .unwrap(),
            0
        );
        assert_eq!(
            run.advance(&Checksum, &x[..cut + 451 * RATE], &mut |_| true)
                .unwrap(),
            1
        );
    }

    /// A crash after some pieces: resuming from the saved state gives the same transcript.
    #[test]
    fn a_checkpoint_resumes_to_the_same_words() {
        let x = audio(1500);
        let want = batch(&x);
        let mut run = LiveTranscription::new(MediaTime::ZERO, Some("en"), None);
        run.advance(&Checksum, &x[..1000 * RATE], &mut |_| true)
            .unwrap();
        let cp = run.checkpoint();
        assert!(cp.from > 0);
        let json = serde_json::to_string(&cp).unwrap();
        drop(run);
        let cp: LiveCheckpoint = serde_json::from_str(&json).unwrap();
        let resumed = LiveTranscription::resume(MediaTime::ZERO, Some("en"), None, cp);
        let got = resumed
            .finish(&Checksum, &x, None, &mut |_| true)
            .unwrap()
            .0;
        assert_eq!(got.words, want.words);
    }

    /// What the job at Stop does with the live state: it resumes from the converted checkpoint and
    /// the transcript equals the batch result.
    #[test]
    fn the_stop_job_resumes_from_the_live_state() {
        let x = audio(1500);
        let want = batch(&x);
        let mut run = LiveTranscription::new(MediaTime::ZERO, Some("en"), None);
        run.advance(&Checksum, &x[..1200 * RATE], &mut |_| true)
            .unwrap();
        let cp = run.to_batch_checkpoint(&x).unwrap();
        assert!(
            cp.done >= 1 && cp.done < cp.pieces,
            "{} of {}",
            cp.done,
            cp.pieces
        );
        let (got, _) = crate::pieces::transcribe_pieces_resumable(
            &Checksum,
            Pcm16k {
                samples: &x,
                start: MediaTime::ZERO,
            },
            Some("en"),
            None,
            Some(cp),
            &mut |_| {},
            None,
            &mut |_| true,
        )
        .unwrap();
        assert_eq!(got.words, want.words);
    }

    /// A piece that is stopped (hold or cancel) is not finished and is read again next time.
    #[test]
    fn a_stopped_piece_is_read_again() {
        let x = audio(800);
        let mut run = LiveTranscription::new(MediaTime::ZERO, Some("en"), None);
        let err = run.advance(&Checksum, &x, &mut |_| false).unwrap_err();
        assert!(crate::transcribe::is_cancelled(&err));
        assert_eq!(run.done_samples(), 0);
        let got = run.finish(&Checksum, &x, None, &mut |_| true).unwrap().0;
        assert_eq!(got.words, batch(&x).words);
    }
}
