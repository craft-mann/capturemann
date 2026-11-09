//! EXP-5.AC6: the size estimate against the real size of an export, on synthetic recordings.
//!
//! Three recordings per format with an estimate (MP4, WebM): a mostly static screen, a typical
//! screen recording and a high-motion one (`make_fixture --content static|typical|motion`, 1080p,
//! 20 s, microphone and system audio), each exported through the real exporter at the recording
//! size, 720p and 480p. **Pass line: the real size is at or below the bound shown (`max_bytes` of
//! `cm_media::estimate`) on every sample.** One miss fails the test.
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
    bound: u64,
    nominal: f64,
    /// The typical size from the synthetic master's rate (information: the rule was calibrated on
    /// real recordings, and a `lavfi` master is unlike one).
    typical: Option<u64>,
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
    let mut quality: Vec<String> = Vec::new();
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
        let master = cm_media::QuickSpec::from_manifest(&manifest)
            .ok()
            .and_then(|spec| estimate::master_rate(&fx.dir, &manifest, &spec));
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
                let Estimated::Bound(e) =
                    estimate::estimate_with_master(&manifest, &choice, codec, master.as_ref())
                        .unwrap()
                else {
                    panic!("MP4 and WebM have a bound");
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
                if size == SizeChoice::Source && content != FixtureContent::Static {
                    quality.push(quality_line(
                        &fx.dir,
                        &out.path,
                        content,
                        format,
                        e.width * e.height,
                    ));
                }
                rows.push(Row {
                    content: content.as_str(),
                    format: format.as_str(),
                    size: match size {
                        SizeChoice::Source => "recording".to_owned(),
                        SizeChoice::Height(h) => format!("{h}p"),
                    },
                    frame: (e.width, e.height),
                    bound: e.max_bytes,
                    typical: e.typical_bytes,
                    nominal: estimate::estimate_bytes(&estimate::Inputs {
                        codec: if format == Format::WebM {
                            estimate::Codec::Vp9
                        } else {
                            codec
                        },
                        format,
                        frame: (e.width, e.height),
                        fps: e.fps,
                        duration_s: e.duration_s,
                        audio_channels: Some(2),
                    }),
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
        "| content | format | size | pixels | bound shown | real | real / bound | real / nominal | typical (information) | real / typical | length est. / real |"
    )
    .unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
    let mut misses = Vec::new();
    for r in &rows {
        let ok = r.real <= r.bound;
        writeln!(
            md,
            "| {} | {} | {} | {}×{} | {} | {:.2} MB {} | {:.2} | {:.3} | {} | {} | {:.1} s / {:.1} s |",
            r.content,
            r.format,
            r.size,
            r.frame.0,
            r.frame.1,
            estimate::label_of(r.bound),
            r.real as f64 / 1e6,
            if ok { "at or below" } else { "**ABOVE**" },
            r.real as f64 / r.bound as f64,
            r.real as f64 / r.nominal,
            r.typical.map_or("none".to_owned(), estimate::typical_label_of),
            r.typical
                .map_or("-".to_owned(), |t| format!("{:.2}", r.real as f64 / t as f64)),
            r.est_s,
            r.real_s,
        )
        .unwrap();
        if !ok {
            misses.push(format!(
                "{} {} {}: real {} B, bound {} B",
                r.content, r.format, r.size, r.real, r.bound
            ));
        }
    }
    writeln!(
        md,
        "\nQuality at the recording size (information, not a pass line):\n"
    )
    .unwrap();
    for q in &quality {
        writeln!(md, "- {q}").unwrap();
    }
    println!("{md}");
    if let Some(path) = std::env::var_os("CM_ESTIMATE_REPORT") {
        std::fs::write(path, &md).unwrap();
    }
    assert!(
        misses.is_empty(),
        "the real size must be at or below the bound on every sample:\n{}",
        misses.join("\n")
    );
}

/// `SSIM All:` and `PSNR average:` of `file` against the master's frames (same size, same start:
/// the export starts at the master's first frame), through the FFmpeg on PATH (the shipped build
/// has no `ssim` or `psnr` filter).
fn ssim_psnr(file: &std::path::Path, master: &std::path::Path) -> Option<(f64, f64)> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "info", "-nostats", "-i"])
        .arg(file)
        .arg("-i")
        .arg(master)
        .args([
            "-lavfi",
            "[0:v]split[a1][a2];[1:v]split[b1][b2];[a1][b1]ssim;[a2][b2]psnr",
            "-f",
            "null",
            "-",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    let after = |key: &str| -> Option<f64> {
        let i = text.rfind(key)? + key.len();
        text[i..].split_whitespace().next()?.parse().ok()
    };
    Some((after("All:")?, after("average:")?))
}

/// One line of the quality section: bits per pixel and SSIM and PSNR of the export against the
/// master, and for WebM the same for an uncapped constant-quality re-encode of the master
/// (`-b:v 0 -crf 32`, the exporter's settings before the cap).
fn quality_line(
    dir: &std::path::Path,
    out: &std::path::Path,
    content: FixtureContent,
    format: Format,
    pixels: u32,
) -> String {
    let master = dir.join("screen.mp4");
    let bytes = std::fs::metadata(out).map_or(0, |m| m.len());
    let bpp = |b: u64| b as f64 * 8.0 / (f64::from(pixels) * 20.0 * 30.0);
    let q = ssim_psnr(out, &master);
    let mut line = format!(
        "{} {}: {:.3} bits per pixel, {}",
        content.as_str(),
        format.as_str(),
        bpp(bytes),
        q.map_or("SSIM/PSNR not measured".to_owned(), |(s, p)| format!(
            "SSIM {s:.4}, PSNR {p:.2} dB (export)"
        ))
    );
    if format == Format::WebM {
        let reference = dir.join("uncapped-reference.webm");
        let ok = std::process::Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-i"])
            .arg(&master)
            .args([
                "-an",
                "-c:v",
                "libvpx-vp9",
                "-row-mt",
                "1",
                "-deadline",
                "good",
                "-cpu-used",
                "4",
                "-g",
                "120",
                "-b:v",
                "0",
                "-crf",
                "32",
            ])
            .arg(&reference)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            let rb = std::fs::metadata(&reference).map_or(0, |m| m.len());
            let rq = ssim_psnr(&reference, &master);
            let _ = write!(
                line,
                "; uncapped reference: {:.3} bits per pixel ({:.1} MB), {}",
                bpp(rb),
                rb as f64 / 1e6,
                rq.map_or("not measured".to_owned(), |(s, p)| format!(
                    "SSIM {s:.4}, PSNR {p:.2} dB"
                ))
            );
        }
    }
    line
}
