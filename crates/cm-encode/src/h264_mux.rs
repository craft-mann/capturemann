//! A fragmented MP4 muxer for H.264 samples that arrive one by one (Windows: the Media Foundation
//! encoder hands over Annex B frames, not a file). It builds the init segment (`ftyp` + `moov`,
//! with the empty sample tables players require) and cuts the stream into CMAF-style fragments
//! (`styp`, `moof`, `mdat`) at sync samples about [`FRAGMENT_SECS`] apart, the layout
//! [`crate::writer::FragmentedMp4Writer`] expects and [`crate::fmp4`] scans. Times are in
//! 120,000 ticks per second. Pure Rust, tested on every OS.

use cm_core::{CmError, TICKS_PER_SECOND};

use crate::h264::{self, AvccSample};
use crate::settings::FRAGMENT_SECS;

/// A fragment is closed at the first sync sample that comes once it holds at least this much
/// (1.5 s, so an encoder that adds an early sync sample does not make a one-frame fragment, while
/// a GOP of exactly [`FRAGMENT_SECS`] always cuts on its own boundary).
const CUT_AFTER_TICKS: i64 = TICKS_PER_SECOND * FRAGMENT_SECS as i64 * 3 / 4;

/// Hundred-nanosecond units (Media Foundation) from ticks, rounded to the nearest.
pub fn ticks_to_hns(ticks: i64) -> i64 {
    (i128::from(ticks) * 10_000_000 + i128::from(TICKS_PER_SECOND) / 2)
        .div_euclid(i128::from(TICKS_PER_SECOND)) as i64
}

/// Ticks from hundred-nanosecond units, rounded to the nearest. `hns_to_ticks(ticks_to_hns(t))`
/// is `t` for every `t`, because a tick (8.33 us) is far longer than the 100 ns rounding.
pub fn hns_to_ticks(hns: i64) -> i64 {
    (i128::from(hns) * i128::from(TICKS_PER_SECOND) + 5_000_000).div_euclid(10_000_000) as i64
}

fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(payload.len() + 8);
    v.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
    v.extend_from_slice(kind);
    v.extend_from_slice(payload);
    v
}

fn full(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut p = vec![version];
    p.extend_from_slice(&flags.to_be_bytes()[1..]);
    p.extend_from_slice(payload);
    boxed(kind, &p)
}

fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flatten().copied().collect()
}

const UNITY_MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

