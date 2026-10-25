//! The pure half of the edited export (EXP-1): everything that turns an [`EditList`] into
//! decoder segments and an audio filter graph, with no FFmpeg, GPU or file access, so it is unit
//! tested with manifest-shaped numbers.
//!
//! # Time
//!
//! The edit list is in session time. Pause gaps are not part of an export (REC-4: every media file
//! is compacted at a pause), so [`with_pauses_cut`] adds each gap as a cut and no kept piece spans
//! one. A piece of `[a, b)` session time then maps to one contiguous run of every track file:
//! `pos = file(a) - file(track.start)` seconds, negative while the track has not started yet.
//!
//! Output frame `k` is at `k / fps`. [`video_segments`] gives, per piece, the frames it owns and
//! where to start reading a track file; the decoder does the speed change and the frame-rate
//! conversion (`setpts` and `fps`), so each segment is one FFmpeg process.

use cm_core::edit::{Piece, Timeline};
use cm_core::{EditList, MediaTime, PauseGap};

use crate::graph::{secs, AUDIO_RATE};
use crate::timeline::TICKS_PER_SECOND;

/// Adds every pause gap as a cut (merged with the user's cuts), so the export skips paused time.
/// The same view the daemon uses for the reported duration: [`EditList::without_pauses`].
pub fn with_pauses_cut(list: &EditList, gaps: &[PauseGap]) -> EditList {
    list.without_pauses(gaps)
}

/// Seconds of a media time.
pub fn secs_of(t: MediaTime) -> f64 {
    t.ticks() as f64 / TICKS_PER_SECOND as f64
}

/// Number of output frames of an export of `duration` at `fps` (at least one).
pub fn frame_count(duration: MediaTime, fps: f64) -> u64 {
    ((secs_of(duration) * fps).round() as u64).max(1)
}

/// A run of output frames read from one track file with one FFmpeg process.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// First output frame index and how many frames this segment owns.
    pub first_frame: u64,
    pub frames: u64,
    /// Where to start reading the file, seconds from its first sample (never negative).
    pub seek_s: f64,
    /// Seconds (output time) to hold the first picture because the track has not started.
    pub hold_s: f64,
    /// Speed factor of the piece.
    pub factor: f64,
}

fn file_pos_s(gaps: &[PauseGap], track_start: MediaTime, t: MediaTime) -> f64 {
    secs_of(cm_core::session_to_file(gaps, t) - cm_core::session_to_file(gaps, track_start))
}

/// Decoder segments of one track for `frames` output frames at `fps`. Every output frame is in
/// exactly one segment; frames are assigned to the piece that owns their time.
pub fn video_segments(
    tl: &Timeline,
    gaps: &[PauseGap],
    track_start: MediaTime,
    fps: f64,
    frames: u64,
) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut next = 0u64;
    for (idx, p) in tl.pieces().iter().enumerate() {
        if next >= frames {
            break;
        }
        let end_s = secs_of(p.out_start + p.out_len);
        let is_last = idx + 1 == tl.pieces().len();
        // Frames whose time lies before the end of this piece; the last piece takes the rest.
        let upto = if is_last {
            frames
        } else {
            ((end_s * fps).ceil() as u64).min(frames)
        };
        if upto <= next {
            continue;
        }
        let t0 =
            MediaTime::from_ticks((next as f64 / fps * TICKS_PER_SECOND as f64).round() as i64);
        let Some(sample) = tl.at_output(t0.max(p.out_start)) else {
            continue;
        };
        let pos = file_pos_s(gaps, track_start, sample.source);
        out.push(Segment {
            first_frame: next,
            frames: upto - next,
            seek_s: pos.max(0.0),
            hold_s: (-pos).max(0.0) / p.factor,
            factor: p.factor,
        });
        next = upto;
    }
    out
}

