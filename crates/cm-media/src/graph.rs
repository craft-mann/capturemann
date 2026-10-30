//! The pure FFmpeg command builder for the quick export: argument list and `-filter_complex`.
//! Nothing here runs FFmpeg, so it is unit tested with manifest-shaped fixtures.
//!
//! # Graph
//!
//! ```text
//! [0:v] setpts=PTS-STARTPTS, fps=F, (scale) ───────────────────────────────► [bg]
//! [1:v] (trim|tpad), hflip?, crop(aspect), scale(w:h), rgba ─┐
//! [2:v] mask.png, gray ──────────────────────────────────────┴ alphamerge ┐
//! [3:v] ring.png, rgba ───────────────────────────────────────── overlay ─┴► [bub]
//! [bg][bub] overlay=x:y, yuv420p ───────────────────────────────────────────► [outv]
//! mic    [m:a] asetpts, (adelay|atrim), 48 kHz ─┐
//! system [s:a] asetpts, (adelay|atrim), 48 kHz ─┴ amix(normalize=0) ─ apad ─► [outa]
//! ```
//!
//! # Timeline
//!
//! Video t = 0 is the recording's `live_at` (the moment every track delivers; the screen's first
//! frame when the manifest has none, see `QuickSpec::origin`). Each other input is placed from its
//! offset to that origin on the file-time axis ([`crate::timeline`]):
//!
//! * camera starts later (typical, +150..250 ms): its first frame is held (`tpad` clone) so the
//!   bubble is there from the first frame; earlier: the front is `trim`med;
//! * mic starts earlier (typical, ~290 ms before the screen): the front is cut with `atrim`; later:
//!   `adelay` pushes it back;
//! * the output length is the screen file's length; audio is padded (`apad`) and the output is cut
//!   with `-t`, so audio and video end together.
//!
//! Loudness normalization to -14 LUFS is for the platform presets (EXP-2, M4); the quick export
//! only sums the sources (`amix normalize=0`) and limits peaks.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::layout::{bubble_geom, BubbleGeom, Shape};
use crate::spec::QuickSpec;
use crate::timeline::{offset_seconds, placement, Placement};
use crate::tools::{Caps, Encoder};

/// H.264 level 5.2 frame area (4096 x 2304); larger masters are scaled down to fit.
pub const MAX_H264_AREA: u64 = 4096 * 2304;
/// Longest side accepted (VideoToolbox and some others refuse more than 4096 wide even when the
/// area fits: a 5120 x 1440 ultrawide master, R5-4).
pub const MAX_H264_SIDE: u64 = 4096;
/// Software encoder quality.
pub const X264_CRF: u32 = 20;
/// Hardware encoder bitrate at 1080p30; scaled by area and fps.
pub const VT_BITRATE_1080P30: f64 = 12_000_000.0;
pub const AUDIO_RATE: u32 = 48_000;
pub const AUDIO_BPS_MONO: u32 = 128_000;
pub const AUDIO_BPS_STEREO: u32 = 192_000;

/// Output size and bubble geometry, decided before assets are rendered.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub out_size: (u32, u32),
    pub fps: f64,
    /// Present when a visible camera is composited.
    pub bubble: Option<BubbleGeom>,
}

fn even_floor(x: f64) -> u32 {
    let n = x.floor().max(2.0) as u32;
    n - n % 2
}

/// Output size: the screen master's size made even and capped to H.264 level 5.2, by area
/// (4096 x 2304) and by side (4096), scaled down proportionally.
pub fn output_size(master: (u32, u32)) -> (u32, u32) {
    let (w, h) = (u64::from(master.0.max(2)), u64::from(master.1.max(2)));
    if w * h <= MAX_H264_AREA && w <= MAX_H264_SIDE && h <= MAX_H264_SIDE {
        return (even_floor(w as f64), even_floor(h as f64));
    }
    let by_area = (MAX_H264_AREA as f64 / (w * h) as f64).sqrt();
    let by_side = (MAX_H264_SIDE as f64 / w.max(h) as f64).min(1.0);
    let f = by_area.min(by_side);
    (even_floor(w as f64 * f), even_floor(h as f64 * f))
}

/// First phase: size, fps and the bubble geometry. `master` is the probed size of `screen.mp4`
/// (the manifest's when there is no ffprobe); `camera_present` says the camera file exists.
pub fn prepare(spec: &QuickSpec, master: (u32, u32), camera_present: bool) -> Prepared {
    let out_size = output_size(master);
    let bubble = spec
        .visible_camera()
        .filter(|_| camera_present)
        .map(|(_, l)| bubble_geom(l, out_size));
    Prepared {
        out_size,
        fps: spec.screen.fps,
        bubble,
    }
}

/// Where the pre-rendered bubble PNGs are.
#[derive(Debug, Clone)]
pub struct BubbleAssets {
    pub mask: PathBuf,
    pub ring: PathBuf,
}

