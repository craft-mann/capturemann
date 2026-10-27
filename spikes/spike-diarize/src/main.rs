//! Spike S7: offline speaker diarization with sherpa-onnx (pyannote segmentation 3.0 + CAM++ + fast clustering).
//!
//! diarize  --wav in.wav --segmentation model.onnx --embedding cam.onnx [--num-clusters N] [--threshold T]
//!          [--threads N] [--window-shift R] [--min-on S] [--min-off S] [--out turns.json]
//! timeline --turns turns.json [--wav in.wav] [--width 100]   (text timeline to check by eye)
//! split    --wav in.wav --turns turns.json --out-dir DIR    (one WAV per speaker, same length as the
//!          input, other speakers muted, so time positions match the original)
use anyhow::{anyhow, bail, Context, Result};
use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractorConfig, Wave,
};
use std::collections::BTreeMap;
use std::time::Instant;

struct Args(Vec<String>);
impl Args {
    fn get(&self, k: &str) -> Option<String> {
        self.0.iter().position(|a| a == k).and_then(|i| self.0.get(i + 1).cloned())
    }
    fn req(&self, k: &str) -> Result<String> {
        self.get(k).ok_or_else(|| anyhow!("missing {k}"))
    }
    fn num<T: std::str::FromStr>(&self, k: &str, d: T) -> Result<T> {
        match self.get(k) {
            Some(v) => v.parse().map_err(|_| anyhow!("bad value for {k}: {v}")),
            None => Ok(d),
        }
    }
}

#[derive(Clone, Debug)]
struct Turn {
    start: f64,
    end: f64,
    speaker: i64,
}

fn read_turns(path: &str) -> Result<Vec<Turn>> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let arr = v["turns"].as_array().context("no turns array")?;
    Ok(arr
        .iter()
        .map(|t| Turn {
            start: t["start"].as_f64().unwrap_or(0.0),
            end: t["end"].as_f64().unwrap_or(0.0),
            speaker: t["speaker"].as_i64().unwrap_or(0),
        })
        .collect())
}

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first().cloned() else {
        bail!("usage: spike-diarize diarize|timeline|split ... (see source header)");
    };
    let a = Args(argv[1..].to_vec());
    match cmd.as_str() {
        "diarize" => diarize(&a),
        "timeline" => timeline(&a),
        "split" => split(&a),
        _ => bail!("unknown command {cmd}"),
    }
}

