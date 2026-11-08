//! Platform bindings of the cursor track that are not Windows or macOS (those keep their own
//! files). Linux: X11 through XInput2 raw events and XFixes; there is no Wayland backend because
//! Wayland gives applications no global pointer position (see `linux_x11.rs`).

pub mod x11_convert;

#[cfg(target_os = "linux")]
pub mod linux_x11;
