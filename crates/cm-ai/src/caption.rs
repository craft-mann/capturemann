//! Captions (engine stream): lines built from a transcript, SRT and VTT output, and the mapping
//! through the edit list (cuts remove lines, speed scales times).
//!
//! * [`build_captions`]: at most 42 characters per row, 2 rows, 1 to 7 s per line; breaks at
//!   pauses, sentence ends and when a row would overflow; two-row lines are split near the
//!   middle.
//! * [`to_srt`] and [`to_vtt`] write lines as they are, so give them lines on the **output**
//!   timeline.
//! * [`map_words_to_timeline`] and [`captions_for_export`] move an unedited transcript onto the
//!   output timeline (AI-1.AC4); [`map_lines_to_timeline`] does it for lines the user edited.

use cm_core::edit::Timeline;
use cm_core::{MediaTime, TICKS_PER_SECOND};
use serde::{Deserialize, Serialize};

use crate::transcribe::Word;

/// Characters per caption row.
pub const MAX_ROW_CHARS: usize = 42;
/// Rows per caption line.
pub const MAX_ROWS: usize = 2;
/// Shortest and longest time a line stays on screen (seconds).
pub const MIN_LINE_SECS: f64 = 1.0;
pub const MAX_LINE_SECS: f64 = 7.0;
/// A silence longer than this between two words starts a new line (seconds).
pub const BREAK_GAP_SECS: f64 = 0.8;

/// One caption line (at most 42 characters per row, 2 rows, 1 to 7 s), in session time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionLine {
    pub start: MediaTime,
    pub end: MediaTime,
    /// Rows joined by `\n`.
    pub text: String,
}

fn secs(t: MediaTime) -> f64 {
    t.as_secs_f64()
}

fn ticks_of(secs: f64) -> MediaTime {
    MediaTime::from_ticks((secs * TICKS_PER_SECOND as f64).round() as i64)
}

/// Greedy word wrap at `width` characters. A word longer than `width` gets its own row.
pub fn wrap_rows(text: &str, width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match rows.last_mut() {
            Some(row) if row.chars().count() + 1 + word.chars().count() <= width => {
                row.push(' ');
                row.push_str(word);
            }
            _ => rows.push(word.to_owned()),
        }
    }
    rows
}

/// The rows of a line: one row when it fits, else two rows split as evenly as the width allows,
/// else the greedy wrap.
pub(crate) fn rows_for(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let total: usize =
        words.iter().map(|w| w.chars().count()).sum::<usize>() + words.len().saturating_sub(1);
    if total <= MAX_ROW_CHARS {
        return vec![words.join(" ")];
    }
    let mut best: Option<(usize, usize)> = None; // (imbalance, split index)
    for i in 1..words.len() {
        let left = words[..i].join(" ");
        let right = words[i..].join(" ");
        let (l, r) = (left.chars().count(), right.chars().count());
        if l <= MAX_ROW_CHARS && r <= MAX_ROW_CHARS {
            let imbalance = l.abs_diff(r);
            if best.is_none_or(|(b, _)| imbalance < b) {
                best = Some((imbalance, i));
            }
        }
    }
    match best {
        Some((_, i)) => vec![words[..i].join(" "), words[i..].join(" ")],
        None => wrap_rows(text, MAX_ROW_CHARS),
    }
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end_matches(['"', '\'', ')', '”', '’'])
        .ends_with(['.', '?', '!', '…', '。', '？', '！'])
}

fn ends_clause(text: &str) -> bool {
    text.ends_with([',', ';', ':', '，', '；'])
}

