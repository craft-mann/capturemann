use super::*;
use crate::transcribe::media_secs;

fn t(s: f64) -> MediaTime {
    media_secs(s)
}

fn word(text: &str, a: f64, b: f64) -> Word {
    Word {
        text: text.into(),
        start: t(a),
        end: t(b),
        confidence: 0.9,
    }
}

fn seg(a: f64, b: f64, spk: u32, energy: f32) -> Segment {
    Segment {
        start: t(a),
        end: t(b),
        speaker: spk,
        energy,
    }
}

fn opts<'a>(headset: bool) -> DiarizeOptions<'a> {
    DiarizeOptions {
        hints: DiarizeHints::default(),
        headset,
        display_name: "Trung",
        created: "2026-10-26T10:00:00Z",
    }
}

fn silence(secs: f64) -> Vec<f32> {
    vec![0.0; (secs * f64::from(SAMPLE_RATE)) as usize]
}

fn run(script: &[(f64, f64, u32, f32)], secs: f64, headset: bool) -> Diarized {
    let d = ScriptedDiarizer::from_secs(script);
    let mix = silence(secs);
    let input = DiarizeInput {
        mix: &mix,
        mic: Some(&mix),
        system: Some(&mix),
        start: MediaTime::ZERO,
        gaps: &[],
    };
    diarize_audio(&d, &input, &opts(headset), &mut |_| true).unwrap()
}

fn plain_input(mix: &[f32]) -> DiarizeInput<'_> {
    DiarizeInput {
        mix,
        mic: None,
        system: None,
        start: MediaTime::ZERO,
        gaps: &[],
    }
}

/// AI-8: speakers are numbered and named by first speech, whatever labels the model used.
#[test]
fn speakers_are_named_by_first_speech() {
    let r = run(
        &[(0.0, 4.0, 7, 0.1), (4.0, 8.0, 3, 0.1), (8.0, 12.0, 7, 0.1)],
        15.0,
        false,
    );
    let names: Vec<_> = r.speakers.iter().map(|s| (s.id, s.name.as_str())).collect();
    assert_eq!(names, vec![(1, "Speaker 1"), (2, "Speaker 2")]);
    let order: Vec<u32> = r.file.segments.iter().map(|s| s.speaker).collect();
    assert_eq!(order, vec![1, 2, 1]);
    assert!(r.file.note.is_none());
    assert_eq!(r.file.model, "scripted");
}

/// AI-8 edge cases: one speaker gets a note, a short recording is skipped with a note.
#[test]
fn one_speaker_and_short_recordings() {
    let r = run(&[(0.0, 12.0, 4, 0.1)], 15.0, false);
    assert_eq!(r.speakers.len(), 1);
    assert_eq!(r.speakers[0].name, "Speaker 1");
    assert!(r.file.note.as_deref().unwrap().contains("one speaker"));

    let s = run(&[(0.0, 5.0, 1, 0.1)], 9.0, false);
    assert!(s.file.segments.is_empty() && s.speakers.is_empty());
    assert!(s.file.note.as_deref().unwrap().contains("shorter than 10"));
}

#[test]
fn hints_are_passed_on_and_checked() {
    let d = ScriptedDiarizer::from_secs(&[(0.0, 12.0, 0, 0.1), (12.0, 14.0, 1, 0.1)]);
    let mix = silence(15.0);
    let input = plain_input(&mix);
    let mut o = opts(false);
    o.hints.num_speakers = Some(2);
    diarize_audio(&d, &input, &o, &mut |_| true).unwrap();
    assert_eq!(d.hints_seen.lock().unwrap()[0].num_speakers, Some(2));
    o.hints = DiarizeHints {
        min_speakers: Some(4),
        max_speakers: Some(2),
        ..Default::default()
    };
    assert!(o.hints.check().is_err());
}

