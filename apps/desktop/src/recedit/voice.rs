//! The audio editor's logic, free of any display (VOC-2, redesign V4): speaker colours, the turns
//! of the transcript, the line under the playhead, the "now speaking" card, markers on the
//! session clock, the waveform columns, the speaker lane, the overview strip and the keys of an
//! audio recording. The GPUI drawing is in `ui::recording_editor::audio_view`.
//!
//! Times are session time (pauses included) unless a function says "recorded", which is the
//! clock of the HUD timer and of `manifest.markers` (pauses removed).

use std::ops::Range;

use cm_ai::caption::CaptionLine;
use cm_ai::recording::file_to_session;
use cm_ai::speakers::{Segment, Speaker, WordLabel};
use cm_ai::transcribe::Word;
use cm_core::{MediaTime, PauseGap};
use cm_library::manifest::RecManifest;
use cm_media::decode::LanePeaks;

use super::geom::Geom;
use super::keys::{map_key, Cmd};
use crate::theme::Color;

// ---- speakers ---------------------------------------------------------------------------

/// The speaker colours of the board (never the status colours), repeating after the fourth.
pub const SPEAKER_COLOURS: [u32; 4] = [0x8FB0FF, 0x7FD6C2, 0xF2B880, 0xC9A7F5];

/// The colour of the speaker at `index` in the speakers list.
pub fn speaker_colour(index: usize) -> Color {
    Color::rgb(SPEAKER_COLOURS[index % SPEAKER_COLOURS.len()])
}

/// Where speaker `id` is in the list (its colour follows the position); an unknown id counts by
/// its number so the colour is still stable.
pub fn speaker_index(speakers: &[Speaker], id: u32) -> usize {
    speakers
        .iter()
        .position(|s| s.id == id)
        .unwrap_or(id.saturating_sub(1) as usize)
}

/// The letter in the avatar: the number of "Speaker 4", else the first letter, upper case.
pub fn initial(name: &str) -> String {
    let name = name.trim();
    if let Some(n) = name.strip_prefix("Speaker ") {
        if let Some(c) = n.trim().chars().next().filter(char::is_ascii_digit) {
            return c.to_string();
        }
    }
    name.chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default()
}

/// `m:ss`, or `h:mm:ss` from one hour (the time chips).
pub fn clock(t: MediaTime) -> String {
    let s = t.as_secs_f64().max(0.0) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `m:ss.f` with the tenth (the transport readout).
pub fn clock_tenths(t: MediaTime) -> String {
    let ticks = t.ticks().max(0) as u64;
    let per = cm_core::TICKS_PER_SECOND as u64;
    let whole = ticks / per;
    let tenth = ticks % per * 10 / per;
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{tenth}")
    } else {
        format!("{m}:{s:02}.{tenth}")
    }
}

// ---- markers ----------------------------------------------------------------------------

/// A marker on the session clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerView {
    pub at: MediaTime,
    pub label: String,
}

/// The markers of a manifest, moved onto the session clock (the manifest keeps recorded time).
pub fn markers_of(m: &RecManifest) -> Vec<MarkerView> {
    let mut out: Vec<MarkerView> = m
        .markers
        .iter()
        .map(|k| MarkerView {
            at: file_to_session(&m.gaps, k.at, false),
            label: k.label.clone().unwrap_or_default(),
        })
        .collect();
    out.sort_by_key(|k| k.at);
    out
}

/// The recorded time (pauses removed) of a session time, the clock `add_marker` takes. Inside a
/// pause it is the moment the pause began.
pub fn recorded_at(gaps: &[PauseGap], session: MediaTime) -> MediaTime {
    let mut sorted: Vec<&PauseGap> = gaps.iter().collect();
    sorted.sort_by_key(|g| g.from);
    let mut removed = 0i64;
    for g in sorted {
        if g.to <= session {
            removed += (g.to - g.from).ticks();
        } else if g.from < session {
            removed += (session - g.from).ticks();
            break;
        } else {
            break;
        }
    }
    MediaTime::from_ticks((session.ticks() - removed).max(0))
}

// ---- turns of the transcript ------------------------------------------------------------

/// One stretch of the transcript by one speaker (or one line when nobody was identified).
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub speaker: Option<u32>,
    pub start: MediaTime,
    pub end: MediaTime,
    /// Indices into the caption lines.
    pub lines: Vec<usize>,
    /// Someone else spoke at once during some of its words.
    pub overlap: bool,
}

/// The speaker of a caption line: the one whose words are most of it (the first on a tie), and
/// whether any of its words had others speaking at once. `words` and `labels` go together and are
/// sorted by time.
pub fn line_speaker(
    line: &CaptionLine,
    words: &[Word],
    labels: &[WordLabel],
) -> (Option<u32>, bool) {
    let first = words.partition_point(|w| mid(w) < line.start);
    let mut counts: Vec<(u32, usize)> = Vec::new();
    let mut overlap = false;
    for (w, l) in words[first..]
        .iter()
        .zip(&labels[first.min(labels.len())..])
    {
        if mid(w) >= line.end {
            break;
        }
        if let Some(id) = l.speaker {
            overlap |= l.overlap;
            match counts.iter_mut().find(|(s, _)| *s == id) {
                Some((_, n)) => *n += 1,
                None => counts.push((id, 1)),
            }
        }
    }
    let best = counts
        .iter()
        .fold(None::<(u32, usize)>, |b, &(s, n)| match b {
            Some((_, bn)) if bn >= n => b,
            _ => Some((s, n)),
        });
    (best.map(|(s, _)| s), overlap)
}

fn mid(w: &Word) -> MediaTime {
    MediaTime::from_ticks((w.start.ticks() + w.end.ticks()) / 2)
}

