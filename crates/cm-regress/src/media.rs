//! Looking at produced media with the package's own `ffprobe` (and `ffmpeg` for a level reading).

use crate::ctx::Ctx;
use crate::model::{fail, Stop, R};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct Stream {
    pub codec_type: String,
    pub codec_name: String,
    pub width: i64,
    pub height: i64,
    pub r_frame_rate: Option<f64>,
    pub avg_frame_rate: Option<f64>,
    pub pix_fmt: String,
    pub color_space: String,
    pub color_transfer: String,
    pub color_primaries: String,
    pub color_range: String,
    pub nb_frames: Option<u64>,
    pub sample_rate: Option<u64>,
    pub channels: Option<u64>,
    /// Display rotation in degrees (side data or tag), 0 when none.
    pub rotation: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub format_name: String,
    pub duration: f64,
    pub size: u64,
    pub streams: Vec<Stream>,
}

impl Probe {
    pub fn video(&self) -> Option<&Stream> {
        self.streams.iter().find(|s| s.codec_type == "video")
    }
    pub fn audio(&self) -> Option<&Stream> {
        self.streams.iter().find(|s| s.codec_type == "audio")
    }
    /// The video stream, or a failure saying the file has none.
    pub fn need_video(&self) -> R<&Stream> {
        self.video()
            .ok_or_else(|| Stop::Fail("the file has no video stream".to_owned()))
    }
}

/// `"30000/1001"` or `"30"` to a number; `0/0` is None.
pub fn parse_rate(s: &str) -> Option<f64> {
    let (n, d) = match s.split_once('/') {
        Some((n, d)) => (n.trim().parse::<f64>().ok()?, d.trim().parse::<f64>().ok()?),
        None => (s.trim().parse::<f64>().ok()?, 1.0),
    };
    if d == 0.0 {
        None
    } else {
        Some(n / d)
    }
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn number(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| {
        x.as_f64()
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    })
}

/// Parses `ffprobe -print_format json -show_format -show_streams`.
pub fn parse_probe(json: &str) -> Result<Probe, String> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| format!("ffprobe printed no JSON: {e}"))?;
    let fmt = v.get("format").cloned().unwrap_or(Value::Null);
    let mut p = Probe {
        format_name: text(&fmt, "format_name").to_owned(),
        duration: number(&fmt, "duration").unwrap_or(0.0),
        size: number(&fmt, "size").unwrap_or(0.0) as u64,
        streams: Vec::new(),
    };
    for s in v
        .get("streams")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut rotation = s
            .pointer("/tags/rotate")
            .and_then(Value::as_str)
            .and_then(|r| r.parse::<i64>().ok())
            .unwrap_or(0);
        for sd in s
            .get("side_data_list")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(r) = sd.get("rotation").and_then(Value::as_f64) {
                rotation = r as i64;
            }
        }
        p.streams.push(Stream {
            codec_type: text(s, "codec_type").to_owned(),
            codec_name: text(s, "codec_name").to_owned(),
            width: s.get("width").and_then(Value::as_i64).unwrap_or(0),
            height: s.get("height").and_then(Value::as_i64).unwrap_or(0),
            r_frame_rate: parse_rate(text(s, "r_frame_rate")),
            avg_frame_rate: parse_rate(text(s, "avg_frame_rate")),
            pix_fmt: text(s, "pix_fmt").to_owned(),
            color_space: text(s, "color_space").to_owned(),
            color_transfer: text(s, "color_transfer").to_owned(),
            color_primaries: text(s, "color_primaries").to_owned(),
            color_range: text(s, "color_range").to_owned(),
            nb_frames: s
                .get("nb_frames")
                .and_then(Value::as_str)
                .and_then(|n| n.parse().ok()),
            sample_rate: s
                .get("sample_rate")
                .and_then(Value::as_str)
                .and_then(|n| n.parse().ok()),
            channels: s.get("channels").and_then(Value::as_u64),
            rotation,
        });
    }
    Ok(p)
}

/// Key-frame times from `ffprobe -show_entries packet=pts_time,flags -of csv=p=0`.
pub fn parse_keyframes(csv: &str) -> Vec<f64> {
    let mut t: Vec<f64> = csv
        .lines()
        .filter_map(|l| {
            let (time, flags) = l.trim().split_once(',')?;
            if flags.contains('K') {
                time.parse().ok()
            } else {
                None
            }
        })
        .collect();
    t.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    t
}

/// The longest and shortest gap between consecutive key frames.
pub fn key_gaps(times: &[f64]) -> Option<(f64, f64)> {
    if times.len() < 2 {
        return None;
    }
    let gaps: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    let max = gaps.iter().cloned().fold(f64::MIN, f64::max);
    let min = gaps.iter().cloned().fold(f64::MAX, f64::min);
    Some((min, max))
}

