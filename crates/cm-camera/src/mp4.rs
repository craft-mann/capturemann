//! A tiny fragmented-MP4 inspector (box walk only): proves a file is fragmented (moov first, one
//! moof/mdat pair per fragment) and gives fragment start times from `tfdt`. Used by the manual
//! probes and the macOS integration test; no decoding.

use std::path::Path;

/// What a fragmented file looks like at the box level.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentReport {
    /// `moov` precedes the first `moof`.
    pub moov_first: bool,
    /// Media timescale of the first track (`mdhd`), 0 if not found.
    pub timescale: u32,
    /// Base media decode time of each fragment, in seconds.
    pub fragment_starts_s: Vec<f64>,
    pub top_level: Vec<String>,
}

impl FragmentReport {
    /// Gaps between consecutive fragment starts (all but the last fragment's own length).
    pub fn fragment_lengths_s(&self) -> Vec<f64> {
        self.fragment_starts_s
            .windows(2)
            .map(|w| w[1] - w[0])
            .collect()
    }
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Iterates the boxes in `b`: (type, payload start, payload end).
fn boxes(b: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= b.len() {
        let Some(size32) = be32(b, pos) else { break };
        let mut hdr = 8usize;
        let size = match size32 {
            0 => b.len() - pos,
            1 => {
                hdr = 16;
                be64(b, pos + 8).map_or(0, |s| usize::try_from(s).unwrap_or(0))
            }
            n => n as usize,
        };
        if size < hdr || pos + size > b.len() {
            break;
        }
        let mut ty = [0u8; 4];
        ty.copy_from_slice(&b[pos + 4..pos + 8]);
        out.push((ty, pos + hdr, pos + size));
        pos += size;
    }
    out
}

fn find<'a>(b: &'a [u8], ty: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(b)
        .into_iter()
        .find(|(t, _, _)| t == ty)
        .map(|(_, s, e)| &b[s..e])
}

/// Walks `bytes` as a fragmented MP4.
pub fn inspect_bytes(bytes: &[u8]) -> FragmentReport {
    let mut r = FragmentReport {
        moov_first: false,
        timescale: 0,
        fragment_starts_s: Vec::new(),
        top_level: Vec::new(),
    };
    let mut seen_moof = false;
    for (ty, s, e) in boxes(bytes) {
        r.top_level.push(String::from_utf8_lossy(&ty).into_owned());
        match &ty {
            b"moov" => {
                r.moov_first = !seen_moof;
                let ts = find(&bytes[s..e], b"trak")
                    .and_then(|t| find(t, b"mdia"))
                    .and_then(|m| find(m, b"mdhd"))
                    .and_then(|h| {
                        // version 0: ts at 12; version 1: ts at 20
                        if *h.first()? == 1 {
                            be32(h, 20)
                        } else {
                            be32(h, 12)
                        }
                    });
                r.timescale = ts.unwrap_or(0);
            }
            b"moof" => {
                seen_moof = true;
                let tfdt = find(&bytes[s..e], b"traf")
                    .and_then(|t| find(t, b"tfdt"))
                    .and_then(|d| {
                        if *d.first()? == 1 {
                            be64(d, 4)
                        } else {
                            be32(d, 4).map(u64::from)
                        }
                    });
                if let (Some(t), ts) = (tfdt, r.timescale) {
                    if ts > 0 {
                        r.fragment_starts_s.push(t as f64 / f64::from(ts));
                    }
                }
            }
            _ => {}
        }
    }
    r
}

pub fn inspect_file(path: &Path) -> std::io::Result<FragmentReport> {
    Ok(inspect_bytes(&std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(ty: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(ty);
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn reads_timescale_and_fragment_starts() {
        let mut mdhd = vec![0u8; 12];
        mdhd.extend_from_slice(&120_000u32.to_be_bytes());
        let moov = mk(b"moov", &mk(b"trak", &mk(b"mdia", &mk(b"mdhd", &mdhd))));
        let frag = |t: u32| {
            let mut tfdt = vec![0u8; 4];
            tfdt.extend_from_slice(&t.to_be_bytes());
            let mut f = mk(b"moof", &mk(b"traf", &mk(b"tfdt", &tfdt)));
            f.extend(mk(b"mdat", &[0; 4]));
            f
        };
        let mut file = mk(b"ftyp", b"cmfc");
        file.extend(moov);
        file.extend(frag(0));
        file.extend(frag(240_000));
        file.extend(frag(480_000));
        let r = inspect_bytes(&file);
        assert!(r.moov_first);
        assert_eq!(r.timescale, 120_000);
        assert_eq!(r.fragment_starts_s, vec![0.0, 2.0, 4.0]);
        assert_eq!(r.fragment_lengths_s(), vec![2.0, 2.0]);
        assert_eq!(r.top_level[0], "ftyp");
    }
}