/// Groups the caption lines into turns: neighbouring lines of the same identified speaker are one
/// turn; lines nobody was identified for stand alone.
pub fn build_turns(lines: &[CaptionLine], words: &[Word], labels: &[WordLabel]) -> Vec<Turn> {
    group_turns(lines, |l| line_speaker(l, words, labels))
}

/// [`build_turns`] for lines that have no words of their own (a translation): the speaker of a
/// line is the one the speaker segments cover most of it with.
pub fn build_turns_by_segments(lines: &[CaptionLine], segments: &[Segment]) -> Vec<Turn> {
    group_turns(lines, |l| line_speaker_by_segments(l, segments))
}

/// The speaker of a line from the speaker segments: the one covering most of the line, and
/// whether a second speaker covers a quarter of it or more (they spoke at once).
pub fn line_speaker_by_segments(line: &CaptionLine, segments: &[Segment]) -> (Option<u32>, bool) {
    let mut cover: Vec<(u32, i64)> = Vec::new();
    for s in segments {
        let (a, b) = (s.start.max(line.start), s.end.min(line.end));
        if b > a {
            let d = (b - a).ticks();
            match cover.iter_mut().find(|(id, _)| *id == s.speaker) {
                Some((_, t)) => *t += d,
                None => cover.push((s.speaker, d)),
            }
        }
    }
    cover.sort_by_key(|(id, t)| (std::cmp::Reverse(*t), *id));
    let span = (line.end - line.start).ticks().max(1);
    let overlap = cover.get(1).is_some_and(|(_, t)| *t * 4 >= span);
    (cover.first().map(|(id, _)| *id), overlap)
}

fn group_turns(
    lines: &[CaptionLine],
    speaker_of: impl Fn(&CaptionLine) -> (Option<u32>, bool),
) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let (speaker, overlap) = speaker_of(line);
        match turns.last_mut() {
            Some(t) if speaker.is_some() && t.speaker == speaker => {
                t.end = line.end;
                t.lines.push(i);
                t.overlap |= overlap;
            }
            _ => turns.push(Turn {
                speaker,
                start: line.start,
                end: line.end,
                lines: vec![i],
                overlap,
            }),
        }
    }
    turns
}

/// What the transcript list shows, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// An index into the turns.
    Turn(usize),
    /// An index into the markers.
    Marker(usize),
}

/// The turns with the markers between them: a marker goes before the first turn that starts at
/// or after it, and the ones after the last turn go at the end.
pub fn rows(turns: &[Turn], markers: &[MarkerView]) -> Vec<Row> {
    let mut out = Vec::with_capacity(turns.len() + markers.len());
    let mut next = 0;
    for (i, t) in turns.iter().enumerate() {
        while next < markers.len() && markers[next].at <= t.start {
            out.push(Row::Marker(next));
            next += 1;
        }
        out.push(Row::Turn(i));
    }
    out.extend((next..markers.len()).map(Row::Marker));
    out
}

/// The index in `rows` of the turn that holds caption line `line`.
pub fn row_of_line(rows: &[Row], turns: &[Turn], line: usize) -> Option<usize> {
    rows.iter().position(|r| match r {
        Row::Turn(t) => turns[*t].lines.contains(&line),
        Row::Marker(_) => false,
    })
}

/// The line the playhead is inside.
pub fn line_at(lines: &[CaptionLine], playhead: MediaTime) -> Option<usize> {
    let i = lines
        .partition_point(|l| l.start <= playhead)
        .checked_sub(1)?;
    (playhead < lines[i].end).then_some(i)
}

/// The line the "now speaking" card shows: the one under the playhead, else the last one that
/// began (the card keeps the last words through a pause).
pub fn last_line_at(lines: &[CaptionLine], playhead: MediaTime) -> Option<usize> {
    lines
        .partition_point(|l| l.start <= playhead)
        .checked_sub(1)
}

/// The byte range of the word under the playhead inside the text of its line (the board marks
/// "pricing" while it is said). The text may have been edited, so the word is looked for by its
/// letters: the n-th time the same word is said in the line is the n-th match in the text.
pub fn highlight_in(
    text: &str,
    line: &CaptionLine,
    words: &[Word],
    playhead: MediaTime,
) -> Option<Range<usize>> {
    let norm = |s: &str| s.trim_matches(|c: char| !c.is_alphanumeric()).to_owned();
    let first = words.partition_point(|w| mid(w) < line.start);
    let in_line: Vec<&Word> = words[first..]
        .iter()
        .take_while(|w| mid(w) < line.end)
        .collect();
    let k = in_line
        .iter()
        .position(|w| w.start <= playhead && playhead < w.end)?;
    let target = norm(&in_line[k].text);
    if target.is_empty() {
        return None;
    }
    let nth = in_line[..k]
        .iter()
        .filter(|w| norm(&w.text) == target)
        .count();
    text.match_indices(&target)
        .nth(nth)
        .map(|(i, m)| i..i + m.len())
}

/// What the card above the timeline shows.
#[derive(Debug, Clone, PartialEq)]
pub struct NowSpeaking {
    /// The speaker's id; `None` when nobody was identified (or there was no diarization).
    pub speaker: Option<u32>,
    /// The caption line, with the word under the playhead marked.
    pub text: String,
    pub highlight: Option<Range<usize>>,
    /// Where the line began, for the time next to the name.
    pub since: MediaTime,
}

/// The card's content at `playhead`; `None` before the first line.
pub fn now_speaking(
    lines: &[CaptionLine],
    words: &[Word],
    labels: &[WordLabel],
    playhead: MediaTime,
) -> Option<NowSpeaking> {
    let i = last_line_at(lines, playhead)?;
    let line = &lines[i];
    let (speaker, _) = line_speaker(line, words, labels);
    Some(NowSpeaking {
        speaker,
        text: line.text.replace('\n', " "),
        highlight: highlight_in(&line.text.replace('\n', " "), line, words, playhead),
        since: line.start,
    })
}

