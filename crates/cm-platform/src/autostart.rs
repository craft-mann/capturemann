//! Start-at-login entries that live in a registry key (Windows: the per-user `Run` key). macOS uses a
//! LaunchAgent file, which the desktop app writes itself; elsewhere nothing is available yet and
//! these functions report no entry.

/// The per-user key Windows reads at sign-in.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The command stored under `name` in the registry key `key` (`HKCU\<key>`).
pub fn get(key: &str, name: &str) -> Option<String> {
    sys::get(key, name)
}

/// Stores `command` under `name`, creating the key when needed.
pub fn set(key: &str, name: &str, command: &str) -> Result<(), String> {
    sys::set(key, name, command)
}

/// Removes `name`; a value that is not there is not an error.
pub fn remove(key: &str, name: &str) -> Result<(), String> {
    sys::remove(key, name)
}

#[cfg(windows)]
mod sys {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    pub fn get(key: &str, name: &str) -> Option<String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let mut bytes = 0u32;
        // SAFETY: both names are NUL-terminated wide strings; a null data pointer only asks for
        // the size, which is written to `bytes`.
        let size = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut bytes),
            )
        };
        if size != ERROR_SUCCESS || bytes == 0 {
            return None;
        }
        let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
        // SAFETY: `buf` holds `bytes` bytes, as the first call said; the length is updated.
        let got = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        };
        if got != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    fn check(what: &str, status: WIN32_ERROR) -> Result<(), String> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("{what}: {}", windows::core::Error::from(status)))
        }
    }

    pub fn set(key: &str, name: &str, command: &str) -> Result<(), String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = u32::try_from(wide.len() * 2).map_err(|_| "the command is too long")?;
        // SAFETY: `wide` is NUL-terminated and `bytes` is its size in bytes; the key and value
        // names are NUL-terminated wide strings.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                REG_SZ.0,
                Some(wide.as_ptr().cast()),
                bytes,
            )
        };
        check("cannot write the start-at-login entry", status)
    }

    pub fn remove(key: &str, name: &str) -> Result<(), String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        // SAFETY: both names are NUL-terminated wide strings.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &key, &name) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check("cannot remove the start-at-login entry", status)
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn get(_key: &str, _name: &str) -> Option<String> {
        None
    }

    pub fn set(_key: &str, _name: &str, _command: &str) -> Result<(), String> {
        Err("this system keeps start-at-login entries elsewhere".to_owned())
    }

    pub fn remove(_key: &str, _name: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(all(test, windows))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Uses a key of its own, never the real `Run` key: nothing here can start at sign-in.
    #[test]
    fn a_value_is_written_read_replaced_and_removed() {
        let key = format!(r"Software\CaptureMann-test\{}", std::process::id());
        assert_eq!(get(&key, "App"), None);
        set(
            &key,
            "App",
            r#""C:\Program Files\CaptureMann\cm-desktop.exe" --background"#,
        )
        .unwrap();
        assert_eq!(
            get(&key, "App").as_deref(),
            Some(r#""C:\Program Files\CaptureMann\cm-desktop.exe" --background"#)
        );
        set(&key, "App", "ünï").unwrap();
        assert_eq!(get(&key, "App").as_deref(), Some("ünï"));
        remove(&key, "App").unwrap();
        remove(&key, "App").unwrap();
        assert_eq!(get(&key, "App"), None);
        // Leave no key of ours behind.
        // SAFETY: the key name is a NUL-terminated wide string.
        let _ = unsafe {
            windows::Win32::System::Registry::RegDeleteTreeW(
                windows::Win32::System::Registry::HKEY_CURRENT_USER,
                &windows::core::HSTRING::from(key.as_str()),
            )
        };
    }
}
