//! OS-specific primitives shared by the headless crates (platform plan, Step 0).
//!
//! `target_os` code belongs in modules like these, in a crate's `sys/` folder or in an app's
//! platform module (a CI check enforces it); everything else is written once for all systems.
//! This crate has no dependency on another CaptureMann crate, so `cm-core` can use it.

pub mod autostart;
pub mod capability;
pub mod clock;
pub mod dirs;
pub mod memory;
pub mod opener;
pub mod priority;
pub mod process;
pub mod sleep;
#[cfg(target_os = "linux")]
pub mod sleep_linux;
pub mod stack;
pub mod sysinfo;
pub mod trash;
pub mod vocab;
