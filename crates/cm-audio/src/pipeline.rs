//! The timestamp-driven track writer shared by the mic and system-audio recorders.
//!
//! Input is a sequence of [`AudioChunk`]s stamped with the OS capture time of their first frame.
//! The pipeline aligns by those stamps, never by sample count (AUD-4, NFR-3):
//!
//! * The first accepted frame defines the track start (`TrackInfo::start`). Frames stamped before
//!   the session origin are dropped.
//! * Every chunk is compared with where the sample count says it should be. If it is more than
//!   `resync_threshold` late, silence is inserted; if more than that early (overlap), the excess
//!   leading frames are trimmed. This bounds the error against the shared clock to the threshold
//!   (20 ms by default, well inside the 50 ms budget) whatever the device clock does, and the
//!   slope of the raw offsets is reported as `drift_ppm`.
//! * Pauses are gates on the stamp axis: frames inside a pause are dropped and the file continues
//!   without a hole (REC-4: "continuous timestamps, no gap in the output"); the pause is recorded
//!   in the track details so the mixer can map file time back to session time.
//! * Real dropouts (device lost, queue overrun) are silence-filled and recorded as gaps.
//!
//! After alignment the audio is mapped to the output channel count, resampled to 48 kHz, written to
//! the raw WAV and (optionally) denoised into a second WAV of identical length.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use cm_core::{CmError, MediaTime, SessionClock, TrackInfo, TrackKind};
use serde_json::json;

use crate::denoise::Denoiser;
use crate::resample::StreamResampler;
use crate::source::{AudioChunk, Counters, SourceFormat};
use crate::wav::WavWriter;

/// Track sample rate (technical design, "Recording pipeline").
pub const TRACK_RATE: u32 = 48_000;
/// Default resync tolerance against the OS timestamps.
pub const DEFAULT_RESYNC_MS: f64 = 20.0;
/// Silence of at least this length is reported as a gap in the track details.
pub const GAP_REPORT_MS: f64 = 100.0;
/// A microphone that delivers nothing but exact digital silence for this long is muted or has
/// lost its access (the OS keeps the stream open and feeds zeros): the recorder warns, once per
/// episode, and keeps recording (R3-21).
pub const DIGITAL_SILENCE_ALERT_MS: f64 = 15_000.0;

const NS: f64 = 1e9;

/// Static configuration of a pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub dir: PathBuf,
    pub clock: SessionClock,
    pub kind: TrackKind,
    pub raw_file: String,
    /// `Some` writes a denoised copy with this name.
    pub denoised_file: Option<String>,
    pub out_channels: u16,
    pub resync_ms: f64,
    /// Added to every capture stamp (calibration of a device whose stamps are systematically early
    /// or late; positive moves audio later on the session timeline). Default 0.
    pub stamp_offset_ms: f64,
}

#[derive(Debug, Clone)]
struct Gate {
    from: i128,
    to: Option<i128>,
}

#[derive(Debug, Clone)]
struct GapRecord {
    from_ns: i128,
    to_ns: i128,
    reason: &'static str,
}

#[derive(Debug, Clone)]
struct DeviceChange {
    at_ns: i128,
    from: String,
    to: String,
}

/// Least-squares slope of the raw offset (host elapsed minus sample-count elapsed) over time.
#[derive(Debug, Default, Clone)]
struct DriftEstimator {
    n: f64,
    sx: f64,
    sy: f64,
    sxx: f64,
    sxy: f64,
    x_min: f64,
    x_max: f64,
}

impl DriftEstimator {
    fn add(&mut self, x_s: f64, y_ns: f64) {
        if self.n == 0.0 {
            self.x_min = x_s;
        }
        self.x_max = x_s;
        self.n += 1.0;
        self.sx += x_s;
        self.sy += y_ns;
        self.sxx += x_s * x_s;
        self.sxy += x_s * y_ns;
    }

    /// (span in seconds, drift in ppm). Positive means the device delivers fewer samples than the
    /// host clock says it should (device clock slow).
    fn result(&self) -> Option<(f64, f64)> {
        let span = self.x_max - self.x_min;
        if self.n < 3.0 || span < 1.0 {
            return None;
        }
        let denom = self.n * self.sxx - self.sx * self.sx;
        if denom.abs() < 1e-12 {
            return None;
        }
        let slope_ns_per_s = (self.n * self.sxy - self.sx * self.sy) / denom;
        Some((span, slope_ns_per_s / 1e9 * 1e6))
    }
}