// ---- the speakers lane ------------------------------------------------------------------

/// A coloured block of the speakers lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneBlock {
    pub start: MediaTime,
    pub end: MediaTime,
    pub speaker: u32,
}

/// Neighbouring segments of one speaker closer than `join` become one block, and the blocks are
/// in time order.
pub fn lane_blocks(segments: &[Segment], join: MediaTime) -> Vec<LaneBlock> {
    let mut sorted: Vec<&Segment> = segments.iter().collect();
    sorted.sort_by_key(|s| (s.start, s.end));
    let mut out: Vec<LaneBlock> = Vec::new();
    for s in sorted {
        match out.last_mut() {
            Some(b) if b.speaker == s.speaker && s.start <= b.end + join => {
                b.end = b.end.max(s.end);
            }
            _ => out.push(LaneBlock {
                start: s.start,
                end: s.end,
                speaker: s.speaker,
            }),
        }
    }
    out
}

// ---- waveforms --------------------------------------------------------------------------

/// A waveform on the session clock: one peak per `1 / per_sec` seconds, silence where the
/// recording was paused.
#[derive(Debug, Clone, PartialEq)]
pub struct Wave {
    pub per_sec: u32,
    pub peaks: Vec<f32>,
}

impl Wave {
    /// Moves the file's peaks onto the session clock (the pause gaps add their length).
    pub fn from_lane(lane: &LanePeaks, gaps: &[PauseGap], end: MediaTime) -> Wave {
        let per = f64::from(lane.per_sec.max(1));
        let len = (end.as_secs_f64() * per).ceil() as usize + 1;
        let mut peaks = vec![0.0_f32; len];
        for (i, p) in lane.peaks.iter().enumerate() {
            let file = lane.start + MediaTime::from_ticks((i as f64 / per * 120_000.0) as i64);
            let at = file_to_session(gaps, file, false).as_secs_f64();
            let slot = (at * per) as usize;
            if let Some(s) = peaks.get_mut(slot) {
                *s = s.max(*p);
            }
        }
        Wave {
            per_sec: lane.per_sec.max(1),
            peaks,
        }
    }

    /// `cols` heights (0.0 to 1.0) for the time from `from` to `to`: the loudest peak under each
    /// column.
    pub fn columns(&self, from: MediaTime, to: MediaTime, cols: usize) -> Vec<f32> {
        let per = f64::from(self.per_sec);
        let (a, b) = (from.as_secs_f64() * per, to.as_secs_f64() * per);
        let span = (b - a).max(f64::EPSILON);
        (0..cols)
            .map(|c| {
                let lo = a + span * c as f64 / cols as f64;
                let hi = a + span * (c + 1) as f64 / cols as f64;
                let (lo_i, hi_i) = (lo.floor().max(0.0) as usize, hi.ceil().max(0.0) as usize);
                self.peaks
                    .get(lo_i..hi_i.max(lo_i + 1).min(self.peaks.len()))
                    .map_or(0.0, |s| s.iter().fold(0.0_f32, |m, p| m.max(*p)))
            })
            .collect()
    }
}

/// A bar's height in px for a peak: a quiet floor so silence is still a line, the rest scaled
/// with a mild curve so speech does not all look the same.
pub fn bar_height(peak: f32, max_h: f32) -> f32 {
    let p = peak.clamp(0.0, 1.0).sqrt();
    (2.0 + p * (max_h - 2.0)).min(max_h)
}

// ---- the overview strip -----------------------------------------------------------------

/// The part of the whole recording the timeline shows, as fractions (0.0 to 1.0) of its length.
pub fn viewport_fractions(g: &Geom) -> (f32, f32) {
    let end = g.end.ticks().max(1) as f64;
    let from = g.scroll.ticks() as f64 / end;
    let to = from + 1.0 / g.zoom.max(1.0);
    (from.clamp(0.0, 1.0) as f32, to.clamp(0.0, 1.0) as f32)
}

/// The timeline's scroll that puts `t` in the middle of what is shown (a click on the overview),
/// kept inside the recording.
pub fn centered_scroll(g: &Geom, t: MediaTime) -> MediaTime {
    let visible = (g.end.ticks() as f64 / g.zoom.max(1.0)) as i64;
    let max = (g.end.ticks() - visible).max(0);
    MediaTime::from_ticks((t.ticks() - visible / 2).clamp(0, max))
}

/// Vertical layout of the audio lanes under the ruler, px from the top of the lanes' area.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneTops {
    pub markers: f32,
    pub speakers: f32,
    /// The top of each waveform row.
    pub waves: Vec<f32>,
    /// Height of the whole area.
    pub height: f32,
}

/// Height of the ruler at the top of the lanes' area.
pub const RULER_H: f32 = super::geom::RULER_H;
/// Height of the markers lane (the flags).
pub const MARKERS_H: f32 = 28.0;
/// Height of the speakers lane.
pub const SPEAKERS_H: f32 = 20.0;
/// Height of one waveform row.
pub const WAVE_H: f32 = 72.0;
/// Space between lanes.
pub const LANE_GAP: f32 = 8.0;

/// Where each lane goes for a recording with `waves` waveform rows.
pub fn lane_tops(waves: usize) -> LaneTops {
    let markers = RULER_H + 4.0;
    let speakers = markers + MARKERS_H + LANE_GAP / 2.0;
    let first = speakers + SPEAKERS_H + LANE_GAP;
    let tops: Vec<f32> = (0..waves)
        .map(|i| first + i as f32 * (WAVE_H + LANE_GAP))
        .collect();
    let height = tops.last().map_or(first, |t| t + WAVE_H) + 4.0;
    LaneTops {
        markers,
        speakers,
        waves: tops,
        height,
    }
}

