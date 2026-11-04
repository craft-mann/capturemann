//! H.264 byte-stream helpers for encoders that hand over Annex B (start-code) samples, such as
//! the Media Foundation H.264 transform on Windows: splitting a sample into NAL units, finding
//! the parameter sets and the sync frame, turning the sample into the length-prefixed form an MP4
//! `mdat` holds, and building the `avcC` box. Pure Rust, tested on every OS.

use cm_core::CmError;

pub const NAL_SLICE_IDR: u8 = 5;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
pub const NAL_AUD: u8 = 9;

/// The NAL unit type of a NAL unit (its first byte, low five bits).
pub fn nal_type(nal: &[u8]) -> u8 {
    nal.first().map_or(0, |b| b & 0x1f)
}

/// Splits an Annex B byte stream (`00 00 01` or `00 00 00 01` before every unit) into NAL units
/// without their start codes. Trailing zero bytes of a unit (the next start code's leading zero)
/// are removed. Bytes before the first start code are ignored.
pub fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut starts: Vec<usize> = Vec::new(); // index of the first byte after a start code
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (n, &s) in starts.iter().enumerate() {
        let mut e = starts.get(n + 1).map_or(data.len(), |next| next - 3);
        while e > s && data[e - 1] == 0 {
            e -= 1; // zero_byte of a four-byte start code, or trailing_zero_8bits
        }
        if e > s {
            out.push(&data[s..e]);
        }
    }
    out
}

/// One encoded frame ready for an MP4 `mdat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvccSample {
    /// The frame's NAL units, each behind a four-byte big-endian length. Parameter sets and
    /// access unit delimiters are not in it (the parameter sets live in `avcC`).
    pub data: Vec<u8>,
    /// The frame holds an IDR slice: a fragment can start here.
    pub sync: bool,
    pub sps: Option<Vec<u8>>,
    pub pps: Option<Vec<u8>>,
}

/// Converts one Annex B access unit to an [`AvccSample`]. An access unit with no slice (nothing
/// but parameter sets, say) is an error: the caller has nothing to write for it.
pub fn annexb_to_avcc(au: &[u8]) -> Result<AvccSample, CmError> {
    let mut out = AvccSample {
        data: Vec::with_capacity(au.len()),
        sync: false,
        sps: None,
        pps: None,
    };
    let mut slices = 0usize;
    for nal in split_annexb(au) {
        match nal_type(nal) {
            NAL_SPS => out.sps = Some(nal.to_vec()),
            NAL_PPS => out.pps = Some(nal.to_vec()),
            NAL_AUD => {}
            t => {
                if t == NAL_SLICE_IDR {
                    out.sync = true;
                }
                if (1..=5).contains(&t) {
                    slices += 1;
                }
                let len = u32::try_from(nal.len())
                    .map_err(|_| CmError::internal("an H.264 NAL unit is larger than 4 GB"))?;
                out.data.extend_from_slice(&len.to_be_bytes());
                out.data.extend_from_slice(nal);
            }
        }
    }
    if slices == 0 {
        return Err(CmError::internal(
            "the encoder returned a frame with no slice data",
        ));
    }
    Ok(out)
}

/// The sequence and picture parameter sets in an Annex B blob (the encoder's sequence header).
pub fn parameter_sets(annexb: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let (mut sps, mut pps) = (None, None);
    for nal in split_annexb(annexb) {
        match nal_type(nal) {
            NAL_SPS if sps.is_none() => sps = Some(nal.to_vec()),
            NAL_PPS if pps.is_none() => pps = Some(nal.to_vec()),
            _ => {}
        }
    }
    (sps, pps)
}

