//! Settings -> Capture & recording and Webcam & audio (REC-2, REC-3, REC-4, CAM-1, AUD-3), as kit
//! settings rows. Every control writes the key the engine reads (see `recording::start` for the
//! list), so the sheet, the hotkey and agents record alike. The excluded-apps list (REC-6) is
//! edited from Privacy: its rows are in `privacy.rs`, the add and remove actions are here.

use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Context};
use serde_json::{json, Value};

use super::rows::{card_group, choice_row, plain_row, seconds_row, tile, toggle_row};
use super::widgets::{caption, group_with_card, segment_with_icon, segmented};
use super::SettingsView;
use crate::recording::geometry::{BubbleShape, BubbleSize};
use crate::recording::start::{
    self as keys, codec_label, default_exclusions, quality_label, size_label, validate_bundle_id,
    RecordingSettings, CODECS, QUALITIES, SIZES,
};
use crate::theme::Theme;
use crate::ui::app;

/// The options of a choice row with their labels in the current language (the value is the
/// stored setting and never changes). `label` is `size_label`, `codec_label` or `quality_label`
/// of the recording start module, so the Settings page and the Record sheet share one message
/// per word.
fn options_of(
    list: &[(&'static str, &'static str)],
    label: fn(&str) -> String,
) -> Vec<(&'static str, String)> {
    list.iter()
        .map(|(value, _english)| (*value, label(value)))
        .collect()
}

fn bubble_shape_label(s: BubbleShape) -> String {
    match s {
        BubbleShape::Circle => tr_ctx!("bubble shape", "Circle"),
        BubbleShape::Rounded => tr_ctx!("bubble shape", "Rounded"),
    }
}

/// A capture delay on the toolbar: "No delay" or "5 s".
fn delay_label(secs: u32) -> String {
    if secs == 0 {
        tr!("No delay")
    } else {
        format!("{secs} s")
    }
}

impl SettingsView {
    pub(super) fn recording_settings(&self, cx: &Context<Self>) -> RecordingSettings {
        RecordingSettings::from_settings(&self.app.read(cx).backend.settings.snapshot())
    }

    fn write(&self, key: &'static str, value: Value, cx: &mut gpui::App) {
        app::set_setting(&self.app, key, value, cx);
    }

