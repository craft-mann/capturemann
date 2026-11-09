//! EXP-5.AC6: the size estimate against the real size of an export, on synthetic recordings.
//!
//! Three recordings per format with an estimate (MP4, WebM): a mostly static screen, a typical
//! screen recording and a high-motion one (`make_fixture --content static|typical|motion`, 1080p,
//! 20 s, microphone and system audio), each exported through the real exporter at the recording
//! size, 720p and 480p. **Pass line: the shown range (`min_bytes` to `max_bytes` of
//! `cm_media::estimate`) contains the real size on every sample.** One miss fails the test.
//!
//! The recordings are synthetic (FFmpeg `lavfi` sources), not real screen recordings: the table
//! is only as good as these three looks. Run it, with the FFmpeg that ships, as
//!
//! ```text
//! CM_ESTIMATE_REPORT=/tmp/estimate.md packaging/ffmpeg/with-bundled-ffmpeg.sh \
//!     cargo test -p cm-daemon --test estimate_vs_real --locked -- --ignored --nocapture
//! ```
//!
//! It needs a GPU (the compositor) and FFmpeg with `libx264` and `lavfi` for the recordings
//! (prints SKIPPED without them) and takes a few minutes, so it is `#[ignore]`d. On Linux without
//! a hardware H.264 encoder the MP4 rows measure the VP9 table.

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;

use cm_daemon::fixture::{make_fixture, FixtureContent, FixtureSpec};
use cm_media::estimate::{self, Choice, Estimated, Format, SizeChoice};
use cm_media::{
    export_quick, ExportOptions, ExportPath, PresetRequest, Presets, QuickRequest, Tools,
};
use serde_json::Value;

struct Row {
    content: &'static str,
    format: &'static str,
    size: String,
    frame: (u32, u32),
    est: (u64, u64),
    real: u64,
    est_s: f64,
    real_s: f64,
}

fn export(
    tools: &Tools,
    dir: &std::path::Path,
    manifest: &Value,
    format: Format,
    size: SizeChoice,
) -> Result<cm_media::QuickOutcome, cm_media::ExportError> {
    let presets = Presets::builtin();
    // The mapping `cm-daemon` makes (`export::effective_preset`): an MP4 of a chosen size is the
    // `source` preset at that frame; the recording size is the plain `quick` export.
    let (preset, options) = match (format, size) {
        (Format::Mp4, SizeChoice::Source) => (None, ExportOptions::default()),
        (Format::Mp4, s) => (
            Some("source"),
            ExportOptions {
                size: Some(s),
                ..ExportOptions::default()
            },
        ),
        (_, s) => (
            Some("webm"),
            ExportOptions {
                size: Some(s),
                ..ExportOptions::default()
            },
        ),
    };
    export_quick(
        tools,
        &QuickRequest {
            bundle_dir: dir,
            manifest,
            prefer_encoder: None,
            path: ExportPath::Auto,
            loudness_lufs: None,
            preset: preset.map(|id| PresetRequest {
                presets: &presets,
                id,
                options,
            }),
            captions: None,
        },
        &mut |_| {},
        &|| false,
    )
}

#[test]
#[ignore = "heavy: 18 real exports of a 20 s 1080p recording; needs a GPU and FFmpeg"]
fn exp5_ac6_the_range_contains_the_real_size() {
    let Ok(tools) = cm_media::tools::locate() else {
        eprintln!("SKIPPED: FFmpeg was not found");
        return;
    };
    let codec = estimate::mp4_codec(Some(&tools)).unwrap();
    let lib = tempfile::tempdir().unwrap();
    let mut rows: Vec<Row> = Vec::new();
    for content in [
        FixtureContent::Static,
        FixtureContent::Typical,
        FixtureContent::Motion,
    ] {
        let spec = FixtureSpec {
            title: format!("est {}", content.as_str()),
            secs: 20,
            size: (1920, 1080),
            content,
            camera: false,
            pause: None,
            ..FixtureSpec::default()
        };
        let fx = match make_fixture(&tools, lib.path(), &spec) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "SKIPPED: the fixture could not be made ({e}); it needs libx264 and lavfi"
                );
                return;
            }
        };
        let manifest = serde_json::to_value(cm_library::read_manifest(&fx.dir).unwrap()).unwrap();
        for format in [Format::Mp4, Format::WebM] {
            for size in [
                SizeChoice::Source,
                SizeChoice::Height(720),
                SizeChoice::Height(480),
            ] {
                let choice = Choice {
                    format,
                    size: Some(size),
                    width: None,
                    max_mb: None,
                };
                let Estimated::Range(e) = estimate::estimate(&manifest, &choice, codec).unwrap()
                else {
                    panic!("MP4 and WebM have an estimate");
                };
                let out = match export(&tools, &fx.dir, &manifest, format, size) {
                    Ok(o) => o,
                    Err(cm_media::ExportError::Failed(err))
                        if err.to_string().to_lowercase().contains("gpu")
                            || err.to_string().to_lowercase().contains("adapter") =>
                    {
                        eprintln!("SKIPPED: no GPU compositor ({err})");
                        return;
                    }
                    Err(err) => panic!("{} {format:?} {size:?}: {err:?}", content.as_str()),
                };
                rows.push(Row {
                    content: content.as_str(),
                    format: format.as_str(),
                    size: match size {
                        SizeChoice::Source => "recording".to_owned(),
                        SizeChoice::Height(h) => format!("{h}p"),
                    },
                    frame: (e.width, e.height),
                    est: (e.min_bytes, e.max_bytes),
                    real: out.size_bytes,
                    est_s: e.duration_s,
                    real_s: out.duration_ms as f64 / 1000.0,
                });
            }
        }
    }

    let mut md = String::new();
    writeln!(
        md,
        "| content | format | size | pixels | estimate | real | real / middle | duration est. / real |"
    )
    .unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|").unwrap();
    let mut misses = Vec::new();
    for r in &rows {
        let mid =
            (r.est.0 as f64 / estimate::RANGE_LOW + r.est.1 as f64 / estimate::RANGE_HIGH) / 2.0;
        let inside = r.est.0 <= r.real && r.real <= r.est.1;
        writeln!(
            md,
            "| {} | {} | {} | {}×{} | {} | {:.1} MB {} | {:.2} | {:.1} s / {:.1} s |",
            r.content,
            r.format,
            r.size,
            r.frame.0,
            r.frame.1,
            estimate::label_of(r.est.0, r.est.1),
            r.real as f64 / 1e6,
            if inside { "inside" } else { "**OUTSIDE**" },
            r.real as f64 / mid,
            r.est_s,
            r.real_s,
        )
        .unwrap();
        if !inside {
            misses.push(format!(
                "{} {} {}: real {} B, range {}..{} B",
                r.content, r.format, r.size, r.real, r.est.0, r.est.1
            ));
        }
    }
    println!("{md}");
    if let Some(path) = std::env::var_os("CM_ESTIMATE_REPORT") {
        std::fs::write(path, &md).unwrap();
    }
    assert!(
        misses.is_empty(),
        "the shown range must contain the real size on every sample:\n{}",
        misses.join("\n")
    );
}
