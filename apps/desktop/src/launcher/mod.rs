//! The command launcher (LNC-1): a keyboard-first list of everything CaptureMann can do.
//!
//! Everything here is display-free and unit tested; the GPUI window lives in
//! `ui::launcher_window` and stays a thin view over [`model::LauncherModel`].

pub mod commands;
pub mod focus;
pub mod mcp_config;
pub mod model;
pub mod plan;
pub mod search;
