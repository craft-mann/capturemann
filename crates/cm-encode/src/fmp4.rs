//! A small, dependency-free parser for the fragmented MP4 files this crate writes.
//!
//! It reads box headers and the `moov` / `moof` metadata only (never the `mdat` payload), which is
//! enough to (a) verify the layout (`moov` first, one `moof` per ~2 s), (b) find the end of the last
//! complete fragment so a crashed recording can be truncated there (REC-8), and (c) build the
//! manifest durations without FFmpeg. [`synth`] builds minimal files for tests.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use cm_core::{CmError, MediaTime};

/// One complete fragment: optional `styp`/`sidx`, `moof` and its `mdat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    /// Byte offset of the first box of the fragment.
    pub offset: u64,
    /// Bytes from `offset` to the end of the `mdat`.
    pub len: u64,
    pub track_id: u32,
    /// `tfdt` base media decode time, in the track timescale.
    pub base_time: u64,
    /// Sum of the sample durations, in the track timescale.
    pub duration: u64,
    pub samples: u32,
    /// The first sample is a sync sample (a fragment can be decoded on its own).
    pub starts_with_sync: bool,
}

/// What `moov` says about the first video track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitInfo {
    /// Byte length of everything up to the end of `moov` (the init segment).
    pub init_len: u64,
    /// `moov` is the first box after `ftyp` (and not behind an `mdat`).
    pub moov_first: bool,
    pub track_id: u32,
    pub timescale: u32,
    pub width: u32,
    pub height: u32,
    /// Sample entry type, e.g. `avc1`, `hvc1`.
    pub codec: String,
    /// `trex` default sample duration, used when a `trun` has none.
    pub default_sample_duration: u32,
}

/// Result of scanning a fragmented MP4.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mp4Scan {
    pub total_len: u64,
    pub init: Option<InitInfo>,
    /// Complete fragments in file order.
    pub fragments: Vec<Fragment>,
    /// End of the last complete fragment, or of `moov` if there is none, or 0 without a
    /// complete `moov`. Truncating the file here leaves a valid file.
    pub valid_len: u64,
}

impl Mp4Scan {
    /// The file ends exactly at the last complete fragment (nothing partial after it).
    pub fn is_clean(&self) -> bool {
        self.init.is_some() && self.valid_len == self.total_len
    }

    fn to_ticks(&self, v: u64) -> Option<MediaTime> {
        let ts = u128::from(self.init.as_ref()?.timescale);
        if ts == 0 {
            return None;
        }
        let t = u128::from(v) * 120_000 / ts;
        Some(MediaTime::from_ticks(i64::try_from(t).ok()?))
    }

    /// Media time of the first sample.
    pub fn start(&self) -> Option<MediaTime> {
        self.to_ticks(self.fragments.first()?.base_time)
    }

    /// Media time of the end of the last sample.
    pub fn end(&self) -> Option<MediaTime> {
        let l = self.fragments.last()?;
        self.to_ticks(l.base_time + l.duration)
    }

    /// Playable duration: end minus start.
    pub fn duration(&self) -> Option<MediaTime> {
        Some(self.end()? - self.start()?)
    }

    pub fn frame_count(&self) -> u64 {
        self.fragments.iter().map(|f| u64::from(f.samples)).sum()
    }

    /// Every fragment is `<= max_secs` long (plus a small slack) and starts on a sync sample.
    pub fn fragment_durations_secs(&self) -> Vec<f64> {
        let ts = self.init.as_ref().map_or(0, |i| i.timescale);
        if ts == 0 {
            return Vec::new();
        }
        self.fragments
            .iter()
            .map(|f| f.duration as f64 / f64::from(ts))
            .collect()
    }
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// A child box inside an in-memory parent: `(kind, payload range start, payload range end)`.
fn children(data: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let size = be32(&data[pos..]) as usize;
        let kind = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
        let (hdr, total) = if size == 1 {
            if pos + 16 > data.len() {
                break;
            }
            (16usize, be64(&data[pos + 8..]) as usize)
        } else if size == 0 {
            (8, data.len() - pos)
        } else {
            (8, size)
        };
        if total < hdr || pos + total > data.len() {
            break;
        }
        out.push((kind, pos + hdr, pos + total));
        pos += total;
    }
    out
}

