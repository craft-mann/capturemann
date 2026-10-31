//! Moving a file or folder to the system Trash, so the user can get it back.

use std::path::Path;

/// Moves `path` to the Trash: macOS `NSFileManager`, the Windows Recycle Bin, the freedesktop.org
/// Trash on Linux. The error text is for the user's log; nothing is deleted when it fails. Other
/// systems have no Trash here and always fail.
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    sys::move_to_trash(path)
}

#[cfg(target_os = "macos")]
mod sys {
    use std::path::Path;

    use objc2_foundation::{NSFileManager, NSString, NSURL};

    pub fn move_to_trash(path: &Path) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or_else(|| "the path is not valid UTF-8".to_owned())?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|e| e.localizedDescription().to_string())
    }
}

#[cfg(any(windows, target_os = "linux"))]
mod sys {
    use std::path::Path;

    pub fn move_to_trash(path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| e.to_string())
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod sys {
    use std::path::Path;

    pub fn move_to_trash(_path: &Path) -> Result<(), String> {
        Err("this system has no Trash".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Puts a small folder in the real Trash of whoever runs it, so it is ignored by default (CI
    /// runs it with `--ignored`).
    #[test]
    #[ignore = "moves a scratch folder to the real Trash"]
    fn a_folder_goes_to_the_trash_and_leaves_its_place() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bundle = dir
            .path()
            .join(format!("cm-trash-test-{}.cmshot", std::process::id()));
        std::fs::create_dir(&bundle).expect("folder");
        std::fs::write(bundle.join("manifest.json"), b"{}").expect("file");
        move_to_trash(&bundle).expect("moved to the Trash");
        assert!(!bundle.exists(), "the folder must be gone from its place");
    }

    #[test]
    fn a_missing_path_is_an_error_and_touches_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("nothing-here");
        assert!(move_to_trash(&missing).is_err());
    }
}