#[test]
fn cancel_stops_the_run() {
    let d = ScriptedDiarizer::from_secs(&[(0.0, 12.0, 0, 0.1)]);
    let mix = silence(15.0);
    let e = diarize_audio(&d, &plain_input(&mix), &opts(false), &mut |_| false).unwrap_err();
    assert!(crate::transcribe::is_cancelled(&e));
}

#[test]
fn no_speaker_model_is_invalid_input_with_a_reason() {
    let e = NoSpeakerModel.ready().unwrap_err();
    assert_eq!(e.code().as_str(), "invalid_input");
    assert_eq!(error_data(&e).unwrap()["reason"], "no_speaker_model");
    let mix = silence(15.0);
    let e = diarize_audio(
        &NoSpeakerModel,
        &plain_input(&mix),
        &opts(false),
        &mut |_| true,
    )
    .unwrap_err();
    assert!(is_no_speaker_model(&e));
    assert!(error_data(&CmError::invalid_input("x")).is_none());
}

/// Pause gaps move file times onto the session clock.
#[test]
fn segments_are_in_session_time() {
    let d = ScriptedDiarizer::from_secs(&[(0.0, 4.0, 0, 0.1), (6.0, 12.0, 1, 0.1)]);
    let mix = silence(15.0);
    let gaps = [PauseGap {
        from: t(5.0),
        to: t(8.0),
    }];
    let input = DiarizeInput {
        gaps: &gaps,
        ..plain_input(&mix)
    };
    let r = diarize_audio(&d, &input, &opts(false), &mut |_| true).unwrap();
    assert_eq!(r.file.segments[1].start, t(9.0));
    assert_eq!(r.file.segments[1].end, t(15.0));
}