fn find<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    children(data)
        .into_iter()
        .find(|(k, _, _)| k == kind)
        .map(|(_, s, e)| &data[s..e])
}

fn parse_moov(moov: &[u8]) -> Option<InitInfo> {
    let mut trex_default = (0u32, 0u32); // (track_id, default duration)
    if let Some(mvex) = find(moov, b"mvex") {
        for (k, s, e) in children(mvex) {
            if &k == b"trex" && e - s >= 24 {
                let p = &mvex[s..e];
                trex_default = (be32(&p[4..]), be32(&p[12..]));
            }
        }
    }
    for (k, s, e) in children(moov) {
        if &k != b"trak" {
            continue;
        }
        let trak = &moov[s..e];
        let mdia = find(trak, b"mdia")?;
        let hdlr = find(mdia, b"hdlr")?;
        if hdlr.len() < 12 || &hdlr[8..12] != b"vide" {
            continue;
        }
        let tkhd = find(trak, b"tkhd")?;
        let (track_id, wh) = if tkhd.first() == Some(&1) {
            (be32(tkhd.get(20..24)?), tkhd.get(88..96)?)
        } else {
            (be32(tkhd.get(12..16)?), tkhd.get(76..84)?)
        };
        let (w, h) = (be32(wh) >> 16, be32(&wh[4..]) >> 16);
        let mdhd = find(mdia, b"mdhd")?;
        let timescale = if mdhd.first() == Some(&1) {
            be32(mdhd.get(20..24)?)
        } else {
            be32(mdhd.get(12..16)?)
        };
        let stsd = find(find(find(mdia, b"minf")?, b"stbl")?, b"stsd")?;
        let entry = stsd.get(8..16)?;
        let codec = String::from_utf8_lossy(&entry[4..8]).into_owned();
        return Some(InitInfo {
            init_len: 0,
            moov_first: false,
            track_id,
            timescale,
            width: w,
            height: h,
            codec,
            default_sample_duration: if trex_default.0 == track_id {
                trex_default.1
            } else {
                0
            },
        });
    }
    None
}

struct MoofInfo {
    track_id: u32,
    base_time: u64,
    duration: u64,
    samples: u32,
    sync: bool,
}

fn parse_moof(moof: &[u8], default_dur: u32) -> Option<MoofInfo> {
    let traf = find(moof, b"traf")?;
    let tfhd = find(traf, b"tfhd")?;
    let tf_flags = be32(tfhd) & 0x00FF_FFFF;
    let track_id = be32(tfhd.get(4..8)?);
    let mut p = 8usize;
    if tf_flags & 0x1 != 0 {
        p += 8;
    }
    if tf_flags & 0x2 != 0 {
        p += 4;
    }
    let mut dflt_dur = default_dur;
    if tf_flags & 0x8 != 0 {
        dflt_dur = be32(tfhd.get(p..p + 4)?);
        p += 4;
    }
    if tf_flags & 0x10 != 0 {
        p += 4;
    }
    let mut dflt_flags = 0u32;
    if tf_flags & 0x20 != 0 {
        dflt_flags = be32(tfhd.get(p..p + 4)?);
    }
    let tfdt = find(traf, b"tfdt")?;
    let base_time = if tfdt.first() == Some(&1) {
        be64(tfdt.get(4..12)?)
    } else {
        u64::from(be32(tfdt.get(4..8)?))
    };
    let mut duration = 0u64;
    let mut samples = 0u32;
    let mut sync = true;
    let mut first = true;
    for (k, s, e) in children(traf) {
        if &k != b"trun" {
            continue;
        }
        let t = &traf[s..e];
        let fl = be32(t) & 0x00FF_FFFF;
        let count = be32(t.get(4..8)?);
        let mut q = 8usize;
        if fl & 0x1 != 0 {
            q += 4;
        }
        let mut first_flags = None;
        if fl & 0x4 != 0 {
            first_flags = Some(be32(t.get(q..q + 4)?));
            q += 4;
        }
        for i in 0..count {
            let mut dur = dflt_dur;
            if fl & 0x100 != 0 {
                dur = be32(t.get(q..q + 4)?);
                q += 4;
            }
            if fl & 0x200 != 0 {
                q += 4;
            }
            let mut sflags = dflt_flags;
            if fl & 0x400 != 0 {
                sflags = be32(t.get(q..q + 4)?);
                q += 4;
            }
            if fl & 0x800 != 0 {
                q += 4;
            }
            if first && i == 0 {
                let f = first_flags.unwrap_or(sflags);
                // sample_is_non_sync_sample is bit 16.
                sync = f & 0x0001_0000 == 0;
                first = false;
            }
            duration += u64::from(dur);
        }
        samples += count;
    }
    Some(MoofInfo {
        track_id,
        base_time,
        duration,
        samples,
        sync,
    })
}

