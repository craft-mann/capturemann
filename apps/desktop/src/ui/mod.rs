//! GPUI rendering. Verified by building and running; the logic it shows lives in the
//! display-free modules of this crate.

pub mod ai;
pub mod ai_view;
pub mod ai_voice;
pub mod app;
pub mod audio_cache;
pub mod brand;
pub mod components;
pub mod confirm_window;
pub mod csd;
pub mod delay_pill;
pub mod editor_doc;
pub mod editor_view;
pub mod export_ui;
pub mod kit;
pub mod launcher_window;
pub mod library_move_flow;
pub mod library_view;
pub mod pages;
pub mod paint;
pub mod permissions_view;
pub mod publish_sheet;
pub mod published_view;
pub mod recording;
pub mod recording_editor;
pub mod report_offer;
pub mod scroll;
pub mod settings;
pub mod settings_view;
pub mod sidebar;
pub mod slider;
pub mod studio;
pub mod text_field;
pub mod thumbs;
pub mod toast_window;
pub mod tooltip;
pub mod upload_ui;
pub mod window_picker;

#[cfg(test)]
mod icon_tests;
