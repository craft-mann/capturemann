//! FFmpeg-gated tests of the import engine (IMP-1, IMP-2). Fixtures are made with the `ffmpeg` on
//! `PATH` (the shipped build has no `lavfi` and no `libx264`); the tests print SKIPPED when it is
//! missing or lacks a component. Nothing touches a screen, camera or microphone.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cm_media::import::classify::{classify, MediaKind, PrepareReason, Status};
use cm_media::import::prepare::{
    decode_sound, editing_copy_encoders, make_editing_copy, verify, wav_seconds, RunFailure,
    VerifyInput,
};
use cm_media::import::probe::{probe, ProbeError};
use cm_media::import::Reason;
use cm_media::tools::{locate, Tools};

/// Runs the `ffmpeg` of `PATH` for a fixture. `None` (SKIPPED) when it lacks `lavfi` or `libx264`.
struct Maker {
    dir: tempfile::TempDir,
}

fn ff(args: &[&str]) -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn maker() -> Option<(Maker, Tools)> {
    let tools = match locate() {
        Ok(t) if t.ffprobe.is_some() => t,
        _ => {
            eprintln!("SKIPPED: FFmpeg/ffprobe not found (brew install ffmpeg)");
            return None;
        }
    };
    let enc = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .ok()?;
    if !String::from_utf8_lossy(&enc.stdout).contains("libx264") {
        eprintln!("SKIPPED: the ffmpeg on PATH has no libx264 (fixtures)");
        return None;
    }
    Some((
        Maker {
            dir: tempfile::tempdir().unwrap(),
        },
        tools,
    ))
}

impl Maker {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A 3 s 320x240 30 fps H.264 + AAC (stereo) MP4.
    fn mp4(&self, name: &str) -> PathBuf {
        let p = self.path(name);
        assert!(ff(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=3",
            "-ac",
            "2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            p.to_str().unwrap(),
        ]));
        p
    }
}

fn status_of(tools: &Tools, p: &Path) -> (MediaKind, Status) {
    let f = probe(tools, p).unwrap();
    let c = classify(&f);
    (c.kind, c.status)
}

#[test]
fn a_plain_mp4_is_ready_and_its_facts_are_read() {
    let Some((m, tools)) = maker() else { return };
    let p = m.mp4("a.mp4");
    let f = probe(&tools, &p).unwrap();
    let v = f.video.clone().unwrap();
    assert_eq!((v.width, v.height, v.codec.as_str()), (320, 240, "h264"));
    assert_eq!(v.fps, Some(30.0));
    assert!(!v.variable_frame_rate, "{v:?}");
    let a = f.audio.clone().unwrap();
    assert_eq!(
        (a.codec.as_str(), a.channels, a.sample_rate),
        ("aac", 2, 44100)
    );
    assert!((f.duration_s - 3.0).abs() < 0.1, "{}", f.duration_s);
    assert_eq!(f.size_bytes, std::fs::metadata(&p).unwrap().len());
    assert_eq!(classify(&f).status, Status::Ready);
}

#[test]
fn mkv_rotation_and_variable_rate_are_prepared_for_editing() {
    let Some((m, tools)) = maker() else { return };
    let mp4 = m.mp4("a.mp4");
    // MKV.
    let mkv = m.path("a.mkv");
    assert!(ff(&[
        "-i",
        mp4.to_str().unwrap(),
        "-c",
        "copy",
        mkv.to_str().unwrap()
    ]));
    assert_eq!(
        status_of(&tools, &mkv),
        (
            MediaKind::Video,
            Status::Prepared(vec![PrepareReason::ContainerMkv])
        )
    );
    // A rotation tag (a phone held upright): 90 degrees swaps the displayed size.
    let rot = m.path("rot.mp4");
    assert!(ff(&[
        "-display_rotation",
        "90",
        "-i",
        mp4.to_str().unwrap(),
        "-c",
        "copy",
        rot.to_str().unwrap()
    ]));
    let f = probe(&tools, &rot).unwrap();
    let v = f.video.as_ref().unwrap();
    assert_eq!(
        v.rotation, 270,
        "ffmpeg's display_rotation is counter-clockwise: {v:?}"
    );
    assert_eq!((v.width, v.height), (240, 320));
    assert_eq!(
        classify(&f).status,
        Status::Prepared(vec![PrepareReason::Rotated])
    );
    // Variable frame rate: every fourth frame dropped, the times kept.
    let vfr = m.path("vfr.mp4");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=320x240:rate=30:duration=4",
        "-vf",
        "select=not(eq(mod(n\\,4)\\,1))",
        "-fps_mode",
        "vfr",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        vfr.to_str().unwrap(),
    ]));
    assert_eq!(
        status_of(&tools, &vfr),
        (
            MediaKind::Video,
            Status::Prepared(vec![PrepareReason::VariableFrameRate])
        )
    );
}