/// Builds caption lines from words (session time, in order). Words with empty text are skipped.
pub fn build_captions(words: &[Word]) -> Vec<CaptionLine> {
    let mut lines: Vec<CaptionLine> = Vec::new();
    let mut cur: Vec<&Word> = Vec::new();
    let flush = |cur: &mut Vec<&Word>, lines: &mut Vec<CaptionLine>| {
        if let (Some(first), Some(last)) = (cur.first(), cur.last()) {
            let text = cur
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(CaptionLine {
                start: first.start,
                end: last.end.max(first.start),
                text: rows_for(&text).join("\n"),
            });
        }
        cur.clear();
    };
    for w in words.iter().filter(|w| !w.text.trim().is_empty()) {
        if let Some(last) = cur.last() {
            let first = cur[0];
            let gap = secs(w.start - last.end);
            let candidate = cur
                .iter()
                .map(|x| x.text.as_str())
                .chain([w.text.as_str()])
                .collect::<Vec<_>>()
                .join(" ");
            let rows = rows_for(&candidate).len();
            let dur = secs(w.end - first.start);
            if gap > BREAK_GAP_SECS || rows > MAX_ROWS || dur > MAX_LINE_SECS {
                flush(&mut cur, &mut lines);
            }
        }
        cur.push(w);
        let first = cur[0];
        let dur = secs(w.end - first.start);
        let chars: usize = cur
            .iter()
            .map(|x| x.text.chars().count() + 1)
            .sum::<usize>()
            - 1;
        let sentence_over = ends_sentence(&w.text) && (cur.len() >= 3 || dur >= MIN_LINE_SECS);
        let clause_over = ends_clause(&w.text) && chars >= 60;
        if sentence_over || clause_over {
            flush(&mut cur, &mut lines);
        }
    }
    flush(&mut cur, &mut lines);
    // A line stays up for at least a second when the next one leaves room.
    for i in 0..lines.len() {
        let room = lines.get(i + 1).map(|n| n.start);
        let want = lines[i].start + ticks_of(MIN_LINE_SECS);
        if lines[i].end < want {
            lines[i].end = match room {
                Some(next) => want.min(next).max(lines[i].end),
                None => want,
            };
        }
    }
    lines
}

fn clock(t: MediaTime, comma: bool) -> String {
    let ms = ((t.ticks().max(0) as f64) * 1000.0 / TICKS_PER_SECOND as f64).round() as i64;
    let (h, m, s, milli) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let sep = if comma { ',' } else { '.' };
    format!("{h:02}:{m:02}:{s:02}{sep}{milli:03}")
}

/// SubRip (`.srt`): numbered cues, `HH:MM:SS,mmm`. Lines with no text are skipped.
pub fn to_srt(lines: &[CaptionLine]) -> String {
    let mut out = String::new();
    let mut n = 0;
    for l in lines.iter().filter(|l| !l.text.trim().is_empty()) {
        n += 1;
        out.push_str(&format!(
            "{n}\n{} --> {}\n{}\n\n",
            clock(l.start, true),
            clock(l.end.max(l.start), true),
            l.text.trim()
        ));
    }
    out
}

/// WebVTT (`.vtt`): `HH:MM:SS.mmm`, with `&`, `<` and `>` escaped in the text.
pub fn to_vtt(lines: &[CaptionLine]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for l in lines.iter().filter(|l| !l.text.trim().is_empty()) {
        let text = l
            .text
            .trim()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out.push_str(&format!(
            "{} --> {}\n{text}\n\n",
            clock(l.start, false),
            clock(l.end.max(l.start), false)
        ));
    }
    out
}

/// Plain text (`.txt`) for other uses: one caption cue per line, its rows joined with a space so
/// each line reads as a sentence. With `timestamps` every line starts with `[mm:ss]` (or
/// `[hh:mm:ss]` for the whole file when a line starts at one hour or later, so the lines align).
/// Lines with no text are skipped; the file ends with one newline (empty when there is no text).
pub fn to_txt(lines: &[CaptionLine], timestamps: bool) -> String {
    let kept: Vec<&CaptionLine> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    let hours = kept
        .iter()
        .any(|l| l.start.ticks() >= 3600 * TICKS_PER_SECOND);
    let mut out = String::new();
    for l in kept {
        if timestamps {
            let secs = (l.start.ticks().max(0) as f64 / TICKS_PER_SECOND as f64).floor() as i64;
            let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
            if hours {
                out.push_str(&format!("[{h:02}:{m:02}:{s:02}] "));
            } else {
                out.push_str(&format!("[{m:02}:{s:02}] "));
            }
        }
        let text: Vec<&str> = l.text.split_whitespace().collect();
        out.push_str(&text.join(" "));
        out.push('\n');
    }
    out
}

/// Output time of a source time inside piece `p`, clamped to the piece.
fn into_piece(p: &cm_core::edit::Piece, t: MediaTime) -> MediaTime {
    let t = t.clamp(p.source.from, p.source.to);
    let into = (t - p.source.from).ticks() as f64 / p.factor;
    p.out_start + MediaTime::from_ticks(into.round() as i64)
}

