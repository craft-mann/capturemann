//! Currently only support utf8 mode

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "std")]
use std::io::{self, Write};

const UTF8_START: &[u8] = &[0x1B, 0x25, 0x47];
const UTF8_END: &[u8] = &[0x1B, 0x25, 0x40];

/// Wrapper for reduce allocation
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CText<'s> {
    utf8: &'s str,
}

impl<'s> fmt::Debug for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> fmt::Display for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> CText<'s> {
    pub const fn new(utf8: &'s str) -> Self {
        Self { utf8 }
    }

    pub const fn len(self) -> usize {
        self.utf8.len() + UTF8_START.len() + UTF8_END.len()
    }

    #[cfg(feature = "std")]
    pub fn write(self, mut out: impl Write) -> io::Result<usize> {
        let mut writed = 0;
        writed += out.write(UTF8_START)?;
        writed += out.write(self.utf8.as_bytes())?;
        writed += out.write(UTF8_END)?;
        Ok(writed)
    }
}

/// Encoding utf8 to COMPOUND_TEXT with utf8 escape
pub fn utf8_to_compound_text(text: &str) -> Vec<u8> {
    let mut ret = Vec::with_capacity(text.len() + 6);
    ret.extend_from_slice(UTF8_START);
    ret.extend_from_slice(text.as_bytes());
    ret.extend_from_slice(UTF8_END);
    ret
}

#[derive(Debug, Clone)]
pub enum DecodeError {
    InvalidEncoding,
    UnsupportedEncoding,
    Utf8Error(alloc::string::FromUtf8Error),
}

impl From<alloc::string::FromUtf8Error> for DecodeError {
    fn from(err: alloc::string::FromUtf8Error) -> Self {
        DecodeError::Utf8Error(err)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEncoding => write!(f, "Invalid compound text"),
            Self::UnsupportedEncoding => write!(f, "This encoding is not supported yet"),
            Self::Utf8Error(e) => write!(f, "Not a valid utf8 {}", e),
        }
    }
}

macro_rules! decode {
    ($decoder:expr, $out:expr, $bytes:expr, $last:expr) => {
        loop {
            let (ret, _, _) = $decoder.decode_to_string($bytes, $out, $last);

            match ret {
                encoding_rs::CoderResult::InputEmpty => break,
                encoding_rs::CoderResult::OutputFull => {
                    $out.reserve(
                        $decoder
                            .max_utf8_buffer_length($bytes.len())
                            .unwrap_or_default(),
                    );
                }
            }
        }
    };
}

fn compound_text_to_utf8_strict(bytes: &[u8]) -> Result<String, DecodeError> {
    let mut iter = bytes.iter();

    match iter.next() {
        None => Ok(String::new()),
        Some(0x1B) => {
            match (iter.next(), iter.next()) {
                // UTF-8
                (Some(0x25), Some(0x47)) => {
                    let left = iter.as_slice();
                    Ok(String::from_utf8(left.split_at(left.len() - 3).0.to_vec())?)
                }
                // 94N
                (Some(0x24), Some(0x28)) => match iter.next() {
                    // JP
                    Some(0x42) => {
                        let left = iter.as_slice();
                        let mut decoder =
                            encoding_rs::ISO_2022_JP.new_decoder_without_bom_handling();
                        let mut out = String::new();

                        decode!(decoder, &mut out, &[0x1B, 0x24, 0x42], false);
                        decode!(decoder, &mut out, left, true);

                        Ok(out)
                    }

                    // CN
                    Some(0x41) => Err(DecodeError::UnsupportedEncoding),

                    // KR
                    Some(0x43) => Err(DecodeError::UnsupportedEncoding),

                    _ => Err(DecodeError::InvalidEncoding),
                },
                // Invalid encode
                _ => Err(DecodeError::InvalidEncoding),
            }
        }
        // unescaped string
        Some(_) => Ok(String::from_utf8(bytes.to_vec())?),
    }
}