#[test]
fn audio_files_are_audio_and_cover_art_is_not_video() {
    let Some((m, tools)) = maker() else { return };
    let wav = m.path("a.wav");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=2",
        wav.to_str().unwrap()
    ]));
    assert_eq!(status_of(&tools, &wav), (MediaKind::Audio, Status::Ready));
    let m4a = m.path("a.m4a");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=2",
        "-c:a",
        "aac",
        m4a.to_str().unwrap()
    ]));
    assert_eq!(status_of(&tools, &m4a), (MediaKind::Audio, Status::Ready));
    // M4A with a cover picture stays audio.
    let cover = m.path("cover.png");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x64:duration=1",
        "-frames:v",
        "1",
        cover.to_str().unwrap()
    ]));
    let art = m.path("art.m4a");
    assert!(ff(&[
        "-i",
        m4a.to_str().unwrap(),
        "-i",
        cover.to_str().unwrap(),
        "-map",
        "0:a",
        "-map",
        "1:v",
        "-c",
        "copy",
        "-disposition:v:0",
        "attached_pic",
        art.to_str().unwrap(),
    ]));
    let f = probe(&tools, &art).unwrap();
    assert!(f.video.is_none(), "{f:?}");
    assert_eq!(classify(&f).kind, MediaKind::Audio);
}

#[test]
fn recognised_but_unsupported_formats_are_named_never_a_crash() {
    let Some((m, tools)) = maker() else { return };
    let tone = ["-f", "lavfi", "-i", "sine=frequency=440:duration=1"];
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("a.flac", [&tone[..], &[][..]].concat()),
        ("a.aiff", tone.to_vec()),
        ("a.aac", [&tone[..], &["-c:a", "aac"][..]].concat()),
    ];
    for (name, args) in cases {
        let p = m.path(name);
        let mut a = args.clone();
        a.push(p.to_str().unwrap());
        assert!(ff(&a), "{name}");
        match probe(&tools, &p) {
            Ok(f) => match classify(&f).status {
                Status::CantImport(pr) => {
                    assert_eq!(pr.reason, Reason::UnsupportedFormat, "{name}")
                }
                other => panic!("{name}: {other:?}"),
            },
            Err(ProbeError::Problem(pr)) => {
                assert_eq!(pr.reason, Reason::UnsupportedFormat, "{name}")
            }
            Err(e) => panic!("{name}: {e:?}"),
        }
    }
    // WebM (VP9 + Opus): the codec is named.
    let webm = m.path("a.webm");
    if ff(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=160x120:rate=10:duration=1",
        "-c:v",
        "libvpx-vp9",
        webm.to_str().unwrap(),
    ]) {
        let (_, st) = status_of(&tools, &webm);
        let Status::CantImport(p) = st else { panic!() };
        assert_eq!(
            (p.reason, p.codec.as_deref()),
            (Reason::UnsupportedFormat, Some("vp9"))
        );
    }
}

