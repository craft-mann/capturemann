//! The longest library folder path the app works with, checked before anything opens it (the
//! library, the headless daemon and the CLI, which must answer at once instead of waiting for a
//! daemon that cannot start).

use std::path::Path;

use crate::CmError;

/// Longest whole path the library may work with: the classic Windows `MAX_PATH` (260), or the
/// usual Unix `PATH_MAX` (1024). The path separator tells the two apart without a `cfg`.
pub const MAX_PATH_CHARS: usize = if std::path::MAIN_SEPARATOR == '\\' {
    260
} else {
    1024
};
/// Longest single folder or file name (NTFS, APFS and ext4 agree on 255).
pub const MAX_NAME_CHARS: usize = 255;
/// Room kept below the limit for what lives inside the library folder: `.capturemann/` and its
/// index, and a bundle folder with its files (`<id>.cmshot/original.png`, `tracks/screen.mp4`).
pub const RESERVED_BELOW_ROOT: usize = 80;

/// Fails fast, with a message the user can act on, for a library folder whose path is too long
/// for the system to open files below it. Without this a path past the limit made every open and
/// list under it fail or stall, and the app sat on "Loading library...". The message names the
/// path and the limit.
pub fn check_root_path_len(root: &Path) -> Result<(), CmError> {
    let s = root.to_string_lossy();
    let total = s.chars().count();
    let limit = MAX_PATH_CHARS - RESERVED_BELOW_ROOT;
    let longest_name = root
        .components()
        .map(|c| c.as_os_str().to_string_lossy().chars().count())
        .max()
        .unwrap_or(0);
    if total > limit || longest_name > MAX_NAME_CHARS {
        return Err(CmError::invalid_input(format!(
            "The library folder's path is too long ({total} characters; at most {limit} work, \
             and no folder name may be longer than {MAX_NAME_CHARS}): choose a shorter path. \
             The path is `{s}`"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_past_the_limit_is_invalid_input_and_names_path_and_limit() {
        let long = format!("/tmp/{}", "ab/".repeat(MAX_PATH_CHARS));
        let e = check_root_path_len(Path::new(&long)).unwrap_err();
        assert_eq!(e.code(), crate::ErrorCode::InvalidInput);
        let m = e.to_string();
        assert!(m.contains(&long), "{m}");
        assert!(
            m.contains(&(MAX_PATH_CHARS - RESERVED_BELOW_ROOT).to_string()),
            "{m}"
        );
        assert!(check_root_path_len(Path::new("/tmp/CaptureMann")).is_ok());
    }
}