/// `ftyp` + `moov` for one H.264 video track. `sps`/`pps` come without start codes.
pub fn init_segment(width: u32, height: u32, sps: &[u8], pps: &[u8]) -> Result<Vec<u8>, CmError> {
    let track_id = 1u32;
    let ts = TICKS_PER_SECOND as u32;
    let ftyp = boxed(b"ftyp", b"iso6\0\0\0\0iso6cmfcmp41");

    let mut mvhd = vec![0u8; 8]; // creation, modification
    mvhd.extend_from_slice(&ts.to_be_bytes());
    mvhd.extend_from_slice(&0u32.to_be_bytes()); // duration: unknown, fragments carry it
    mvhd.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // rate 1.0
    mvhd.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
    mvhd.extend_from_slice(&[0u8; 10]);
    for m in UNITY_MATRIX {
        mvhd.extend_from_slice(&m.to_be_bytes());
    }
    mvhd.extend_from_slice(&[0u8; 24]);
    mvhd.extend_from_slice(&(track_id + 1).to_be_bytes());
    let mvhd = full(b"mvhd", 0, 0, &mvhd);

    let mut tkhd = vec![0u8; 8];
    tkhd.extend_from_slice(&track_id.to_be_bytes());
    tkhd.extend_from_slice(&[0u8; 4]);
    tkhd.extend_from_slice(&0u32.to_be_bytes());
    tkhd.extend_from_slice(&[0u8; 8]);
    tkhd.extend_from_slice(&[0u8; 8]); // layer, alternate group, volume, reserved
    for m in UNITY_MATRIX {
        tkhd.extend_from_slice(&m.to_be_bytes());
    }
    tkhd.extend_from_slice(&(width << 16).to_be_bytes());
    tkhd.extend_from_slice(&(height << 16).to_be_bytes());
    let tkhd = full(b"tkhd", 0, 3, &tkhd); // enabled, in movie

    let mut mdhd = vec![0u8; 8];
    mdhd.extend_from_slice(&ts.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&[0x55, 0xC4, 0, 0]); // language "und"
    let mdhd = full(b"mdhd", 0, 0, &mdhd);

    let mut hdlr = vec![0u8; 4];
    hdlr.extend_from_slice(b"vide");
    hdlr.extend_from_slice(&[0u8; 12]);
    hdlr.extend_from_slice(b"VideoHandler\0");
    let hdlr = full(b"hdlr", 0, 0, &hdlr);

    let vmhd = full(b"vmhd", 0, 1, &[0u8; 8]);
    let dref = full(b"dref", 0, 0, &{
        let mut d = 1u32.to_be_bytes().to_vec();
        d.extend_from_slice(&full(b"url ", 0, 1, &[]));
        d
    });
    let dinf = boxed(b"dinf", &dref);

    // Sample entry: BT.709, limited range, so players decode the colours the encoder was told.
    let colr = boxed(
        b"colr",
        &[b"nclx".as_slice(), &[0, 1, 0, 1, 0, 1, 0]].concat(),
    );
    let mut entry = vec![0u8; 6];
    entry.extend_from_slice(&1u16.to_be_bytes()); // data reference index
    entry.extend_from_slice(&[0u8; 16]);
    entry.extend_from_slice(&(width as u16).to_be_bytes());
    entry.extend_from_slice(&(height as u16).to_be_bytes());
    entry.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // 72 dpi
    entry.extend_from_slice(&0x0048_0000u32.to_be_bytes());
    entry.extend_from_slice(&[0u8; 4]);
    entry.extend_from_slice(&1u16.to_be_bytes()); // frame count
    entry.extend_from_slice(&[0u8; 32]); // compressor name
    entry.extend_from_slice(&0x0018u16.to_be_bytes()); // depth
    entry.extend_from_slice(&0xFFFFu16.to_be_bytes()); // pre-defined
    entry.extend_from_slice(&h264::avcc_box(sps, pps)?);
    entry.extend_from_slice(&colr);
    let entry = boxed(b"avc1", &entry);
    let mut stsd = 1u32.to_be_bytes().to_vec();
    stsd.extend_from_slice(&entry);
    let empty_table = |kind: &[u8; 4]| full(kind, 0, 0, &0u32.to_be_bytes());
    let stbl = boxed(
        b"stbl",
        &concat(&[
            full(b"stsd", 0, 0, &stsd),
            empty_table(b"stts"),
            empty_table(b"stsc"),
            full(b"stsz", 0, 0, &[0u8; 8]),
            empty_table(b"stco"),
        ]),
    );
    let minf = boxed(b"minf", &concat(&[vmhd, dinf, stbl]));
    let mdia = boxed(b"mdia", &concat(&[mdhd, hdlr, minf]));
    let trak = boxed(b"trak", &concat(&[tkhd, mdia]));

    let mut trex = track_id.to_be_bytes().to_vec();
    trex.extend_from_slice(&1u32.to_be_bytes()); // default sample description index
    trex.extend_from_slice(&[0u8; 12]);
    let mvex = boxed(b"mvex", &full(b"trex", 0, 0, &trex));
    let moov = boxed(b"moov", &concat(&[mvhd, trak, mvex]));
    Ok(concat(&[ftyp, moov]))
}

/// One frame for the muxer.
#[derive(Debug, Clone)]
pub struct MuxSample {
    /// File time of the frame, in ticks.
    pub pts: i64,
    pub data: Vec<u8>,
    pub sync: bool,
}

impl From<(i64, AvccSample)> for MuxSample {
    fn from((pts, s): (i64, AvccSample)) -> Self {
        MuxSample {
            pts,
            data: s.data,
            sync: s.sync,
        }
    }
}

/// Collects samples and returns finished fragments.
pub struct FragmentCutter {
    track_id: u32,
    sequence: u32,
    pending: Vec<MuxSample>,
    /// Duration given to the last sample of the stream, which has no successor.
    last_duration: i64,
}

impl FragmentCutter {
    /// `frame_ticks` is the nominal frame duration (the last sample's duration).
    pub fn new(frame_ticks: i64) -> Self {
        FragmentCutter {
            track_id: 1,
            sequence: 0,
            pending: Vec::new(),
            last_duration: frame_ticks.max(1),
        }
    }

    /// Adds a sample (in increasing `pts` order). Returns the fragment that this sample closed,
    /// if it is a sync sample and the open fragment is long enough.
    pub fn push(&mut self, s: MuxSample) -> Option<Vec<u8>> {
        let mut done = None;
        if s.sync {
            if let Some(first) = self.pending.first() {
                if s.pts - first.pts >= CUT_AFTER_TICKS {
                    done = Some(self.cut(s.pts));
                }
            }
        }
        // A fragment must start with a sync sample: anything before the first one is dropped.
        if self.pending.is_empty() && !s.sync {
            return done;
        }
        self.pending.push(s);
        done
    }

