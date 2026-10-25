//! The crash-safe WAV writer lives in `cm_core::wav` so the library's crash recovery can repair
//! WAV files without depending on the audio stack; this re-exports it.

pub use cm_core::wav::{read_info, repair_wav, WavInfo, WavWriter, WAV_HEADER_LEN};
