//! Theme tokens as GPUI colors, the bundled Geist fonts, SVG assets and text helpers.

use gpui::{
    rgba, App, AssetSource, FontFeatures, Hsla, Rgba, SharedString, Styled, WindowAppearance,
};
use std::borrow::Cow;
use std::sync::Arc;

use crate::theme::Color;

/// Geist 1.8.2 (UI, weights 400/500/600/700) and Geist Mono 1.8.2 (400/500), SIL OFL 1.1
/// (license text in `assets/fonts/OFL-LICENSE.txt`). This release covers all Vietnamese letters
/// (`tests/font_coverage.rs` fails if a bundled face loses one); the 1.2 files used before lacked 88. Each weight is a separate static TTF registered under the
/// same family, so GPUI/font-kit selects the face by `font_weight` (the previous Inter-only
/// build rendered 500/600 as regular).
const FONTS: [&[u8]; 6] = [
    include_bytes!("../../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Geist-Bold.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Medium.ttf"),
];

pub const UI_FONT: &str = "Geist";
pub const MONO_FONT: &str = "Geist Mono";

pub fn load_fonts(cx: &App) {
    if let Err(e) = cx
        .text_system()
        .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
    {
        log::warn!("could not register the Geist fonts: {e}");
    }
}

/// Embedded SVG assets (the logo corners and the Lucide icons in use). GPUI renders them as
/// alpha masks tinted with the element's text color, rasterized at the display scale.
pub struct Assets;