#[test]
fn the_shipped_ffprobe_names_what_it_cannot_open() {
    // The shipped build has no AVI, AIFF, FLAC or AAC demuxer (MP3 has one since 0.3.1): the first bytes decide.
    let dir = std::env::var_os("CM_IMPORT_BUNDLED_FFMPEG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/ffmpeg/out")
        });
    let (ffmpeg, ffprobe) = (dir.join("ffmpeg"), dir.join("ffprobe"));
    if !ffmpeg.is_file() || !ffprobe.is_file() {
        eprintln!(
            "SKIPPED: no shipped FFmpeg in {} (make ffmpeg)",
            dir.display()
        );
        return;
    }
    let Some((m, _)) = maker() else { return };
    let shipped = Tools {
        ffmpeg,
        ffprobe: Some(ffprobe),
    };
    let tone = ["-f", "lavfi", "-i", "sine=frequency=440:duration=1"];
    for (name, extra, want) in [
        ("a.flac", vec![], "flac"),
        ("a.aiff", vec![], "aiff"),
        ("a.aac", vec!["-c:a", "aac"], "aac"),
    ] {
        let p = m.path(name);
        let mut a: Vec<&str> = tone.to_vec();
        a.extend(extra);
        a.push(p.to_str().unwrap());
        assert!(ff(&a));
        match probe(&shipped, &p) {
            Err(ProbeError::Problem(pr)) => {
                assert_eq!(
                    (pr.reason, pr.format.as_deref()),
                    (Reason::UnsupportedFormat, Some(want))
                );
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    // The shipped ffprobe reads what the import supports.
    let mp4 = m.mp4("s.mp4");
    assert_eq!(
        classify(&probe(&shipped, &mp4).unwrap()).status,
        Status::Ready
    );
}

#[test]
fn broken_and_empty_files_are_unreadable() {
    let Some((m, tools)) = maker() else { return };
    let mp4 = m.mp4("a.mp4");
    let bytes = std::fs::read(&mp4).unwrap();
    let cut = m.path("cut.mp4");
    std::fs::write(&cut, &bytes[..bytes.len() / 3]).unwrap();
    let junk = m.path("junk.mov");
    std::fs::write(&junk, b"this is not a movie at all, just text").unwrap();
    let empty = m.path("empty.wav");
    std::fs::write(&empty, b"").unwrap();
    for p in [&cut, &junk, &empty] {
        match probe(&tools, p) {
            Err(ProbeError::Problem(pr)) => {
                assert_eq!(pr.reason, Reason::UnreadableFile, "{}", p.display())
            }
            other => panic!("{}: {other:?}", p.display()),
        }
    }
    assert!(matches!(
        probe(&tools, &m.path("nope.mp4")),
        Err(ProbeError::Other(e)) if e.code() == cm_core::ErrorCode::NotFound
    ));
}

#[test]
fn the_sound_is_decoded_to_a_48k_float_wav_and_verified() {
    let Some((m, tools)) = maker() else { return };
    let mp4 = m.mp4("a.mp4");
    let f = probe(&tools, &mp4).unwrap();
    let wav = m.path("mic.wav");
    let ch =
        decode_sound(&tools, &f, &wav, &mut |_| {}, &|| false).unwrap_or_else(|_| panic!("decode"));
    assert_eq!(ch, 2);
    let r = hound::WavReader::open(&wav).unwrap();
    let s = r.spec();
    assert_eq!(
        (s.sample_rate, s.channels, s.bits_per_sample),
        (48_000, 2, 32)
    );
    assert_eq!(s.sample_format, hound::SampleFormat::Float);
    assert!((wav_seconds(&wav).unwrap() - 3.0).abs() < 0.1);
    // A byte-identical master verifies; a changed one does not.
    let master = m.path("master.mp4");
    std::fs::copy(&mp4, &master).unwrap();
    let input = VerifyInput {
        tools: &tools,
        source: &f,
        kind: MediaKind::Video,
        master: &master,
        sound: Some(&wav),
        editing_copy: None,
    };
    verify(&input).unwrap();
    let mut bytes = std::fs::read(&mp4).unwrap();
    bytes.push(0);
    std::fs::write(&master, bytes).unwrap();
    let e = verify(&input).unwrap_err();
    assert_eq!(e.reason, Reason::UnreadableFile);
    // Cancelling removes the partial file.
    let part = m.path("part.wav");
    assert!(matches!(
        decode_sound(&tools, &f, &part, &mut |_| {}, &|| true),
        Err(RunFailure::Cancelled)
    ));
    assert!(!part.exists());
}

#[test]
fn an_editing_copy_is_constant_rate_upright_and_the_same_length() {
    let Some((m, tools)) = maker() else { return };
    match editing_copy_encoders(&tools, "h264") {
        Ok((ladder, _, _)) if !ladder.is_empty() => {}
        _ => {
            eprintln!("SKIPPED: no hardware video encoder in this FFmpeg");
            return;
        }
    }
    // A rotated, variable-rate MKV: all three reasons at once.
    let vfr = m.path("vfr.mp4");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=320x240:rate=30:duration=4",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=4",
        "-vf",
        "select=not(eq(mod(n\\,4)\\,1))",
        "-fps_mode",
        "vfr",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-shortest",
        vfr.to_str().unwrap(),
    ]));
    let rot = m.path("rot.mkv");
    assert!(ff(&[
        "-display_rotation",
        "90",
        "-i",
        vfr.to_str().unwrap(),
        "-c",
        "copy",
        rot.to_str().unwrap()
    ]));
    let f = probe(&tools, &rot).unwrap();
    let Status::Prepared(why) = classify(&f).status else {
        panic!("{f:?}")
    };
    assert!(why.contains(&PrepareReason::ContainerMkv));
    let out = m.path("screen.edit.mp4");
    let mut last = 0.0;
    match make_editing_copy(&tools, &f, &out, &mut |p| last = p, &|| false) {
        Ok(copy) => eprintln!("editing copy by {} ({})", copy.encoder, copy.codec),
        Err(RunFailure::NoHardwareEncoder) => {
            eprintln!("SKIPPED: this machine has no hardware video encoder");
            return;
        }
        Err(RunFailure::Failed(e)) => panic!("{e}"),
        Err(RunFailure::Cancelled) => panic!("cancelled"),
    }
    assert!(last > 0.0);
    let c = probe(&tools, &out).unwrap();
    let v = c.video.unwrap();
    assert!(!v.variable_frame_rate, "{v:?}");
    assert_eq!(v.rotation, 0);
    assert_eq!(
        (v.width, v.height),
        (
            f.video.as_ref().unwrap().width,
            f.video.as_ref().unwrap().height
        )
    );
    assert!(
        (c.duration_s - f.duration_s).abs() <= 1.0 / 20.0 + 0.05,
        "{} vs {}",
        c.duration_s,
        f.duration_s
    );
    // The cancel path removes the output.
    let out2 = m.path("cancelled.mp4");
    assert!(matches!(
        make_editing_copy(&tools, &f, &out2, &mut |_| {}, &|| true),
        Err(RunFailure::Cancelled)
    ));
    assert!(!out2.exists());
}