/// The `avcC` box (AVCDecoderConfigurationRecord) for one SPS and one PPS, four-byte NAL
/// lengths. For the High profiles the chroma and bit-depth fields are written as 4:2:0, 8 bit,
/// which is all a hardware screen encoder produces from NV12.
pub fn avcc_box(sps: &[u8], pps: &[u8]) -> Result<Vec<u8>, CmError> {
    if sps.len() < 4 || nal_type(sps) != NAL_SPS {
        return Err(CmError::internal(
            "the H.264 sequence parameter set is invalid",
        ));
    }
    if pps.is_empty() || nal_type(pps) != NAL_PPS {
        return Err(CmError::internal(
            "the H.264 picture parameter set is invalid",
        ));
    }
    let (sl, pl) = (
        u16::try_from(sps.len()).map_err(|_| CmError::internal("SPS too long"))?,
        u16::try_from(pps.len()).map_err(|_| CmError::internal("PPS too long"))?,
    );
    let profile = sps[1];
    let mut p = vec![1, profile, sps[2], sps[3], 0xFF, 0xE1];
    p.extend_from_slice(&sl.to_be_bytes());
    p.extend_from_slice(sps);
    p.push(1);
    p.extend_from_slice(&pl.to_be_bytes());
    p.extend_from_slice(pps);
    if matches!(
        profile,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134
    ) {
        p.extend_from_slice(&[0xFD, 0xF8, 0xF8, 0]);
    }
    let mut b = ((p.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend_from_slice(b"avcC");
    b.extend_from_slice(&p);
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn au() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&[0, 0, 0, 1, 0x09, 0xF0]); // AUD
        v.extend_from_slice(&[0, 0, 0, 1, 0x67, 100, 0, 40, 0xAA]); // SPS (High, level 4.0)
        v.extend_from_slice(&[0, 0, 1, 0x68, 0xEE, 0x3C]); // PPS with a three-byte code
        v.extend_from_slice(&[0, 0, 0, 1, 0x65, 1, 2, 3]); // IDR
        v
    }

    #[test]
    fn splits_three_and_four_byte_start_codes() {
        let stream = au();
        let units = split_annexb(&stream);
        let types: Vec<u8> = units.iter().map(|u| nal_type(u)).collect();
        assert_eq!(types, [9, 7, 8, 5]);
        assert_eq!(units[2], &[0x68, 0xEE, 0x3C]);
        assert_eq!(units[3], &[0x65, 1, 2, 3]);
    }

    #[test]
    fn a_unit_keeps_its_own_trailing_data_but_not_the_next_zero_byte() {
        // 0x65 .. 0x00 belongs to the unit only when it is not the start of a four-byte code.
        let v = [0, 0, 1, 0x41, 9, 0, 0, 0, 1, 0x41, 7];
        let units = split_annexb(&v);
        assert_eq!(units, [&[0x41, 9][..], &[0x41, 7][..]]);
    }

    #[test]
    fn garbage_without_a_start_code_has_no_units() {
        assert!(split_annexb(&[1, 2, 3, 4, 5]).is_empty());
        assert!(split_annexb(&[]).is_empty());
        assert!(split_annexb(&[0, 0, 1]).is_empty());
    }

    #[test]
    fn avcc_conversion_strips_parameter_sets_and_marks_the_idr() {
        let s = annexb_to_avcc(&au()).unwrap();
        assert!(s.sync);
        assert_eq!(s.sps.as_deref(), Some(&[0x67, 100, 0, 40, 0xAA][..]));
        assert_eq!(s.pps.as_deref(), Some(&[0x68, 0xEE, 0x3C][..]));
        assert_eq!(s.data, [0, 0, 0, 4, 0x65, 1, 2, 3]);
    }

    #[test]
    fn a_p_frame_is_not_sync_and_a_frame_without_slices_is_an_error() {
        let p = annexb_to_avcc(&[0, 0, 0, 1, 0x41, 9, 9]).unwrap();
        assert!(!p.sync);
        assert_eq!(p.data, [0, 0, 0, 3, 0x41, 9, 9]);
        assert!(annexb_to_avcc(&[0, 0, 0, 1, 0x67, 100, 0, 40, 0xAA]).is_err());
        assert!(annexb_to_avcc(&[]).is_err());
    }

    #[test]
    fn sequence_header_blob_gives_the_parameter_sets() {
        let (sps, pps) = parameter_sets(&au());
        assert_eq!(sps.unwrap()[0], 0x67);
        assert_eq!(pps.unwrap()[0], 0x68);
        assert_eq!(parameter_sets(&[0, 0, 0, 1, 0x65, 1]), (None, None));
    }

    #[test]
    fn avcc_box_layout() {
        let sps = [0x67, 100, 0, 40, 0xAA];
        let pps = [0x68, 0xEE, 0x3C];
        let b = avcc_box(&sps, &pps).unwrap();
        assert_eq!(&b[4..8], b"avcC");
        assert_eq!(
            u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize,
            b.len()
        );
        assert_eq!(&b[8..14], &[1, 100, 0, 40, 0xFF, 0xE1]);
        assert_eq!(&b[14..16], &[0, 5]);
        assert_eq!(&b[16..21], &sps);
        assert_eq!(b[21], 1);
        assert_eq!(&b[22..24], &[0, 3]);
        assert_eq!(&b[24..27], &pps);
        // High profile carries the chroma / bit depth extension.
        assert_eq!(&b[27..], &[0xFD, 0xF8, 0xF8, 0]);
        // Main profile does not.
        let main = avcc_box(&[0x67, 77, 0, 40], &pps).unwrap();
        assert_eq!(main.len(), 8 + 6 + 2 + 4 + 1 + 2 + 3);
    }

    #[test]
    fn bad_parameter_sets_are_errors() {
        assert!(avcc_box(&[0x67], &[0x68, 1]).is_err());
        assert!(avcc_box(&[0x68, 1, 2, 3], &[0x68, 1]).is_err());
        assert!(avcc_box(&[0x67, 66, 0, 30], &[]).is_err());
    }
}