fn diarize(a: &Args) -> Result<()> {
    let wav = a.req("--wav")?;
    let threads: i32 = a.num("--threads", 4)?;
    let num_clusters: i32 = a.num("--num-clusters", -1)?;
    let threshold: f32 = a.num("--threshold", 0.5)?;
    let config = OfflineSpeakerDiarizationConfig {
        segmentation: OfflineSpeakerSegmentationModelConfig {
            pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                model: Some(a.req("--segmentation")?),
                window_shift_ratio: a.num("--window-shift", 0.1)?,
            },
            num_threads: threads,
            ..Default::default()
        },
        embedding: SpeakerEmbeddingExtractorConfig {
            model: Some(a.req("--embedding")?),
            num_threads: threads,
            ..Default::default()
        },
        clustering: FastClusteringConfig { num_clusters, threshold, compute_confidence: false },
        min_duration_on: a.num("--min-on", 0.3)?,
        min_duration_off: a.num("--min-off", 0.5)?,
    };
    let t0 = Instant::now();
    let sd = OfflineSpeakerDiarization::create(&config).ok_or_else(|| anyhow!("create failed (model paths?)"))?;
    let load_s = t0.elapsed().as_secs_f64();
    let w = Wave::read(&wav).ok_or_else(|| anyhow!("cannot read {wav} (needs mono 16-bit WAV)"))?;
    if w.sample_rate() != sd.sample_rate() {
        bail!("WAV is {} Hz, model needs {} Hz", w.sample_rate(), sd.sample_rate());
    }
    let dur = w.num_samples() as f64 / w.sample_rate() as f64;
    let t1 = Instant::now();
    let r = sd.process(w.samples()).ok_or_else(|| anyhow!("process failed"))?;
    let proc_s = t1.elapsed().as_secs_f64();
    let segs = r.sort_by_start_time();
    let turns: Vec<_> = segs
        .iter()
        .map(|s| serde_json::json!({"start": round3(s.start as f64), "end": round3(s.end as f64), "speaker": s.speaker}))
        .collect();
    let out = serde_json::json!({
        "audio": wav, "duration_s": round3(dur),
        "num_clusters_requested": num_clusters, "threshold": threshold,
        "num_speakers": r.num_speakers(), "num_turns": turns.len(),
        "load_s": round3(load_s), "process_s": round3(proc_s), "rtf": round3(proc_s / dur),
        "turns": turns,
    });
    let s = serde_json::to_string_pretty(&out)?;
    match a.get("--out") {
        Some(p) => std::fs::write(p, s)?,
        None => println!("{s}"),
    }
    eprintln!(
        "duration {dur:.1}s speakers {} turns {} load {load_s:.2}s process {proc_s:.2}s RTF {:.4}",
        r.num_speakers(),
        turns.len(),
        proc_s / dur
    );
    Ok(())
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn timeline(a: &Args) -> Result<()> {
    let turns = read_turns(&a.req("--turns")?)?;
    let width: usize = a.num("--width", 100)?;
    let end = turns.iter().map(|t| t.end).fold(0.0, f64::max);
    let mut per: BTreeMap<i64, (f64, usize)> = BTreeMap::new();
    for t in &turns {
        let e = per.entry(t.speaker).or_default();
        e.0 += t.end - t.start;
        e.1 += 1;
    }
    println!("turns: {}, last end {:.1}s", turns.len(), end);
    for (s, (d, n)) in &per {
        println!("  speaker {s}: {d:.1}s speech in {n} turns ({:.0}% of audio)", 100.0 * d / end.max(1.0));
    }
    let short = turns.iter().filter(|t| t.end - t.start < 1.0).count();
    let mut overlaps = 0.0;
    for (i, t) in turns.iter().enumerate() {
        for u in &turns[i + 1..] {
            if u.start >= t.end {
                break;
            }
            if u.speaker != t.speaker {
                overlaps += t.end.min(u.end) - u.start;
            }
        }
    }
    let flips = turns.windows(2).filter(|w| w[0].speaker != w[1].speaker && w[1].start - w[0].end < 0.5).count();
    println!("  turns under 1 s: {short}; overlap between speakers: {overlaps:.1}s; speaker changes with gap < 0.5 s: {flips}");
    // One row per speaker, one column = end/width seconds, 10-minute rows.
    let row_s = 600.0f64.min(end.max(1.0));
    let col_s = row_s / width as f64;
    let rows = (end / row_s).ceil() as usize;
    for r in 0..rows {
        println!("\n{:>6.0}s .. {:.0}s  (one column = {:.1}s; # = speaker talks in that column)", r as f64 * row_s, (r + 1) as f64 * row_s, col_s);
        for s in per.keys() {
            let mut line = String::new();
            for c in 0..width {
                let (c0, c1) = (r as f64 * row_s + c as f64 * col_s, r as f64 * row_s + (c + 1) as f64 * col_s);
                let cover: f64 = turns
                    .iter()
                    .filter(|t| t.speaker == *s)
                    .map(|t| (t.end.min(c1) - t.start.max(c0)).max(0.0))
                    .sum();
                line.push(if cover > 0.5 * col_s { '#' } else if cover > 0.0 { '+' } else { '.' });
            }
            println!("  S{s} {line}");
        }
    }
    println!("\nturn list:");
    for t in &turns {
        println!("  {:7.2} - {:7.2}  ({:5.2}s)  speaker {}", t.start, t.end, t.end - t.start, t.speaker);
    }
    Ok(())
}

fn split(a: &Args) -> Result<()> {
    let w = Wave::read(&a.req("--wav")?).ok_or_else(|| anyhow!("cannot read wav"))?;
    let turns = read_turns(&a.req("--turns")?)?;
    let dir = a.req("--out-dir")?;
    std::fs::create_dir_all(&dir)?;
    let sr = w.sample_rate();
    let samples = w.samples();
    let speakers: std::collections::BTreeSet<i64> = turns.iter().map(|t| t.speaker).collect();
    for s in speakers {
        let mut out = vec![0.0f32; samples.len()];
        for t in turns.iter().filter(|t| t.speaker == s) {
            let i0 = ((t.start * sr as f64) as usize).min(samples.len());
            let i1 = ((t.end * sr as f64) as usize).min(samples.len());
            out[i0..i1].copy_from_slice(&samples[i0..i1]);
        }
        let p = format!("{dir}/speaker_{s}.wav");
        if !sherpa_onnx::write(&p, &out, sr) {
            bail!("write {p} failed");
        }
        println!("wrote {p}");
    }
    Ok(())
}