// ---- MP3 (0.3.1; MP3-1 in `specs/features/18-import.md`) ----

/// True when the `ffmpeg` on `PATH` can write MP3 (fixtures only; the shipped build never encodes it).
fn can_make_mp3() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("libmp3lame"))
}

/// A 2 s 440 Hz tone as MP3 with the given extra encoder or muxer arguments.
fn make_mp3(m: &Maker, name: &str, extra: &[&str]) -> PathBuf {
    let p = m.path(name);
    let mut a = vec![
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=44100:duration=2",
    ];
    a.extend(["-c:a", "libmp3lame"]);
    a.extend(extra);
    a.push(p.to_str().unwrap());
    assert!(ff(&a), "{name}");
    p
}

/// The first samples of a decoded `mic.wav` (channel 0) and its length in seconds.
fn first_peak(wav: &Path, ms: u32) -> f32 {
    let mut r = hound::WavReader::open(wav).unwrap();
    let spec = r.spec();
    let n = (spec.sample_rate * ms / 1000) as usize * spec.channels as usize;
    r.samples::<f32>()
        .take(n)
        .map(|x| x.unwrap().abs())
        .fold(0.0, f32::max)
}

#[test]
fn an_mp3_is_ready_audio_whose_sound_has_the_files_length_and_no_leading_gap() {
    if !can_make_mp3() {
        eprintln!("SKIPPED: the ffmpeg on PATH has no libmp3lame (fixtures)");
        return;
    }
    let Some((m, tools)) = maker() else { return };
    // Constant bit rate with an ID3v2 title and a cover picture; variable bit rate mono at
    // 22.05 kHz (MPEG-2); no Xing/Info header, so the length is only an estimate.
    let cover = m.path("cover.png");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x64:duration=1",
        "-frames:v",
        "1",
        cover.to_str().unwrap()
    ]));
    let tagged = m.path("tagged.mp3");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=44100:duration=2",
        "-i",
        cover.to_str().unwrap(),
        "-map",
        "0:a",
        "-map",
        "1:v",
        "-c:a",
        "libmp3lame",
        "-ac",
        "2",
        "-b:a",
        "128k",
        "-c:v",
        "copy",
        "-disposition:v:0",
        "attached_pic",
        "-id3v2_version",
        "3",
        "-metadata",
        "title=  Weekly sync  ",
        tagged.to_str().unwrap()
    ]));
    let vbr_mono = make_mp3(&m, "vbr.mp3", &["-q:a", "4", "-ac", "1", "-ar", "22050"]);
    let no_header = make_mp3(
        &m,
        "noxing.mp3",
        &["-ac", "2", "-b:a", "96k", "-write_xing", "0"],
    );
    for (p, want_title, channels) in [
        (&tagged, Some("Weekly sync"), 2),
        (&vbr_mono, None, 1),
        (&no_header, None, 2),
    ] {
        let f = probe(&tools, p).unwrap();
        assert_eq!(
            f.family(),
            cm_media::import::probe::Family::Mp3,
            "{}",
            p.display()
        );
        assert!(f.video.is_none(), "cover art is not video: {f:?}");
        assert_eq!(f.title.as_deref(), want_title);
        let c = classify(&f);
        assert_eq!((c.kind, c.status), (MediaKind::Audio, Status::Ready));
        let wav = m.path("mic.wav");
        let ch = decode_sound(&tools, &f, &wav, &mut |_| {}, &|| false)
            .unwrap_or_else(|_| panic!("decode {}", p.display()));
        assert_eq!(ch, channels);
        let spec = hound::WavReader::open(&wav).unwrap().spec();
        assert_eq!((spec.sample_rate, spec.bits_per_sample), (48_000, 32));
        // The decoded length is the length of the sound within 50 ms (an MP3 is whole frames of
        // 26 ms, so the file can end up to one frame long).
        let secs = wav_seconds(&wav).unwrap();
        assert!((secs - 2.0).abs() < 0.05, "{}: {secs}", p.display());
        // The tone starts at once: no encoder delay of silence at the start.
        assert!(first_peak(&wav, 30) > 0.05, "{}", p.display());
        // The copy in the bundle verifies like any other audio master.
        let master = m.path("master.mp3");
        std::fs::copy(p, &master).unwrap();
        verify(&VerifyInput {
            tools: &tools,
            source: &f,
            kind: MediaKind::Audio,
            master: &master,
            sound: Some(&wav),
            editing_copy: None,
        })
        .unwrap_or_else(|e| panic!("verify {}: {e:?}", p.display()));
    }
}