/// ISO 8859-1 (the default G1 set of COMPOUND_TEXT): every byte is the code point of the same value.
fn latin1_to_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Reads COMPOUND_TEXT as segments: ISO 8859-1 by default, UTF-8 between `ESC % G` and `ESC % @`,
/// any other escape sequence dropped. Text with no escape at all is UTF-8 when it is valid UTF-8
/// (what the upstream decoder always assumed) and ISO 8859-1 otherwise.
fn compound_text_lossy(bytes: &[u8]) -> String {
    if !bytes.contains(&0x1B) {
        return match core::str::from_utf8(bytes) {
            Ok(text) => String::from(text),
            Err(_) => latin1_to_string(bytes),
        };
    }
    let mut out = String::new();
    let mut utf8 = false;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1B {
            // ESC, intermediate bytes 0x20..=0x2F, then one final byte.
            let mut j = i + 1;
            while j < bytes.len() && (0x20..=0x2F).contains(&bytes[j]) {
                j += 1;
            }
            match bytes.get(i + 1..=j) {
                Some([0x25, 0x47]) => utf8 = true,
                Some([0x25, 0x40]) => utf8 = false,
                _ => {}
            }
            i = j + 1;
        } else {
            let end = bytes[i..]
                .iter()
                .position(|&b| b == 0x1B)
                .map_or(bytes.len(), |n| i + n);
            let run = &bytes[i..end];
            if utf8 {
                out.push_str(&String::from_utf8_lossy(run));
            } else {
                out.push_str(&latin1_to_string(run));
            }
            i = end;
        }
    }
    out
}

/// COMPOUND_TEXT to UTF-8. CaptureMann change: this never fails. XIM servers (IBus with a
/// Vietnamese engine, for one) send "Vi\xEA" for the draft of "Viê" (ISO 8859-1 bytes with no
/// escape sequence) and "Vi ESC%G ệ ESC%@ t" for "Việt" (UTF-8 segments in the middle). Both are
/// valid COMPOUND_TEXT; the upstream decoder returned an error that `zed-xim` turns into a panic
/// (the app quit on the first accented letter) or kept the escape bytes in the text.
pub fn compound_text_to_utf8(bytes: &[u8]) -> Result<String, DecodeError> {
    // The 94N sets (Japanese) are only handled by the strict decoder.
    if bytes.starts_with(&[0x1B, 0x24, 0x28]) {
        if let Ok(text) = compound_text_to_utf8_strict(bytes) {
            return Ok(text);
        }
    }
    Ok(compound_text_lossy(bytes))
}

#[cfg(test)]
mod tests {
    #[test]
    fn korean() {
        const UTF8: &str = "가나다";
        const COMP: &[u8] = &[
            27, 37, 71, 234, 176, 128, 235, 130, 152, 235, 139, 164, 27, 37, 64,
        ];
        assert_eq!(crate::utf8_to_compound_text(UTF8), COMP);
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn iso_2011_jp() {
        const UTF8: &str = "東京";
        const COMP: &[u8] = &[27, 36, 40, 66, 69, 108, 53, 126];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn latin1_without_escape_is_decoded() {
        // What IBus sends for the draft "Viê" (and "á", "ạ" is outside Latin-1 and comes escaped).
        assert_eq!(crate::compound_text_to_utf8(&[86, 105, 234]).unwrap(), "Viê");
        assert_eq!(crate::compound_text_to_utf8(&[0xE1]).unwrap(), "á");
    }

    #[test]
    fn utf8_segments_in_the_middle_are_decoded() {
        // "Vi", ESC % G, "ệ" (E1 BB 87), ESC % @, "t": what IBus sends for "Việt".
        let comp = [
            86, 105, 0x1B, 0x25, 0x47, 0xE1, 0xBB, 0x87, 0x1B, 0x25, 0x40, 116,
        ];
        assert_eq!(crate::compound_text_to_utf8(&comp).unwrap(), "Việt");
    }

    #[test]
    fn plain_utf8_and_empty_still_work() {
        assert_eq!(crate::compound_text_to_utf8(b"").unwrap(), "");
        assert_eq!(crate::compound_text_to_utf8("ơ ư".as_bytes()).unwrap(), "ơ ư");
    }

    #[test]
    fn unknown_escapes_do_not_fail() {
        // ESC - A designates ISO 8859-1 to G1; ESC ( B designates ASCII to G0.
        assert_eq!(
            crate::compound_text_to_utf8(&[0x1B, 0x2D, 0x41, 0x56, 0xEA]).unwrap(),
            "Vê"
        );
        assert_eq!(
            crate::compound_text_to_utf8(&[0x1B, 0x28, 0x42, b'a']).unwrap(),
            "a"
        );
        // Truncated escape.
        assert_eq!(crate::compound_text_to_utf8(&[0x1B, 0x28]).unwrap(), "");
    }
}
