//! The system's own words in the UI ("this Mac" or "this computer", "System Settings" or
//! "Settings", "Finder" or "File Explorer"), translated.
//!
//! The English table is `cm_platform::vocab` (shared with the headless crates); this file gives each
//! entry a `tr_ctx!` so a language can translate every word once and the sentences that use them
//! stay one message with a placeholder (`{this_computer}`, `{settings_app}`, ...). A test keeps the
//! two English tables identical. [`Platform`] is a parameter so every variant is testable anywhere.
//!
//! Sentences about an idea that exists on one system only (macOS' "remove the entry with the minus
//! button", "restart after granting Screen Recording") are chosen by platform where they are shown,
//! never written with a placeholder.

use cm_i18n::{tr, tr_ctx};
pub use cm_platform::vocab::{Platform, Term};

/// The translated text of `term` on `platform`.
pub fn term(platform: Platform, term: Term) -> String {
    use Platform::{Linux, Mac, Windows};
    match (term, platform) {
        (Term::ThisComputer, Mac) => tr_ctx!("platform word", "this Mac"),
        (Term::ThisComputer, Windows | Linux) => tr_ctx!("platform word", "this computer"),
        (Term::ThisComputerCap, Mac) => tr_ctx!("platform word", "This Mac"),
        (Term::ThisComputerCap, Windows | Linux) => tr_ctx!("platform word", "This computer"),
        (Term::YourComputer, Mac) => tr_ctx!("platform word", "your Mac"),
        (Term::YourComputer, Windows | Linux) => tr_ctx!("platform word", "your computer"),
        (Term::Computers, Mac) => tr_ctx!("platform word", "Macs"),
        (Term::Computers, Windows | Linux) => tr_ctx!("platform word", "computers"),
        (Term::SettingsApp, Mac) => tr_ctx!("platform word", "System Settings"),
        (Term::SettingsApp, Windows | Linux) => tr_ctx!("platform word", "Settings"),
        (Term::OpenSettings, Mac) => tr!("Open System Settings"),
        (Term::OpenSettings, Windows | Linux) => tr!("Open Settings"),
        (Term::TrayArea, Mac) => tr_ctx!("platform word", "menu bar"),
        (Term::TrayArea, Windows) => tr_ctx!("platform word", "notification area"),
        (Term::TrayArea, Linux) => tr_ctx!("platform word", "system tray"),
        (Term::SecretStore, Mac) => tr_ctx!("platform word", "Keychain"),
        (Term::SecretStore, Windows) => tr_ctx!("platform word", "Credential Manager"),
        (Term::SecretStore, Linux) => tr_ctx!("platform word", "keyring"),
        (Term::Trash, Mac | Linux) => tr_ctx!("platform word", "the Trash"),
        (Term::Trash, Windows) => tr_ctx!("platform word", "the Recycle Bin"),
        (Term::OsName, Mac) => tr_ctx!("platform word", "macOS"),
        (Term::OsName, Windows) => tr_ctx!("platform word", "Windows"),
        (Term::OsName, Linux) => tr_ctx!("platform word", "Linux"),
        (Term::RevealLabel, Mac) => tr!("Reveal in Finder"),
        (Term::RevealLabel, Windows) => tr!("Show in File Explorer"),
        (Term::RevealLabel, Linux) => tr!("Show in folder"),
        (Term::ShowLabel, Mac) => tr_ctx!("button", "Show in Finder"),
        (Term::ShowLabel, Windows) => tr_ctx!("button", "Show in File Explorer"),
        (Term::ShowLabel, Linux) => tr_ctx!("button", "Show in folder"),
        (Term::PrivacyCamera, Mac) => {
            tr_ctx!(
                "platform word",
                "System Settings > Privacy & Security > Camera"
            )
        }
        (Term::PrivacyCamera, Windows) => {
            tr_ctx!("platform word", "Settings > Privacy & security > Camera")
        }
        (Term::PrivacyCamera, Linux) => tr_ctx!("platform word", "your system settings"),
        (Term::PrivacyMicrophone, Mac) => {
            tr_ctx!(
                "platform word",
                "System Settings > Privacy & Security > Microphone"
            )
        }
        (Term::PrivacyMicrophone, Windows) => {
            tr_ctx!(
                "platform word",
                "Settings > Privacy & security > Microphone"
            )
        }
        (Term::PrivacyMicrophone, Linux) => tr_ctx!("platform word", "your system settings"),
    }
}

/// [`term`] for the system this build runs on.
pub fn here(t: Term) -> String {
    term(Platform::current(), t)
}

/// "this Mac" / "this computer".
pub fn this_computer() -> String {
    here(Term::ThisComputer)
}

/// "System Settings" / "Settings".
pub fn settings_app() -> String {
    here(Term::SettingsApp)
}