#[test]
fn an_mp3_is_found_by_its_content_not_its_name() {
    if !can_make_mp3() {
        eprintln!("SKIPPED: the ffmpeg on PATH has no libmp3lame (fixtures)");
        return;
    }
    let Some((m, tools)) = maker() else { return };
    let mp3 = make_mp3(&m, "a.mp3", &["-b:a", "128k"]);
    let renamed = m.path("really-an-mp3.wav");
    std::fs::copy(&mp3, &renamed).unwrap();
    assert_eq!(
        status_of(&tools, &renamed),
        (MediaKind::Audio, Status::Ready)
    );
}

#[test]
fn a_damaged_mp3_is_unreadable_not_unsupported() {
    let Some((m, tools)) = maker() else { return };
    // An ID3 header with nothing behind it, and bytes that only look like a frame header.
    let id3 = m.path("id3-only.mp3");
    std::fs::write(&id3, b"ID3\x04\0\0\0\0\0\x10not an mp3, just a tag").unwrap();
    let junk = m.path("junk.mp3");
    std::fs::write(&junk, [0xFF, 0xFB, 0x90, 0x00, 1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    for p in [&id3, &junk] {
        match probe(&tools, p) {
            Err(ProbeError::Problem(pr)) => {
                assert_eq!(pr.reason, Reason::UnreadableFile, "{}", p.display())
            }
            other => panic!("{}: {other:?}", p.display()),
        }
    }
}