/// The video filter chain of a segment, ending in the pixel format and size the reader expects.
pub fn segment_filters(
    seg: &Segment,
    fps: f64,
    size: (u32, u32),
    pre: &[String],
    pix_fmt: &str,
) -> String {
    let mut f: Vec<String> = Vec::new();
    f.push(format!("setpts=(PTS-STARTPTS)/{}", secs(seg.factor)));
    if seg.hold_s > 0.0005 {
        f.push(format!(
            "tpad=start_duration={}:start_mode=clone",
            secs(seg.hold_s)
        ));
    }
    f.push(format!("fps={}", secs((fps * 1000.0).round() / 1000.0)));
    f.extend(pre.iter().cloned());
    f.push(format!("scale={}:{}:flags=bicubic", size.0, size.1));
    f.push("setsar=1".to_owned());
    f.push(format!("format={pix_fmt}"));
    f.join(",")
}

// ---- audio ------------------------------------------------------------------------------------

/// Which recorded source an audio chain belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    Mic,
    System,
}

/// The hook for audio processing. The default [`FfmpegAudio`] uses `atempo`; the cm-audio DSP
/// (AUD-4 gains, loudness, pitch-preserving stretch) plugs in here with its own filter chains.
pub trait AudioProcessor: Send + Sync {
    /// Filters that change the speed of the audio by `factor` (1.0 returns none).
    fn stretch(&self, factor: f64) -> Vec<String>;
    /// Filters applied to a whole track after its pieces are joined (gain, denoise), by kind.
    fn track(&self, kind: AudioKind) -> Vec<String>;
    /// Mixes decoded PCM on the output timeline (cuts, speed with the pitch kept, gains, mutes,
    /// loudness). `None` means this processor works with filter chains instead
    /// ([`AudioProcessor::stretch`] and [`AudioProcessor::track`]).
    fn mix(&self, _input: &MixInput<'_>) -> Option<Result<MixedPcm, cm_core::CmError>> {
        None
    }
}

/// One decoded audio file for [`AudioProcessor::mix`]: interleaved f32 at [`AUDIO_RATE`].
#[derive(Debug, Clone)]
pub struct PcmTrack {
    pub kind: AudioKind,
    pub samples: Vec<f32>,
    pub channels: u16,
    /// `TrackInfo.start`: session time of the file's first sample.
    pub start: MediaTime,
}

/// What a PCM mixer needs: the decoded tracks, the pauses, the (pause-free) timeline, the
/// per-source gains and mutes of `edits[0].audio`, and the loudness target.
pub struct MixInput<'a> {
    pub tracks: &'a [PcmTrack],
    pub gaps: &'a [PauseGap],
    pub timeline: &'a Timeline,
    pub audio: &'a cm_core::edit::AudioMix,
    pub out_channels: u16,
    pub loudness_lufs: Option<f64>,
}

/// The mix on the output timeline.
#[derive(Debug, Clone)]
pub struct MixedPcm {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
    pub normalise_gain_db: f64,
    pub integrated_lufs: Option<f64>,
    pub true_peak_dbtp: f64,
}

/// `atempo` stretching, no per-track processing.
#[derive(Debug, Clone, Copy, Default)]
pub struct FfmpegAudio;

impl AudioProcessor for FfmpegAudio {
    fn stretch(&self, factor: f64) -> Vec<String> {
        let mut left = factor;
        let mut out = Vec::new();
        if (left - 1.0).abs() < 1e-9 {
            return out;
        }
        // atempo takes 0.5..=2.0 per stage in older FFmpeg builds.
        while left > 2.0 {
            out.push("atempo=2".to_owned());
            left /= 2.0;
        }
        while left < 0.5 {
            out.push("atempo=0.5".to_owned());
            left /= 0.5;
        }
        out.push(format!("atempo={}", secs(left)));
        out
    }

    fn track(&self, _kind: AudioKind) -> Vec<String> {
        Vec::new()
    }
}

