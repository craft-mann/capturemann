//! OS-specific primitives shared by the headless crates (platform plan, Step 0).
//!
//! `target_os` code belongs in modules like these, in a crate's `sys/` folder or in an app's
//! platform module (a CI check enforces it); everything else is written once for all systems.
//! This crate has no dependency on another CaptureMann crate, so `cm-core` can use it.

pub mod clock;
