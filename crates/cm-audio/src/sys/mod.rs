//! Platform bindings. All `unsafe` in this crate lives under this module, each block with a SAFETY
//! comment.

#[cfg(target_os = "macos")]
pub mod mac_permission;
#[cfg(target_os = "macos")]
pub mod mac_sck;

pub mod linux_pipewire;