/// Moves words onto the output timeline (AI-1.AC4): a word whose middle is cut or outside the
/// trim is dropped, a word spoken while the audio is muted (speed above 2x without "keep audio")
/// is dropped, and the others take their output times, scaled by the segment's speed.
pub fn map_words_to_timeline(words: &[Word], tl: &Timeline) -> Vec<Word> {
    words
        .iter()
        .filter_map(|w| {
            let mid = MediaTime::from_ticks((w.start.ticks() + w.end.ticks()) / 2);
            let p = tl
                .pieces()
                .iter()
                .find(|p| p.source.from <= mid && mid < p.source.to)?;
            if !p.audio {
                return None;
            }
            let start = into_piece(p, w.start);
            let end = into_piece(p, w.end).max(start);
            Some(Word {
                start,
                end,
                ..w.clone()
            })
        })
        .collect()
}

/// Captions for an export: the words that survive the edit list, laid out again on the output
/// timeline. Cut content is never referenced (AI-1.AC4).
pub fn captions_for_export(words: &[Word], tl: &Timeline) -> Vec<CaptionLine> {
    build_captions(&map_words_to_timeline(words, tl))
}

/// The caption lines for an output timeline, for burning in or for the editor's preview: the
/// user's edited lines when there are any, else lines built from the transcript's words; either
/// way cut content is gone and speed is applied (AI-1.AC4).
pub fn cues_for_timeline(
    stored: Option<&[CaptionLine]>,
    words: &[Word],
    tl: &Timeline,
) -> Vec<CaptionLine> {
    match stored {
        Some(lines) if !lines.is_empty() => map_lines_to_timeline(lines, tl),
        _ => captions_for_export(words, tl),
    }
}

