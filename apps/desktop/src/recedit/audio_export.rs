//! The Export audio sheet's choices (VOC-2, redesign V7), free of any display: the format and
//! tracks, the loudness switch, the copy folder, the `export_audio` call they make and the size
//! the footer estimates. The sheet is drawn in `ui::recording_editor::export_sheet`.

use serde_json::{json, Value};

/// The loudness target of "Even out the loudness", LUFS (the daemon's default for M4A).
pub const TARGET_LUFS: f64 = -16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    M4a,
    Wav,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::M4a => "m4a",
            Format::Wav => "wav",
        }
    }

    /// The upper-case name on the cards and the button.
    pub fn label(self) -> &'static str {
        match self {
            Format::M4a => "M4A",
            Format::Wav => "WAV",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tracks {
    #[default]
    Mix,
    Mic,
    System,
}

impl Tracks {
    pub const ALL: [Tracks; 3] = [Tracks::Mix, Tracks::Mic, Tracks::System];

    pub fn as_str(self) -> &'static str {
        match self {
            Tracks::Mix => "mix",
            Tracks::Mic => "mic",
            Tracks::System => "system",
        }
    }
}

/// Which audio tracks the recording has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Available {
    pub mic: bool,
    pub system: bool,
}

impl Available {
    /// Whether the choice can be exported from this recording.
    pub fn allows(self, t: Tracks) -> bool {
        match t {
            Tracks::Mix => self.mic || self.system,
            Tracks::Mic => self.mic,
            Tracks::System => self.system,
        }
    }

    /// Any audio at all.
    pub fn any(self) -> bool {
        self.mic || self.system
    }
}

/// The sheet's state.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub format: Format,
    pub tracks: Tracks,
    pub loudness: bool,
    /// The folder of "Also save a copy to".
    pub copy_dir: Option<String>,
    /// The user flipped the loudness switch: choosing a format no longer resets it.
    loudness_touched: bool,
}

impl Default for Form {
    /// M4A, Mix, loudness on (the default for M4A), no copy folder.
    fn default() -> Self {
        Form {
            format: Format::M4a,
            tracks: Tracks::Mix,
            loudness: true,
            copy_dir: None,
            loudness_touched: false,
        }
    }
}

impl Form {
    /// The form for a recording: the copy folder the last export used.
    pub fn new(copy_dir: Option<String>) -> Form {
        Form {
            copy_dir,
            ..Form::default()
        }
    }

    /// Picks a format. Loudness follows the format (on for M4A, off for WAV) until the user has
    /// touched the switch.
    pub fn set_format(&mut self, f: Format) {
        self.format = f;
        if !self.loudness_touched {
            self.loudness = f == Format::M4a;
        }
    }

    pub fn toggle_loudness(&mut self) {
        self.loudness = !self.loudness;
        self.loudness_touched = true;
    }

    /// Picks the tracks when the recording has them.
    pub fn set_tracks(&mut self, t: Tracks, avail: Available) {
        if avail.allows(t) {
            self.tracks = t;
        }
    }

    /// The `export_audio` parameters. The loudness is always explicit (a number, or `null`), so
    /// the call does what the sheet shows whatever the daemon's own defaults are.
    pub fn params(&self, capture_id: &str) -> Value {
        let mut p = json!({
            "capture_id": capture_id,
            "format": self.format.as_str(),
            "tracks": self.tracks.as_str(),
            "loudness_lufs": if self.loudness { json!(TARGET_LUFS) } else { Value::Null },
        });
        if let Some(d) = self.copy_dir.as_ref().filter(|d| !d.trim().is_empty()) {
            p["out_dir"] = json!(d);
        }
        p
    }

    /// Channels the file will have: the mix and the system track are stereo when the system
    /// track is in them, the microphone alone is mono (as the exporter decides).
    pub fn channels(&self, avail: Available) -> u32 {
        let system = match self.tracks {
            Tracks::Mic => false,
            Tracks::System => true,
            Tracks::Mix => avail.system,
        };
        if system {
            2
        } else {
            1
        }
    }

    /// About how many bytes the file will have after `secs` seconds of audio.
    pub fn estimate_bytes(&self, secs: f64, avail: Available) -> u64 {
        let ch = u64::from(self.channels(avail));
        let per_sec = match self.format {
            // AAC-LC: 128 kbps mono, 192 kbps stereo.
            Format::M4a => {
                if ch >= 2 {
                    192_000 / 8
                } else {
                    128_000 / 8
                }
            }
            // 48 kHz, 3 bytes per sample.
            Format::Wav => 48_000 * 3 * ch,
        };
        (secs.max(0.0) * per_sec as f64).round() as u64
    }
}

/// `44 MB` (MB = 10^6 bytes), `1.2 GB` from a thousand of them, `300 KB` below one.
pub fn size_text(bytes: u64) -> String {
    let b = bytes as f64;
    if bytes >= 1_000_000_000 {
        format!("{} GB", cm_i18n::fmt::decimal(b / 1e9, 1))
    } else if bytes >= 1_000_000 {
        format!("{} MB", (b / 1e6).round() as u64)
    } else {
        format!("{} KB", ((b / 1e3).round() as u64).max(1))
    }
}