/// See the module docs.
pub struct Pipeline {
    cfg: PipelineConfig,
    counters: Arc<Counters>,
    in_rate: u32,
    in_channels: u16,
    out_channels: u16,
    resampler: StreamResampler,
    denoiser: Option<Denoiser>,
    raw: Option<WavWriter>,
    den: Option<WavWriter>,
    origin_ns: i128,
    ts_shift_ns: i128,
    ts_checked: bool,
    gates: Vec<Gate>,
    stop_ns: Option<i128>,
    pending_reset: bool,
    ref_ns: Option<i128>,
    ref_frames: u64,
    first_ns: Option<i128>,
    gaps: Vec<GapRecord>,
    pending_gap_reason: Option<&'static str>,
    device_changes: Vec<DeviceChange>,
    device_name: String,
    device_id: String,
    first_device_rate: u32,
    first_device_channels: u16,
    resync_count: u64,
    inserted_frames: u64,
    trimmed_frames: u64,
    max_offset_ns: i128,
    epoch_start_ns: i128,
    epoch_received: u64,
    drift: DriftEstimator,
    best_drift: Option<(f64, f64)>,
    error: Option<CmError>,
    tmp: Vec<f32>,
    tmp2: Vec<f32>,
    hint: Option<Arc<Mutex<Option<serde_json::Value>>>>,
    /// Input frames in a row that were exactly 0.0 (microphone only).
    zero_run_frames: u64,
    silence_alerted: bool,
    silence_pending: bool,
}

fn map_io(e: std::io::Error, what: &str) -> CmError {
    if e.kind() == std::io::ErrorKind::StorageFull {
        CmError::disk_full(format!("{what}: disk full"))
    } else {
        CmError::internal(format!("{what}: {e}"))
    }
}

/// Removes the frame range `[a, b)` from a list of disjoint, ordered kept ranges.
fn subtract(keep: &[(usize, usize)], a: usize, b: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(keep.len() + 1);
    for &(s, e) in keep {
        if b <= s || a >= e {
            out.push((s, e));
            continue;
        }
        if a > s {
            out.push((s, a));
        }
        if b < e {
            out.push((b, e));
        }
    }
    out
}

/// Maps interleaved `data` from `from` channels to `to` channels.
///
/// Devices with more than two channels (audio interfaces, aggregate devices): only channel 1
/// (mono output) or channels 1 and 2 (stereo output) are recorded and the others are ignored
/// (R3-22). Averaging them in would halve the level of the common case of one microphone on
/// input 1 of a multichannel interface.
pub fn map_channels(data: &[f32], from: u16, to: u16) -> Vec<f32> {
    let (from, to) = (usize::from(from.max(1)), usize::from(to.max(1)));
    if from == to {
        return data.to_vec();
    }
    let frames = data.len() / from;
    let mut out = Vec::with_capacity(frames * to);
    for f in data.chunks_exact(from) {
        match (from, to) {
            (1, _) => out.extend(std::iter::repeat_n(f[0], to)),
            (2, 1) => out.push((f[0] + f[1]) * 0.5),
            (_, 1) => out.push(f[0]),
            _ => (0..to).for_each(|c| out.push(f[c.min(from - 1)])),
        }
    }
    out
}

