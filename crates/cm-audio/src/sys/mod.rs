//! Platform bindings. All `unsafe` in this crate lives under this module, each block with a SAFETY
//! comment.

#[cfg(target_os = "macos")]
pub mod mac_permission;
#[cfg(target_os = "macos")]
pub mod mac_sck;
#[cfg(windows)]
pub mod win_consent;
#[cfg(windows)]
pub mod win_process_loopback;

pub mod linux_pipewire;