    /// The last fragment, with the last sample lasting one nominal frame.
    pub fn finish(&mut self) -> Option<Vec<u8>> {
        let end = self.pending.last()?.pts + self.last_duration;
        Some(self.cut(end))
    }

    /// Frames held in the open fragment.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    fn cut(&mut self, next_pts: i64) -> Vec<u8> {
        let samples = std::mem::take(&mut self.pending);
        self.sequence += 1;
        build_fragment(self.track_id, self.sequence, &samples, next_pts)
    }
}

/// `styp` + `moof` + `mdat` for `samples`; sample `i` lasts until sample `i + 1` starts and the
/// last one until `end_pts`.
pub fn build_fragment(
    track_id: u32,
    sequence: u32,
    samples: &[MuxSample],
    end_pts: i64,
) -> Vec<u8> {
    let styp = boxed(b"styp", b"msdh\0\0\0\0msdhmsix");
    let mfhd = full(b"mfhd", 0, 0, &sequence.to_be_bytes());
    let tfhd = full(b"tfhd", 0, 0x02_0000, &track_id.to_be_bytes()); // default-base-is-moof
    let base = samples.first().map_or(0, |s| s.pts).max(0) as u64;
    let tfdt = full(b"tfdt", 1, 0, &base.to_be_bytes());
    let mut trun = (samples.len() as u32).to_be_bytes().to_vec();
    trun.extend_from_slice(&0i32.to_be_bytes()); // data offset, patched below
    for (i, s) in samples.iter().enumerate() {
        let next = samples.get(i + 1).map_or(end_pts, |n| n.pts);
        let dur = (next - s.pts).clamp(1, i64::from(u32::MAX)) as u32;
        trun.extend_from_slice(&dur.to_be_bytes());
        trun.extend_from_slice(&(s.data.len() as u32).to_be_bytes());
        let flags: u32 = if s.sync { 0x0200_0000 } else { 0x0101_0000 };
        trun.extend_from_slice(&flags.to_be_bytes());
    }
    // data-offset, sample-duration, sample-size, sample-flags present.
    let trun = full(b"trun", 0, 0x701, &trun);
    let traf = boxed(b"traf", &concat(&[tfhd, tfdt, trun]));
    let mut moof = boxed(b"moof", &concat(&[mfhd, traf]));
    let off = (moof.len() + 8) as i32;
    let pos = moof.len() - samples.len() * 12 - 4;
    moof[pos..pos + 4].copy_from_slice(&off.to_be_bytes());
    let payload: Vec<u8> = samples
        .iter()
        .flat_map(|s| s.data.iter().copied())
        .collect();
    concat(&[styp, moof, boxed(b"mdat", &payload)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmp4::scan_bytes;

    const SPS: [u8; 5] = [0x67, 100, 0, 40, 0xAA];
    const PPS: [u8; 3] = [0x68, 0xEE, 0x3C];

    fn frame(pts: i64, sync: bool) -> MuxSample {
        MuxSample {
            pts,
            data: vec![if sync { 0x65 } else { 0x41 }; 10],
            sync,
        }
    }

    /// A file of `n` frames at `fps` with a sync sample every `gop` frames.
    fn file(n: i64, fps: i64, gop: i64) -> (Vec<u8>, usize) {
        let step = TICKS_PER_SECOND / fps;
        let mut bytes = init_segment(1920, 1080, &SPS, &PPS).unwrap();
        let mut cut = FragmentCutter::new(step);
        let mut fragments = 0;
        for i in 0..n {
            if let Some(f) = cut.push(frame(i * step, i % gop == 0)) {
                bytes.extend(f);
                fragments += 1;
            }
        }
        if let Some(f) = cut.finish() {
            bytes.extend(f);
            fragments += 1;
        }
        (bytes, fragments)
    }

    #[test]
    fn hns_and_ticks_round_trip_for_every_tick() {
        for t in (0..4_000_000i64).chain([123_456_789_012, 3_600 * 120_000 * 24]) {
            assert_eq!(hns_to_ticks(ticks_to_hns(t)), t, "{t}");
        }
        assert_eq!(ticks_to_hns(120_000), 10_000_000);
        assert_eq!(hns_to_ticks(10_000_000), 120_000);
        assert_eq!(ticks_to_hns(4_000), 333_333); // 33.33 ms, one frame at 30 fps
    }

    #[test]
    fn init_segment_is_scanned_as_a_real_avc1_track() {
        let init = init_segment(1920, 1080, &SPS, &PPS).unwrap();
        let scan = scan_bytes(&init).unwrap();
        let i = scan.init.unwrap();
        assert!(i.moov_first);
        assert_eq!((i.width, i.height), (1920, 1080));
        assert_eq!(i.codec, "avc1");
        assert_eq!(i.timescale, 120_000);
        assert_eq!(i.init_len as usize, init.len());
    }

    #[test]
    fn init_segment_has_the_tables_players_need_and_the_avcc() {
        let init = init_segment(1280, 720, &SPS, &PPS).unwrap();
        let has = |k: &[u8; 4]| init.windows(4).any(|w| w == k);
        for k in [
            b"vmhd", b"dinf", b"dref", b"stsd", b"stts", b"stsc", b"stsz", b"stco", b"avcC",
            b"colr", b"mvex", b"trex",
        ] {
            assert!(has(k), "missing {}", String::from_utf8_lossy(k));
        }
        assert!(init_segment(1280, 720, &[1, 2], &PPS).is_err());
    }

    #[test]
    fn sixty_frame_gops_make_two_second_fragments_with_exact_times() {
        // 12 s at 30 fps, a sync sample every 60 frames.
        let (bytes, n) = file(360, 30, 60);
        assert_eq!(n, 6);
        let scan = scan_bytes(&bytes).unwrap();
        assert!(scan.is_clean());
        assert_eq!(scan.fragments.len(), 6);
        assert_eq!(scan.frame_count(), 360);
        assert!(scan.fragments.iter().all(|f| f.starts_with_sync));
        for (i, f) in scan.fragments.iter().enumerate() {
            assert_eq!(f.base_time, i as u64 * 240_000);
            assert_eq!(f.duration, 240_000);
            assert_eq!(f.samples, 60);
        }
        assert_eq!(scan.duration().unwrap().ticks(), 12 * 120_000);
    }

    #[test]
    fn the_last_partial_fragment_is_flushed_with_a_nominal_last_frame() {
        let (bytes, n) = file(100, 30, 60); // 3.33 s
        assert_eq!(n, 2);
        let scan = scan_bytes(&bytes).unwrap();
        assert_eq!(scan.frame_count(), 100);
        assert_eq!(scan.duration().unwrap().ticks(), 100 * 4_000);
    }

    #[test]
    fn an_early_sync_sample_does_not_cut_a_tiny_fragment() {
        let step = 4_000;
        let mut cut = FragmentCutter::new(step);
        let mut out = Vec::new();
        for i in 0..20 {
            // A sync sample every 10 frames (0.33 s): no cut before 1.5 s.
            out.extend(cut.push(frame(i * step, i % 10 == 0)));
        }
        assert!(out.is_empty());
        assert_eq!(cut.pending(), 20);
        // A long GOP: still cuts at the first sync sample after 1.5 s.
        for i in 20..60 {
            out.extend(cut.push(frame(i * step, i == 50)));
        }
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn frames_before_the_first_sync_sample_are_dropped() {
        let mut cut = FragmentCutter::new(4_000);
        assert!(cut.push(frame(0, false)).is_none());
        assert_eq!(cut.pending(), 0);
        cut.push(frame(4_000, true));
        assert_eq!(cut.pending(), 1);
        assert!(cut.finish().is_some());
        assert!(cut.finish().is_none());
    }

    #[test]
    fn sample_durations_follow_the_gaps_in_pts() {
        // A held frame: 3 slots long between the second and third sample.
        let samples = vec![frame(0, true), frame(4_000, false), frame(16_000, false)];
        let f = build_fragment(1, 1, &samples, 20_000);
        let mut bytes = init_segment(640, 360, &SPS, &PPS).unwrap();
        bytes.extend(f);
        let scan = scan_bytes(&bytes).unwrap();
        assert_eq!(scan.fragments[0].duration, 20_000);
        assert_eq!(scan.fragments[0].samples, 3);
    }

    #[test]
    fn mdat_holds_the_sample_bytes_in_order() {
        let samples = vec![
            MuxSample {
                pts: 0,
                data: vec![1, 2, 3],
                sync: true,
            },
            MuxSample {
                pts: 4_000,
                data: vec![4, 5],
                sync: false,
            },
        ];
        let f = build_fragment(1, 1, &samples, 8_000);
        assert_eq!(&f[f.len() - 5..], &[1, 2, 3, 4, 5]);
        // The trun data offset points at the first mdat payload byte.
        let moof_at = f.windows(4).position(|w| w == b"moof").unwrap() - 4;
        let trun_at = f.windows(4).position(|w| w == b"trun").unwrap();
        let off = i32::from_be_bytes(f[trun_at + 12..trun_at + 16].try_into().unwrap());
        assert_eq!(moof_at + off as usize, f.len() - 5);
    }
}