/// The logo corners, then the official Lucide icons (`assets/icons/README.md`).
pub(crate) const ASSETS: &[(&str, &str)] = &[
    (
        "brand/mark-corners.svg",
        include_str!("../../assets/brand/mark-corners.svg"),
    ),
    // "a CraftMann app" (CraftMann brand kit, logo/svg/endorsement-a-craftmann-app-on-dark.svg, text as
    // outlines). The on-dark and on-light files differ only in fill, so one asset is tinted with
    // `text_3`, which is the kit's #A1A1A9 on dark and #44444B on light.
    (
        "brand/endorsement-craftmann.svg",
        include_str!("../../assets/brand/endorsement-craftmann.svg"),
    ),
    ("icons/x.svg", include_str!("../../assets/icons/x.svg")),
    (
        "icons/bookmark.svg",
        include_str!("../../assets/icons/bookmark.svg"),
    ),
    (
        "icons/chevron-down.svg",
        include_str!("../../assets/icons/chevron-down.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_str!("../../assets/icons/chevron-right.svg"),
    ),
    (
        "icons/file-text.svg",
        include_str!("../../assets/icons/file-text.svg"),
    ),
    (
        "icons/graduation-cap.svg",
        include_str!("../../assets/icons/graduation-cap.svg"),
    ),
    (
        "icons/hourglass.svg",
        include_str!("../../assets/icons/hourglass.svg"),
    ),
    (
        "icons/presentation.svg",
        include_str!("../../assets/icons/presentation.svg"),
    ),
    (
        "icons/users.svg",
        include_str!("../../assets/icons/users.svg"),
    ),
    (
        "icons/brand-youtube.svg",
        include_str!("../../assets/icons/brand-youtube.svg"),
    ),
    (
        "icons/brand-tiktok.svg",
        include_str!("../../assets/icons/brand-tiktok.svg"),
    ),
    (
        "icons/brand-facebook.svg",
        include_str!("../../assets/icons/brand-facebook.svg"),
    ),
    (
        "icons/brand-google-drive.svg",
        include_str!("../../assets/icons/brand-google-drive.svg"),
    ),
    (
        "icons/brand-onedrive.svg",
        include_str!("../../assets/icons/brand-onedrive.svg"),
    ),
    (
        "icons/brand-dropbox.svg",
        include_str!("../../assets/icons/brand-dropbox.svg"),
    ),
    (
        "icons/search.svg",
        include_str!("../../assets/icons/search.svg"),
    ),
    (
        "icons/video.svg",
        include_str!("../../assets/icons/video.svg"),
    ),
    (
        "icons/send.svg",
        include_str!("../../assets/icons/send.svg"),
    ),
    (
        "icons/cloud.svg",
        include_str!("../../assets/icons/cloud.svg"),
    ),
    ("icons/bot.svg", include_str!("../../assets/icons/bot.svg")),
    (
        "icons/image.svg",
        include_str!("../../assets/icons/image.svg"),
    ),
    (
        "icons/mouse-pointer-2.svg",
        include_str!("../../assets/icons/mouse-pointer-2.svg"),
    ),
    (
        "icons/move-up-right.svg",
        include_str!("../../assets/icons/move-up-right.svg"),
    ),
    (
        "icons/slash.svg",
        include_str!("../../assets/icons/slash.svg"),
    ),
    (
        "icons/square.svg",
        include_str!("../../assets/icons/square.svg"),
    ),
    (
        "icons/circle.svg",
        include_str!("../../assets/icons/circle.svg"),
    ),
    (
        "icons/type.svg",
        include_str!("../../assets/icons/type.svg"),
    ),
    (
        "icons/highlighter.svg",
        include_str!("../../assets/icons/highlighter.svg"),
    ),
    (
        "icons/hash.svg",
        include_str!("../../assets/icons/hash.svg"),
    ),
    (
        "icons/droplet.svg",
        include_str!("../../assets/icons/droplet.svg"),
    ),
    (
        "icons/grid-3x3.svg",
        include_str!("../../assets/icons/grid-3x3.svg"),
    ),
    (
        "icons/crop.svg",
        include_str!("../../assets/icons/crop.svg"),
    ),
    (
        "icons/undo-2.svg",
        include_str!("../../assets/icons/undo-2.svg"),
    ),
    (
        "icons/redo-2.svg",
        include_str!("../../assets/icons/redo-2.svg"),
    ),
    (
        "icons/rotate-ccw.svg",
        include_str!("../../assets/icons/rotate-ccw.svg"),
    ),
    (
        "icons/zoom-in.svg",
        include_str!("../../assets/icons/zoom-in.svg"),
    ),
    (
        "icons/zoom-out.svg",
        include_str!("../../assets/icons/zoom-out.svg"),
    ),
    (
        "icons/maximize.svg",
        include_str!("../../assets/icons/maximize.svg"),
    ),
    (
        "icons/rec-pause.svg",
        include_str!("../../assets/icons/rec-pause.svg"),
    ),
    (
        "icons/rec-play.svg",
        include_str!("../../assets/icons/rec-play.svg"),
    ),
    (
        "icons/rec-mic.svg",
        include_str!("../../assets/icons/rec-mic.svg"),
    ),
    (
        "icons/rec-mic-off.svg",
        include_str!("../../assets/icons/rec-mic-off.svg"),
    ),
    (
        "icons/rec-video.svg",
        include_str!("../../assets/icons/rec-video.svg"),
    ),
    (
        "icons/rec-video-off.svg",
        include_str!("../../assets/icons/rec-video-off.svg"),
    ),
    (
        "icons/rec-pencil.svg",
        include_str!("../../assets/icons/rec-pencil.svg"),
    ),
    (
        "icons/rec-user.svg",
        include_str!("../../assets/icons/rec-user.svg"),
    ),
    (
        "icons/rec-restart.svg",
        include_str!("../../assets/icons/rec-restart.svg"),
    ),
    (
        "icons/rec-check.svg",
        include_str!("../../assets/icons/rec-check.svg"),
    ),
    (
        "icons/app-window.svg",
        include_str!("../../assets/icons/app-window.svg"),
    ),
    (
        "icons/archive-restore.svg",
        include_str!("../../assets/icons/archive-restore.svg"),
    ),
    (
        "icons/arrow-down.svg",
        include_str!("../../assets/icons/arrow-down.svg"),
    ),
    (
        "icons/arrow-down-a-z.svg",
        include_str!("../../assets/icons/arrow-down-a-z.svg"),
    ),
    (
        "icons/arrow-up.svg",
        include_str!("../../assets/icons/arrow-up.svg"),
    ),
    (
        "icons/arrow-up-right.svg",
        include_str!("../../assets/icons/arrow-up-right.svg"),
    ),
    (
        "icons/audio-lines.svg",
        include_str!("../../assets/icons/audio-lines.svg"),
    ),
    ("icons/ban.svg", include_str!("../../assets/icons/ban.svg")),
    (
        "icons/blend.svg",
        include_str!("../../assets/icons/blend.svg"),
    ),
    (
        "icons/calendar.svg",
        include_str!("../../assets/icons/calendar.svg"),
    ),
    (
        "icons/camera.svg",
        include_str!("../../assets/icons/camera.svg"),
    ),
    (
        "icons/captions.svg",
        include_str!("../../assets/icons/captions.svg"),
    ),
    (
        "icons/check.svg",
        include_str!("../../assets/icons/check.svg"),
    ),
    (
        "icons/check-check.svg",
        include_str!("../../assets/icons/check-check.svg"),
    ),
    (
        "icons/chevron-left.svg",
        include_str!("../../assets/icons/chevron-left.svg"),
    ),
    (
        "icons/circle-alert.svg",
        include_str!("../../assets/icons/circle-alert.svg"),
    ),
    (
        "icons/circle-check.svg",
        include_str!("../../assets/icons/circle-check.svg"),
    ),
    (
        "icons/circle-dashed.svg",
        include_str!("../../assets/icons/circle-dashed.svg"),
    ),
    (
        "icons/circle-dot.svg",
        include_str!("../../assets/icons/circle-dot.svg"),
    ),
    (
        "icons/circle-pause.svg",
        include_str!("../../assets/icons/circle-pause.svg"),
    ),
    (
        "icons/clock.svg",
        include_str!("../../assets/icons/clock.svg"),
    ),
    (
        "icons/cloud-check.svg",
        include_str!("../../assets/icons/cloud-check.svg"),
    ),
    (
        "icons/cloud-upload.svg",
        include_str!("../../assets/icons/cloud-upload.svg"),
    ),
    (
        "icons/cm-capture-region.svg",
        include_str!("../../assets/icons/cm-capture-region.svg"),
    ),
    (
        "icons/cm-corner-bottom-left.svg",
        include_str!("../../assets/icons/cm-corner-bottom-left.svg"),
    ),
    (
        "icons/cm-corner-bottom-right.svg",
        include_str!("../../assets/icons/cm-corner-bottom-right.svg"),
    ),
    (
        "icons/cm-corner-top-left.svg",
        include_str!("../../assets/icons/cm-corner-top-left.svg"),
    ),
    (
        "icons/cm-corner-top-right.svg",
        include_str!("../../assets/icons/cm-corner-top-right.svg"),
    ),
    (
        "icons/cm-layout-bubble.svg",
        include_str!("../../assets/icons/cm-layout-bubble.svg"),
    ),
    (
        "icons/cm-layout-camera.svg",
        include_str!("../../assets/icons/cm-layout-camera.svg"),
    ),
    (
        "icons/cm-layout-screen.svg",
        include_str!("../../assets/icons/cm-layout-screen.svg"),
    ),
    (
        "icons/cm-layout-split.svg",
        include_str!("../../assets/icons/cm-layout-split.svg"),
    ),
    (
        "icons/cm-record.svg",
        include_str!("../../assets/icons/cm-record.svg"),
    ),
    (
        "icons/cm-split.svg",
        include_str!("../../assets/icons/cm-split.svg"),
    ),
    (
        "icons/cm-stop.svg",
        include_str!("../../assets/icons/cm-stop.svg"),
    ),
    (
        "icons/cm-trim-in.svg",
        include_str!("../../assets/icons/cm-trim-in.svg"),
    ),
    (
        "icons/cm-trim-out.svg",
        include_str!("../../assets/icons/cm-trim-out.svg"),
    ),
    (
        "icons/cm-webcam-bubble.svg",
        include_str!("../../assets/icons/cm-webcam-bubble.svg"),
    ),
    (
        "icons/copy.svg",
        include_str!("../../assets/icons/copy.svg"),
    ),
    (
        "icons/copy-plus.svg",
        include_str!("../../assets/icons/copy-plus.svg"),
    ),
    (
        "icons/download.svg",
        include_str!("../../assets/icons/download.svg"),
    ),
    (
        "icons/external-link.svg",
        include_str!("../../assets/icons/external-link.svg"),
    ),
    ("icons/eye.svg", include_str!("../../assets/icons/eye.svg")),
    (
        "icons/eye-off.svg",
        include_str!("../../assets/icons/eye-off.svg"),
    ),
    (
        "icons/file-input.svg",
        include_str!("../../assets/icons/file-input.svg"),
    ),
    (
        "icons/file-output.svg",
        include_str!("../../assets/icons/file-output.svg"),
    ),
    (
        "icons/file-video.svg",
        include_str!("../../assets/icons/file-video.svg"),
    ),
    (
        "icons/file-x-2.svg",
        include_str!("../../assets/icons/file-x-2.svg"),
    ),
    (
        "icons/film.svg",
        include_str!("../../assets/icons/film.svg"),
    ),
    (
        "icons/flip-horizontal-2.svg",
        include_str!("../../assets/icons/flip-horizontal-2.svg"),
    ),
    (
        "icons/folder.svg",
        include_str!("../../assets/icons/folder.svg"),
    ),
    (
        "icons/folder-open.svg",
        include_str!("../../assets/icons/folder-open.svg"),
    ),
    (
        "icons/folder-output.svg",
        include_str!("../../assets/icons/folder-output.svg"),
    ),
    (
        "icons/folder-plus.svg",
        include_str!("../../assets/icons/folder-plus.svg"),
    ),
    (
        "icons/gauge.svg",
        include_str!("../../assets/icons/gauge.svg"),
    ),
    (
        "icons/globe.svg",
        include_str!("../../assets/icons/globe.svg"),
    ),
    (
        "icons/hand.svg",
        include_str!("../../assets/icons/hand.svg"),
    ),
    (
        "icons/hard-drive.svg",
        include_str!("../../assets/icons/hard-drive.svg"),
    ),
    (
        "icons/image-plus.svg",
        include_str!("../../assets/icons/image-plus.svg"),
    ),
    (
        "icons/images.svg",
        include_str!("../../assets/icons/images.svg"),
    ),
    (
        "icons/info.svg",
        include_str!("../../assets/icons/info.svg"),
    ),
    (
        "icons/keyboard.svg",
        include_str!("../../assets/icons/keyboard.svg"),
    ),
    (
        "icons/laptop.svg",
        include_str!("../../assets/icons/laptop.svg"),
    ),
    (
        "icons/layout-grid.svg",
        include_str!("../../assets/icons/layout-grid.svg"),
    ),
    (
        "icons/link.svg",
        include_str!("../../assets/icons/link.svg"),
    ),
    (
        "icons/list-checks.svg",
        include_str!("../../assets/icons/list-checks.svg"),
    ),
    (
        "icons/list.svg",
        include_str!("../../assets/icons/list.svg"),
    ),
    (
        "icons/loader-circle.svg",
        include_str!("../../assets/icons/loader-circle.svg"),
    ),
    (
        "icons/locate.svg",
        include_str!("../../assets/icons/locate.svg"),
    ),
    (
        "icons/locate-fixed.svg",
        include_str!("../../assets/icons/locate-fixed.svg"),
    ),
    (
        "icons/lock.svg",
        include_str!("../../assets/icons/lock.svg"),
    ),
    (
        "icons/magnet.svg",
        include_str!("../../assets/icons/magnet.svg"),
    ),
    (
        "icons/merge.svg",
        include_str!("../../assets/icons/merge.svg"),
    ),
    ("icons/mic.svg", include_str!("../../assets/icons/mic.svg")),
    (
        "icons/minus.svg",
        include_str!("../../assets/icons/minus.svg"),
    ),
    (
        "icons/monitor.svg",
        include_str!("../../assets/icons/monitor.svg"),
    ),
    (
        "icons/moon.svg",
        include_str!("../../assets/icons/moon.svg"),
    ),
    (
        "icons/mouse-pointer-click.svg",
        include_str!("../../assets/icons/mouse-pointer-click.svg"),
    ),
    (
        "icons/octagon-alert.svg",
        include_str!("../../assets/icons/octagon-alert.svg"),
    ),
    (
        "icons/pen-line.svg",
        include_str!("../../assets/icons/pen-line.svg"),
    ),
    (
        "icons/pencil.svg",
        include_str!("../../assets/icons/pencil.svg"),
    ),
    ("icons/pin.svg", include_str!("../../assets/icons/pin.svg")),
    (
        "icons/play.svg",
        include_str!("../../assets/icons/play.svg"),
    ),
    (
        "icons/plug.svg",
        include_str!("../../assets/icons/plug.svg"),
    ),
    (
        "icons/plus.svg",
        include_str!("../../assets/icons/plus.svg"),
    ),
    (
        "icons/ratio.svg",
        include_str!("../../assets/icons/ratio.svg"),
    ),
    (
        "icons/rectangle-horizontal.svg",
        include_str!("../../assets/icons/rectangle-horizontal.svg"),
    ),
    (
        "icons/rectangle-vertical.svg",
        include_str!("../../assets/icons/rectangle-vertical.svg"),
    ),
    (
        "icons/refresh-cw.svg",
        include_str!("../../assets/icons/refresh-cw.svg"),
    ),
    (
        "icons/rotate-cw.svg",
        include_str!("../../assets/icons/rotate-cw.svg"),
    ),
    (
        "icons/save.svg",
        include_str!("../../assets/icons/save.svg"),
    ),
    (
        "icons/scissors.svg",
        include_str!("../../assets/icons/scissors.svg"),
    ),
    (
        "icons/screen-share.svg",
        include_str!("../../assets/icons/screen-share.svg"),
    ),
    (
        "icons/search-x.svg",
        include_str!("../../assets/icons/search-x.svg"),
    ),
    (
        "icons/settings.svg",
        include_str!("../../assets/icons/settings.svg"),
    ),
    (
        "icons/settings-2.svg",
        include_str!("../../assets/icons/settings-2.svg"),
    ),
    (
        "icons/shield-alert.svg",
        include_str!("../../assets/icons/shield-alert.svg"),
    ),
    (
        "icons/shield-check.svg",
        include_str!("../../assets/icons/shield-check.svg"),
    ),
    (
        "icons/shrink.svg",
        include_str!("../../assets/icons/shrink.svg"),
    ),
    (
        "icons/sliders-horizontal.svg",
        include_str!("../../assets/icons/sliders-horizontal.svg"),
    ),
    (
        "icons/sparkles.svg",
        include_str!("../../assets/icons/sparkles.svg"),
    ),
    (
        "icons/square-pen.svg",
        include_str!("../../assets/icons/square-pen.svg"),
    ),
    (
        "icons/star.svg",
        include_str!("../../assets/icons/star.svg"),
    ),
    ("icons/sun.svg", include_str!("../../assets/icons/sun.svg")),
    (
        "icons/timer.svg",
        include_str!("../../assets/icons/timer.svg"),
    ),
    (
        "icons/trash-2.svg",
        include_str!("../../assets/icons/trash-2.svg"),
    ),
    (
        "icons/triangle-alert.svg",
        include_str!("../../assets/icons/triangle-alert.svg"),
    ),
    (
        "icons/unplug.svg",
        include_str!("../../assets/icons/unplug.svg"),
    ),
    (
        "icons/user-plus.svg",
        include_str!("../../assets/icons/user-plus.svg"),
    ),
    (
        "icons/mic-off.svg",
        include_str!("../../assets/icons/mic-off.svg"),
    ),
    (
        "icons/camera-off.svg",
        include_str!("../../assets/icons/camera-off.svg"),
    ),
    ("icons/usb.svg", include_str!("../../assets/icons/usb.svg")),
    (
        "icons/move.svg",
        include_str!("../../assets/icons/move.svg"),
    ),
    (
        "icons/video-off.svg",
        include_str!("../../assets/icons/video-off.svg"),
    ),
    (
        "icons/volume-2.svg",
        include_str!("../../assets/icons/volume-2.svg"),
    ),
    (
        "icons/volume-x.svg",
        include_str!("../../assets/icons/volume-x.svg"),
    ),
    (
        "icons/wallpaper.svg",
        include_str!("../../assets/icons/wallpaper.svg"),
    ),
    (
        "icons/webcam.svg",
        include_str!("../../assets/icons/webcam.svg"),
    ),
    (
        "icons/wifi-off.svg",
        include_str!("../../assets/icons/wifi-off.svg"),
    ),
    ("icons/zap.svg", include_str!("../../assets/icons/zap.svg")),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(ASSETS
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, s)| Cow::Borrowed(s.as_bytes()))
            .or_else(|| crate::ui::launcher_window::asset(path)))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(ASSETS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect())
    }
}