/// A word the daemon wrote in English (a parameter read back out of its message) in the current
/// language, when it is one of the vocabulary's entries on any system; anything else comes back as
/// it is.
pub fn translate_known(english: &str) -> String {
    for p in Platform::ALL {
        for t in Term::ALL {
            if cm_platform::vocab::english(p, t) == english {
                return term(p, t);
            }
        }
    }
    english.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_daemon_parameter_that_is_a_vocabulary_entry_is_translated_and_others_are_kept() {
        assert_eq!(translate_known("macOS"), "macOS");
        assert_eq!(
            translate_known("Settings > Privacy & security > Camera"),
            "Settings > Privacy & security > Camera"
        );
        assert_eq!(translate_known("delete"), "delete");
    }

    /// With no language set the UI is English, so this is the English table of `cm_platform`.
    #[test]
    fn the_translated_table_has_the_english_of_the_shared_table() {
        for p in Platform::ALL {
            for t in Term::ALL {
                assert_eq!(
                    term(p, t),
                    cm_platform::vocab::english(p, t),
                    "{t:?} on {p:?}"
                );
            }
        }
    }

    /// Messages that name a system on purpose, with the reason. Everything else in the catalogue
    /// that names macOS things must go through the vocabulary (`{this_computer}`, `{settings_app}`,
    /// `{trash}`, ...), so a new "this Mac" in a sentence fails here.
    const MACOS_ONLY_BY_DESIGN: &[(&str, &str)] = &[
        ("Spotlight", "the annotation tool of that name (ANN-7), not the macOS search"),
        ("Another copy ({name}) is running", "macOS-only code path (cfg(target_os = \"macos\") in main.rs: osascript dialog about a second app bundle)"),
        ("CaptureMann isn't allowed to read these files. Allow it in System Settings", "the macOS branch; the other systems have their own message"),
        ("Deleting remote copies is permanent", "Google Drive's own Trash, not the system's"),
        ("FFmpeg at `{path}` cannot be run (permission denied). FFmpeg was not found: install it with Homebrew", "the macOS and Linux wording; Windows has ffmpeg_*_windows templates"),
        ("FFmpeg isn’t installed, so the combined video", "the macOS and Linux branch of export_skipped_body(windows)"),
        ("FFmpeg was not found: install it with Homebrew", "the macOS and Linux template (ffmpeg_not_found); Windows has ffmpeg_not_found_windows"),
        ("For online calls: the other people’s voices. Needs Screen Recording permission on macOS", "the Mac branch of system_hint(platform)"),
        ("Hold a modifier key (Control, Option or Shift, Command or Windows) too.", "lists the keys of every system"),
        ("If the switch for CaptureMann already looks on in System Settings", "macOS TCC advice; shown only for Input Monitoring, which exists on the Mac only"),
        ("Open System Settings", "the Mac branch of Term::OpenSettings; permissions.rs picks Open Settings elsewhere"),
        ("Renders one video", "brew branch of auto_export_hint(windows)"),
        ("Reveal in Finder", "the Mac branch of Term::RevealLabel"),
        ("Screen Recording access is off. Turn it on in System Settings", "macOS only: Windows and Linux have no Screen Recording switch, the daemon never raises it there"),
        ("Screen Recording permission is not granted. Enable it in System Settings", "macOS only, as above"),
        ("Show in Finder", "the Mac branch of Term::ShowLabel"),
        ("System audio needs Screen Recording permission on macOS.", "shown only when the Screen Recording permission is blocked, which is macOS only"),
        ("The recording is saved, but the single MP4 needs FFmpeg. Install it with Homebrew", "the macOS and Linux branch of ffmpeg_missing_body(windows)"),
        ("This computer has no usable hardware H.264 encoder for this recording. Try a lower resolution, update your operating system", "the macOS id lists the minimum of every system; Windows and Linux have their own ids"),
        ("Waits this long before a screenshot starts, from the dock", "the dock of any desktop"),
        ("macOS keeps these under your control", "the Mac branch of intro_note(os)"),
        ("macOS shows its own dialog only once", "Screen Recording card, macOS only"),
        ("macOS usually keeps refusing capture", "Screen Recording card, macOS only"),
        ("macOS usually needs the app restarted", "Screen Recording banner, macOS only"),
    ];

    /// Regression guard: no message outside the vocabulary names a macOS-only thing unless it is on
    /// the list above. `platform word` messages are the vocabulary itself.
    #[test]
    fn macos_words_stay_behind_the_vocabulary() {
        let pot = include_str!("../../../locales/capturemann.pot");
        let words = [
            "this Mac",
            "your Mac",
            "Macs",
            "macOS",
            "Finder",
            "Keychain",
            "menu bar",
            "System Settings",
            "Spotlight",
            "Homebrew",
            "the Trash",
            "Apple",
        ];
        let mut bad = Vec::new();
        for block in pot.split("\n\n") {
            if block.contains("msgctxt \"platform word\"") {
                continue;
            }
            let Some(at) = block.find("msgid ") else {
                continue;
            };
            let id: String = block[at..]
                .lines()
                .take_while(|l| !l.starts_with("msgstr") && !l.starts_with("msgid_plural"))
                .map(|l| l.trim_start_matches("msgid ").trim().trim_matches('"'))
                .collect();
            if words.iter().any(|w| id.contains(w))
                && !MACOS_ONLY_BY_DESIGN.iter().any(|(p, _)| id.starts_with(p))
            {
                bad.push(id);
            }
        }
        assert!(
            bad.is_empty(),
            "name the system through platform_text: {bad:#?}"
        );
        for (p, why) in MACOS_ONLY_BY_DESIGN {
            assert!(pot.contains(p), "stale allow-list entry {p:?} ({why})");
        }
    }

    /// On Windows and Linux the vocabulary has no macOS word, so a sentence filled from it has none.
    #[test]
    fn other_systems_speak_their_own_words() {
        for p in [Platform::Windows, Platform::Linux] {
            for t in Term::ALL {
                let s = term(p, t);
                for w in [
                    "Mac",
                    "Finder",
                    "Keychain",
                    "menu bar",
                    "System Settings",
                    "Trash",
                ] {
                    if t == Term::Trash && p == Platform::Linux {
                        continue; // the freedesktop.org Trash is called Trash
                    }
                    assert!(!s.contains(w), "{t:?} on {p:?}: {s}");
                }
            }
        }
        assert_eq!(term(Platform::Windows, Term::Trash), "the Recycle Bin");
        assert_eq!(term(Platform::Windows, Term::TrayArea), "notification area");
        assert_eq!(
            term(Platform::Windows, Term::SecretStore),
            "Credential Manager"
        );
    }
}