impl Pipeline {
    pub fn new(
        cfg: PipelineConfig,
        input: &SourceFormat,
        counters: Arc<Counters>,
    ) -> Result<Self, CmError> {
        let out_channels = cfg.out_channels.clamp(1, 2);
        let raw = WavWriter::create(&cfg.dir.join(&cfg.raw_file), out_channels, TRACK_RATE)
            .map_err(|e| map_io(e, &cfg.raw_file))?;
        let (den, denoiser) = match &cfg.denoised_file {
            Some(name) => (
                Some(
                    WavWriter::create(&cfg.dir.join(name), out_channels, TRACK_RATE)
                        .map_err(|e| map_io(e, name))?,
                ),
                Some(Denoiser::new(out_channels)),
            ),
            None => (None, None),
        };
        let origin_ns = i128::from(cfg.clock.origin_ns());
        Ok(Pipeline {
            resampler: StreamResampler::new(input.sample_rate, TRACK_RATE, out_channels)?,
            in_rate: input.sample_rate,
            in_channels: input.channels,
            out_channels,
            denoiser,
            raw: Some(raw),
            den,
            origin_ns,
            ts_shift_ns: 0,
            ts_checked: false,
            gates: Vec::new(),
            stop_ns: None,
            pending_reset: false,
            ref_ns: None,
            ref_frames: 0,
            first_ns: None,
            gaps: Vec::new(),
            pending_gap_reason: None,
            device_changes: Vec::new(),
            device_name: input.device_name.clone(),
            device_id: input.device_id.clone(),
            first_device_rate: input.sample_rate,
            first_device_channels: input.channels,
            resync_count: 0,
            inserted_frames: 0,
            trimmed_frames: 0,
            max_offset_ns: 0,
            epoch_start_ns: 0,
            epoch_received: 0,
            drift: DriftEstimator::default(),
            best_drift: None,
            error: None,
            tmp: Vec::new(),
            tmp2: Vec::new(),
            hint: None,
            zero_run_frames: 0,
            silence_alerted: false,
            silence_pending: false,
            counters,
            cfg,
        })
    }

    /// Where the track's recovery facts go as soon as its first sample is placed (R3-2): the
    /// session copies them into `recording.lock` so a crash keeps the audio's start offset.
    pub fn set_hint_cell(&mut self, cell: Arc<Mutex<Option<serde_json::Value>>>) {
        self.hint = Some(cell);
    }

    fn publish_hint(&self, first_ns: i128) {
        let Some(cell) = &self.hint else {
            return;
        };
        let main_file = self
            .cfg
            .denoised_file
            .clone()
            .unwrap_or_else(|| self.cfg.raw_file.clone());
        let v = json!({
            "start": self.media_time(first_ns).ticks(),
            "file": main_file,
            "raw_file": self.cfg.raw_file,
            "denoised_file": self.cfg.denoised_file,
            "channels": self.out_channels,
            "rate": TRACK_RATE,
            "sample_format": "f32",
        });
        *cell.lock().unwrap_or_else(|p| p.into_inner()) = Some(v);
    }

    /// Test hook: the raw file fails (disk full) after `bytes` of audio.
    #[doc(hidden)]
    pub fn inject_write_fault_after(&mut self, bytes: u64) {
        if let Some(w) = self.raw.take() {
            self.raw = Some(w.with_fault_after(bytes));
        }
    }

    pub fn out_channels(&self) -> u16 {
        self.out_channels
    }

    /// Frames written to the raw file so far.
    pub fn frames_written(&self) -> u64 {
        self.raw.as_ref().map_or(0, WavWriter::frames)
    }

    pub fn first_error(&self) -> Option<&CmError> {
        self.error.as_ref()
    }

    fn expected_ns(&self) -> Option<i128> {
        self.ref_ns.map(|r| {
            r + i128::from(self.ref_frames) * 1_000_000_000 / i128::from(self.in_rate.max(1))
        })
    }

    fn frames_to_ns(&self, frames: u64) -> i128 {
        i128::from(frames) * 1_000_000_000 / i128::from(self.in_rate.max(1))
    }

    fn ns_to_frames(&self, ns: i128) -> i64 {
        // Rounded to nearest.
        let num =
            ns.clamp(-1_000_000_000_000_000, 1_000_000_000_000_000) * i128::from(self.in_rate);
        i64::try_from((num + 500_000_000).div_euclid(1_000_000_000)).unwrap_or(i64::MAX)
    }

    fn host_ns_of(&self, t: MediaTime) -> i128 {
        i128::from(self.cfg.clock.to_host_ns(t))
    }

    fn media_time(&self, host_ns: i128) -> MediaTime {
        self.cfg
            .clock
            .from_host_ns(u64::try_from(host_ns.max(0)).unwrap_or(u64::MAX))
    }