/// One recorded audio file placed on the session timeline.
#[derive(Debug, Clone, Copy)]
pub struct AudioSource {
    pub kind: AudioKind,
    /// Index of the FFmpeg input that reads the file.
    pub input: usize,
    pub start: MediaTime,
}

/// The `-filter_complex` part for audio: each source is cut to the kept pieces, stretched, padded
/// to the piece's output length (so the sources never drift against the picture), joined and
/// mixed into `[outa]`. Returns `None` without a source.
pub fn audio_graph(
    tl: &Timeline,
    gaps: &[PauseGap],
    sources: &[AudioSource],
    layout: &str,
    amix_normalize: bool,
    proc: &dyn AudioProcessor,
) -> Option<String> {
    if sources.is_empty() {
        return None;
    }
    let mut graph: Vec<String> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let pieces = tl.pieces();
    for (si, src) in sources.iter().enumerate() {
        // One labelled pad can feed one filter: split the input once per piece.
        let split: Vec<String> = (0..pieces.len())
            .map(|pi| format!("[s{si}_{pi}]"))
            .collect();
        graph.push(format!(
            "[{}:a]asplit={}{}",
            src.input,
            pieces.len(),
            split.concat()
        ));
        let mut joined: Vec<String> = Vec::new();
        for (pi, p) in pieces.iter().enumerate() {
            let label = format!("[p{si}_{pi}]");
            graph.push(piece_chain(p, gaps, src, &split[pi], &label, layout, proc));
            joined.push(label);
        }
        let mut tail: Vec<String> = proc.track(src.kind);
        tail.push(format!("aresample={AUDIO_RATE}"));
        let tail = tail.join(",");
        graph.push(format!(
            "{}concat=n={}:v=0:a=1,{tail}[t{si}]",
            joined.concat(),
            joined.len()
        ));
        labels.push(format!("[t{si}]"));
    }
    if labels.len() == 2 {
        let norm = if amix_normalize { ":normalize=0" } else { "" };
        graph.push(format!(
            "{}amix=inputs=2:duration=longest:dropout_transition=0{norm},alimiter=limit=0.97:level=disabled[outa]",
            labels.concat()
        ));
    } else {
        graph.push(format!("{}anull[outa]", labels[0]));
    }
    Some(graph.join(";"))
}