/// Scans a fragmented MP4 from any seekable reader of `len` bytes.
///
/// Never fails on a damaged or truncated file: it reports what is complete (see
/// [`Mp4Scan::valid_len`]). I/O errors are returned.
pub fn scan<R: Read + Seek>(r: &mut R, len: u64) -> Result<Mp4Scan, CmError> {
    let io = |e: std::io::Error| CmError::internal(format!("reading MP4: {e}"));
    let mut out = Mp4Scan {
        total_len: len,
        ..Default::default()
    };
    let mut pos = 0u64;
    let mut first_after_ftyp = true;
    let mut pending: Option<u64> = None; // start of styp/sidx before a moof
    let mut moof: Option<MoofInfo> = None;
    let mut moof_start = 0u64;
    while pos + 8 <= len {
        r.seek(SeekFrom::Start(pos)).map_err(io)?;
        let mut h = [0u8; 16];
        let want = usize::try_from((len - pos).min(16)).unwrap_or(16);
        r.read_exact(&mut h[..want]).map_err(io)?;
        let size = u64::from(be32(&h));
        let kind = [h[4], h[5], h[6], h[7]];
        let (hdr, total) = match size {
            1 if want >= 16 => (16u64, be64(&h[8..])),
            1 => break,
            0 => (8, len - pos),
            s => (8, s),
        };
        if total < hdr {
            break;
        }
        let end = pos + total;
        match &kind {
            b"moov" => {
                if end > len || out.init.is_some() {
                    break;
                }
                let mut buf = vec![0u8; usize::try_from(total - hdr).unwrap_or(0)];
                r.seek(SeekFrom::Start(pos + hdr)).map_err(io)?;
                r.read_exact(&mut buf).map_err(io)?;
                if let Some(mut info) = parse_moov(&buf) {
                    info.init_len = end;
                    info.moov_first = first_after_ftyp;
                    out.init = Some(info);
                    out.valid_len = end;
                } else {
                    break;
                }
            }
            b"styp" | b"sidx" => {
                pending.get_or_insert(pos);
            }
            b"moof" => {
                if end > len || out.init.is_none() {
                    break;
                }
                let mut buf = vec![0u8; usize::try_from(total - hdr).unwrap_or(0)];
                r.seek(SeekFrom::Start(pos + hdr)).map_err(io)?;
                r.read_exact(&mut buf).map_err(io)?;
                let dflt = out.init.as_ref().map_or(0, |i| i.default_sample_duration);
                match parse_moof(&buf, dflt) {
                    Some(m) => {
                        moof = Some(m);
                        moof_start = pending.take().unwrap_or(pos);
                    }
                    None => break,
                }
            }
            b"mdat" => {
                if let Some(m) = moof.take() {
                    if end > len {
                        break; // fragment payload cut short: not complete
                    }
                    out.fragments.push(Fragment {
                        offset: moof_start,
                        len: end - moof_start,
                        track_id: m.track_id,
                        base_time: m.base_time,
                        duration: m.duration,
                        samples: m.samples,
                        starts_with_sync: m.sync,
                    });
                    out.valid_len = end;
                } else if out.init.is_some() || end > len {
                    // An mdat with no moof (a non-fragmented file) or cut short.
                    break;
                }
            }
            _ => {}
        }
        if &kind != b"ftyp" {
            first_after_ftyp = false;
        }
        pos = end;
    }
    Ok(out)
}

