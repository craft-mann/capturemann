//! The camera privacy switches of Windows, read from the registry (never prompts, opens no device):
//! `CapabilityAccessManager\ConsentStore\webcam` for the device (HKLM), the user (HKCU) and desktop
//! apps (`NonPackaged`), and the group policy `LetAppsAccessCamera`.

use windows::core::HSTRING;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

use crate::convert::windows_camera_state;
use crate::permission::CameraPermission;

const STORE: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\webcam";
const POLICY: &str = r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy";

fn read_string(root: HKEY, key: &str, name: &str) -> Option<String> {
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut bytes = 0u32;
    // SAFETY: both names are NUL-terminated wide strings; a null data pointer only asks for the
    // size, which is written to `bytes`.
    let size = unsafe {
        RegGetValueW(
            root,
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
            root,
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

fn read_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut value = 0u32;
    let mut bytes = std::mem::size_of::<u32>() as u32;
    // SAFETY: `value` is a writable u32 and `bytes` says so; the names are NUL-terminated.
    let got = unsafe {
        RegGetValueW(
            root,
            &key,
            &name,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut bytes),
        )
    };
    (got == ERROR_SUCCESS).then_some(value)
}

/// Current camera permission without prompting (Windows never asks a desktop app).
pub fn permission_status() -> CameraPermission {
    let machine = read_string(HKEY_LOCAL_MACHINE, STORE, "Value");
    let user = read_string(HKEY_CURRENT_USER, STORE, "Value");
    let desktop = read_string(HKEY_CURRENT_USER, &format!(r"{STORE}\NonPackaged"), "Value");
    let policy = read_dword(HKEY_LOCAL_MACHINE, POLICY, "LetAppsAccessCamera");
    windows_camera_state(
        policy,
        machine.as_deref(),
        user.as_deref(),
        desktop.as_deref(),
    )
}