fn piece_chain(
    p: &Piece,
    gaps: &[PauseGap],
    src: &AudioSource,
    input_label: &str,
    out_label: &str,
    layout: &str,
    proc: &dyn AudioProcessor,
) -> String {
    let out_len = secs_of(p.out_len);
    let norm = format!("aresample={AUDIO_RATE},aformat=sample_fmts=fltp:channel_layouts={layout}");
    let silence = || {
        // A muted piece (above 2x without keep_audio) or a track that has not started.
        format!(
            "anullsrc=r={AUDIO_RATE}:cl={layout},atrim=end={},asetpts=PTS-STARTPTS{out_label}",
            secs(out_len)
        )
    };
    if !p.audio {
        // The unused split pad still has to be consumed.
        return format!("{input_label}anullsink;{}", silence());
    }
    let pos = file_pos_s(gaps, src.start, p.source.from);
    let len_src = secs_of(p.source.len());
    if pos + len_src <= 0.0 {
        return format!("{input_label}anullsink;{}", silence());
    }
    let mut f: Vec<String> = vec![format!(
        "atrim=start={}:end={}",
        secs(pos.max(0.0)),
        secs(pos + len_src)
    )];
    f.push("asetpts=PTS-STARTPTS".into());
    if pos < -0.0005 {
        let ms = ((-pos) * 1000.0).round() as i64;
        f.push(format!("adelay=delays={ms}:all=1"));
    }
    f.extend(proc.stretch(p.factor));
    f.push(norm);
    f.push(format!("apad=whole_dur={}", secs(out_len)));
    f.push(format!("atrim=end={}", secs(out_len)));
    f.push("asetpts=PTS-STARTPTS".into());
    format!("{input_label}{}{out_label}", f.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{EditContext, EditList, Span};

    const TPS: i64 = TICKS_PER_SECOND;

    fn t(s: f64) -> MediaTime {
        MediaTime::from_ticks((s * TPS as f64).round() as i64)
    }

    fn cx(end: f64) -> EditContext<'static> {
        EditContext {
            end: t(end),
            live_at: None,
            gaps: &[],
        }
    }

    fn list(end: f64) -> EditList {
        EditList::full(&cx(end))
    }

    fn gap(a: f64, b: f64) -> PauseGap {
        PauseGap {
            from: t(a),
            to: t(b),
        }
    }

    /// EDT-1: pause gaps are skipped like cuts and merge with the user's cuts.
    #[test]
    fn pauses_become_cuts_and_merge() {
        let l = list(60.0).cut(&[Span::new(t(11.0), t(14.0))]).unwrap();
        let l = with_pauses_cut(&l, &[gap(10.0, 12.0), gap(30.0, 31.0)]);
        assert_eq!(
            l.cuts,
            vec![Span::new(t(10.0), t(14.0)), Span::new(t(30.0), t(31.0))]
        );
        // Total kept: 60 - 4 - 1.
        let d = secs_of(l.timeline().duration());
        assert!((d - 55.0).abs() < 1e-6, "{d}");
    }

    /// EDT-1.AC1: a 60 s recording trimmed to 2..58 s lasts 56 s (± 1 frame).
    #[test]
    fn edt1_ac1_trim_duration_and_frames() {
        let l = list(60.0).trim(t(2.0), t(58.0), &cx(60.0)).unwrap();
        let tl = l.timeline();
        assert!((secs_of(tl.duration()) - 56.0).abs() < 1.0 / 30.0);
        assert_eq!(frame_count(tl.duration(), 30.0), 1680);
    }

    /// Every output frame belongs to exactly one segment, in order, with no gaps.
    #[test]
    fn segments_cover_every_frame_once() {
        let l = list(60.0)
            .cut(&[Span::new(t(10.0), t(15.0))])
            .unwrap()
            .set_speed(Span::new(t(30.0), t(50.0)), 2.0, false)
            .unwrap();
        let tl = l.timeline();
        let frames = frame_count(tl.duration(), 30.0);
        let segs = video_segments(&tl, &[], t(0.2), 30.0, frames);
        assert!(segs.len() >= 3);
        let mut next = 0;
        for s in &segs {
            assert_eq!(s.first_frame, next);
            assert!(s.frames > 0);
            next += s.frames;
        }
        assert_eq!(next, frames);
        // After the 5 s cut the reader starts at source 15 s (minus the track's own start 0.2 s).
        let second = &segs[1];
        assert!((second.seek_s - 14.8).abs() < 0.05, "{}", second.seek_s);
        // The sped-up part reads at 2x.
        assert!(segs.iter().any(|s| (s.factor - 2.0).abs() < 1e-9));
    }

    /// EDT-2.AC1: a 20 s segment at 2x lasts 10 s (± 1 frame).
    #[test]
    fn edt2_ac1_speed_halves_the_segment() {
        let l = list(60.0)
            .set_speed(Span::new(t(20.0), t(40.0)), 2.0, false)
            .unwrap();
        let d = secs_of(l.timeline().duration());
        assert!((d - 50.0).abs() <= 1.0 / 30.0 + 1e-9, "{d}");
    }

    /// A camera that starts after the piece does: its first picture is held, and nothing is
    /// read before the track's first sample.
    #[test]
    fn a_late_track_holds_its_first_frame() {
        let l = list(10.0);
        let tl = l.timeline();
        let segs = video_segments(&tl, &[], t(0.19), 30.0, 300);
        assert_eq!(segs.len(), 1);
        // The piece starts at 0 and the track at 0.19 s: the file position is -0.19 s.
        assert_eq!(segs[0].seek_s, 0.0);
        assert!((segs[0].hold_s - 0.19).abs() < 1e-9, "{}", segs[0].hold_s);
    }

    #[test]
    fn segment_filters_apply_speed_hold_fps_and_size() {
        let seg = Segment {
            first_frame: 0,
            frames: 10,
            seek_s: 1.0,
            hold_s: 0.19,
            factor: 2.0,
        };
        let f = segment_filters(&seg, 30.0, (1920, 1080), &[], "nv12");
        assert_eq!(
            f,
            "setpts=(PTS-STARTPTS)/2,tpad=start_duration=0.19:start_mode=clone,fps=30,scale=1920:1080:flags=bicubic,setsar=1,format=nv12"
        );
    }

    /// EDT-2.AC3: above 2x the audio is replaced by silence of the same length; below it is cut,
    /// stretched and padded to the piece's output length.
    #[test]
    fn edt2_ac3_fast_pieces_are_silent_and_slow_ones_stretched() {
        let l = list(30.0)
            .set_speed(Span::new(t(10.0), t(20.0)), 4.0, false)
            .unwrap()
            .set_speed(Span::new(t(20.0), t(30.0)), 1.5, false)
            .unwrap();
        let tl = l.timeline();
        let src = [AudioSource {
            kind: AudioKind::Mic,
            input: 1,
            start: t(0.0),
        }];
        let g = audio_graph(&tl, &[], &src, "mono", true, &FfmpegAudio).unwrap();
        assert!(g.contains("[1:a]asplit=3"));
        // Piece 1 (4x, no keep_audio): silence of 2.5 s.
        assert!(g.contains("anullsrc=r=48000:cl=mono,atrim=end=2.5"), "{g}");
        // Piece 2 (1.5x): atempo and a pad to 6.666667 s.
        assert!(g.contains("atempo=1.5"), "{g}");
        assert!(g.contains("apad=whole_dur=6.666667"), "{g}");
        assert!(g.ends_with("[t0]anull[outa]"), "{g}");
    }

    /// AUD-4 / EDT-1.AC2: a mic that started before the screen is trimmed at the front of the
    /// first piece, one that started later is delayed, and a cut leaves the rest in step.
    #[test]
    fn audio_offsets_and_cuts_stay_in_step() {
        let l = list(20.0).cut(&[Span::new(t(5.0), t(10.0))]).unwrap();
        let tl = l.timeline();
        let src = [
            AudioSource {
                kind: AudioKind::Mic,
                input: 1,
                start: t(-0.29),
            },
            AudioSource {
                kind: AudioKind::System,
                input: 2,
                start: t(0.5),
            },
        ];
        let g = audio_graph(&tl, &[], &src, "stereo", false, &FfmpegAudio).unwrap();
        // Mic: piece 0 starts at file pos 0.29 s.
        assert!(g.contains("atrim=start=0.29:end=5.29"), "{g}");
        // After the cut the second piece reads from 10.29 s.
        assert!(g.contains("atrim=start=10.29:end=20.29"), "{g}");
        // System starts 0.5 s into the first piece: delayed by 500 ms.
        assert!(g.contains("adelay=delays=500:all=1"), "{g}");
        assert!(g.contains("amix=inputs=2"), "{g}");
        assert!(!g.contains("normalize"), "{g}");
    }

    #[test]
    fn stretch_chains_stay_within_the_atempo_range() {
        let p = FfmpegAudio;
        assert!(p.stretch(1.0).is_empty());
        assert_eq!(p.stretch(4.0), vec!["atempo=2", "atempo=2"]);
        assert_eq!(p.stretch(0.25), vec!["atempo=0.5", "atempo=0.5"]);
        assert_eq!(p.stretch(1.5), vec!["atempo=1.5"]);
    }
}