/// Parses one standalone media segment (`[styp] [sidx] moof mdat`) as an encoder hands it over,
/// without the init segment. Times are in the track timescale. `None` if it is incomplete.
pub fn parse_segment(bytes: &[u8]) -> Option<Fragment> {
    let kids = children(bytes);
    let (_, ms, me) = kids.iter().find(|(k, _, _)| k == b"moof")?;
    kids.iter().find(|(k, _, _)| k == b"mdat")?;
    let m = parse_moof(&bytes[*ms..*me], 0)?;
    Some(Fragment {
        offset: 0,
        len: bytes.len() as u64,
        track_id: m.track_id,
        base_time: m.base_time,
        duration: m.duration,
        samples: m.samples,
        starts_with_sync: m.sync,
    })
}

/// Scans an in-memory file.
pub fn scan_bytes(bytes: &[u8]) -> Result<Mp4Scan, CmError> {
    scan(&mut std::io::Cursor::new(bytes), bytes.len() as u64)
}

/// Scans a file on disk.
pub fn scan_file(path: &Path) -> Result<Mp4Scan, CmError> {
    let mut f = std::fs::File::open(path)
        .map_err(|e| CmError::not_found(format!("cannot open `{}`: {e}", path.display())))?;
    let len = f
        .metadata()
        .map_err(|e| CmError::internal(format!("cannot stat `{}`: {e}", path.display())))?
        .len();
    scan(&mut f, len)
}

/// Truncates the file to its last complete fragment (REC-8 recovery). Returns the scan of the
/// repaired file and the number of bytes removed. Errors with `invalid_input` when not even the
/// init segment is complete (nothing to recover).
pub fn truncate_to_last_fragment(path: &Path) -> Result<(Mp4Scan, u64), CmError> {
    let mut scan = scan_file(path)?;
    if scan.init.is_none() || scan.fragments.is_empty() {
        return Err(CmError::invalid_input(format!(
            "`{}` has no complete fragment to recover",
            path.display()
        )));
    }
    let removed = scan.total_len - scan.valid_len;
    if removed > 0 {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| CmError::internal(format!("cannot open `{}`: {e}", path.display())))?;
        f.set_len(scan.valid_len)
            .and_then(|()| f.sync_all())
            .map_err(|e| CmError::internal(format!("cannot truncate `{}`: {e}", path.display())))?;
        scan.total_len = scan.valid_len;
    }
    Ok((scan, removed))
}