    /// Stops writing from `at` (host time via the clock).
    pub fn pause(&mut self, at: MediaTime) {
        let at = self.host_ns_of(at);
        if self.gates.last().is_some_and(|g| g.to.is_none()) {
            return;
        }
        self.gates.push(Gate { from: at, to: None });
    }

    /// Resumes writing from `at`.
    pub fn resume(&mut self, at: MediaTime) {
        let at = self.host_ns_of(at);
        if let Some(g) = self.gates.last_mut() {
            if g.to.is_none() {
                g.to = Some(at.max(g.from));
            }
        }
    }

    /// Ignores frames stamped at or after `at`.
    pub fn set_stop(&mut self, at: MediaTime) {
        self.stop_ns = Some(self.host_ns_of(at));
    }

    /// Switches to a new input device (default-device fallback). The silence between the last
    /// frame of the old device and the first of the new one is filled by the timestamp logic and
    /// reported as a gap.
    pub fn set_input(&mut self, input: &SourceFormat) -> Result<(), CmError> {
        self.tmp.clear();
        let mut tail = std::mem::take(&mut self.tmp);
        self.resampler.finish(&mut tail)?;
        self.write_out(&tail);
        tail.clear();
        self.tmp = tail;
        self.resampler = StreamResampler::new(input.sample_rate, TRACK_RATE, self.out_channels)?;
        if let Some(exp) = self.expected_ns() {
            self.ref_ns = Some(exp);
            self.ref_frames = 0;
        }
        self.device_changes.push(DeviceChange {
            at_ns: self.expected_ns().unwrap_or(self.origin_ns),
            from: self.device_name.clone(),
            to: input.device_name.clone(),
        });
        self.in_rate = input.sample_rate;
        self.in_channels = input.channels;
        self.device_name = input.device_name.clone();
        self.device_id = input.device_id.clone();
        self.pending_gap_reason = Some("device_changed");
        self.new_drift_epoch();
        Ok(())
    }

    fn new_drift_epoch(&mut self) {
        if let Some((span, ppm)) = self.drift.result() {
            if self.best_drift.is_none_or(|(s, _)| span > s) {
                self.best_drift = Some((span, ppm));
            }
        }
        self.drift = DriftEstimator::default();
        self.epoch_received = 0;
        self.epoch_start_ns = self.expected_ns().unwrap_or(0);
    }

    /// Processes one chunk from the source.
    pub fn push_chunk(&mut self, chunk: AudioChunk) {
        if self.error.is_some() {
            return;
        }
        let n = chunk.frames();
        if n == 0 {
            return;
        }
        let mut t0 =
            i128::from(chunk.host_ns) + self.ts_shift_ns + (self.cfg.stamp_offset_ms * 1e6) as i128;
        if !self.ts_checked {
            self.ts_checked = true;
            // Sanity check the timebase once: a capture stamp seconds away from "now" means the
            // source clock is not the session clock; fall back to arrival time minus duration.
            let now = i128::from(self.cfg.clock.to_host_ns(self.cfg.clock.now()));
            let end = t0 + self.frames_to_ns(n as u64);
            let skew = now - end;
            if skew.abs() > 5_000_000_000 {
                self.ts_shift_ns = skew;
                t0 += skew;
            }
        }
        self.watch_digital_silence(&chunk.data, n);
        let data = map_channels(&chunk.data, chunk.channels, self.out_channels);
        let ch = usize::from(self.out_channels);

        // Gate the chunk on the stamp axis.
        let mut keep: Vec<(usize, usize)> = vec![(0, n)];
        let mut pause_drops: Vec<(usize, usize)> = Vec::new();
        let frame_at = |this: &Self, t: i128| -> usize {
            let rel = this.ns_to_frames(t - t0);
            usize::try_from(rel.clamp(0, n as i64)).unwrap_or(0)
        };
        let mut gates: Vec<(i128, Option<i128>, bool)> =
            vec![(i128::MIN / 4, Some(self.origin_ns), false)];
        gates.extend(self.gates.iter().map(|g| (g.from, g.to, true)));
        if let Some(s) = self.stop_ns {
            gates.push((s, None, false));
        }
        for (from, to, is_pause) in gates {
            let a = frame_at(self, from);
            let b = to.map_or(n, |t| frame_at(self, t));
            if b <= a {
                continue;
            }
            if is_pause {
                pause_drops.push((a, b));
            }
            keep = subtract(&keep, a, b);
        }

        let mut cursor = 0usize;
        let mut reset = self.pending_reset;
        for (a, b) in keep {
            let paused_before = pause_drops.iter().any(|&(ps, pe)| pe > cursor && ps < a);
            let start = t0 + i128::from(self.ns_of_frames_in(a));
            self.ingest(start, &data[a * ch..b * ch], reset || paused_before);
            reset = false;
            cursor = b;
        }
        self.pending_reset = reset || pause_drops.iter().any(|&(_, pe)| pe > cursor);
        self.counters.written.fetch_add(1, Ordering::Relaxed);
    }

