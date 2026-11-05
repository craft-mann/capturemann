//! The words a person sees for the system they are on ("this Mac" or "this computer", "Finder" or
//! "File Explorer", "System Settings" or "Settings"), as plain English.
//!
//! Every sentence that names the system is written once with a placeholder (`{this_computer}`,
//! `{settings_app}`, ...) and the placeholder is filled from this table. The desktop app localises
//! the same table (`apps/desktop/src/platform_text.rs`, tested against this one); the headless
//! crates, which only speak English, use [`english`] directly. [`Platform`] is a parameter, so
//! every variant can be tested on any machine; [`Platform::current`] is the only `target_os` here.
//!
//! The macOS English is the wording the app has always used and tests pin it.

/// The operating system a text is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

impl Platform {
    /// The system this build runs on.
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Mac
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }

    /// All variants, for tables and tests.
    pub const ALL: [Platform; 3] = [Platform::Mac, Platform::Windows, Platform::Linux];
}

/// One system-specific word or short phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Term {
    /// "this Mac" / "this computer" (inside a sentence).
    ThisComputer,
    /// "This Mac" / "This computer" (start of a sentence).
    ThisComputerCap,
    /// "your Mac" / "your computer".
    YourComputer,
    /// "Macs" / "computers": "a good choice on 8 GB Macs".
    Computers,
    /// "System Settings" / "Settings".
    SettingsApp,
    /// The button that opens the settings app.
    OpenSettings,
    /// "menu bar" / "notification area" / "system tray" (no article).
    TrayArea,
    /// "Keychain" / "Credential Manager" / "keyring".
    SecretStore,
    /// "the Trash" / "the Recycle Bin".
    Trash,
    /// "macOS" / "Windows" / "Linux".
    OsName,
    /// The menu entry that shows a file in the file manager.
    RevealLabel,
    /// The button that shows a file in the file manager.
    ShowLabel,
    /// Where the camera switch is.
    PrivacyCamera,
    /// Where the microphone switch is.
    PrivacyMicrophone,
}

impl Term {
    /// Every term, for tables and tests.
    pub const ALL: [Term; 14] = [
        Term::ThisComputer,
        Term::ThisComputerCap,
        Term::YourComputer,
        Term::Computers,
        Term::SettingsApp,
        Term::OpenSettings,
        Term::TrayArea,
        Term::SecretStore,
        Term::Trash,
        Term::OsName,
        Term::RevealLabel,
        Term::ShowLabel,
        Term::PrivacyCamera,
        Term::PrivacyMicrophone,
    ];
}

/// The English text of `term` on `platform`.
pub fn english(platform: Platform, term: Term) -> &'static str {
    use Platform::{Linux, Mac, Windows};
    match (term, platform) {
        (Term::ThisComputer, Mac) => "this Mac",
        (Term::ThisComputer, _) => "this computer",
        (Term::ThisComputerCap, Mac) => "This Mac",
        (Term::ThisComputerCap, _) => "This computer",
        (Term::YourComputer, Mac) => "your Mac",
        (Term::YourComputer, _) => "your computer",
        (Term::Computers, Mac) => "Macs",
        (Term::Computers, _) => "computers",
        (Term::SettingsApp, Mac) => "System Settings",
        (Term::SettingsApp, _) => "Settings",
        (Term::OpenSettings, Mac) => "Open System Settings",
        (Term::OpenSettings, _) => "Open Settings",
        (Term::TrayArea, Mac) => "menu bar",
        (Term::TrayArea, Windows) => "notification area",
        (Term::TrayArea, Linux) => "system tray",
        (Term::SecretStore, Mac) => "Keychain",
        (Term::SecretStore, Windows) => "Credential Manager",
        (Term::SecretStore, Linux) => "keyring",
        (Term::Trash, Windows) => "the Recycle Bin",
        (Term::Trash, _) => "the Trash",
        (Term::OsName, Mac) => "macOS",
        (Term::OsName, Windows) => "Windows",
        (Term::OsName, Linux) => "Linux",
        (Term::RevealLabel, Mac) => "Reveal in Finder",
        (Term::RevealLabel, Windows) => "Show in File Explorer",
        (Term::RevealLabel, Linux) => "Show in folder",
        (Term::ShowLabel, Mac) => "Show in Finder",
        (Term::ShowLabel, Windows) => "Show in File Explorer",
        (Term::ShowLabel, Linux) => "Show in folder",
        (Term::PrivacyCamera, Mac) => "System Settings > Privacy & Security > Camera",
        (Term::PrivacyCamera, Windows) => "Settings > Privacy & security > Camera",
        (Term::PrivacyCamera, Linux) => "your system settings",
        (Term::PrivacyMicrophone, Mac) => "System Settings > Privacy & Security > Microphone",
        (Term::PrivacyMicrophone, Windows) => "Settings > Privacy & security > Microphone",
        (Term::PrivacyMicrophone, Linux) => "your system settings",
    }
}

/// [`english`] for the system this build runs on.
pub fn here(term: Term) -> &'static str {
    english(Platform::current(), term)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full table, one row per term: mac, windows, linux.
    #[test]
    fn the_table() {
        let rows: [(Term, [&str; 3]); 14] = [
            (
                Term::ThisComputer,
                ["this Mac", "this computer", "this computer"],
            ),
            (
                Term::ThisComputerCap,
                ["This Mac", "This computer", "This computer"],
            ),
            (
                Term::YourComputer,
                ["your Mac", "your computer", "your computer"],
            ),
            (Term::Computers, ["Macs", "computers", "computers"]),
            (
                Term::SettingsApp,
                ["System Settings", "Settings", "Settings"],
            ),
            (
                Term::OpenSettings,
                ["Open System Settings", "Open Settings", "Open Settings"],
            ),
            (
                Term::TrayArea,
                ["menu bar", "notification area", "system tray"],
            ),
            (
                Term::SecretStore,
                ["Keychain", "Credential Manager", "keyring"],
            ),
            (Term::Trash, ["the Trash", "the Recycle Bin", "the Trash"]),
            (Term::OsName, ["macOS", "Windows", "Linux"]),
            (
                Term::RevealLabel,
                [
                    "Reveal in Finder",
                    "Show in File Explorer",
                    "Show in folder",
                ],
            ),
            (
                Term::ShowLabel,
                ["Show in Finder", "Show in File Explorer", "Show in folder"],
            ),
            (
                Term::PrivacyCamera,
                [
                    "System Settings > Privacy & Security > Camera",
                    "Settings > Privacy & security > Camera",
                    "your system settings",
                ],
            ),
            (
                Term::PrivacyMicrophone,
                [
                    "System Settings > Privacy & Security > Microphone",
                    "Settings > Privacy & security > Microphone",
                    "your system settings",
                ],
            ),
        ];
        for (term, want) in rows {
            for (p, w) in Platform::ALL.iter().zip(want) {
                assert_eq!(english(*p, term), w, "{term:?} on {p:?}");
            }
        }
        assert_eq!(rows.len(), Term::ALL.len());
    }

    #[test]
    fn windows_and_linux_never_name_apple_things() {
        for term in Term::ALL {
            for p in [Platform::Windows, Platform::Linux] {
                let s = english(p, term);
                for bad in [
                    "Mac",
                    "Finder",
                    "Keychain",
                    "menu bar",
                    "System Settings",
                    "Homebrew",
                ] {
                    assert!(!s.contains(bad), "{term:?} on {p:?}: {s}");
                }
            }
        }
    }
}