/// Everything [`build`] needs.
pub struct BuildInput<'a> {
    pub spec: &'a QuickSpec,
    /// The bundle folder holding the track files.
    pub bundle: &'a Path,
    pub prepared: &'a Prepared,
    pub assets: Option<&'a BubbleAssets>,
    pub encoder: Encoder,
    pub caps: &'a Caps,
    /// Output length in seconds (the screen file's length).
    pub duration_s: f64,
    /// Which optional tracks exist on disk.
    pub have_mic: bool,
    pub have_system: bool,
    /// The temporary output path.
    pub out: &'a Path,
}

/// The command and what it does.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Arguments after the program name.
    pub args: Vec<String>,
    pub filter_complex: String,
    pub duration_s: f64,
    pub out_size: (u32, u32),
    pub fps: f64,
    pub encoder: Encoder,
    pub bubble: Option<BubbleGeom>,
    /// Output channels, `None` without audio.
    pub audio_channels: Option<u32>,
    /// Seconds from the video start to each track's first sample.
    pub camera_offset_s: Option<f64>,
    pub mic_offset_s: Option<f64>,
    pub system_offset_s: Option<f64>,
    pub video_bitrate_bps: Option<u64>,
    /// Summary stored in the manifest `exports[].settings`.
    pub settings: Value,
}