/// AI-8.AC7: in headset mode the mic is the user (named by the setting) and only the system
/// track goes to the diarizer.
#[test]
fn headset_mode_labels_the_mic_and_diarizes_only_the_system_track() {
    let sr = f64::from(SAMPLE_RATE);
    let mut mic = silence(30.0);
    for (i, s) in mic
        .iter_mut()
        .enumerate()
        .take((6.0 * sr) as usize)
        .skip((2.0 * sr) as usize)
    {
        *s = if i % 2 == 0 { 0.3 } else { -0.3 };
    }
    let system = silence(30.0);
    // The model hears two remote people, in its own labels.
    let d = ScriptedDiarizer::from_secs(&[(8.0, 12.0, 9, 0.2), (14.0, 18.0, 5, 0.2)]);
    let input = DiarizeInput {
        mix: &system,
        mic: Some(&mic),
        system: Some(&system),
        start: MediaTime::ZERO,
        gaps: &[],
    };
    let r = diarize_audio(&d, &input, &opts(true), &mut |_| true).unwrap();
    let names: Vec<_> = r.speakers.iter().map(|s| (s.id, s.name.as_str())).collect();
    assert_eq!(
        names,
        vec![(1, "Trung"), (2, "Speaker 1"), (3, "Speaker 2")]
    );
    let me: Vec<_> = r
        .file
        .segments
        .iter()
        .filter(|s| s.speaker == ME_ID)
        .collect();
    assert_eq!(me.len(), 1);
    assert!((me[0].start.as_secs_f64() - 2.0).abs() < 0.15);
    assert!((me[0].end.as_secs_f64() - 6.0).abs() < 0.15);
    // Words on the mic track get the user's name, remote words get Speaker n.
    let words = [
        word("hello", 3.0, 3.5),
        word("hi", 9.0, 9.4),
        word("yo", 15.0, 15.4),
    ];
    let labels = assign_words(&words, &r.file.segments);
    assert_eq!(
        labels
            .iter()
            .map(|l| l.speaker.unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    // No mic: headset mode cannot work.
    let bad = DiarizeInput { mic: None, ..input };
    assert!(diarize_audio(&d, &bad, &opts(true), &mut |_| true).is_err());
    // The default name when the setting is empty.
    assert_eq!(display_name_from(&Map::new()), "Me");
    let mut m = Map::new();
    m.insert(SETTING_DISPLAY_NAME.into(), json!("  Linh   Tran "));
    assert_eq!(display_name_from(&m), "Linh Tran");
    m.insert(SETTING_DISPLAY_NAME.into(), json!("   "));
    assert_eq!(display_name_from(&m), "Me");
}

/// AI-8: the speaker active at the word's midpoint; overlap goes to the louder one.
#[test]
fn words_get_the_speaker_at_their_midpoint() {
    let segs = [seg(0.0, 2.0, 1, 0.1), seg(2.0, 4.0, 2, 0.1)];
    let words = [
        word("a", 0.5, 1.0),
        word("b", 1.8, 2.4),
        word("c", 2.5, 3.0),
    ];
    let l = assign_words(&words, &segs);
    // "b" has its midpoint at 2.1 s, in speaker 2's segment.
    assert_eq!(
        l.iter().map(|x| x.speaker).collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(2)]
    );
    assert!(l.iter().all(|x| !x.overlap));

    let loud = [seg(0.0, 4.0, 1, 0.05), seg(1.0, 3.0, 2, 0.4)];
    let l = assign_words(&[word("x", 1.5, 2.0), word("y", 3.2, 3.6)], &loud);
    assert_eq!(
        l[0],
        WordLabel {
            speaker: Some(2),
            overlap: true
        }
    );
    assert_eq!(
        l[1],
        WordLabel {
            speaker: Some(1),
            overlap: false
        }
    );
}

#[test]
fn a_word_in_a_gap_takes_the_nearest_speaker_and_none_without_segments() {
    let segs = [seg(0.0, 2.0, 1, 0.0), seg(5.0, 7.0, 2, 0.0)];
    let l = assign_words(&[word("a", 2.2, 2.6), word("b", 4.4, 4.8)], &segs);
    assert_eq!(l[0].speaker, Some(1));
    assert_eq!(l[1].speaker, Some(2));
    assert_eq!(assign_words(&[word("a", 0.0, 1.0)], &[])[0].speaker, None);
}

#[test]
fn turns_group_consecutive_words() {
    let words = [
        word("Hi", 0.0, 0.5),
        word("there", 0.6, 1.0),
        word("Hello", 2.0, 2.5),
    ];
    let labels = assign_words(&words, &[seg(0.0, 1.5, 1, 0.0), seg(1.5, 3.0, 2, 0.0)]);
    let t = turns(&words, &labels);
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].text, "Hi there");
    assert_eq!(t[1].speaker, Some(2));
}

fn state() -> SpeakerState {
    SpeakerState {
        speakers: vec![
            Speaker {
                id: 1,
                name: "Speaker 1".into(),
            },
            Speaker {
                id: 2,
                name: "Speaker 2".into(),
            },
            Speaker {
                id: 3,
                name: "Speaker 3".into(),
            },
        ],
        segments: vec![
            seg(0.0, 4.0, 1, 0.1),
            seg(4.0, 8.0, 2, 0.1),
            seg(8.0, 12.0, 3, 0.1),
        ],
    }
}

fn edit_rename(id: u32, name: &str) -> SpeakerEdit {
    SpeakerEdit {
        rename: vec![(id, name.into())],
        ..Default::default()
    }
}

fn edit_merge(from: u32, into: u32) -> SpeakerEdit {
    SpeakerEdit {
        merge: vec![(from, into)],
        ..Default::default()
    }
}

