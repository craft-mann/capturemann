//! The publish sheet (PUB-2, DESIGN §5.6): publish a recording to YouTube. A full-window page
//! in the style of the recording pages (toolbar with a crumb back to the recording, a centered
//! column), opened with the Publish button of the recording details page and the recording
//! editor.
//!
//! Everything it decides lives in `crate::publish` (form, validation mapping, job, stage), tested
//! without a display; the daemon calls are `publish::backend`. This file draws and wires.
//! The daemon, not this page, asks the user to confirm before anything is sent (Confirm tier).

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cm_daemon::events::Event;
use cm_i18n::{tr, tr_ctx};
use cm_publish::accounts::{PublishAccount, PublishAccountStore};
use cm_publish::{PlatformRules, Privacy};
use gpui::{
    div, img, prelude::*, px, AnyElement, App, Bounds, ClipboardItem, Context, Entity, FocusHandle,
    FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    PathPromptOptions, Pixels, Render, RenderImage, Role, SharedString, Task, Window,
};
use image::Frame;

use crate::poster::decode_poster;
use crate::publish::backend as pback;
use crate::publish::cancel::{CancelState, Step};
use crate::publish::form::{privacy_note, Check, Field, PublishForm, Quota, Readiness};
use crate::publish::job::{Failure, Outcome, OwnFailure, Tracker};
use crate::publish::sheet::{progress_text, request_key, retry_key, Stage, View};
use crate::publish::thumb;
use crate::publish_connect::privacy_label;
use crate::recording::exports::Phase as ExportPhase;
use crate::recording::files::{self, Details};
use crate::recording::model::clock_text;
use crate::theme::{scale, type_scale, Theme};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{
    button, button_with_icon, card, icon, icon_button, label_fill, shrink_text, switch, ButtonKind,
};
use crate::ui::export_ui;
use crate::ui::kit;
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{
    caption, disabled_button, segment, segment_with_icon, segmented, settings_row,
};
use crate::ui::slider::{self, SliderLook};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};

mod actions;
#[cfg(test)]
mod input_tests;
mod load;
mod sheet;
mod thumbnail;
mod view;
mod widgets;

use widgets::{card_title, field_block, initial, privacy_blurb, privacy_icon, progress_bar};

static TRACKER: LazyLock<Mutex<Tracker>> = LazyLock::new(Mutex::default);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Feeds a daemon `job.*` event to the publish jobs and wakes the page.
pub fn on_event(app: &gpui::Entity<AppModel>, ev: &Event, cx: &mut App) {
    if !ev.name.starts_with("job.") {
        return;
    }
    if lock(&TRACKER).on_event(&ev.name, &ev.data) {
        app.update(cx, |_, cx| cx.notify());
    }
}

/// Opens the sheet for a recording.
pub fn open(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    app::set_route(app, Route::Publish(capture_id.to_owned()), cx);
}

/// What the page knows about the recording.
struct Loaded {
    details: Details,
}

impl Loaded {
    /// The export that would be uploaded: the newest one still on disk.
    fn export(&self) -> Option<(PathBuf, &files::ExportLine)> {
        self.details
            .exports
            .iter()
            .rev()
            .find(|e| e.exists)
            .map(|e| (self.details.dir.join(&e.file), e))
    }

    fn duration_secs(&self) -> Option<f64> {
        self.export()
            .and_then(|(_, e)| e.duration_ms)
            .map(|ms| ms as f64 / 1000.0)
            .or(self.details.duration_s)
    }
}

enum Load {
    Idle,
    Loading,
    Ready(Box<Loaded>),
    Failed(files::FileError),
}

/// The thumbnail picker.
struct Thumb {
    image: Option<Arc<RenderImage>>,
    /// The slider position (0 to 1) over the export.
    frac: f32,
    /// The picture shown is the frame at `frac` (not an image file).
    from_frame: bool,
    busy: bool,
    /// The pointer is down on the slider: the frame is taken when it is released.
    dragging: bool,
    /// Counts slider moves, so a frame is only taken for the last of a burst.
    seq: u64,
    note: Option<ThumbNote>,
}

/// A line under the thumbnail picker; the words are made when drawn.
enum ThumbNote {
    /// The picture could not be decoded for the preview (the reason).
    NoPreview(files::PosterError),
    /// What the frame extraction said.
    Said(thumb::ExtractError),
}

impl ThumbNote {
    fn text(&self) -> String {
        match self {
            ThumbNote::NoPreview(e) => tr!(
                "The picture can\u{2019}t be shown here ({reason}); it is still uploaded if the type and size are accepted.",
                reason = e.to_string()
            ),
            ThumbNote::Said(e) => e.text(),
        }
    }
}

impl Default for Thumb {
    fn default() -> Self {
        Thumb {
            image: None,
            frac: 0.5,
            from_frame: false,
            busy: false,
            dragging: false,
            seq: 0,
            note: None,
        }
    }
}

/// How long the slider rests before the frame is taken (keys and slow drags).
const FRAME_DEBOUNCE: Duration = Duration::from_millis(300);