/// Builders for minimal fragmented MP4 files. They carry the boxes [`scan`] reads but no real
/// video: not decodable, only for tests and the synthetic encoder.
pub mod synth {
    /// Shape of a synthetic video track.
    #[derive(Debug, Clone)]
    pub struct Spec {
        pub width: u32,
        pub height: u32,
        pub timescale: u32,
        pub codec: [u8; 4],
        pub track_id: u32,
        /// Child boxes of the sample entry after the fixed fields (`avcC` for real H.264).
        pub sample_entry_extra: Vec<u8>,
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec {
                width: 1920,
                height: 1080,
                timescale: 120_000,
                codec: *b"avc1",
                track_id: 1,
                sample_entry_extra: Vec::new(),
            }
        }
    }

    pub fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
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

    /// `ftyp` + `moov` (init segment, moov first).
    pub fn init_segment(s: &Spec) -> Vec<u8> {
        let ftyp = boxed(b"ftyp", b"iso6\0\0\0\0iso6cmfcmp41");
        let mut mvhd = vec![0u8; 8]; // creation, modification
        mvhd.extend_from_slice(&s.timescale.to_be_bytes());
        mvhd.extend_from_slice(&0u32.to_be_bytes()); // duration unknown
        mvhd.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // rate
        mvhd.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
        mvhd.extend_from_slice(&[0u8; 10]);
        for m in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
            mvhd.extend_from_slice(&m.to_be_bytes());
        }
        mvhd.extend_from_slice(&[0u8; 24]);
        mvhd.extend_from_slice(&(s.track_id + 1).to_be_bytes());
        let mvhd = full(b"mvhd", 0, 0, &mvhd);

        let mut tkhd = vec![0u8; 8];
        tkhd.extend_from_slice(&s.track_id.to_be_bytes());
        tkhd.extend_from_slice(&[0u8; 4]); // reserved
        tkhd.extend_from_slice(&0u32.to_be_bytes()); // duration
        tkhd.extend_from_slice(&[0u8; 8]); // reserved
        tkhd.extend_from_slice(&[0u8; 8]); // layer, alt group, volume, reserved
        for m in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
            tkhd.extend_from_slice(&m.to_be_bytes());
        }
        tkhd.extend_from_slice(&(s.width << 16).to_be_bytes());
        tkhd.extend_from_slice(&(s.height << 16).to_be_bytes());
        let tkhd = full(b"tkhd", 0, 3, &tkhd);

        let mut mdhd = vec![0u8; 8];
        mdhd.extend_from_slice(&s.timescale.to_be_bytes());
        mdhd.extend_from_slice(&0u32.to_be_bytes());
        mdhd.extend_from_slice(&[0x55, 0xC4, 0, 0]);
        let mdhd = full(b"mdhd", 0, 0, &mdhd);

        let mut hdlr = vec![0u8; 4];
        hdlr.extend_from_slice(b"vide");
        hdlr.extend_from_slice(&[0u8; 12]);
        hdlr.push(0);
        let hdlr = full(b"hdlr", 0, 0, &hdlr);

        let mut entry = vec![0u8; 6];
        entry.extend_from_slice(&1u16.to_be_bytes());
        entry.extend_from_slice(&[0u8; 16]);
        entry.extend_from_slice(&(s.width as u16).to_be_bytes());
        entry.extend_from_slice(&(s.height as u16).to_be_bytes());
        entry.extend_from_slice(&0x0048_0000u32.to_be_bytes());
        entry.extend_from_slice(&0x0048_0000u32.to_be_bytes());
        entry.extend_from_slice(&[0u8; 4]);
        entry.extend_from_slice(&1u16.to_be_bytes());
        entry.extend_from_slice(&[0u8; 32]);
        entry.extend_from_slice(&0x0018u16.to_be_bytes());
        entry.extend_from_slice(&0xFFFFu16.to_be_bytes());
        entry.extend_from_slice(&s.sample_entry_extra);
        let entry = boxed(&s.codec, &entry);
        let mut stsd = 1u32.to_be_bytes().to_vec();
        stsd.extend_from_slice(&entry);
        let stbl = boxed(b"stbl", &full(b"stsd", 0, 0, &stsd));
        let minf = boxed(b"minf", &stbl);
        let mdia = boxed(b"mdia", &concat(&[mdhd, hdlr, minf]));
        let trak = boxed(b"trak", &concat(&[tkhd, mdia]));

        let mut trex = s.track_id.to_be_bytes().to_vec();
        trex.extend_from_slice(&1u32.to_be_bytes());
        trex.extend_from_slice(&[0u8; 12]);
        let mvex = boxed(b"mvex", &full(b"trex", 0, 0, &trex));
        let moov = boxed(b"moov", &concat(&[mvhd, trak, mvex]));
        concat(&[ftyp, moov])
    }

    /// One CMAF-style fragment (`styp`, `moof`, `mdat`) of `samples` samples of
    /// `sample_duration` ticks starting at `base_time`; `sample_size` bytes each.
    pub fn fragment(
        s: &Spec,
        sequence: u32,
        base_time: u64,
        samples: u32,
        sample_duration: u32,
        sample_size: u32,
    ) -> Vec<u8> {
        let styp = boxed(b"styp", b"msdh\0\0\0\0msdhmsix");
        let mfhd = full(b"mfhd", 0, 0, &sequence.to_be_bytes());
        let tfhd = full(b"tfhd", 0, 0x02_0000, &s.track_id.to_be_bytes());
        let tfdt = full(b"tfdt", 1, 0, &base_time.to_be_bytes());
        let mut trun = samples.to_be_bytes().to_vec();
        trun.extend_from_slice(&0i32.to_be_bytes()); // data offset, patched below
        for i in 0..samples {
            trun.extend_from_slice(&sample_duration.to_be_bytes());
            trun.extend_from_slice(&sample_size.to_be_bytes());
            // First sample sync (flags 0x02000000), the rest non-sync (0x01010000).
            let f: u32 = if i == 0 { 0x0200_0000 } else { 0x0101_0000 };
            trun.extend_from_slice(&f.to_be_bytes());
        }
        let trun_box = full(b"trun", 0, 0x701, &trun);
        let traf = boxed(b"traf", &concat(&[tfhd, tfdt, trun_box]));
        let mut moof = boxed(b"moof", &concat(&[mfhd, traf]));
        // Patch data_offset (moof size + mdat header) into the trun payload.
        let off = (moof.len() + 8) as i32;
        let pos = moof.len() - (samples as usize * 12) - 4;
        moof[pos..pos + 4].copy_from_slice(&off.to_be_bytes());
        let mdat = boxed(b"mdat", &vec![0u8; (samples * sample_size) as usize]);
        concat(&[styp, moof, mdat])
    }

    /// One sample of [`fragment_samples`].
    pub struct Sample<'a> {
        pub duration: u32,
        pub sync: bool,
        pub data: &'a [u8],
    }

    /// One CMAF-style fragment (`styp`, `moof`, `mdat`) carrying real sample bytes, with a
    /// per-sample duration, size and sync flag.
    pub fn fragment_samples(
        s: &Spec,
        sequence: u32,
        base_time: u64,
        samples: &[Sample<'_>],
    ) -> Vec<u8> {
        let styp = boxed(b"styp", b"msdh\0\0\0\0msdhmsix");
        let mfhd = full(b"mfhd", 0, 0, &sequence.to_be_bytes());
        let tfhd = full(b"tfhd", 0, 0x02_0000, &s.track_id.to_be_bytes());
        let tfdt = full(b"tfdt", 1, 0, &base_time.to_be_bytes());
        let mut trun = (samples.len() as u32).to_be_bytes().to_vec();
        trun.extend_from_slice(&0i32.to_be_bytes()); // data offset, patched below
        for smp in samples {
            trun.extend_from_slice(&smp.duration.to_be_bytes());
            trun.extend_from_slice(&(smp.data.len() as u32).to_be_bytes());
            let f: u32 = if smp.sync { 0x0200_0000 } else { 0x0101_0000 };
            trun.extend_from_slice(&f.to_be_bytes());
        }
        let trun_box = full(b"trun", 0, 0x701, &trun);
        let traf = boxed(b"traf", &concat(&[tfhd, tfdt, trun_box]));
        let mut moof = boxed(b"moof", &concat(&[mfhd, traf]));
        let off = (moof.len() + 8) as i32;
        let pos = moof.len() - (samples.len() * 12) - 4;
        moof[pos..pos + 4].copy_from_slice(&off.to_be_bytes());
        let payload: Vec<u8> = samples
            .iter()
            .flat_map(|x| x.data.iter().copied())
            .collect();
        concat(&[styp, moof, boxed(b"mdat", &payload)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synth::*;

    /// Init + `n` fragments of 2 s at 30 fps in a 120 kHz timescale.
    fn file(n: u32) -> Vec<u8> {
        let s = Spec::default();
        let mut v = init_segment(&s);
        for i in 0..n {
            v.extend(fragment(&s, i + 1, u64::from(i) * 240_000, 60, 4000, 50));
        }
        v
    }

    #[test]
    fn parses_init_and_fragments() {
        let bytes = file(5);
        let scan = scan_bytes(&bytes).unwrap();
        let init = scan.init.as_ref().unwrap();
        assert!(init.moov_first);
        assert_eq!((init.width, init.height), (1920, 1080));
        assert_eq!(init.codec, "avc1");
        assert_eq!(init.timescale, 120_000);
        assert_eq!(scan.fragments.len(), 5);
        assert!(scan.is_clean());
        assert!(scan.fragments.iter().all(|f| f.starts_with_sync));
        assert_eq!(scan.frame_count(), 300);
        assert_eq!(scan.start().unwrap().ticks(), 0);
        assert_eq!(scan.end().unwrap().ticks(), 5 * 240_000);
        assert_eq!(scan.duration().unwrap().as_secs_f64(), 10.0);
        for d in scan.fragment_durations_secs() {
            assert!((d - 2.0).abs() < 1e-9);
        }
    }

    #[test]
    fn rec8_truncation_anywhere_recovers_up_to_the_last_complete_fragment() {
        let bytes = file(4);
        let full = scan_bytes(&bytes).unwrap();
        let init_len = full.init.as_ref().unwrap().init_len;
        // Cut at every byte offset after the init segment, in steps, plus the exact boundaries.
        let mut cuts: Vec<u64> = (init_len..bytes.len() as u64).step_by(7).collect();
        cuts.extend(full.fragments.iter().map(|f| f.offset + f.len));
        cuts.extend(full.fragments.iter().map(|f| f.offset));
        for cut in cuts {
            let s = scan_bytes(&bytes[..cut as usize]).unwrap();
            let complete = full
                .fragments
                .iter()
                .filter(|f| f.offset + f.len <= cut)
                .count();
            assert_eq!(s.fragments.len(), complete, "cut {cut}");
            let want_valid = full
                .fragments
                .iter()
                .take(complete)
                .next_back()
                .map_or(init_len, |f| f.offset + f.len);
            assert_eq!(s.valid_len, want_valid, "cut {cut}");
            assert_eq!(s.is_clean(), cut == want_valid, "cut {cut}");
        }
    }

    #[test]
    fn cut_inside_init_has_nothing_to_recover() {
        let bytes = file(2);
        let s = scan_bytes(&bytes[..40]).unwrap();
        assert!(s.init.is_none());
        assert_eq!(s.valid_len, 0);
        assert!(s.fragments.is_empty());
    }

    #[test]
    fn truncate_file_removes_the_partial_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        let mut bytes = file(3);
        let good = bytes.len();
        bytes.extend(&fragment(&Spec::default(), 4, 720_000, 60, 4000, 50)[..100]);
        std::fs::write(&path, &bytes).unwrap();
        let (scan, removed) = truncate_to_last_fragment(&path).unwrap();
        assert_eq!(removed, 100);
        assert_eq!(scan.fragments.len(), 3);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), good as u64);
        assert!(scan_file(&path).unwrap().is_clean());
        // A second run is a no-op.
        assert_eq!(truncate_to_last_fragment(&path).unwrap().1, 0);
    }

    #[test]
    fn no_fragment_is_an_error_not_an_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        std::fs::write(&path, init_segment(&Spec::default())).unwrap();
        assert!(truncate_to_last_fragment(&path).is_err());
    }

    #[test]
    fn moov_after_mdat_is_reported_as_not_first() {
        let s = Spec::default();
        let mut v = boxed(b"ftyp", b"iso6\0\0\0\0");
        v.extend(boxed(b"mdat", &[0u8; 16]));
        let init = init_segment(&s);
        // Append just the moov (skip the ftyp of the synthetic init).
        let ftyp_len = u32::from_be_bytes([init[0], init[1], init[2], init[3]]) as usize;
        v.extend(&init[ftyp_len..]);
        // An mdat without a moof before the moov is not a fragmented layout; the scan stops at it.
        let s = scan_bytes(&v).unwrap();
        assert!(s.fragments.is_empty());
        assert!(!s.init.unwrap().moov_first);
    }

    #[test]
    fn garbage_and_zero_length_input_do_not_panic() {
        assert!(scan_bytes(&[]).unwrap().init.is_none());
        assert!(scan_bytes(&[0xFF; 64]).unwrap().init.is_none());
        let mut v = file(2);
        v.extend_from_slice(&[0, 0, 0, 3, b'x', b'x', b'x', b'x']); // size < header
        assert_eq!(scan_bytes(&v).unwrap().fragments.len(), 2);
    }
}