/// The file name of a finished export for the toast (`path`'s last part).
pub fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// `~/Documents/Meetings` for a folder under the home folder, else the path as it is.
pub fn tilde(path: &str, home: Option<&str>) -> String {
    match home.filter(|h| !h.is_empty()) {
        Some(h) if path == h => "~".to_owned(),
        Some(h) => path
            .strip_prefix(h)
            .filter(|rest| rest.starts_with('/'))
            .map_or_else(|| path.to_owned(), |rest| format!("~{rest}")),
        None => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: Available = Available {
        mic: true,
        system: true,
    };
    const MIC: Available = Available {
        mic: true,
        system: false,
    };

    /// V7: M4A, Mix, loudness on by default, and the call says so explicitly.
    #[test]
    fn the_default_call_is_m4a_mix_at_minus_16() {
        let f = Form::default();
        assert_eq!(
            f.params("rec_1"),
            json!({"capture_id": "rec_1", "format": "m4a", "tracks": "mix", "loudness_lufs": -16.0})
        );
    }

    /// V7: loudness follows the format until the user decides.
    #[test]
    fn loudness_follows_the_format_until_touched() {
        let mut f = Form::default();
        f.set_format(Format::Wav);
        assert!(!f.loudness);
        assert_eq!(f.params("c")["loudness_lufs"], Value::Null);
        f.set_format(Format::M4a);
        assert!(f.loudness);
        // The user turns it off: M4A keeps it off, and WAV does not turn it back on.
        f.toggle_loudness();
        assert!(!f.loudness);
        f.set_format(Format::Wav);
        f.set_format(Format::M4a);
        assert!(!f.loudness);
        // On for WAV is possible and explicit.
        f.set_format(Format::Wav);
        f.toggle_loudness();
        assert_eq!(f.params("c")["loudness_lufs"], json!(-16.0));
    }

    #[test]
    fn tracks_need_the_recording_to_have_them() {
        let mut f = Form::default();
        f.set_tracks(Tracks::System, MIC);
        assert_eq!(f.tracks, Tracks::Mix);
        f.set_tracks(Tracks::Mic, MIC);
        assert_eq!(f.tracks, Tracks::Mic);
        assert!(MIC.allows(Tracks::Mix));
        assert!(!Available {
            mic: false,
            system: false
        }
        .allows(Tracks::Mix));
        assert_eq!(f.params("c")["tracks"], json!("mic"));
    }

    #[test]
    fn the_copy_folder_is_sent_only_when_set() {
        let mut f = Form::new(Some("/Users/a/Documents/Meetings".into()));
        assert_eq!(
            f.params("c")["out_dir"],
            json!("/Users/a/Documents/Meetings")
        );
        f.copy_dir = Some("  ".into());
        assert!(f.params("c").get("out_dir").is_none());
        f.copy_dir = None;
        assert!(f.params("c").get("out_dir").is_none());
    }

    /// V7 footer: sizes by the bit rates of the spec.
    #[test]
    fn the_size_estimate_follows_the_bit_rates() {
        let mut f = Form::default();
        // Mix with a system track is stereo: 192 kbps = 24 kB/s.
        assert_eq!(f.estimate_bytes(100.0, BOTH), 2_400_000);
        // The microphone alone is mono: 128 kbps = 16 kB/s.
        assert_eq!(f.estimate_bytes(100.0, MIC), 1_600_000);
        f.set_tracks(Tracks::Mic, BOTH);
        assert_eq!(f.estimate_bytes(100.0, BOTH), 1_600_000);
        // 45:48 of mono M4A is about 44 MB, as on the board.
        assert_eq!(
            size_text(f.estimate_bytes(45.0 * 60.0 + 48.0, BOTH)),
            "44 MB"
        );
        f.set_format(Format::Wav);
        assert_eq!(f.estimate_bytes(10.0, BOTH), 1_440_000);
        f.set_tracks(Tracks::System, BOTH);
        assert_eq!(f.estimate_bytes(10.0, BOTH), 2_880_000);
        assert_eq!(f.estimate_bytes(-5.0, BOTH), 0);
    }

    #[test]
    fn sizes_read_in_decimal_units() {
        assert_eq!(size_text(300_000), "300 KB");
        assert_eq!(size_text(10), "1 KB");
        assert_eq!(size_text(44_400_000), "44 MB");
        assert_eq!(size_text(1_260_000_000), "1.3 GB");
    }

    #[test]
    fn paths_for_the_toast_and_the_folder_chip() {
        assert_eq!(file_name("/a/b/Weekly sync.m4a"), "Weekly sync.m4a");
        assert_eq!(
            tilde("/Users/a/Documents/Meetings", Some("/Users/a")),
            "~/Documents/Meetings"
        );
        assert_eq!(tilde("/Users/ab/x", Some("/Users/a")), "/Users/ab/x");
        assert_eq!(tilde("/Users/a", Some("/Users/a")), "~");
        assert_eq!(tilde("/Volumes/x", None), "/Volumes/x");
    }

    #[test]
    fn labels_match_the_cards() {
        assert_eq!(Format::M4a.label(), "M4A");
        assert_eq!(Format::Wav.as_str(), "wav");
        assert_eq!(Tracks::ALL.map(Tracks::as_str), ["mix", "mic", "system"]);
    }
}