/// Where a marker's tick goes on the overview (0.0 to 1.0).
pub fn fraction_of(t: MediaTime, end: MediaTime) -> f32 {
    (t.ticks() as f64 / end.ticks().max(1) as f64).clamp(0.0, 1.0) as f32
}

// ---- keys -------------------------------------------------------------------------------

/// Width of a marker flag (the white label chip with its bookmark glyph), px. Used by the
/// drawing and by the click test, so they agree.
pub fn flag_width(label: &str) -> f32 {
    let chars = label.chars().count().min(24) as f32;
    if label.is_empty() {
        26.0
    } else {
        34.0 + chars * 6.8
    }
}

/// The marker whose flag is under `x` (flags hang right of their time), the last one drawn on
/// top. `x_of` maps a time to the lane's x.
pub fn marker_at(markers: &[MarkerView], x_of: impl Fn(MediaTime) -> f32, x: f32) -> Option<usize> {
    markers.iter().enumerate().rev().find_map(|(i, m)| {
        let left = x_of(m.at);
        (x >= left - 2.0 && x <= left + flag_width(&m.label)).then_some(i)
    })
}

/// What the audio editor knows about a recording beyond the edit list: markers, speakers and
/// translations. Read again when the library changes (a marker, a diarization, a translation).
#[derive(Debug, Clone, Default)]
pub struct VoiceData {
    /// The recording is `kind: "audio"`.
    pub audio: bool,
    pub markers: Vec<MarkerView>,
    pub speakers: Vec<Speaker>,
    pub segments: Vec<Segment>,
    /// The translations by language code, with their lines.
    pub translations: Vec<(String, Vec<CaptionLine>)>,
    /// The reports of the recording (AI-9), oldest first. Filled by [`VoiceData::read`].
    pub reports: Vec<cm_ai::report::store::Report>,
    /// The use-case preset the recording was made with (`room`, `online`, ...), when it has one.
    pub preset: Option<String>,
    pub has_mic: bool,
    pub has_system: bool,
    /// When the recording was made (the date in the file tags).
    pub created: Option<time::OffsetDateTime>,
    /// Counts the reads, so a cache keyed on it knows the data changed.
    pub rev: u64,
}

impl VoiceData {
    /// From a manifest and the translations found in the bundle.
    pub fn from_parts(m: &RecManifest, translations: Vec<(String, Vec<CaptionLine>)>) -> VoiceData {
        let present = |name: &str| {
            m.tracks.get(name).is_some_and(|t| {
                t.get("missing").and_then(serde_json::Value::as_bool) != Some(true)
            })
        };
        let state = cm_ai::speakers::SpeakerState::from_manifest(m).unwrap_or_default();
        VoiceData {
            audio: m.capture_kind() == cm_library::manifest::CaptureKind::Audio,
            markers: markers_of(m),
            speakers: state.speakers,
            segments: state.segments,
            translations,
            reports: Vec::new(),
            preset: m
                .extra
                .get("preset")
                .or_else(|| m.settings.get("preset"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            has_mic: present("mic"),
            has_system: present("system"),
            created: Some(m.created),
            rev: 0,
        }
    }

    /// Reads the translations of the bundle in `dir` (a file that cannot be read is skipped).
    pub fn read(dir: &std::path::Path, m: &RecManifest) -> VoiceData {
        let translations = cm_ai::translate::list_translations(dir)
            .into_iter()
            .filter_map(|code| {
                let t = cm_ai::translate::read_translation(dir, &code).ok()??;
                Some((code, t.captions()))
            })
            .collect();
        let mut data = VoiceData::from_parts(m, translations);
        // A report that cannot be read is skipped (the next read tries again).
        let index = cm_ai::report::store::index_of(m);
        data.reports = index
            .iter()
            .filter_map(|e| cm_ai::report::store::read(dir, &index, &e.id).ok())
            .collect();
        data
    }

    /// The lines of a translation.
    pub fn lines_for(&self, language: &str) -> Option<&[CaptionLine]> {
        self.translations
            .iter()
            .find(|(c, _)| c == language)
            .map(|(_, l)| l.as_slice())
    }

    /// The audio tracks the recording has.
    pub fn available(&self) -> super::audio_export::Available {
        super::audio_export::Available {
            mic: self.has_mic,
            system: self.has_system,
        }
    }

    /// The name of a speaker.
    pub fn name_of(&self, id: u32) -> String {
        cm_ai::speakers::name_of(&self.speakers, id)
    }
}

/// The name of a language in that language (the switcher's chips), the code in capitals when it
/// is not one of the common ones.
pub fn language_name(code: &str) -> String {
    let base = cm_ai::translate::base_language(code);
    match base {
        "en" => "English",
        "vi" => "Ti\u{1EBF}ng Vi\u{1EC7}t",
        "es" => "Espa\u{F1}ol",
        "fr" => "Fran\u{E7}ais",
        "de" => "Deutsch",
        "pt" => "Portugu\u{EA}s",
        "it" => "Italiano",
        "ja" => "\u{65E5}\u{672C}\u{8A9E}",
        "ko" => "\u{D55C}\u{AD6D}\u{C5B4}",
        "zh" => "\u{4E2D}\u{6587}",
        "ru" => "\u{420}\u{443}\u{441}\u{441}\u{43A}\u{438}\u{439}",
        "id" => "Bahasa Indonesia",
        _ => return code.to_uppercase(),
    }
    .to_owned()
}

/// The languages the Translate chooser offers (code), most common first.
pub const TRANSLATE_TARGETS: [&str; 9] = ["en", "vi", "es", "fr", "de", "pt", "ja", "ko", "zh"];

/// The targets still worth offering: not the transcript's own language, not one already made.
pub fn translate_choices(original: &str, data: &VoiceData) -> Vec<&'static str> {
    let own = cm_ai::translate::base_language(original);
    TRANSLATE_TARGETS
        .into_iter()
        .filter(|c| *c != own && data.lines_for(c).is_none())
        .collect()
}

/// How far J and L jump, seconds.
pub const SHUTTLE_SECS: i64 = 5;

/// What a key does in the audio editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKey {
    Cmd(Cmd),
    /// Set a marker at the playhead (M).
    Marker,
}