    /// Tracks how long the microphone has delivered exact zeros (R3-21).
    fn watch_digital_silence(&mut self, data: &[f32], frames: usize) {
        if self.cfg.kind != TrackKind::Mic {
            return;
        }
        if data.iter().any(|v| *v != 0.0) {
            self.zero_run_frames = 0;
            self.silence_alerted = false;
            return;
        }
        self.zero_run_frames += frames as u64;
        let limit = (DIGITAL_SILENCE_ALERT_MS / 1000.0 * f64::from(self.in_rate.max(1))) as u64;
        if self.zero_run_frames >= limit && !self.silence_alerted {
            self.silence_alerted = true;
            self.silence_pending = true;
        }
    }

    /// True once per episode of digital silence (the recorder turns it into a warning).
    pub fn take_silence_alert(&mut self) -> bool {
        std::mem::take(&mut self.silence_pending)
    }

    fn ns_of_frames_in(&self, frames: usize) -> i64 {
        i64::try_from(self.frames_to_ns(frames as u64)).unwrap_or(i64::MAX)
    }

    fn ingest(&mut self, start: i128, data: &[f32], reset: bool) {
        let ch = usize::from(self.out_channels);
        let n = data.len() / ch;
        if n == 0 {
            return;
        }
        let mut data = data;
        if reset || self.ref_ns.is_none() {
            if self.first_ns.is_none() {
                self.first_ns = Some(start);
                self.publish_hint(start);
            }
            if self.ref_ns.is_some() {
                self.new_drift_epoch();
            }
            self.ref_ns = Some(start);
            self.ref_frames = 0;
            self.epoch_start_ns = start;
            self.epoch_received = 0;
        } else if let Some(expected) = self.expected_ns() {
            let offset = start - expected;
            let thr = (self.cfg.resync_ms * 1e6) as i128;
            if offset.abs() > self.max_offset_ns && offset.abs() < 1_000_000_000 {
                self.max_offset_ns = offset.abs();
            }
            // Drift sample: host elapsed in this epoch minus what the sample count says.
            let h = (start - self.epoch_start_ns) as f64;
            let s = self.frames_to_ns(self.epoch_received) as f64;
            if offset.abs() < (GAP_REPORT_MS * 1e6) as i128 {
                self.drift.add(h / NS, h - s);
            }
            if offset > thr {
                let k = self.ns_to_frames(offset).max(0) as u64;
                self.insert_silence(k);
                self.resync_count += 1;
                self.counters.late.fetch_add(1, Ordering::Relaxed);
                self.inserted_frames += k;
                if offset as f64 >= GAP_REPORT_MS * 1e6 {
                    self.gaps.push(GapRecord {
                        from_ns: expected,
                        to_ns: start,
                        reason: self.pending_gap_reason.take().unwrap_or("dropout"),
                    });
                    self.new_drift_epoch();
                }
            } else if offset < -thr {
                let k = usize::try_from(self.ns_to_frames(-offset).max(0)).unwrap_or(0);
                self.resync_count += 1;
                self.counters.late.fetch_add(1, Ordering::Relaxed);
                let k = k.min(n);
                self.trimmed_frames += k as u64;
                self.epoch_received += k as u64;
                if k == n {
                    return;
                }
                data = &data[k * ch..];
            }
        }
        let n = data.len() / ch;
        self.epoch_received += n as u64;
        self.feed(data);
    }