pub fn rgba_of(c: Color) -> Rgba {
    rgba(c.0)
}

pub fn hsla_of(c: Color) -> Hsla {
    Hsla::from(rgba(c.0))
}

fn features(tags: &[&str]) -> FontFeatures {
    FontFeatures(Arc::new(
        tags.iter().map(|t| ((*t).to_owned(), 1)).collect(),
    ))
}

/// Default OpenType features for UI text (contextual alternates and kerning).
pub fn ui_features() -> FontFeatures {
    features(&["calt", "kern"])
}

/// Tabular numbers for every live number (timers, sizes, dates, counts).
pub fn tnum_features() -> FontFeatures {
    features(&["calt", "kern", "tnum"])
}

pub trait TextExt: Styled + Sized {
    /// Tabular figures for values that change.
    fn tnum(self) -> Self {
        self.font_features(tnum_features())
    }

    /// Geist Mono: timecodes, shortcuts, paths, tool names and tiny labels.
    fn mono(self) -> Self {
        self.font_family(MONO_FONT)
    }
}

impl<T: Styled + Sized> TextExt for T {}

pub fn is_light(a: WindowAppearance) -> bool {
    matches!(a, WindowAppearance::Light | WindowAppearance::VibrantLight)
}

pub fn shared(s: &str) -> SharedString {
    SharedString::from(s.to_owned())
}