fn tool(ctx: &Ctx, name: &str) -> PathBuf {
    let p = ctx.pkg.exe(name);
    if p.is_file() {
        p
    } else {
        // A build tree has no bundled copy: fall back to PATH (and say so in the evidence).
        PathBuf::from(crate::sys::exe_name(name))
    }
}

pub fn probe(ctx: &mut Ctx, file: &Path) -> R<Probe> {
    let mut c = Command::new(tool(ctx, "ffprobe"));
    c.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(file);
    let out = ctx.run(c, 60)?;
    if !out.success() {
        return fail(format!(
            "ffprobe failed on {}: {}",
            file.display(),
            crate::util::cut(&crate::util::one_line(&out.text()), 300)
        ));
    }
    parse_probe(&out.stdout).map_err(Stop::Fail)
}

pub fn keyframes(ctx: &mut Ctx, file: &Path) -> R<Vec<f64>> {
    let mut c = Command::new(tool(ctx, "ffprobe"));
    c.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "packet=pts_time,flags",
        "-of",
        "csv=p=0",
    ])
    .arg(file);
    let out = ctx.run(c, 120)?;
    if !out.success() {
        return fail(format!(
            "ffprobe packets failed: {}",
            crate::util::cut(&out.text(), 200)
        ));
    }
    Ok(parse_keyframes(&out.stdout))
}

/// Mean and peak level in dB of a window of an audio file, read by FFmpeg's `volumedetect`.
pub fn audio_level(ctx: &mut Ctx, file: &Path, from_s: f64, dur_s: f64) -> R<(f64, f64)> {
    let mut c = Command::new(tool(ctx, "ffmpeg"));
    c.args([
        "-hide_banner",
        "-nostats",
        "-ss",
        &format!("{from_s:.3}"),
        "-t",
        &format!("{dur_s:.3}"),
        "-i",
    ])
    .arg(file)
    .args(["-af", "volumedetect", "-vn", "-f", "null", "-"]);
    let out = ctx.run(c, 120)?;
    parse_volumedetect(&out.stderr).ok_or_else(|| {
        Stop::Fail(format!(
            "no volumedetect output: {}",
            crate::util::cut(&out.stderr, 300)
        ))
    })
}

pub fn parse_volumedetect(stderr: &str) -> Option<(f64, f64)> {
    let grab = |key: &str| -> Option<f64> {
        let l = stderr.lines().find(|l| l.contains(key))?;
        l.split(key)
            .nth(1)?
            .trim()
            .trim_end_matches("dB")
            .trim()
            .parse()
            .ok()
    };
    Some((grab("mean_volume:")?, grab("max_volume:")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"streams":[
      {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"r_frame_rate":"30/1","avg_frame_rate":"30000/1001","pix_fmt":"yuv420p","color_space":"bt709","color_range":"tv","duration":"6.000000","nb_frames":"180","side_data_list":[{"side_data_type":"Display Matrix","rotation":-90}]},
      {"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":1,"r_frame_rate":"0/0","avg_frame_rate":"0/0"}],
      "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"6.010000","size":"123456"}}"#;

    #[test]
    fn ffprobe_json_parses() {
        let p = parse_probe(SAMPLE).unwrap();
        assert_eq!(p.streams.len(), 2);
        let v = p.video().unwrap();
        assert_eq!(
            (v.width, v.height, v.codec_name.as_str()),
            (1920, 1080, "h264")
        );
        assert_eq!(v.r_frame_rate, Some(30.0));
        assert!((v.avg_frame_rate.unwrap() - 29.97).abs() < 0.01);
        assert_eq!(v.rotation, -90);
        assert_eq!(v.nb_frames, Some(180));
        assert_eq!(v.color_space, "bt709");
        let a = p.audio().unwrap();
        assert_eq!(
            (a.sample_rate, a.channels, a.r_frame_rate),
            (Some(48000), Some(1), None)
        );
        assert!((p.duration - 6.01).abs() < 1e-9 && p.size == 123456);
        assert!(parse_probe("garbage").is_err());
    }

    #[test]
    fn rates() {
        assert_eq!(parse_rate("30/1"), Some(30.0));
        assert_eq!(parse_rate("0/0"), None);
        assert_eq!(parse_rate("25"), Some(25.0));
    }

    #[test]
    fn keyframes_and_gaps() {
        let csv = "0.000000,K__\n0.033333,___\n2.000000,K__\n2.033333,___\n4.000000,K_\n";
        let k = parse_keyframes(csv);
        assert_eq!(k, vec![0.0, 2.0, 4.0]);
        assert_eq!(key_gaps(&k), Some((2.0, 2.0)));
        assert_eq!(key_gaps(&[1.0]), None);
    }

    #[test]
    fn volumedetect() {
        let s = "[Parsed_volumedetect_0 @ 0x1] mean_volume: -23.4 dB\n[Parsed_volumedetect_0 @ 0x1] max_volume: -6.0 dB\n";
        assert_eq!(parse_volumedetect(s), Some((-23.4, -6.0)));
        assert_eq!(parse_volumedetect("nothing"), None);
    }
}