    fn insert_silence(&mut self, mut frames: u64) {
        let ch = usize::from(self.out_channels);
        let block = u64::from(self.in_rate.max(1));
        while frames > 0 {
            let k = frames.min(block);
            let zeros = vec![0.0f32; k as usize * ch];
            self.feed(&zeros);
            frames -= k;
        }
    }

    /// Feeds aligned input-rate audio through the resampler and writers; advances the timeline.
    fn feed(&mut self, data: &[f32]) {
        let ch = usize::from(self.out_channels);
        self.ref_frames += (data.len() / ch) as u64;
        let mut out = std::mem::take(&mut self.tmp);
        out.clear();
        if let Err(e) = self.resampler.process(data, &mut out) {
            self.error = Some(e);
        }
        self.write_out(&out);
        self.tmp = out;
    }

    fn write_out(&mut self, out: &[f32]) {
        if out.is_empty() || self.error.is_some() {
            return;
        }
        if let Some(w) = self.raw.as_mut() {
            if let Err(e) = w.write(out) {
                self.error = Some(map_io(e, &self.cfg.raw_file));
                return;
            }
        }
        if let (Some(d), Some(w)) = (self.denoiser.as_mut(), self.den.as_mut()) {
            self.tmp2.clear();
            d.process(out, &mut self.tmp2);
            if let Err(e) = w.write(&self.tmp2) {
                self.error = Some(map_io(
                    e,
                    self.cfg.denoised_file.as_deref().unwrap_or("denoised"),
                ));
            }
        }
    }

    /// Flushes resampler and denoiser, finalizes the WAV files and describes the track.
    pub fn finish(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        let mut tail = Vec::new();
        if self.error.is_none() {
            if let Err(e) = self.resampler.finish(&mut tail) {
                self.error = Some(e);
            }
        }
        self.write_out(&tail);
        if self.error.is_none() {
            if let (Some(d), Some(w)) = (self.denoiser.as_mut(), self.den.as_mut()) {
                let mut rest = Vec::new();
                d.finish(&mut rest);
                if let Err(e) = w.write(&rest) {
                    self.error = Some(map_io(e, "denoised"));
                }
            }
        }
        let mut frames = self.raw.as_ref().map_or(0, WavWriter::frames);
        // `finalize` repairs a file the disk refused to patch (it keeps what is on disk); the
        // errors only say that the track stopped growing (R3-1).
        let raw_res = self
            .raw
            .take()
            .map(WavWriter::finalize)
            .transpose()
            .map_err(|e| map_io(e, &self.cfg.raw_file));
        let den_res = self
            .den
            .take()
            .map(WavWriter::finalize)
            .transpose()
            .map_err(|e| map_io(e, "denoised"));
        let mut failure = self.error.take();
        if let Err(e) = raw_res {
            failure.get_or_insert(e);
        }
        if let Err(e) = den_res {
            failure.get_or_insert(e);
        }
        if failure.is_some() {
            // What is really on disk, not what was handed to the writer.
            if let Ok(i) = crate::wav::read_info(&self.cfg.dir.join(&self.cfg.raw_file)) {
                frames = i.frames();
            }
        }
        Ok(self.info(frames, at, failure))
    }

