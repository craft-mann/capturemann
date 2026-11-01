//! The few OS-specific parts of screen capture: what the session is and whether the OS lets this
//! process capture. (`target_os` belongs here, see `scripts/check_target_os.py`.)

// Used by the xcap window list off macOS; macOS has its own bounds (tests build it everywhere).
#[cfg(any(not(target_os = "macos"), test))]
pub mod dwm_frame;
pub mod linux_recorder;
pub mod mutter;
pub mod permission;
pub mod pipewire_linux;
pub mod portal;
pub mod session;
pub mod x11_names;