/// Moves caption lines the user edited onto the output timeline. A line inside one kept piece is
/// mapped as is and a line cut away is dropped. A line that spans a cut has no per-word times, so
/// each word is placed by its position in the line's time: words that land inside the cut are
/// dropped (no caption references cut content, AI-1.AC4) and the rest become one line per kept
/// piece. The placement is an estimate; the transcript's own words are exact
/// ([`captions_for_export`]).
pub fn map_lines_to_timeline(lines: &[CaptionLine], tl: &Timeline) -> Vec<CaptionLine> {
    let audible: Vec<&cm_core::edit::Piece> = tl.pieces().iter().filter(|p| p.audio).collect();
    let mut out = Vec::new();
    for line in lines {
        if line.end <= line.start {
            continue;
        }
        let whole = audible
            .iter()
            .find(|p| p.source.from <= line.start && line.end <= p.source.to);
        if let Some(p) = whole {
            out.push(CaptionLine {
                start: into_piece(p, line.start),
                end: into_piece(p, line.end),
                text: line.text.clone(),
            });
            continue;
        }
        let words: Vec<&str> = line.text.split_whitespace().collect();
        let n = words.len() as i64;
        if n == 0 {
            continue;
        }
        let dur = (line.end - line.start).ticks();
        // Word `i` occupies [from_i, to_i) of the line's time.
        let slot = |i: i64| {
            (
                line.start + MediaTime::from_ticks(dur * i / n),
                line.start + MediaTime::from_ticks(dur * (i + 1) / n),
            )
        };
        let mut groups: Vec<(usize, i64, i64)> = Vec::new(); // (piece index, first word, last word)
        for i in 0..n {
            let (from, to) = slot(i);
            let mid = MediaTime::from_ticks((from.ticks() + to.ticks()) / 2);
            let Some(pi) = audible
                .iter()
                .position(|p| p.source.from <= mid && mid < p.source.to)
            else {
                continue;
            };
            match groups.last_mut() {
                Some(g) if g.0 == pi && g.2 + 1 == i => g.2 = i,
                _ => groups.push((pi, i, i)),
            }
        }
        for (pi, first, last) in groups {
            let p = audible[pi];
            let start = slot(first).0.max(p.source.from);
            let end = slot(last).1.min(p.source.to);
            if start >= end {
                continue;
            }
            let chunk = &words[first as usize..=last as usize];
            out.push(CaptionLine {
                start: into_piece(p, start),
                end: into_piece(p, end),
                text: rows_for(&chunk.join(" ")).join("\n"),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::media_secs;
    use cm_core::edit::{EditContext, EditList, Span, SpeedSpan};

    fn w(text: &str, start: f64, end: f64) -> Word {
        Word {
            text: text.into(),
            start: media_secs(start),
            end: media_secs(end),
            confidence: 0.9,
        }
    }

    fn cue(start: f64, text: &str) -> CaptionLine {
        CaptionLine {
            start: media_secs(start),
            end: media_secs(start + 2.0),
            text: text.into(),
        }
    }

    /// AI-1: the plain-text export joins the rows of a cue into one readable line.
    #[test]
    fn txt_joins_rows_and_skips_empty_lines() {
        let lines = [
            cue(0.0, "Hello there,\nthis is a test."),
            cue(3.0, "   "),
            cue(5.0, "Second  line."),
        ];
        assert_eq!(
            to_txt(&lines, false),
            "Hello there, this is a test.\nSecond line.\n"
        );
        assert_eq!(to_txt(&[], false), "");
    }

    #[test]
    fn txt_timestamps_use_minutes_or_hours_for_the_whole_file() {
        let short = [cue(0.0, "a"), cue(65.4, "b"), cue(3599.9, "c")];
        assert_eq!(to_txt(&short, true), "[00:00] a\n[01:05] b\n[59:59] c\n");
        let long = [cue(5.0, "a"), cue(3600.0, "b"), cue(3725.0, "c")];
        assert_eq!(
            to_txt(&long, true),
            "[00:00:05] a\n[01:00:00] b\n[01:02:05] c\n"
        );
    }

    /// Cuts apply to the txt export like the other formats (AI-1.AC4): a cut line is absent and
    /// later timestamps shift.
    #[test]
    fn txt_follows_the_edit_mapping() {
        let lines = [
            cue(10.0, "kept first"),
            cue(62.0, "inside the cut"),
            cue(80.0, "kept last"),
        ];
        let tl = timeline(&[(60.0, 70.0)], &[], 120.0);
        let mapped = map_lines_to_timeline(&lines, &tl);
        assert_eq!(
            to_txt(&mapped, true),
            "[00:10] kept first\n[01:10] kept last\n"
        );
    }

    /// Words spoken evenly: `n` words of 0.3 s starting at `t0`.
    fn speech(t0: f64, text: &str) -> Vec<Word> {
        text.split(' ')
            .enumerate()
            .map(|(i, x)| w(x, t0 + i as f64 * 0.35, t0 + i as f64 * 0.35 + 0.3))
            .collect()
    }

    fn timeline(cuts: &[(f64, f64)], speeds: &[(f64, f64, f64)], end: f64) -> Timeline {
        let mut l = EditList::full(&EditContext {
            end: media_secs(end),
            live_at: None,
            gaps: &[],
        });
        l.cuts = cuts
            .iter()
            .map(|(a, b)| Span::new(media_secs(*a), media_secs(*b)))
            .collect();
        l.speeds = speeds
            .iter()
            .map(|(a, b, f)| SpeedSpan {
                span: Span::new(media_secs(*a), media_secs(*b)),
                factor: *f,
                keep_audio: false,
            })
            .collect();
        l.timeline()
    }

    #[test]
    fn rows_fit_the_width_and_two_row_lines_split_near_the_middle() {
        assert_eq!(wrap_rows("aaa bbb ccc", 7), ["aaa bbb", "ccc"]);
        assert_eq!(
            wrap_rows("averyveryverylongword x", 5),
            ["averyveryverylongword", "x"]
        );
        let text = "this is a sentence that is long enough to need two rows on screen";
        let rows = rows_for(text);
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter().all(|r| r.chars().count() <= MAX_ROW_CHARS),
            "{rows:?}"
        );
        let diff = rows[0].chars().count().abs_diff(rows[1].chars().count());
        assert!(diff < 12, "{rows:?}");
        assert_eq!(rows_for("short one"), ["short one"]);
    }

    #[test]
    fn lines_obey_the_size_and_time_rules() {
        let base = "So today we are going to look at how the new export sheet works and then \
                    we will walk through every preset one by one and compare the results";
        let text = format!("{base} {base}");
        let text = text.as_str();
        let lines = build_captions(&speech(0.0, text));
        assert!(lines.len() >= 3, "{lines:?}");
        for l in &lines {
            let rows: Vec<&str> = l.text.split('\n').collect();
            assert!(rows.len() <= MAX_ROWS, "{l:?}");
            assert!(
                rows.iter().all(|r| r.chars().count() <= MAX_ROW_CHARS),
                "{l:?}"
            );
            let d = secs(l.end - l.start);
            assert!(
                (MIN_LINE_SECS - 1e-6..=MAX_LINE_SECS + 1e-6).contains(&d),
                "{d} {l:?}"
            );
        }
        // Every word appears once, in order.
        let joined = lines
            .iter()
            .map(|l| l.text.replace('\n', " "))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(joined, text);
        // Lines never overlap.
        assert!(lines.windows(2).all(|p| p[0].end <= p[1].start));
    }

    #[test]
    fn a_pause_or_a_sentence_end_starts_a_new_line() {
        let mut words = speech(0.0, "Hello there my friend.");
        words.extend(speech(1.6, "Second one here"));
        let lines = build_captions(&words);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Hello there my friend.");
        let mut gapped = speech(0.0, "one two");
        gapped.extend(speech(5.0, "three four"));
        assert_eq!(build_captions(&gapped).len(), 2);
    }

    #[test]
    fn a_short_line_is_held_for_a_second_when_room_allows() {
        let lines = build_captions(&[
            w("Yes.", 2.0, 2.3),
            w("Later", 6.0, 6.4),
            w("words", 6.5, 6.9),
        ]);
        assert_eq!(lines[0].end, media_secs(3.0));
        // Not past the next line: the next one starts 0.95 s in, so the hold stops there.
        let tight = build_captions(&[w("Hi.", 0.0, 0.1), w("Next", 0.95, 1.3)]);
        assert_eq!(tight.len(), 2);
        assert_eq!(tight[0].end, media_secs(0.95));
    }

    #[test]
    fn no_words_no_lines() {
        assert!(build_captions(&[]).is_empty());
        assert!(build_captions(&[w("  ", 0.0, 1.0)]).is_empty());
    }

    #[test]
    fn srt_and_vtt_format() {
        let lines = vec![
            CaptionLine {
                start: media_secs(1.5),
                end: media_secs(3.25),
                text: "Hello\nworld".into(),
            },
            CaptionLine {
                start: media_secs(3600.0 + 61.001),
                end: media_secs(3600.0 + 62.0),
                text: "Tom & <Jerry>".into(),
            },
        ];
        assert_eq!(
            to_srt(&lines),
            "1\n00:00:01,500 --> 00:00:03,250\nHello\nworld\n\n2\n01:01:01,001 --> 01:01:02,000\nTom & <Jerry>\n\n"
        );
        assert_eq!(
            to_vtt(&lines),
            "WEBVTT\n\n00:00:01.500 --> 00:00:03.250\nHello\nworld\n\n01:01:01.001 --> 01:01:02.000\nTom &amp; &lt;Jerry&gt;\n\n"
        );
        assert_eq!(to_srt(&[]), "");
        assert_eq!(to_vtt(&[]), "WEBVTT\n\n");
    }

    /// AI-1.AC4: a cut from 1:00 to 1:10 shifts later captions by -10 s and no caption mentions
    /// the cut words.
    #[test]
    fn ac4_a_cut_shifts_later_lines_and_drops_cut_words() {
        let mut words = speech(58.0, "before the cut.");
        words.extend(speech(61.0, "secret words inside the cut."));
        words.extend(speech(71.0, "after the cut."));
        let tl = timeline(&[(60.0, 70.0)], &[], 100.0);
        let lines = captions_for_export(&words, &tl);
        let srt = to_srt(&lines);
        assert!(!srt.contains("secret"), "{srt}");
        let texts: Vec<String> = lines.iter().map(|l| l.text.replace('\n', " ")).collect();
        assert_eq!(texts, ["before the cut.", "after the cut."]);
        // "after" was spoken at 71.0 and is shown at 61.0.
        assert_eq!(lines[1].start, media_secs(61.0));
        assert!(srt.contains("00:01:01,000 --> "), "{srt}");
        // Mapped words keep their order and spacing.
        let mapped = map_words_to_timeline(&words, &tl);
        assert_eq!(mapped.len(), 6);
        assert_eq!(mapped[3].start, media_secs(61.0));
    }

    #[test]
    fn trim_and_speed_clip_and_scale() {
        let mut l = EditList::full(&EditContext {
            end: media_secs(100.0),
            live_at: None,
            gaps: &[],
        });
        l.start = media_secs(10.0);
        l.end = media_secs(40.0);
        l.speeds = vec![SpeedSpan {
            span: Span::new(media_secs(20.0), media_secs(30.0)),
            factor: 2.0,
            keep_audio: false,
        }];
        let tl = l.timeline();
        let words = [
            w("early", 5.0, 5.5),  // before the trim
            w("kept", 12.0, 12.5), // 2 s into the kept range
            w("fast", 24.0, 25.0), // at 2x: 5 s output
            w("late", 45.0, 45.5), // after the trim
        ];
        let mapped = map_words_to_timeline(&words, &tl);
        let names: Vec<&str> = mapped.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(names, ["kept", "fast"]);
        assert_eq!(mapped[0].start, media_secs(2.0));
        // 10 s of source before the sped span, then (24-20)/2 = 2 s into it.
        assert_eq!(mapped[1].start, media_secs(12.0));
        assert_eq!(mapped[1].end, media_secs(12.5));
    }

    /// AI-1 burn-in: edited lines win over the transcript, a missing or empty edit falls back to
    /// the words, and either way the cut is applied.
    #[test]
    fn cues_for_timeline_prefers_edited_lines_and_applies_the_cut() {
        let tl = timeline(&[(10.0, 14.0)], &[], 30.0);
        let words = speech(2.0, "from the transcript words");
        let edited = [CaptionLine {
            start: media_secs(20.0),
            end: media_secs(22.0),
            text: "edited line".into(),
        }];
        let a = cues_for_timeline(Some(&edited), &words, &tl);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].text, "edited line");
        assert_eq!(a[0].start, media_secs(16.0), "shifted by the 4 s cut");
        let b = cues_for_timeline(None, &words, &tl);
        assert_eq!(b[0].text, "from the transcript words");
        let c = cues_for_timeline(Some(&[]), &words, &tl);
        assert_eq!(c, b, "an empty edit means no edit");
    }

    #[test]
    fn words_in_muted_fast_segments_are_dropped() {
        let tl = timeline(&[], &[(10.0, 20.0, 4.0)], 30.0);
        let mapped =
            map_words_to_timeline(&[w("inaudible", 12.0, 12.5), w("audible", 22.0, 22.5)], &tl);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].text, "audible");
    }

    #[test]
    fn edited_lines_are_mapped_dropped_or_split_at_the_cut() {
        let tl = timeline(&[(10.0, 14.0)], &[], 30.0);
        let line = |a: f64, b: f64, t: &str| CaptionLine {
            start: media_secs(a),
            end: media_secs(b),
            text: t.into(),
        };
        let out = map_lines_to_timeline(
            &[
                line(1.0, 3.0, "kept as is"),
                line(11.0, 13.0, "inside the cut"),
                line(8.0, 16.0, "one two three four five six seven eight"),
                line(20.0, 22.0, "after"),
            ],
            &tl,
        );
        assert_eq!(out[0].text, "kept as is");
        assert_eq!(out[0].start, media_secs(1.0));
        // The straddling line has 8 words over 8 s. Words 3 to 6 sit inside the cut and are
        // dropped; "one two" ends at the cut and "seven eight" starts at its end (10 s out).
        assert_eq!(out[1].text, "one two");
        assert_eq!(
            (out[1].start, out[1].end),
            (media_secs(8.0), media_secs(10.0))
        );
        assert_eq!(out[2].text, "seven eight");
        assert_eq!(
            (out[2].start, out[2].end),
            (media_secs(10.0), media_secs(12.0))
        );
        assert_eq!(out[3].text, "after");
        assert_eq!(out[3].start, media_secs(16.0));
        assert!(out.iter().all(|l| !l.text.contains("inside")));
        assert!(out.iter().all(|l| !l.text.contains("three")));
        assert_eq!(out.len(), 4);
    }
}