    /// Capture & recording: what `start_recording` uses when a request does not say.
    pub(super) fn capture(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.recording_settings(cx);
        let auto_export = crate::recording::exports::auto_export_on(
            &self.app.read(cx).backend.settings.snapshot(),
        );
        let fps_note = (s.fps == 60).then(|| {
            div().px(px(4.)).child(caption(
                t,
                tr!("60 fps is accepted. The on-device 60 fps self-test isn\u{2019}t built yet, so this computer hasn\u{2019}t been checked for it; if frames drop, the HUD shows a warning."),
            ))
        });
        let fps_segs = [30u32, 60].into_iter().map(|n| {
            let app = self.app.clone();
            segment_with_icon(t, format!("fps-{n}"), &format!("{n} fps"), s.fps == n, None)
                .on_click(move |_, _, cx| app::set_setting(&app, keys::KEY_FPS, json!(n), cx))
        });
        // CAP-5: the capture delay, shared with the tray submenu and remembered. It lived in the
        // library header, which the capture dock replaced (OD-20).
        let delay_now = self.app.read(cx).capture_delay_s;
        let delay_choices = crate::delay::CHOICES.into_iter().map(|secs| {
            let app = self.app.clone();
            segment_with_icon(
                t,
                ("cap-delay", secs),
                &delay_label(secs),
                delay_now == secs,
                None,
            )
            .on_click(move |_, _, cx| app::set_capture_delay(&app, secs, cx))
        });
        let a = &self.app;
        let sizes = options_of(&SIZES, size_label);
        let codecs = options_of(&CODECS, codec_label);
        let qualities = options_of(&QUALITIES, quality_label);
        // TODO(design): the board's banner ("Native on your display is W x H. At these settings a
        // 10-minute recording is about N GB.") needs a display lookup and a size model; not built.
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(
                group_with_card(
                    t,
                    &tr_ctx!("settings group", "Video"),
                    None,
                    card_group(t, "recording-video-settings", &tr!("Video settings"))
                        .child(choice_row(
                            t,
                            a,
                            "rec-size",
                            tile::RESOLUTION,
                            &tr!("Resolution"),
                            &tr!("Native records at the display\u{2019}s own pixels, with no cap. A smaller size scales down."),
                            keys::KEY_SIZE,
                            &sizes,
                            &s.size,
                            false,
                        ))
                        .child(plain_row(
                            t,
                            tile::FPS,
                            &tr!("Frame rate"),
                            &tr!("30 fps suits most screens; 60 fps is smoother for motion."),
                            Some(
                                segmented(t, "rec-fps", &tr!("Frame rate"))
                                    .children(fps_segs)
                                    .into_any_element(),
                            ),
                            false,
                        ))
                        .child(choice_row(
                            t,
                            a,
                            "rec-codec",
                            tile::CODEC,
                            &tr!("Codec"),
                            &tr!("Auto picks HEVC when this computer\u{2019}s encoder supports it, else H.264."),
                            keys::KEY_CODEC,
                            &codecs,
                            &s.codec,
                            false,
                        ))
                        .child(choice_row(
                            t,
                            a,
                            "rec-quality",
                            tile::QUALITY,
                            &tr!("Quality"),
                            &tr!("Higher quality makes bigger files."),
                            keys::KEY_QUALITY,
                            &qualities,
                            &s.quality,
                            true,
                        )),
                )
                .children(fps_note),
            )
            .child(group_with_card(
                t,
                &tr_ctx!("settings group", "Recording"),
                None,
                card_group(t, "recording-behavior-settings", &tr!("Recording behavior"))
                    .child(toggle_row(
                        t,
                        a,
                        "rec-cursor",
                        tile::CURSOR,
                        &tr!("Record the cursor"),
                        &tr!("Cursor position and clicks are saved with the recording, so the editor can draw and zoom on them."),
                        keys::KEY_CURSOR,
                        s.cursor,
                        false,
                    ))
                    .child(toggle_row(
                        t,
                        a,
                        "rec-auto-export",
                        tile::AUTO_EXPORT,
                        &tr!("Create a single MP4 after recording"),
                        &tr!("Renders one video with the screen, the camera bubble and your audio into the recording\u{2019}s exports folder. The original tracks are never changed. Needs FFmpeg (brew install ffmpeg)."),
                        crate::recording::exports::KEY_AUTO_EXPORT,
                        auto_export,
                        true,
                    )),
            ))
            .child(group_with_card(
                t,
                &tr_ctx!("settings group", "Countdown"),
                None,
                card_group(t, "recording-countdown-settings", &tr!("Countdown settings"))
                    .child(seconds_row(
                        t,
                        a,
                        "rec-countdown",
                        tile::COUNTDOWN,
                        &tr!("Countdown before recording"),
                        &tr!("Space skips it, Esc cancels it."),
                        keys::KEY_COUNTDOWN,
                        s.countdown_s,
                        false,
                    ))
                    .child(seconds_row(
                        t,
                        a,
                        "rec-resume-countdown",
                        tile::COUNTDOWN,
                        &tr!("Countdown before resuming"),
                        &tr!("After a pause."),
                        keys::KEY_RESUME_COUNTDOWN,
                        s.resume_countdown_s,
                        false,
                    ))
                    .child(plain_row(
                        t,
                        tile::SCREENSHOT_DELAY,
                        &tr!("Before a screenshot"),
                        &tr!("Waits this long before a screenshot starts, from the dock, a shortcut, the launcher or the tray. Esc cancels the wait."),
                        Some(
                            segmented(t, "cap-delay-choices", &tr!("Before a screenshot"))
                                .children(delay_choices)
                                .into_any_element(),
                        ),
                        true,
                    )),
            ))
            .child(caption(
                t,
                tr!("These are the defaults for every recording, including ones an agent starts. A request can override them for one recording."),
            ))
            .into_any_element()
    }

