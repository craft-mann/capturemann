//! CaptureMann desktop app: library, settings and (next slice) editor, as a thin shell over
//! the embedded `cm-daemon` (AGT-1).
//!
//! Everything except `ui` and `platform` is display-free and unit tested; the GPUI rendering
//! is verified by building and running the app.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod agent_status;
pub mod ai;
pub mod alloc;
pub mod backend;
pub mod clipboard;
pub mod cloud_connect;
pub mod confirm;
pub mod daemon_text;
pub mod delay;
pub mod delete_flow;
pub mod destinations;
pub mod editor;
pub mod flows;
#[cfg(test)]
mod i18n_events_tests;
#[cfg(test)]
mod i18n_net_tests;
pub mod instance;
pub mod launcher;
#[cfg(test)]
mod layout_width_tests;
pub mod library;
pub mod library_move;
pub mod login_item;
pub mod nav;
pub mod overlay;
pub mod permissions;
pub mod platform;
pub mod platform_clipboard;
pub mod platform_hotkeys;
pub mod platform_linux;
pub mod platform_linux_menu;
pub mod platform_window;
pub mod poster;
pub mod preview;
pub mod publish;
pub mod publish_connect;
pub mod published;
pub mod recedit;
pub mod recording;
pub mod remote_text;
pub mod save_folder;
pub mod settings;
pub mod shortcuts;
pub mod studio_window;
pub mod text_buffer;
#[cfg(test)]
mod text_metrics_tests;
pub mod theme;
pub mod toast;
pub mod trace;
pub mod ui;
pub mod window_picker;