/// AI-8.AC4: a rename changes the name everywhere and nothing else.
#[test]
fn rename_changes_the_name_only() {
    let s = state();
    let n = apply_edit(&s, &edit_rename(2, " Linh ")).unwrap();
    assert_eq!(n.name_of(2), "Linh");
    assert_eq!(n.segments, s.segments);
    // Original untouched (undo restores it from the stored copy).
    assert_eq!(s.name_of(2), "Speaker 2");
    for bad in ["", "   ", "speaker 1", "SPEAKER 3"] {
        assert!(apply_edit(&s, &edit_rename(2, bad)).is_err(), "{bad:?}");
    }
    assert!(apply_edit(&s, &edit_rename(9, "X"))
        .unwrap_err()
        .to_string()
        .contains("no speaker 9"));
    assert!(apply_edit(&s, &SpeakerEdit::default()).is_err());
}

/// AI-8.AC5: after a merge no word keeps the old speaker.
#[test]
fn merge_leaves_no_trace_of_the_old_speaker() {
    let s = state();
    let n = apply_edit(&s, &edit_merge(3, 1)).unwrap();
    assert!(n.speakers.iter().all(|x| x.id != 3));
    assert!(n.segments.iter().all(|x| x.speaker != 3));
    let words = [word("a", 9.0, 9.5), word("b", 1.0, 1.5)];
    let l = assign_words(&words, &n.segments);
    assert!(l.iter().all(|x| x.speaker != Some(3)));
    assert_eq!(l[0].speaker, Some(1));
    assert!(apply_edit(&s, &edit_merge(1, 1)).is_err());
    assert!(apply_edit(&s, &edit_merge(1, 7)).is_err());
    // Merging touching segments of the same speaker joins them.
    let m = apply_edit(&state(), &edit_merge(2, 1)).unwrap();
    assert_eq!(m.segments[0], seg(0.0, 8.0, 1, 0.1));
}

#[test]
fn assign_cuts_segments_and_reassigns_a_range() {
    let s = state();
    let a = Assign {
        start: t(2.0),
        end: t(6.0),
        speaker: 3,
    };
    let n = apply_edit(
        &s,
        &SpeakerEdit {
            assign: vec![a],
            ..Default::default()
        },
    )
    .unwrap();
    let w = assign_words(
        &[
            word("a", 1.0, 1.4),
            word("b", 2.5, 2.9),
            word("c", 5.0, 5.4),
            word("d", 6.5, 6.9),
        ],
        &n.segments,
    );
    assert_eq!(
        w.iter().map(|x| x.speaker.unwrap()).collect::<Vec<_>>(),
        vec![1, 3, 3, 2]
    );
    assert!(n.segments.windows(2).all(|p| p[0].start <= p[1].start));
    for bad in [
        Assign {
            start: t(3.0),
            end: t(3.0),
            speaker: 1,
        },
        Assign {
            start: t(1.0),
            end: t(3.0),
            speaker: 8,
        },
    ] {
        let e = SpeakerEdit {
            assign: vec![bad],
            ..Default::default()
        };
        assert!(apply_edit(&s, &e).is_err());
    }
}

#[test]
fn a_failed_edit_changes_nothing() {
    let s = state();
    let e = SpeakerEdit {
        rename: vec![(1, "Ann".into())],
        merge: vec![(2, 9)],
        ..Default::default()
    };
    assert!(apply_edit(&s, &e).is_err());
    assert_eq!(s, state());
}

#[test]
fn diarization_file_round_trips_and_reports_damage() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_diarization(dir.path()).unwrap().is_none());
    let r = run(&[(0.0, 5.0, 0, 0.1), (5.0, 12.0, 1, 0.2)], 15.0, false);
    write_diarization(dir.path(), &r.file).unwrap();
    assert_eq!(read_diarization(dir.path()).unwrap().unwrap(), r.file);
    let text = fs::read_to_string(diarization_path(dir.path())).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert!(v["segments"].is_array() && v["model"] == "scripted" && v["created"].is_string());
    assert!(find_voiceprint_files(&[dir.path()]).is_empty());
    fs::write(diarization_path(dir.path()), "{ nope").unwrap();
    assert!(read_diarization(dir.path()).is_err());
}