/// The audio editor's keys: Space plays and pauses, J jumps back, K pauses, L plays (and jumps
/// forward while playing), M sets a marker; the editing keys are the video editor's, minus Zoom.
pub fn audio_key(key: &str, shift: bool, cmd: bool, alt: bool, playing: bool) -> Option<AudioKey> {
    if !cmd && !alt && !shift {
        match key {
            "m" => return Some(AudioKey::Marker),
            "j" => {
                return Some(AudioKey::Cmd(Cmd::StepSecs {
                    secs: -SHUTTLE_SECS,
                }))
            }
            "k" => return playing.then_some(AudioKey::Cmd(Cmd::PlayPause)),
            "l" => {
                return Some(AudioKey::Cmd(if playing {
                    Cmd::StepSecs { secs: SHUTTLE_SECS }
                } else {
                    Cmd::PlayPause
                }))
            }
            _ => {}
        }
    }
    match map_key(key, shift, cmd, alt)? {
        Cmd::AddZoom => None,
        c => Some(AudioKey::Cmd(c)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: i64) -> MediaTime {
        MediaTime::from_millis(ms)
    }

    fn line(a: i64, b: i64, text: &str) -> CaptionLine {
        CaptionLine {
            start: t(a),
            end: t(b),
            text: text.to_owned(),
        }
    }

    fn word(a: i64, b: i64, text: &str) -> Word {
        Word {
            text: text.to_owned(),
            start: t(a),
            end: t(b),
            confidence: 1.0,
        }
    }

    fn label(s: Option<u32>, overlap: bool) -> WordLabel {
        WordLabel {
            speaker: s,
            overlap,
        }
    }

    /// V4: the four speaker colours of the board, repeating, none of them a status colour.
    #[test]
    fn speaker_colours_repeat_and_avoid_status_colours() {
        assert_eq!(speaker_colour(0), Color::rgb(0x8FB0FF));
        assert_eq!(speaker_colour(3), Color::rgb(0xC9A7F5));
        assert_eq!(speaker_colour(4), speaker_colour(0));
        for status in [0xFF4F3A, 0x3DD68C, 0xF5B84A] {
            assert!(!SPEAKER_COLOURS.contains(&status));
        }
    }

    #[test]
    fn avatars_and_clocks() {
        assert_eq!(initial("Minh"), "M");
        assert_eq!(initial("Trung (me)"), "T");
        assert_eq!(initial("Speaker 4"), "4");
        assert_eq!(initial("  linh"), "L");
        assert_eq!(initial(""), "");
        assert_eq!(clock(t(18 * 60_000 + 22_000)), "18:22");
        assert_eq!(clock(t(3_725_000)), "1:02:05");
        assert_eq!(clock_tenths(t(18 * 60_000 + 24_600)), "18:24.6");
        assert_eq!(clock_tenths(t(0)), "0:00.0");
    }

    #[test]
    fn speaker_index_follows_the_list_position() {
        let list = vec![
            Speaker {
                id: 5,
                name: "A".into(),
            },
            Speaker {
                id: 2,
                name: "B".into(),
            },
        ];
        assert_eq!(speaker_index(&list, 2), 1);
        assert_eq!(speaker_index(&list, 5), 0);
        assert_eq!(speaker_index(&list, 3), 2);
    }

    /// VOC-1.AC4 / V4: a marker is stored on the recorded clock; it is drawn on the session clock
    /// and the two clocks agree through a pause.
    #[test]
    fn marker_clocks_agree_through_a_pause() {
        let gaps = vec![PauseGap {
            from: t(60_000),
            to: t(90_000),
        }];
        // 60 s into the file is after the pause, so 90 s on the session clock.
        assert_eq!(file_to_session(&gaps, t(61_000), false), t(91_000));
        assert_eq!(recorded_at(&gaps, t(91_000)), t(61_000));
        assert_eq!(recorded_at(&gaps, t(30_000)), t(30_000));
        // Inside the pause: where the pause began.
        assert_eq!(recorded_at(&gaps, t(75_000)), t(60_000));
        assert_eq!(recorded_at(&[], t(5_000)), t(5_000));
    }

    #[test]
    fn markers_of_a_manifest_move_to_the_session_clock() {
        let text = serde_json::json!({
            "schema": 1, "id": "rec_1", "title": "Sync", "kind": "audio",
            "created": "2026-10-26T09:00:00Z", "tracks": {},
            "markers": [{"at": 120_000 * 61, "label": "Pricing"}, {"at": 120_000 * 10}],
            "gaps": [{"from": 120_000 * 30, "to": 120_000 * 60}]
        })
        .to_string();
        let m = RecManifest::from_json(&text).unwrap_or_else(|e| panic!("{e:?}"));
        let v = markers_of(&m);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].at, t(10_000));
        assert_eq!(v[0].label, "");
        assert_eq!(v[1].label, "Pricing");
        // 61 s of recorded time is 91 s on the session clock after the 30 s pause.
        assert_eq!(v[1].at, t(91_000));
    }

    fn sample() -> (Vec<CaptionLine>, Vec<Word>, Vec<WordLabel>) {
        let lines = vec![
            line(0, 2_000, "Hello there."),
            line(2_000, 4_000, "How are you?"),
            line(4_000, 6_000, "Fine, thanks."),
            line(6_000, 8_000, "Oh really."),
        ];
        let words = vec![
            word(0, 1_000, "Hello"),
            word(1_000, 2_000, "there."),
            word(2_000, 3_000, "How"),
            word(3_000, 3_500, "are"),
            word(3_500, 4_000, "you?"),
            word(4_000, 5_000, "Fine,"),
            word(5_000, 6_000, "thanks."),
            word(6_000, 7_000, "Oh"),
            word(7_000, 8_000, "really."),
        ];
        let labels = vec![
            label(Some(1), false),
            label(Some(1), false),
            label(Some(1), false),
            label(Some(1), false),
            label(Some(1), false),
            label(Some(2), false),
            label(Some(2), true),
            label(None, false),
            label(None, false),
        ];
        (lines, words, labels)
    }

    /// V4: neighbouring lines of one speaker are one turn, the overlap flag is kept, and lines
    /// nobody was identified for stand alone.
    #[test]
    fn lines_group_into_turns_by_speaker() {
        let (lines, words, labels) = sample();
        let turns = build_turns(&lines, &words, &labels);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].speaker, Some(1));
        assert_eq!(turns[0].lines, vec![0, 1]);
        assert_eq!((turns[0].start, turns[0].end), (t(0), t(4_000)));
        assert!(!turns[0].overlap);
        assert_eq!(turns[1].speaker, Some(2));
        assert!(turns[1].overlap);
        assert_eq!(turns[2].speaker, None);
    }

    #[test]
    fn without_diarization_every_line_is_its_own_turn() {
        let (lines, words, _) = sample();
        let labels: Vec<WordLabel> = words.iter().map(|_| label(None, false)).collect();
        let turns = build_turns(&lines, &words, &labels);
        assert_eq!(turns.len(), 4);
        assert!(turns
            .iter()
            .all(|t| t.speaker.is_none() && t.lines.len() == 1));
    }

    #[test]
    fn a_line_takes_the_speaker_of_most_of_its_words() {
        let l = line(0, 4_000, "x");
        let words = vec![
            word(0, 1_000, "a"),
            word(1_000, 2_000, "b"),
            word(2_000, 3_000, "c"),
        ];
        let labels = vec![
            label(Some(2), false),
            label(Some(1), false),
            label(Some(1), false),
        ];
        assert_eq!(line_speaker(&l, &words, &labels), (Some(1), false));
        // A tie goes to the first speaker heard.
        let labels = vec![
            label(Some(2), false),
            label(Some(1), false),
            label(None, false),
        ];
        assert_eq!(line_speaker(&l, &words, &labels), (Some(2), false));
        assert_eq!(line_speaker(&l, &[], &[]), (None, false));
    }

    /// V4: markers sit between the turns, in time order, and the last ones go at the end.
    #[test]
    fn markers_go_between_the_turns() {
        let (lines, words, labels) = sample();
        let turns = build_turns(&lines, &words, &labels);
        let markers = vec![
            MarkerView {
                at: t(0),
                label: "Start".into(),
            },
            MarkerView {
                at: t(4_000),
                label: "Pricing".into(),
            },
            MarkerView {
                at: t(5_000),
                label: "Inside".into(),
            },
            MarkerView {
                at: t(60_000),
                label: "End".into(),
            },
        ];
        let r = rows(&turns, &markers);
        assert_eq!(
            r,
            vec![
                Row::Marker(0),
                Row::Turn(0),
                Row::Marker(1),
                Row::Turn(1),
                Row::Marker(2),
                Row::Turn(2),
                Row::Marker(3),
            ]
        );
        assert_eq!(row_of_line(&r, &turns, 1), Some(1));
        assert_eq!(row_of_line(&r, &turns, 2), Some(3));
        assert_eq!(rows(&turns, &[]).len(), 3);
        assert_eq!(rows(&[], &markers).len(), 4);
    }

    #[test]
    fn the_line_under_the_playhead_and_the_last_line_heard() {
        let (lines, ..) = sample();
        assert_eq!(line_at(&lines, t(0)), Some(0));
        assert_eq!(line_at(&lines, t(3_999)), Some(1));
        assert_eq!(line_at(&lines, t(8_000)), None);
        let gappy = vec![line(1_000, 2_000, "a"), line(5_000, 6_000, "b")];
        assert_eq!(line_at(&gappy, t(500)), None);
        assert_eq!(line_at(&gappy, t(3_000)), None);
        assert_eq!(last_line_at(&gappy, t(3_000)), Some(0));
        assert_eq!(last_line_at(&gappy, t(500)), None);
    }

    /// V4: the card marks the word being said, and finds the right one of two equal words.
    #[test]
    fn the_word_under_the_playhead_is_marked_in_the_text() {
        let l = line(0, 4_000, "So the pricing page, the pricing.");
        let words = vec![
            word(0, 500, " So"),
            word(500, 1_000, " the"),
            word(1_000, 2_000, " pricing"),
            word(2_000, 2_500, " page,"),
            word(2_500, 3_000, " the"),
            word(3_000, 4_000, " pricing."),
        ];
        let text = &l.text;
        let r = highlight_in(text, &l, &words, t(1_500)).unwrap();
        assert_eq!(&text[r.clone()], "pricing");
        assert_eq!(r.start, 7);
        let r2 = highlight_in(text, &l, &words, t(3_500)).unwrap();
        assert_eq!(r2.start, 25);
        assert_eq!(&text[r2], "pricing");
        assert!(highlight_in(text, &l, &words, t(9_000)).is_none());
        // An edited text without the word marks nothing.
        assert!(highlight_in("something else", &l, &words, t(1_500)).is_none());
    }

    #[test]
    fn now_speaking_has_the_speaker_the_line_and_the_start() {
        let (lines, words, labels) = sample();
        let n = now_speaking(&lines, &words, &labels, t(5_200)).unwrap();
        assert_eq!(n.speaker, Some(2));
        assert_eq!(n.text, "Fine, thanks.");
        assert_eq!(n.since, t(4_000));
        assert_eq!(n.highlight, Some(6..12));
        assert!(now_speaking(&[], &[], &[], t(0)).is_none());
    }

    /// V4: the speakers lane joins close segments of one speaker and keeps order.
    #[test]
    fn lane_blocks_join_close_segments_of_one_speaker() {
        let seg = |a, b, s| Segment::new(t(a), t(b), s);
        let blocks = lane_blocks(
            &[
                seg(5_000, 6_000, 2),
                seg(0, 1_000, 1),
                seg(1_200, 2_000, 1),
                seg(2_000, 3_000, 2),
                seg(9_000, 10_000, 1),
            ],
            t(500),
        );
        assert_eq!(
            blocks,
            vec![
                LaneBlock {
                    start: t(0),
                    end: t(2_000),
                    speaker: 1
                },
                LaneBlock {
                    start: t(2_000),
                    end: t(3_000),
                    speaker: 2
                },
                LaneBlock {
                    start: t(5_000),
                    end: t(6_000),
                    speaker: 2
                },
                LaneBlock {
                    start: t(9_000),
                    end: t(10_000),
                    speaker: 1
                },
            ]
        );
        assert!(lane_blocks(&[], t(500)).is_empty());
    }

    /// VOC-2.AC1: a waveform lands on the session clock: silence in a pause, the file's start
    /// respected, and a column is the loudest peak under it.
    #[test]
    fn waves_follow_the_session_clock() {
        let lane = LanePeaks {
            start: t(0),
            per_sec: 2,
            // 0..1 s file time: loud then quiet; 1..2 s: peak 0.9 in the second half.
            peaks: vec![0.8, 0.1, 0.2, 0.9],
        };
        // A 4 s pause after the first second of the file.
        let gaps = vec![PauseGap {
            from: t(1_000),
            to: t(5_000),
        }];
        let w = Wave::from_lane(&lane, &gaps, t(6_000));
        assert_eq!(w.per_sec, 2);
        assert_eq!(w.peaks[0], 0.8);
        assert_eq!(w.peaks[1], 0.1);
        assert_eq!(w.peaks[2], 0.0);
        assert_eq!(w.peaks[9], 0.0);
        assert_eq!(w.peaks[10], 0.2);
        assert_eq!(w.peaks[11], 0.9);
        let cols = w.columns(t(0), t(6_000), 3);
        assert_eq!(cols, vec![0.8, 0.0, 0.9]);
        // Zoomed far in, a column is one peak and out of range is silence.
        assert_eq!(w.columns(t(0), t(500), 1), vec![0.8]);
        assert_eq!(w.columns(t(100_000), t(101_000), 2), vec![0.0, 0.0]);
    }

    #[test]
    fn bars_have_a_floor_and_a_ceiling() {
        assert_eq!(bar_height(0.0, 40.0), 2.0);
        assert_eq!(bar_height(1.0, 40.0), 40.0);
        assert!(bar_height(0.25, 40.0) > bar_height(0.0625, 40.0));
        assert_eq!(bar_height(9.0, 40.0), 40.0);
    }

    #[test]
    fn the_overview_box_follows_the_timeline() {
        let mut g = Geom::new(800.0, t(100_000));
        assert_eq!(viewport_fractions(&g), (0.0, 1.0));
        g.zoom = 4.0;
        g.scroll = t(50_000);
        let (a, b) = viewport_fractions(&g);
        assert!((a - 0.5).abs() < 1e-6 && (b - 0.75).abs() < 1e-6);
        assert_eq!(fraction_of(t(25_000), t(100_000)), 0.25);
        assert_eq!(fraction_of(t(500_000), t(100_000)), 1.0);
    }

    #[test]
    fn translations_have_no_words_so_segments_decide_the_speaker() {
        let seg = |a, b, s| Segment::new(t(a), t(b), s);
        let lines = vec![
            line(0, 2_000, "a"),
            line(2_000, 4_000, "b"),
            line(4_000, 6_000, "c"),
            line(6_000, 8_000, "d"),
        ];
        let segments = vec![
            seg(0, 3_000, 1),
            seg(1_500, 2_500, 2), // half of "a": at once
            seg(3_000, 6_000, 2),
        ];
        let turns = build_turns_by_segments(&lines, &segments);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].speaker, Some(1));
        assert!(turns[0].overlap);
        assert_eq!(turns[0].lines, vec![0]);
        assert_eq!(turns[1].speaker, Some(2));
        assert_eq!(turns[1].lines, vec![1, 2]);
        assert_eq!(turns[2].speaker, None);
        assert_eq!(line_speaker_by_segments(&lines[3], &[]), (None, false));
    }

    #[test]
    fn flags_are_hit_where_they_are_drawn() {
        let markers = vec![
            MarkerView {
                at: t(1_000),
                label: "Beta".into(),
            },
            MarkerView {
                at: t(5_000),
                label: String::new(),
            },
        ];
        let x_of = |m: MediaTime| m.as_secs_f64() as f32 * 10.0;
        assert_eq!(marker_at(&markers, x_of, 12.0), Some(0));
        assert_eq!(
            marker_at(&markers, x_of, 10.0 + flag_width("Beta") + 5.0),
            None
        );
        assert_eq!(marker_at(&markers, x_of, 51.0), Some(1));
        assert_eq!(marker_at(&markers, x_of, 90.0), None);
        assert!(flag_width("a longer label") > flag_width("a"));
        assert_eq!(flag_width(""), 26.0);
    }

    #[test]
    fn voice_data_comes_from_the_manifest() {
        let text = serde_json::json!({
            "schema": 1, "id": "rec_1", "title": "Sync", "kind": "audio",
            "created": "2026-10-26T09:00:00Z",
            "tracks": {"mic": {"file": "mic.wav"}, "system": {"file": "system.wav", "missing": true}},
            "markers": [{"at": 120_000, "label": "A"}],
            "speakers": [{"id": 1, "name": "Trung (me)"}, {"id": 2, "name": "Linh"}],
            "speaker_segments": [{"start": 0, "end": 120_000, "speaker": 2}]
        })
        .to_string();
        let m = RecManifest::from_json(&text).unwrap_or_else(|e| panic!("{e:?}"));
        let d = VoiceData::from_parts(&m, vec![("vi".to_owned(), vec![line(0, 1_000, "Xin")])]);
        assert!(d.audio && d.has_mic && !d.has_system);
        assert_eq!(d.markers.len(), 1);
        assert_eq!(d.speakers.len(), 2);
        assert_eq!(d.segments.len(), 1);
        assert_eq!(d.name_of(2), "Linh");
        assert_eq!(d.name_of(9), "Speaker 9");
        assert!(d.lines_for("vi").is_some() && d.lines_for("en").is_none());
        assert!(d
            .available()
            .allows(super::super::audio_export::Tracks::Mic));
        assert!(!d
            .available()
            .allows(super::super::audio_export::Tracks::System));
        // A recording without speakers or markers has none.
        let bare = serde_json::json!({"schema": 1, "id": "r", "title": "t",
            "created": "2026-10-26T09:00:00Z"})
        .to_string();
        let m2 = RecManifest::from_json(&bare).unwrap_or_else(|e| panic!("{e:?}"));
        let b = VoiceData::from_parts(&m2, Vec::new());
        assert!(!b.audio && b.speakers.is_empty() && b.markers.is_empty());
    }

    #[test]
    fn language_names_and_translate_choices() {
        assert_eq!(language_name("vi"), "Ti\u{1EBF}ng Vi\u{1EC7}t");
        assert_eq!(language_name("pt-br"), "Portugu\u{EA}s");
        assert_eq!(language_name("xx"), "XX");
        let mut d = VoiceData::default();
        d.translations.push(("vi".into(), Vec::new()));
        let c = translate_choices("en", &d);
        assert!(!c.contains(&"en") && !c.contains(&"vi") && c.contains(&"es"));
        // The transcript's own language (with a region) is not offered either.
        assert!(!translate_choices("fr-CA", &VoiceData::default()).contains(&"fr"));
    }

    #[test]
    fn lanes_stack_under_the_ruler_without_overlap() {
        let l = lane_tops(2);
        assert!(l.markers >= RULER_H);
        assert!(l.speakers >= l.markers + MARKERS_H);
        assert!(l.waves[0] >= l.speakers + SPEAKERS_H);
        assert!(l.waves[1] >= l.waves[0] + WAVE_H);
        assert!(l.height >= l.waves[1] + WAVE_H);
        assert_eq!(lane_tops(1).waves.len(), 1);
        // No waveform rows: the area still holds the markers and the speakers.
        let none = lane_tops(0);
        assert!(none.waves.is_empty() && none.height > none.speakers);
    }

    #[test]
    fn the_overview_click_centres_the_view_inside_the_recording() {
        let mut g = Geom::new(800.0, t(100_000));
        g.zoom = 4.0;
        // The view is 25 s wide.
        assert_eq!(centered_scroll(&g, t(50_000)), t(37_500));
        assert_eq!(centered_scroll(&g, t(2_000)), t(0));
        assert_eq!(centered_scroll(&g, t(99_000)), t(75_000));
        g.zoom = 1.0;
        assert_eq!(centered_scroll(&g, t(50_000)), t(0));
    }

    /// VOC-2: Space, J, K, L and M.
    #[test]
    fn audio_keys() {
        let k = |key, playing| audio_key(key, false, false, false, playing);
        assert_eq!(k("space", false), Some(AudioKey::Cmd(Cmd::PlayPause)));
        assert_eq!(k("m", false), Some(AudioKey::Marker));
        assert_eq!(
            k("j", true),
            Some(AudioKey::Cmd(Cmd::StepSecs { secs: -5 }))
        );
        // K pauses, and does nothing when already paused.
        assert_eq!(k("k", true), Some(AudioKey::Cmd(Cmd::PlayPause)));
        assert_eq!(k("k", false), None);
        // L plays, and jumps ahead while playing (not the video editor's loop).
        assert_eq!(k("l", false), Some(AudioKey::Cmd(Cmd::PlayPause)));
        assert_eq!(k("l", true), Some(AudioKey::Cmd(Cmd::StepSecs { secs: 5 })));
        // The editing keys stay; Zoom has no meaning for audio.
        assert_eq!(k("s", false), Some(AudioKey::Cmd(Cmd::Split)));
        assert_eq!(k("z", false), None);
        assert_eq!(
            audio_key("z", false, true, false, false),
            Some(AudioKey::Cmd(Cmd::Undo))
        );
        // Cmd+M is not a marker.
        assert_eq!(audio_key("m", false, true, false, false), None);
    }
}