    /// Webcam & audio.
    pub(super) fn webcam(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.app.read(cx).backend.settings.snapshot();
        let defaults = crate::recording::start::SheetDefaults::from_settings(&snap);
        let s = RecordingSettings::from_settings(&snap);
        let b = defaults.bubble;
        let remember_position = snap
            .get(keys::KEY_BUBBLE_REMEMBER)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let app = self.app.clone();
        let shapes = [BubbleShape::Circle, BubbleShape::Rounded]
            .into_iter()
            .map(|sh| {
                let app = app.clone();
                let glyph = match sh {
                    BubbleShape::Circle => "circle",
                    BubbleShape::Rounded => "rectangle-horizontal",
                };
                segment_with_icon(
                    t,
                    format!("bubble-shape-{}", sh.key()),
                    &bubble_shape_label(sh),
                    b.shape == sh,
                    Some(glyph),
                )
                .on_click(move |_, _, cx| {
                    app::set_setting(&app, keys::KEY_BUBBLE_SHAPE, json!(sh.key()), cx)
                })
            });
        let sizes = BubbleSize::ALL.into_iter().map(|sz| {
            let app = app.clone();
            segment_with_icon(
                t,
                format!("bubble-size-{}", sz.key()),
                &format!("{} \u{00B7} {} px", sz.key().to_uppercase(), sz.px() as u32),
                b.size == sz,
                None,
            )
            .on_click(move |_, _, cx| {
                app::set_setting(&app, keys::KEY_BUBBLE_SIZE, json!(sz.key()), cx)
            })
        });
        // TODO(design): the board's preview card and its Camera and Microphone dropdowns are not
        // built (no preview feature; devices are chosen in the Record sheet).
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(group_with_card(
                t,
                &tr!("Webcam bubble"),
                None,
                card_group(t, "webcam-bubble-settings", &tr!("Webcam bubble settings"))
                    .child(plain_row(
                        t,
                        tile::BUBBLE_SHAPE,
                        &tr_ctx!("bubble", "Shape"),
                        &tr!("A circle, or a 4:3 rounded rectangle. The ring is always the record color."),
                        Some(
                            segmented(t, "bubble-shape", &tr!("Bubble shape"))
                                .children(shapes)
                                .into_any_element(),
                        ),
                        false,
                    ))
                    .child(plain_row(
                        t,
                        tile::BUBBLE_SIZE,
                        &tr_ctx!("bubble", "Size"),
                        &tr!("You can change it while recording from the bubble\u{2019}s hover toolbar."),
                        Some(
                            segmented(t, "bubble-size", &tr!("Bubble size"))
                                .children(sizes)
                                .into_any_element(),
                        ),
                        false,
                    ))
                    .child(toggle_row(
                        t,
                        &app,
                        "bubble-mirror",
                        tile::MIRROR,
                        &tr!("Mirror the preview"),
                        &tr!("Flips what you see, like a mirror. The recorded camera track is never flipped."),
                        keys::KEY_BUBBLE_MIRROR,
                        b.mirror,
                        false,
                    ))
                    .child(toggle_row(
                        t,
                        &app,
                        "bubble-remember-position",
                        tile::POSITION,
                        &tr!("Remember where I put the bubble"),
                        &tr!("Off: every recording starts with the bubble in the bottom-left corner. On: it starts where you last left it."),
                        keys::KEY_BUBBLE_REMEMBER,
                        remember_position,
                        false,
                    ))
                    .child(plain_row(
                        t,
                        tile::POSITION,
                        &tr_ctx!("bubble", "Position"),
                        &tr!("Drag the bubble anywhere, before or during a recording. It snaps to a corner or edge midpoint when you let go near one."),
                        None,
                        true,
                    )),
            ))
            .child(group_with_card(
                t,
                &tr!("Microphone and system audio"),
                None,
                card_group(t, "audio-settings", &tr!("Audio settings"))
                    .child(toggle_row(
                        t,
                        &app,
                        "audio-noise",
                        tile::NOISE,
                        &tr!("Noise suppression"),
                        &tr!("Reduces steady background noise on the microphone track. The original sound is kept too."),
                        keys::KEY_NOISE,
                        s.noise_suppression,
                        false,
                    ))
                    .child(plain_row(
                        t,
                        tile::DEVICES,
                        &tr_ctx!("settings row", "Devices"),
                        &tr!("The camera, microphone and system audio are chosen in the Record sheet; your choices are remembered."),
                        None,
                        true,
                    )),
            ))
            .into_any_element()
    }

    /// Adds the typed bundle id to `privacy.excluded_apps` (REC-6).
    pub(super) fn add_exclusion(&mut self, cx: &mut Context<Self>) {
        let raw = self.exclude_input.read(cx).text().to_owned();
        match validate_bundle_id(&raw) {
            Ok(id) => {
                let mut list = self.recording_settings(cx).excluded_apps;
                if !list.contains(&id) && !default_exclusions().iter().any(|(_, d)| *d == id) {
                    list.push(id);
                    self.write(keys::KEY_EXCLUDED, json!(list), cx);
                }
                self.exclude_error = None;
                self.exclude_input.update(cx, |f, cx| f.clear(cx));
            }
            Err(why) => self.exclude_error = Some(why),
        }
        cx.notify();
    }

    pub(super) fn remove_exclusion(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut list = self.recording_settings(cx).excluded_apps;
        list.retain(|x| x != id);
        self.write(keys::KEY_EXCLUDED, json!(list), cx);
    }
}