#[test]
fn the_voiceprint_scan_finds_the_obvious() {
    let dir = tempfile::tempdir().unwrap();
    assert!(find_voiceprint_files(&[dir.path()]).is_empty());
    fs::write(dir.path().join("model.onnx"), b"weights").unwrap();
    fs::write(dir.path().join("a.json"), br#"{"segments":[]}"#).unwrap();
    assert!(find_voiceprint_files(&[dir.path()]).is_empty());
    fs::write(dir.path().join("speaker1.npy"), b"x").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/b.json"), br#"{"embedding":[0.1]}"#).unwrap();
    fs::write(dir.path().join("my_voiceprint.dat"), b"x").unwrap();
    assert_eq!(find_voiceprint_files(&[dir.path()]).len(), 3);
}

fn spoken(a: f64, b: f64, text: &str, who: Option<&str>) -> SpokenLine {
    SpokenLine {
        line: CaptionLine {
            start: t(a),
            end: t(b),
            text: text.into(),
        },
        speaker: who.map(str::to_owned),
    }
}

#[test]
fn exports_carry_the_names() {
    let lines = [
        spoken(1.0, 3.0, "Hello & welcome", Some("Linh")),
        spoken(3.0, 5.0, "to the <demo>", Some("Linh")),
        spoken(65.0, 67.0, "Thanks", Some("Trung")),
        spoken(70.0, 71.0, "Unknown", None),
    ];
    let vtt = to_vtt_voiced(&lines);
    assert!(vtt.starts_with("WEBVTT\n\n"));
    assert!(vtt.contains("00:00:01.000 --> 00:00:03.000\n<v Linh>Hello &amp; welcome\n"));
    assert!(vtt.contains("<v Trung>Thanks"));
    assert!(vtt.contains("\nUnknown\n"));

    let plain = to_srt_named(&lines, false);
    assert!(plain.contains("1\n00:00:01,000 --> 00:00:03,000\nHello & welcome\n"));
    assert!(!plain.contains("LINH"));
    let named = to_srt_named(&lines, true);
    assert!(named.contains("LINH: Hello & welcome"));
    assert!(named.contains("TRUNG: Thanks"));
    assert!(named.contains("\nUnknown\n"));

    let txt = to_txt_speakers(&lines, true);
    assert_eq!(
        txt,
        "[00:01] Linh: Hello & welcome to the <demo>\n[01:05] Trung: Thanks\n[01:10] Unknown\n"
    );
    assert!(to_txt_speakers(&lines, false).starts_with("Linh: Hello"));
    let md = to_markdown_speakers(&lines);
    assert!(md.starts_with("**[00:01] Linh:** Hello & welcome to the <demo>\n\n"));
}

#[test]
fn a_caption_line_takes_its_dominant_speaker() {
    let segs = [seg(0.0, 3.0, 1, 0.1), seg(3.0, 4.0, 2, 0.1)];
    let line = |a: f64, b: f64| CaptionLine {
        start: t(a),
        end: t(b),
        text: "x".into(),
    };
    assert_eq!(line_speaker(&line(2.0, 4.0), &segs), Some((1, true)));
    assert_eq!(line_speaker(&line(0.5, 3.5), &segs), Some((1, false)));
    assert_eq!(line_speaker(&line(0.0, 2.0), &segs), Some((1, false)));
    assert_eq!(line_speaker(&line(9.0, 10.0), &segs), Some((2, false)));
    assert_eq!(line_speaker(&line(0.0, 1.0), &[]), None);
}

#[test]
fn state_round_trips_through_manifest_values() {
    let s = state();
    let m = s.manifest_values();
    assert_eq!(m[MANIFEST_SPEAKERS][1]["name"], "Speaker 2");
    assert_eq!(m[MANIFEST_SEGMENTS][0]["speaker"], 1);
    let back: Vec<Segment> = serde_json::from_value(m[MANIFEST_SEGMENTS].clone()).unwrap();
    assert_eq!(back, s.segments);
}