    fn info(&mut self, frames: u64, at: MediaTime, failure: Option<CmError>) -> TrackInfo {
        self.new_drift_epoch();
        let start = self.first_ns.map_or(at, |s| self.media_time(s));
        let mut end = self.expected_ns().map_or(at, |e| self.media_time(e));
        // A track that stopped growing (disk full, I/O error, the 4 GiB RIFF limit is handled by
        // RF64) ends where its file ends; the session keeps it and warns (R3-1).
        let truncated_at = failure.as_ref().map(|_| {
            let written_ns = i128::from(frames) * 1_000_000_000 / i128::from(TRACK_RATE);
            let t = self
                .first_ns
                .map_or(start, |f| self.media_time(f + written_ns));
            end = end.min(t);
            t.ticks()
        });
        let ticks = |ns: i128| self.media_time(ns).ticks();
        let pauses: Vec<_> = self
            .gates
            .iter()
            .filter(|g| {
                // Only pauses between the first and the last sample matter to the mixer.
                self.first_ns.is_some_and(|f| g.from > f)
            })
            .map(|g| {
                json!({
                    "from": ticks(g.from),
                    "to": g.to.map_or(end.ticks(), ticks),
                })
            })
            .collect();
        let gaps: Vec<_> = self
            .gaps
            .iter()
            .map(|g| json!({"from": ticks(g.from_ns), "to": ticks(g.to_ns), "reason": g.reason}))
            .collect();
        let changes: Vec<_> = self
            .device_changes
            .iter()
            .map(|c| json!({"at": ticks(c.at_ns), "from": c.from, "to": c.to}))
            .collect();
        let main_file = self
            .cfg
            .denoised_file
            .clone()
            .unwrap_or_else(|| self.cfg.raw_file.clone());
        let duration_ms = frames as f64 * 1000.0 / f64::from(TRACK_RATE);
        let details = json!({
            "rate": TRACK_RATE,
            "channels": self.out_channels,
            "sample_format": "f32",
            "raw_file": self.cfg.raw_file,
            "denoised_file": self.cfg.denoised_file,
            "denoise": self.cfg.denoised_file.is_some(),
            "frames": frames,
            "duration_ms": duration_ms,
            "device": {"name": self.device_name, "id": self.device_id},
            "source_rate": self.first_device_rate,
            "source_channels": self.first_device_channels,
            "first_sample_ticks": start.ticks(),
            "drift_ppm": self.best_drift.map(|(_, ppm)| ppm),
            "max_offset_ms": self.max_offset_ns as f64 / 1e6,
            "resync_threshold_ms": self.cfg.resync_ms,
            "stamp_offset_ms": self.cfg.stamp_offset_ms,
            "resyncs": {
                "count": self.resync_count,
                "inserted_ms": self.inserted_frames as f64 * 1000.0 / f64::from(self.in_rate.max(1)),
                "trimmed_ms": self.trimmed_frames as f64 * 1000.0 / f64::from(self.in_rate.max(1)),
            },
            "gaps": gaps,
            "pauses": pauses,
            "device_changes": changes,
            "timestamp_shifted": self.ts_shift_ns != 0,
            "truncated_at": truncated_at,
            "write_error": failure.as_ref().map(ToString::to_string),
        });
        TrackInfo {
            kind: self.cfg.kind,
            file: main_file,
            start,
            end,
            details,
            stats: self.counters.snapshot(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtract_splits_and_clips_ranges() {
        assert_eq!(subtract(&[(0, 10)], 3, 5), vec![(0, 3), (5, 10)]);
        assert_eq!(subtract(&[(0, 10)], 0, 4), vec![(4, 10)]);
        assert_eq!(subtract(&[(0, 10)], 8, 10), vec![(0, 8)]);
        assert_eq!(subtract(&[(0, 10)], 0, 10), Vec::<(usize, usize)>::new());
        assert_eq!(subtract(&[(0, 4), (6, 10)], 3, 7), vec![(0, 3), (7, 10)]);
        assert_eq!(subtract(&[(0, 4)], 6, 9), vec![(0, 4)]);
    }

    #[test]
    fn channel_mapping_rules() {
        assert_eq!(map_channels(&[1.0, 2.0], 1, 1), vec![1.0, 2.0]);
        assert_eq!(map_channels(&[1.0, 2.0], 1, 2), vec![1.0, 1.0, 2.0, 2.0]);
        assert_eq!(map_channels(&[1.0, 3.0, 5.0, 7.0], 2, 1), vec![2.0, 6.0]);
        assert_eq!(
            map_channels(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, 2),
            vec![1.0, 2.0, 4.0, 5.0]
        );
        assert_eq!(map_channels(&[1.0, 2.0, 3.0], 3, 1), vec![1.0]);
    }

    #[test]
    fn drift_estimator_recovers_the_slope() {
        let mut d = DriftEstimator::default();
        for i in 0..100 {
            let x = f64::from(i) * 0.5; // seconds
            d.add(x, x * 1e9 * 250e-6); // 250 ppm
        }
        let (span, ppm) = d.result().unwrap();
        assert!(span > 49.0);
        assert!((ppm - 250.0).abs() < 0.01, "{ppm}");
        assert!(DriftEstimator::default().result().is_none());
    }
}