/// Seconds with microsecond resolution, no exponent.
pub(crate) fn secs(x: f64) -> String {
    let s = format!("{x:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `30` or `29.97`.
pub(crate) fn rate(f: f64) -> String {
    secs((f * 1000.0).round() / 1000.0)
}

/// Target bitrate of the hardware encoder: 12 Mbps at 1080p30, scaled by area and fps, 2..60 Mbps.
pub fn videotoolbox_bitrate(size: (u32, u32), fps: f64) -> u64 {
    let area = f64::from(size.0) * f64::from(size.1) / (1920.0 * 1080.0);
    let rate = (VT_BITRATE_1080P30 * area * (fps / 30.0).max(1.0)).clamp(2_000_000.0, 60_000_000.0);
    rate.round() as u64
}

fn video_filters_for_camera(
    offset: f64,
    mirror: bool,
    shape: Shape,
    w: u32,
    h: u32,
    in_idx: usize,
) -> String {
    let mut f: Vec<String> = vec!["setpts=PTS-STARTPTS".into()];
    match placement(offset) {
        Placement::Aligned => {}
        Placement::Delay(d) => f.push(format!("tpad=start_duration={}:start_mode=clone", secs(d))),
        Placement::Trim(t) => {
            f.push(format!("trim=start={}", secs(t)));
            f.push("setpts=PTS-STARTPTS".into());
        }
    }
    if mirror {
        f.push("hflip".into());
    }
    let aspect = 1.0 / shape.aspect_h_over_w();
    let a = secs((aspect * 10_000.0).round() / 10_000.0);
    f.push(format!("crop=w='min(iw,ih*{a})':h='min(ih,iw/{a})'"));
    f.push(format!("scale={w}:{h}:flags=lanczos"));
    f.push("format=rgba".into());
    format!("[{in_idx}:v]{}[cam]", f.join(","))
}

fn audio_chain(in_idx: usize, offset: f64, layout: &str, label: &str) -> String {
    let mut f: Vec<String> = vec!["asetpts=PTS-STARTPTS".into()];
    match placement(offset) {
        Placement::Aligned => {}
        Placement::Delay(d) => {
            let ms = (d * 1000.0).round() as i64;
            f.push(format!("adelay=delays={ms}:all=1"));
        }
        Placement::Trim(t) => {
            f.push(format!("atrim=start={}", secs(t)));
            f.push("asetpts=PTS-STARTPTS".into());
        }
    }
    f.push(format!("aresample={AUDIO_RATE}"));
    f.push(format!("aformat=sample_fmts=fltp:channel_layouts={layout}"));
    format!("[{in_idx}:a]{}[{label}]", f.join(","))
}

/// The video encoder arguments of an encoder at an output size and rate.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoCodec {
    pub args: Vec<String>,
    /// Target bitrate; `None` for the CRF software encoder.
    pub bitrate: Option<u64>,
    pub gop: u32,
}

/// `-c:v ...` and its rate control for `encoder`. Arguments other than VideoToolbox's and
/// libx264's have only been checked as strings (tested with fakes, no Windows machine or
/// discrete GPU).
pub fn video_codec(encoder: Encoder, out_size: (u32, u32), fps: f64) -> VideoCodec {
    video_codec_at(encoder, out_size, fps, None)
}

/// [`video_codec`] with a bitrate a preset asks for (EXP-2); `None` uses the size-based default.
/// With a bitrate, `libx264` rate-controls by it too instead of the CRF.
pub fn video_codec_at(
    encoder: Encoder,
    out_size: (u32, u32),
    fps: f64,
    target_bps: Option<u64>,
) -> VideoCodec {
    let gop = ((fps / 2.0).round() as u32).max(1);
    let mut bitrate = None;
    let bps = target_bps.unwrap_or_else(|| videotoolbox_bitrate(out_size, fps));
    let (g, b, crf) = (gop.to_string(), bps.to_string(), X264_CRF.to_string());
    let codec_args: Vec<&str> = match encoder {
        Encoder::VideoToolbox => vec![
            "-c:v",
            "h264_videotoolbox",
            "-profile:v",
            "high",
            "-b:v",
            &b,
            "-g",
            &g,
        ],
        Encoder::H264Mf => vec!["-c:v", "h264_mf", "-b:v", &b, "-g", &g],
        Encoder::LibOpenH264 => vec!["-c:v", "libopenh264", "-b:v", &b, "-g", &g],
        Encoder::Nvenc => vec![
            "-c:v",
            "h264_nvenc",
            "-preset",
            "p4",
            "-profile:v",
            "high",
            "-b:v",
            &b,
            "-g",
            &g,
        ],
        Encoder::Qsv => vec![
            "-c:v",
            "h264_qsv",
            "-preset",
            "veryfast",
            "-profile:v",
            "high",
            "-b:v",
            &b,
            "-g",
            &g,
        ],
        Encoder::Amf => vec![
            "-c:v",
            "h264_amf",
            "-quality",
            "balanced",
            "-profile:v",
            "high",
            "-b:v",
            &b,
            "-g",
            &g,
        ],
        Encoder::Libx264 if target_bps.is_some() => vec![
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-b:v",
            &b,
            "-profile:v",
            "high",
            "-g",
            &g,
        ],
        Encoder::Libx264 => {
            bitrate = None;
            vec![
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                &crf,
                "-profile:v",
                "high",
                "-g",
                &g,
            ]
        }
    };
    if encoder != Encoder::Libx264 || target_bps.is_some() {
        bitrate = Some(bps);
    }
    VideoCodec {
        args: codec_args.into_iter().map(String::from).collect(),
        bitrate,
        gop,
    }
}

/// Builds the FFmpeg arguments and filter graph.
pub fn build(i: &BuildInput<'_>) -> Plan {
    let spec = i.spec;
    let p = i.prepared;
    let fps = p.fps;
    let dur = i.duration_s;
    let path = |f: &str| i.bundle.join(f).to_string_lossy().into_owned();

    let mut args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "warning",
        "-nostats",
        "-progress",
        "pipe:1",
    ]
    .map(String::from)
    .to_vec();
    let mut graph: Vec<String> = Vec::new();
    let mut next = 0usize;
    let mut input = |args: &mut Vec<String>, pre: &[&str], file: String| -> usize {
        args.extend(pre.iter().map(|s| (*s).to_owned()));
        args.push("-i".into());
        args.push(file);
        next += 1;
        next - 1
    };

    // ---- video ----
    let screen_idx = input(&mut args, &[], path(&spec.screen.file));
    // The export starts at the recording's `live_at`: the screen's front is cut off and every
    // other track is placed against that origin.
    let origin = spec.origin();
    let front = spec.front_trim_seconds();
    let mut bg = Vec::new();
    if front > 0.0005 {
        bg.push(format!("trim=start={}", secs(front)));
    }
    bg.push("setpts=PTS-STARTPTS".to_owned());
    bg.push(format!("fps={}", rate(fps)));
    if p.out_size != spec.screen.size {
        bg.push(format!(
            "scale={}:{}:flags=lanczos",
            p.out_size.0, p.out_size.1
        ));
    }
    bg.push("setsar=1".into());
    let mut camera_offset = None;
    let mut have_bubble = false;
    match (spec.visible_camera(), &p.bubble, i.assets) {
        (Some((cam, layout)), Some(g), Some(assets)) => {
            have_bubble = true;
            let off = offset_seconds(cam.start, origin, &spec.gaps);
            camera_offset = Some(off);
            let cam_idx = input(&mut args, &[], path(&cam.file));
            let loop_pre = [
                "-loop".to_owned(),
                "1".to_owned(),
                "-framerate".to_owned(),
                rate(fps),
                "-t".to_owned(),
                secs(dur),
            ];
            let loop_pre: Vec<&str> = loop_pre.iter().map(String::as_str).collect();
            let mask_idx = input(
                &mut args,
                &loop_pre,
                assets.mask.to_string_lossy().into_owned(),
            );
            let ring_idx = input(
                &mut args,
                &loop_pre,
                assets.ring.to_string_lossy().into_owned(),
            );
            graph.push(format!("[{screen_idx}:v]{}[bg]", bg.join(",")));
            graph.push(video_filters_for_camera(
                off,
                layout.mirror.unwrap_or(false),
                layout.shape,
                g.w,
                g.h,
                cam_idx,
            ));
            graph.push(format!("[{mask_idx}:v]format=gray[mask]"));
            graph.push("[cam][mask]alphamerge[camm]".into());
            graph.push(format!("[{ring_idx}:v]format=rgba[ring]"));
            graph.push("[camm][ring]overlay=x=0:y=0:format=auto[bub]".into());
            graph.push(format!(
                "[bg][bub]overlay=x={}:y={}:format=auto:eof_action=repeat,format=yuv420p[outv]",
                g.x, g.y
            ));
        }
        _ => {
            graph.push(format!(
                "[{screen_idx}:v]{},format=yuv420p[outv]",
                bg.join(",")
            ));
        }
    }

    // ---- audio ----
    let mic = spec.mic.as_ref().filter(|_| i.have_mic);
    let system = spec.system.as_ref().filter(|_| i.have_system);
    let channels = [mic.map(|m| m.channels), system.map(|s| s.channels)]
        .into_iter()
        .flatten()
        .max();
    let mut mic_offset = None;
    let mut system_offset = None;
    if let Some(ch) = channels {
        let layout = if ch >= 2 { "stereo" } else { "mono" };
        let mut labels = Vec::new();
        if let Some(m) = mic {
            let off = offset_seconds(m.start, origin, &spec.gaps);
            mic_offset = Some(off);
            let idx = input(&mut args, &[], path(&m.file));
            graph.push(audio_chain(idx, off, layout, "a_mic"));
            labels.push("[a_mic]");
        }
        if let Some(s) = system {
            let off = offset_seconds(s.start, origin, &spec.gaps);
            system_offset = Some(off);
            let idx = input(&mut args, &[], path(&s.file));
            graph.push(audio_chain(idx, off, layout, "a_sys"));
            labels.push("[a_sys]");
        }
        let pad = format!("apad=whole_dur={}", secs(dur));
        if labels.len() == 2 {
            let norm = if i.caps.amix_normalize() {
                ":normalize=0"
            } else {
                ""
            };
            graph.push(format!(
                "{}amix=inputs=2:duration=longest:dropout_transition=0{norm},alimiter=limit=0.97:level=disabled,{pad}[outa]",
                labels.concat()
            ));
        } else {
            graph.push(format!("{}{pad}[outa]", labels[0]));
        }
    }

    // ---- encoders and output ----
    let filter_complex = graph.join(";");
    args.extend(["-filter_complex".into(), filter_complex.clone()]);
    args.extend(["-map".into(), "[outv]".into()]);
    if channels.is_some() {
        args.extend(["-map".into(), "[outa]".into()]);
    }
    args.extend(["-map_metadata".into(), "-1".into()]);
    let codec = video_codec(i.encoder, p.out_size, fps);
    let (gop, bitrate) = (codec.gop, codec.bitrate);
    args.extend(codec.args);
    args.extend([
        "-pix_fmt".into(),
        i.encoder.pix_fmt().into(),
        "-tag:v".into(),
        "avc1".into(),
    ]);
    if i.caps.fps_mode() {
        args.extend(["-fps_mode".into(), "cfr".into()]);
    }
    if let Some(ch) = channels {
        let bps = if ch >= 2 {
            AUDIO_BPS_STEREO
        } else {
            AUDIO_BPS_MONO
        };
        args.extend(
            [
                "-c:a",
                "aac",
                "-b:a",
                &bps.to_string(),
                "-ar",
                &AUDIO_RATE.to_string(),
                "-ac",
                &ch.to_string(),
            ]
            .map(String::from),
        );
    }
    args.extend(["-movflags", "+faststart", "-f", "mp4", "-t", &secs(dur)].map(String::from));
    args.push(i.out.to_string_lossy().into_owned());

    let settings = json!({
        "size": [p.out_size.0, p.out_size.1],
        "fps": fps,
        "video": match i.encoder {
            Encoder::Libx264 => json!({"codec": "h264", "encoder": i.encoder.name(), "profile": "high",
                                       "crf": X264_CRF, "preset": "veryfast", "gop": gop,
                                       "lgpl_safe": false}),
            _ => json!({"codec": "h264", "encoder": i.encoder.name(), "profile": "high",
                        "bitrate_bps": bitrate, "gop": gop, "lgpl_safe": i.encoder.lgpl_safe()}),
        },
        "audio": channels.map(|ch| json!({
            "codec": "aac", "rate": AUDIO_RATE, "channels": ch,
            "bitrate_bps": if ch >= 2 { AUDIO_BPS_STEREO } else { AUDIO_BPS_MONO },
            "mic": mic.is_some(), "system": system.is_some(),
            "loudness_normalization": false,
        })),
        "camera": have_bubble.then(|| json!({
            "offset_s": camera_offset,
            "layout": spec.layout.as_ref().map(crate::layout::CameraLayout::to_json),
            "bubble_px": p.bubble.as_ref().map(|g| json!([g.x, g.y, g.w, g.h])),
            "layout_source": "layout at stop",
        })),
        "offsets_s": {"camera": camera_offset, "mic": mic_offset, "system": system_offset},
        "notes": spec.notes,
        "ffmpeg": i.caps.version_line,
    });

    Plan {
        args,
        filter_complex,
        duration_s: dur,
        out_size: p.out_size,
        fps,
        encoder: i.encoder,
        bubble: if have_bubble { p.bubble.clone() } else { None },
        audio_channels: channels,
        camera_offset_s: camera_offset,
        mic_offset_s: mic_offset,
        system_offset_s: system_offset,
        video_bitrate_bps: bitrate,
        settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::fixtures::manifest;
    use crate::tools::caps_from_output;
    use serde_json::json;

    fn caps(version: &str) -> Caps {
        caps_from_output(
            &format!("ffmpeg version {version}"),
            " V h264_videotoolbox x\n V libx264 y\n A aac z\n",
        )
    }

    fn plan_for(m: &Value, encoder: Encoder, mic: bool, sys: bool) -> Plan {
        plan_with(m, encoder, mic, sys, &caps("7.1"))
    }

    fn plan_with(m: &Value, encoder: Encoder, mic: bool, sys: bool, caps: &Caps) -> Plan {
        let spec = QuickSpec::from_manifest(m).unwrap();
        let prepared = prepare(&spec, spec.screen.size, true);
        let assets = BubbleAssets {
            mask: "/w/mask.png".into(),
            ring: "/w/ring.png".into(),
        };
        build(&BuildInput {
            spec: &spec,
            bundle: Path::new("/b/Lesson 1.cmrec"),
            prepared: &prepared,
            assets: Some(&assets),
            encoder,
            caps,
            duration_s: 10.0,
            have_mic: mic,
            have_system: sys,
            out: Path::new("/b/Lesson 1.cmrec/exports/x.part"),
        })
    }

    /// R5-4: every encoder gets its own arguments; only libx264 uses a CRF and is not LGPL safe.
    #[test]
    fn r5_4_each_encoder_has_its_arguments_and_lgpl_flag() {
        for (enc, name, hw_args) in [
            (Encoder::H264Mf, "h264_mf", false),
            (Encoder::LibOpenH264, "libopenh264", false),
            (Encoder::Nvenc, "h264_nvenc", true),
            (Encoder::Qsv, "h264_qsv", true),
            (Encoder::Amf, "h264_amf", true),
            (Encoder::VideoToolbox, "h264_videotoolbox", true),
        ] {
            let p = plan_for(&manifest(), enc, true, true);
            assert_eq!(arg_after(&p.args, "-c:v"), Some(name));
            assert!(
                arg_after(&p.args, "-b:v").is_some(),
                "{name} takes a bitrate"
            );
            assert!(arg_after(&p.args, "-crf").is_none(), "{name}");
            assert_eq!(arg_after(&p.args, "-pix_fmt"), Some(enc.pix_fmt()));
            assert_eq!(p.settings["video"]["lgpl_safe"], true, "{name}");
            assert_eq!(
                arg_after(&p.args, "-profile:v").is_some(),
                hw_args,
                "{name}"
            );
            // VideoToolbox-only flags never leak into the others.
            assert!(!p.args.contains(&"-allow_sw".to_owned()));
        }
        let x = plan_for(&manifest(), Encoder::Libx264, true, true);
        assert_eq!(x.settings["video"]["lgpl_safe"], false);
        assert_eq!(arg_after(&x.args, "-crf"), Some("20"));
    }

    /// R5-4: a master over 4096 on one side is scaled to fit even when its area is small enough.
    #[test]
    fn r5_4_each_side_is_capped_at_4096() {
        assert_eq!(output_size((5120, 1440)), (4096, 1152));
        assert_eq!(output_size((1440, 5120)), (1152, 4096));
        assert_eq!(output_size((4096, 2304)), (4096, 2304));
        let (w, h) = output_size((8000, 800));
        assert!(
            w <= 4096 && h <= 4096 && w % 2 == 0 && h % 2 == 0,
            "{w}x{h}"
        );
        // Aspect preserved within a pixel.
        assert!((f64::from(w) / f64::from(h) - 10.0).abs() < 0.05);
    }

    fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let i = args.iter().position(|a| a == flag)?;
        args.get(i + 1).map(String::as_str)
    }

    /// EXP-1 + REC-1: the full graph for screen + camera + mic + system with the REC-1 offsets
    /// (camera +190 ms, mic -290 ms, system aligned).
    #[test]
    fn full_graph_with_offsets() {
        let p = plan_for(&manifest(), Encoder::Libx264, true, true);
        let g = &p.filter_complex;
        // Background: screen from its first frame at the screen fps, size untouched.
        assert!(
            g.contains("[0:v]setpts=PTS-STARTPTS,fps=30,setsar=1[bg]"),
            "{g}"
        );
        // Camera starts 190 ms after the screen: its first frame is held, mirrored (true), covered
        // to a square and scaled to the bubble size.
        assert!(
            g.contains("[1:v]setpts=PTS-STARTPTS,tpad=start_duration=0.19:start_mode=clone,hflip,crop=w='min(iw,ih*1)':h='min(ih,iw/1)',scale=240:240:flags=lanczos,format=rgba[cam]"),
            "{g}"
        );
        assert!(g.contains("[2:v]format=gray[mask]"));
        assert!(g.contains("[cam][mask]alphamerge[camm]"));
        assert!(g.contains("[3:v]format=rgba[ring]"));
        assert!(g.contains("[camm][ring]overlay=x=0:y=0:format=auto[bub]"));
        // 240 pt of a 1280 pt area on a 1280 px output, bottom-right with the 24 pt margin.
        assert!(
            g.contains(
                "[bg][bub]overlay=x=1016:y=456:format=auto:eof_action=repeat,format=yuv420p[outv]"
            ),
            "{g}"
        );
        // Mic starts 290 ms before the screen: cut; system aligned: no delay or trim.
        assert!(g.contains("[4:a]asetpts=PTS-STARTPTS,atrim=start=0.29,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a_mic]"), "{g}");
        assert!(g.contains("[5:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a_sys]"), "{g}");
        assert!(g.contains("[a_mic][a_sys]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0,alimiter=limit=0.97:level=disabled,apad=whole_dur=10[outa]"), "{g}");
        assert_eq!(p.camera_offset_s, Some(0.19));
        assert!((p.mic_offset_s.unwrap() + 0.29).abs() < 1e-9);
        assert_eq!(p.system_offset_s, Some(0.0));
        assert_eq!(p.audio_channels, Some(2));
        // Inputs in graph order, the bubble PNGs looped for the output length.
        let ins: Vec<&str> = p
            .args
            .windows(2)
            .filter(|w| w[0] == "-i")
            .map(|w| w[1].as_str())
            .collect();
        // Files inside the bundle are joined with the platform separator.
        let in_bundle = |f: &str| {
            std::path::Path::new("/b/Lesson 1.cmrec")
                .join(f)
                .to_string_lossy()
                .into_owned()
        };
        assert_eq!(
            ins,
            [
                in_bundle("screen.mp4").as_str(),
                in_bundle("camera.mp4").as_str(),
                "/w/mask.png",
                "/w/ring.png",
                in_bundle("mic.denoised.wav").as_str(),
                in_bundle("system.wav").as_str()
            ]
        );
        let joined = p.args.join(" ");
        assert!(
            joined.contains("-loop 1 -framerate 30 -t 10 -i /w/mask.png"),
            "{joined}"
        );
    }

    #[test]
    fn output_options_are_h264_high_aac_faststart_cfr() {
        let p = plan_for(&manifest(), Encoder::Libx264, true, true);
        let a = &p.args;
        assert_eq!(arg_after(a, "-c:v"), Some("libx264"));
        assert_eq!(arg_after(a, "-profile:v"), Some("high"));
        assert_eq!(arg_after(a, "-crf"), Some("20"));
        assert_eq!(arg_after(a, "-pix_fmt"), Some("yuv420p"));
        assert_eq!(arg_after(a, "-c:a"), Some("aac"));
        assert_eq!(arg_after(a, "-b:a"), Some("192000"));
        assert_eq!(arg_after(a, "-ar"), Some("48000"));
        assert_eq!(arg_after(a, "-ac"), Some("2"));
        assert_eq!(arg_after(a, "-movflags"), Some("+faststart"));
        assert_eq!(arg_after(a, "-fps_mode"), Some("cfr"));
        assert_eq!(arg_after(a, "-t"), Some("10"));
        assert_eq!(arg_after(a, "-g"), Some("15"));
        assert_eq!(a.last().unwrap(), "/b/Lesson 1.cmrec/exports/x.part");
        assert_eq!(arg_after(a, "-progress"), Some("pipe:1"));
        // Old FFmpeg has no -fps_mode and no amix normalize.
        let old = plan_with(&manifest(), Encoder::Libx264, true, true, &caps("4.3"));
        assert!(!old.args.contains(&"-fps_mode".to_owned()));
        assert!(!old.filter_complex.contains("normalize"));
    }

    #[test]
    fn hardware_encoder_uses_a_bitrate_scaled_by_area() {
        let p = plan_for(&manifest(), Encoder::VideoToolbox, true, true);
        assert_eq!(arg_after(&p.args, "-c:v"), Some("h264_videotoolbox"));
        // 1280x720 is 0.444 of 1080p: 12 Mbps * 0.444 = 5.33 Mbps.
        assert_eq!(arg_after(&p.args, "-b:v"), Some("5333333"));
        assert_eq!(p.video_bitrate_bps, Some(5_333_333));
        assert_eq!(p.settings["video"]["encoder"], "h264_videotoolbox");
        assert_eq!(videotoolbox_bitrate((1920, 1080), 30.0), 12_000_000);
        assert_eq!(videotoolbox_bitrate((1920, 1080), 60.0), 24_000_000);
        assert_eq!(videotoolbox_bitrate((3840, 2160), 30.0), 48_000_000);
        assert_eq!(videotoolbox_bitrate((320, 180), 30.0), 2_000_000);
        assert_eq!(videotoolbox_bitrate((4096, 2304), 60.0), 60_000_000);
    }

    /// Mirror off -> no hflip; rounded -> 4:3 crop, scaled to the 4:3 box.
    #[test]
    fn mirror_and_shape_change_the_camera_chain() {
        let mut m = manifest();
        m["tracks"]["camera"]["mirror"] = json!(false);
        m["settings"]["camera"]["mirror"] = json!(false);
        m["settings"]["camera"]["shape"] = json!("rounded");
        let p = plan_for(&m, Encoder::Libx264, true, true);
        let g = &p.filter_complex;
        assert!(!g.contains("hflip"), "{g}");
        assert!(
            g.contains(
                "crop=w='min(iw,ih*1.3333)':h='min(ih,iw/1.3333)',scale=240:180:flags=lanczos"
            ),
            "{g}"
        );
    }

    /// A camera that starts before the screen is trimmed; no offset, no tpad/trim at all.
    #[test]
    fn camera_before_the_screen_is_trimmed_and_equal_starts_are_untouched() {
        let mut m = manifest();
        m["tracks"]["camera"]["start"] = json!(46_800 - 12_000); // 100 ms earlier
        let g = plan_for(&m, Encoder::Libx264, true, true).filter_complex;
        assert!(
            g.contains("[1:v]setpts=PTS-STARTPTS,trim=start=0.1,setpts=PTS-STARTPTS,hflip"),
            "{g}"
        );
        m["tracks"]["camera"]["start"] = json!(46_800);
        let g = plan_for(&m, Encoder::Libx264, true, true).filter_complex;
        assert!(g.contains("[1:v]setpts=PTS-STARTPTS,hflip"), "{g}");
        assert!(!g.contains("tpad") && !g.contains(",trim="));
    }

    /// The export starts at the manifest's `live_at` (here the camera's first frame, 190 ms after
    /// the screen's): the screen front is trimmed and every track is placed against that origin.
    #[test]
    fn exp1_the_export_starts_at_live_at() {
        let mut m = manifest();
        m["live_at"] = json!(69_600);
        let p = plan_for(&m, Encoder::Libx264, true, true);
        let g = &p.filter_complex;
        assert!(
            g.contains("[0:v]trim=start=0.19,setpts=PTS-STARTPTS,fps=30"),
            "{g}"
        );
        assert_eq!(p.camera_offset_s, Some(0.0));
        assert!((p.mic_offset_s.unwrap() + 0.48).abs() < 1e-9);
        assert!((p.system_offset_s.unwrap() + 0.19).abs() < 1e-9);
        assert!(!g.contains("tpad"), "{g}");
    }

    /// A long pre-roll (the camera warms up for 2.5 s): the front trim is that long, and a
    /// camera that starts before the origin is trimmed, not held.
    #[test]
    fn exp1_a_long_pre_roll_is_trimmed_from_the_screen() {
        let mut m = manifest();
        let live = 46_800 + 300_000; // screen start + 2.5 s
        m["live_at"] = json!(live);
        m["tracks"]["camera"]["start"] = json!(live);
        let p = plan_for(&m, Encoder::Libx264, true, true);
        assert!(
            p.filter_complex
                .contains("trim=start=2.5,setpts=PTS-STARTPTS,fps=30"),
            "{}",
            p.filter_complex
        );
        assert_eq!(p.camera_offset_s, Some(0.0));
        // The mic started before the screen: 2.79 s of it is cut off the front.
        assert!((p.mic_offset_s.unwrap() + 2.79).abs() < 1e-9);
    }

    /// No `live_at`, or one outside the screen track, keeps the screen's first frame as the
    /// start (older recordings).
    #[test]
    fn exp1_without_a_usable_live_at_the_screen_start_is_the_origin() {
        let mut m = manifest();
        for live in [json!(null), json!(46_800), json!(100), json!(5_000_000)] {
            m["live_at"] = live;
            let p = plan_for(&m, Encoder::Libx264, true, true);
            assert!(
                p.filter_complex.contains("[0:v]setpts=PTS-STARTPTS,fps=30"),
                "{}",
                p.filter_complex
            );
            assert!((p.camera_offset_s.unwrap() - 0.19).abs() < 1e-9);
        }
    }

    /// REC-4: a pause before a late track start is removed from the offset.
    #[test]
    fn pauses_before_a_start_do_not_shift_it() {
        let mut m = manifest();
        // Mic joins after the pause (session 800,000 = file 680,000); screen at file 46,800.
        m["tracks"]["mic"]["start"] = json!(800_000);
        let p = plan_for(&m, Encoder::Libx264, true, false);
        let expect = (680_000.0 - 46_800.0) / 120_000.0;
        assert!((p.mic_offset_s.unwrap() - expect).abs() < 1e-9);
        assert!(
            p.filter_complex.contains("adelay=delays=5277:all=1"),
            "{}",
            p.filter_complex
        );
    }

    #[test]
    fn mic_only_is_mono_system_only_is_stereo() {
        let p = plan_for(&manifest(), Encoder::Libx264, true, false);
        assert_eq!(p.audio_channels, Some(1));
        assert!(p.filter_complex.contains("[a_mic]apad=whole_dur=10[outa]"));
        assert!(!p.filter_complex.contains("amix"));
        assert_eq!(arg_after(&p.args, "-b:a"), Some("128000"));
        assert_eq!(arg_after(&p.args, "-ac"), Some("1"));
        let s = plan_for(&manifest(), Encoder::Libx264, false, true);
        assert_eq!(s.audio_channels, Some(2));
        assert!(s.filter_complex.contains("[a_sys]apad"));
    }

    #[test]
    fn no_audio_means_no_audio_map() {
        let p = plan_for(&manifest(), Encoder::Libx264, false, false);
        assert_eq!(p.audio_channels, None);
        assert!(!p.args.contains(&"-c:a".to_owned()));
        assert!(!p.args.iter().any(|a| a == "[outa]"));
        assert!(p.settings["audio"].is_null());
    }

    /// No camera: screen + audio only, no PNG inputs, no overlay.
    #[test]
    fn camera_less_export_has_no_bubble() {
        let mut m = manifest();
        m["tracks"].as_object_mut().unwrap().remove("camera");
        let spec = QuickSpec::from_manifest(&m).unwrap();
        let prepared = prepare(&spec, spec.screen.size, false);
        assert!(prepared.bubble.is_none());
        let p = build(&BuildInput {
            spec: &spec,
            bundle: Path::new("/b"),
            prepared: &prepared,
            assets: None,
            encoder: Encoder::Libx264,
            caps: &caps("7.1"),
            duration_s: 3.0,
            have_mic: true,
            have_system: false,
            out: Path::new("/b/o.part"),
        });
        assert!(!p.filter_complex.contains("overlay"));
        assert!(
            p.filter_complex
                .starts_with("[0:v]setpts=PTS-STARTPTS,fps=30,setsar=1,format=yuv420p[outv];"),
            "{}",
            p.filter_complex
        );
        assert!(!p.args.contains(&"-loop".to_owned()));
        assert!(p.bubble.is_none() && p.camera_offset_s.is_none());
        assert!(p.settings["camera"].is_null());
    }

    #[test]
    fn a_hidden_bubble_exports_screen_and_audio_only() {
        let mut m = manifest();
        m["edits"][0]["layout"] =
            json!({"camera": {"visible": false, "rect": [0.5, 0.5, 0.2, 0.2]}});
        let spec = QuickSpec::from_manifest(&m).unwrap();
        let prepared = prepare(&spec, spec.screen.size, true);
        assert!(prepared.bubble.is_none());
    }

    #[test]
    fn sizes_are_even_and_capped_to_the_h264_level() {
        assert_eq!(output_size((1280, 720)), (1280, 720));
        assert_eq!(output_size((1281, 721)), (1280, 720));
        assert_eq!(output_size((3024, 1964)), (3024, 1964));
        // 5K: scaled down to fit 4096x2304 worth of pixels, aspect kept, even.
        let (w, h) = output_size((5120, 2880));
        assert_eq!((w, h), (4096, 2304));
        let (w, h) = output_size((6016, 3384));
        assert!(u64::from(w) * u64::from(h) <= MAX_H264_AREA && w % 2 == 0 && h % 2 == 0);
        assert!((f64::from(w) / f64::from(h) - 6016.0 / 3384.0).abs() < 0.01);
        // A scaled master adds a scale filter.
        let mut m = manifest();
        m["tracks"]["screen"]["size"] = json!([5120, 2880]);
        let g = plan_for(&m, Encoder::Libx264, false, false).filter_complex;
        assert!(g.contains("scale=4096:2304:flags=lanczos"), "{g}");
    }

    #[test]
    fn number_formatting() {
        assert_eq!(secs(0.19), "0.19");
        assert_eq!(secs(10.0), "10");
        assert_eq!(secs(1.0 / 3.0), "0.333333");
        assert_eq!(rate(29.97), "29.97");
        assert_eq!(rate(60.0), "60");
    }
}