pub struct PublishPage {
    app: Entity<AppModel>,
    id: Option<String>,
    load: Load,
    rules: PlatformRules,
    form: PublishForm,
    title_input: Entity<TextField>,
    description_input: Entity<TextField>,
    tags_input: Entity<TextField>,
    playlist_input: Entity<TextField>,
    thumb_focus: FocusHandle,
    slider_bounds: Rc<Cell<Bounds<Pixels>>>,
    accounts: Vec<PublishAccount>,
    account: usize,
    accounts_error: Option<String>,
    quota: Option<Quota>,
    stage: Stage,
    /// Cancel was pressed for the running job and it has not stopped yet.
    cancel: CancelState,
    thumb: Thumb,
    counter: u64,
    copied: bool,
    epoch: u64,
    /// The language the text fields' labels were made in.
    labels_epoch: u64,
    tasks: Vec<Task<()>>,
}

/// The label and the hint of the title, tags and playlist fields (the description has its own).
struct FieldLabels {
    title: (String, String),
    description: (String, String),
    tags: (String, String),
    playlist: (String, String),
}

impl FieldLabels {
    fn current() -> Self {
        FieldLabels {
            title: (
                tr_ctx!("video field", "Title"),
                tr_ctx!("video field hint", "Title"),
            ),
            description: (
                tr_ctx!("video field", "Description"),
                tr!("What is this video about?"),
            ),
            tags: (
                tr_ctx!("video field", "Tags"),
                tr!("demo, screen recording"),
            ),
            playlist: (
                tr_ctx!("video field", "Playlist"),
                tr!("PLxxxxxxxxxxxxxxxx or a playlist address"),
            ),
        }
    }
}

impl PublishPage {
    /// Puts the text fields' labels and hints into the current language.
    fn relabel(&mut self, cx: &mut Context<Self>) {
        self.labels_epoch = cm_i18n::epoch();
        let l = FieldLabels::current();
        for (f, (label, hint)) in [
            (&self.title_input, l.title),
            (&self.description_input, l.description),
            (&self.tags_input, l.tags),
            (&self.playlist_input, l.playlist),
        ] {
            f.update(cx, |f, cx| f.set_labels(label, hint, cx));
        }
    }
}

impl PublishPage {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        // An export finishing (`library_epoch`) is what the page waits for when there is none.
        cx.observe(&app, |this, app, cx| {
            let epoch = app.read(cx).library_epoch;
            if epoch != this.epoch {
                this.epoch = epoch;
                this.reload(cx);
            } else {
                cx.notify();
            }
        })
        .detach();
        let labels = FieldLabels::current();
        let field = |cx: &mut Context<Self>, (label, hint): (String, String)| {
            let f = cx.new(|cx| TextField::new(label, hint, cx).without_icon());
            cx.subscribe(&f, |_, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Changed(_)) {
                    cx.notify();
                }
            })
            .detach();
            f
        };
        let title_input = field(cx, labels.title);
        let description_input = {
            let f = cx.new(|cx| {
                TextField::new(labels.description.0, labels.description.1, cx)
                    .without_icon()
                    .multiline(4, 10)
            });
            cx.subscribe(&f, |_, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Changed(_)) {
                    cx.notify();
                }
            })
            .detach();
            f
        };
        let tags_input = field(cx, labels.tags);
        let playlist_input = field(cx, labels.playlist);
        let thumb_focus = cx.focus_handle();
        let epoch = app.read(cx).library_epoch;
        PublishPage {
            app,
            id: None,
            load: Load::Idle,
            rules: PlatformRules::youtube(),
            form: PublishForm::new("", Privacy::Unlisted),
            title_input,
            description_input,
            tags_input,
            playlist_input,
            thumb_focus,
            slider_bounds: Rc::default(),
            accounts: Vec::new(),
            account: 0,
            accounts_error: None,
            quota: None,
            stage: Stage::Editing,
            cancel: CancelState::default(),
            thumb: Thumb::default(),
            counter: 0,
            copied: false,
            epoch,
            labels_epoch: cm_i18n::epoch(),
            tasks: Vec::new(),
        }
    }

    /// Shows the sheet for recording `id` (a no-op while it is already shown).
    pub fn set_capture(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.id.as_deref() == Some(id) {
            return;
        }
        self.id = Some(id.to_owned());
        self.load = Load::Loading;
        self.stage = Stage::Editing;
        self.thumb = Thumb::default();
        self.copied = false;
        self.form = PublishForm::new("", Privacy::Unlisted);
        for f in [
            &self.description_input,
            &self.tags_input,
            &self.playlist_input,
        ] {
            f.update(cx, |f, cx| f.clear(cx));
        }
        self.load_accounts(cx);
        self.reload(cx);
    }

    pub fn is_open(&self) -> bool {
        self.id.is_some()
    }

    /// Forgets the recording when the page is left.
    pub fn close(&mut self, _cx: &mut Context<Self>) {
        self.id = None;
        self.load = Load::Idle;
        self.tasks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_options_have_distinct_icons_and_lines() {
        let all = [Privacy::Private, Privacy::Unlisted, Privacy::Public];
        let icons: Vec<_> = all.iter().map(|p| privacy_icon(*p)).collect();
        assert_eq!(icons, ["lock", "link", "globe"]);
        let lines: std::collections::HashSet<_> = all.iter().map(|p| privacy_blurb(*p)).collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn the_channel_tile_uses_the_initial_and_survives_an_empty_name() {
        assert_eq!(initial("the AI Explorer"), "T");
        assert_eq!(initial("  \u{e9}cole"), "\u{c9}");
        assert_eq!(initial(""), "?");
    }
}
