//! The guide editor (GDE-2, design board S8): a full-window page for a Click Guide. The step list
//! is on the left (reorder, duplicate, delete, note steps, numbering), the selected step on the
//! right (title, note, the picture with its badge and ring, Edit image, Retake, focus crop).
//! The guide title and intro sit above the list. Every change is one `set_guide` call, so it is
//! one undo step of the guide's own history (`undo` and `redo` with the guide id). Exports are in
//! [`export`].
//!
//! The page holds no guide data of its own: it reads `get_guide` and calls the daemon's actions
//! like the CLI and agents do.

use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine as _;
use gpui::{
    div, img, prelude::*, px, Context, Entity, FontWeight, ObjectFit, Render, RenderImage, Role,
    SharedString, Subscription, Window,
};
use image::Frame;
use serde_json::{json, Value};

use cm_i18n::{tr, tr_ctx, trn};

use crate::backend::Backend;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{
    badge, button_with_icon, crumb_link, icon, icon_button, page_toolbar_actions, ButtonKind,
};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{CaretAt, TextField, TextFieldEvent};

pub mod export;
pub mod hud;

/// One step as `get_guide` describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRow {
    pub id: String,
    pub label: String,
    pub is_note: bool,
    pub title: String,
    pub note: String,
    pub image_id: Option<String>,
    pub image_file: Option<String>,
    pub has_proposals: bool,
    pub focus_crop: bool,
    pub click: Option<(f64, f64)>,
}

/// A guide as `get_guide` describes it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GuideData {
    pub id: String,
    pub dir: PathBuf,
    pub title: String,
    pub intro: String,
    pub numbering: String,
    pub steps: Vec<StepRow>,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl GuideData {
    pub fn parse(v: &Value) -> Option<GuideData> {
        let s = |x: &Value| x.as_str().unwrap_or_default().to_owned();
        let steps = v["steps"]
            .as_array()?
            .iter()
            .map(|st| StepRow {
                id: s(&st["id"]),
                label: s(&st["label"]),
                is_note: st["type"] == "note",
                title: s(&st["title"]),
                note: s(&st["note"]),
                image_id: st["image_id"].as_str().map(str::to_owned),
                image_file: st["image_file"].as_str().map(str::to_owned),
                has_proposals: st["has_proposals"] == true,
                focus_crop: st["focus_crop"] == true,
                click: st["click"]["x"].as_f64().zip(st["click"]["y"].as_f64()),
            })
            .collect();
        Some(GuideData {
            id: s(&v["guide_id"]),
            dir: PathBuf::from(s(&v["path"])),
            title: s(&v["title"]),
            intro: s(&v["intro"]),
            numbering: s(&v["numbering"]),
            steps,
            can_undo: v["can_undo"] == true,
            can_redo: v["can_redo"] == true,
        })
    }
}

enum Load {
    Idle,
    Loading,
    Ready(Box<GuideData>),
    Failed(String),
}

pub struct GuideView {
    pub(crate) app: Entity<AppModel>,
    id: Option<String>,
    load: Load,
    selected: Option<String>,
    title: Entity<TextField>,
    intro: Entity<TextField>,
    step_title: Entity<TextField>,
    step_note: Entity<TextField>,
    /// The picture of the selected step (rendered: badge, ring, crop) and the thumbnails.
    picture: Option<(String, Arc<RenderImage>)>,
    thumbs: Vec<(String, Arc<RenderImage>)>,
    pub(crate) error: Option<String>,
    pub(crate) export: Option<export::ExportState>,
    epoch: u64,
    _subs: Vec<Subscription>,
}

impl GuideView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| {
            TextField::new(tr!("Guide title"), tr_ctx!("field", "Title"), cx).without_icon()
        });
        let intro = cx.new(|cx| {
            TextField::new(tr!("Intro"), tr!("A sentence or two before step 1"), cx)
                .without_icon()
                .multiline(2, 4)
        });
        let step_title = cx.new(|cx| {
            TextField::new(tr!("Step title"), tr!("What the reader does"), cx).without_icon()
        });
        let step_note = cx.new(|cx| {
            TextField::new(tr!("Step note"), tr!("A note (optional)"), cx)
                .without_icon()
                .multiline(2, 5)
        });
        let subs = vec![
            cx.observe(&app, |this, app, cx| {
                let epoch = app.read(cx).library_epoch;
                if epoch != this.epoch {
                    this.epoch = epoch;
                    this.reload(cx);
                } else {
                    cx.notify();
                }
            }),
            cx.subscribe(&title, |this, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Submitted) {
                    this.commit(cx);
                }
            }),
            cx.subscribe(&intro, |this, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Submitted) {
                    this.commit(cx);
                }
            }),
            cx.subscribe(&step_title, |this, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Submitted) {
                    // Enter saves and moves to the next step (S8).
                    this.commit(cx);
                    this.select_next(cx);
                }
            }),
            cx.subscribe(&step_note, |this, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Submitted) {
                    this.commit(cx);
                }
            }),
        ];
        let epoch = app.read(cx).library_epoch;
        GuideView {
            app,
            id: None,
            load: Load::Idle,
            selected: None,
            title,
            intro,
            step_title,
            step_note,
            picture: None,
            thumbs: Vec::new(),
            error: None,
            export: None,
            epoch,
            _subs: subs,
        }
    }

    /// Shows guide `id` (a no-op while it is already shown).
    pub fn set_guide(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.id.as_deref() == Some(id) {
            return;
        }
        self.id = Some(id.to_owned());
        self.load = Load::Loading;
        self.selected = None;
        self.picture = None;
        self.thumbs.clear();
        self.error = None;
        self.export = None;
        self.reload(cx);
    }

    pub(crate) fn data(&self) -> Option<&GuideData> {
        match &self.load {
            Load::Ready(d) => Some(d),
            _ => None,
        }
    }

    pub(crate) fn guide_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Runs a daemon action on the runtime and hands the result back on the UI thread.
    pub(crate) fn call(
        &mut self,
        method: &'static str,
        params: Value,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Result<Value, String>, &mut Context<Self>) + 'static,
    ) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            self.error = Some(tr!("The library is not available"));
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let r = backend
                .rt
                .spawn(async move {
                    Backend::call_coded(&daemon, method, params)
                        .await
                        .map_err(|e| crate::daemon_text::call_error_text(&e))
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| then(this, r, cx));
        })
        .detach();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        self.call(
            "get_guide",
            json!({ "guide_id": id }),
            cx,
            |this, r, cx| match r.ok().and_then(|v| GuideData::parse(&v)) {
                Some(d) => this.loaded(d, cx),
                None => {
                    this.load = Load::Failed(tr!("This guide could not be opened"));
                    cx.notify();
                }
            },
        );
    }

    fn loaded(&mut self, d: GuideData, cx: &mut Context<Self>) {
        if self
            .selected
            .as_ref()
            .is_none_or(|s| !d.steps.iter().any(|x| &x.id == s))
        {
            self.selected = d.steps.first().map(|s| s.id.clone());
        }
        self.title.update(cx, |t, cx| t.set_text(&d.title, cx));
        self.intro.update(cx, |t, cx| t.set_text(&d.intro, cx));
        self.load_selected_fields(&d, cx);
        self.load_thumbs(&d, cx);
        self.load = Load::Ready(Box::new(d));
        self.load_picture(cx);
        cx.notify();
    }

    fn load_selected_fields(&mut self, d: &GuideData, cx: &mut Context<Self>) {
        let sel = self
            .selected
            .as_ref()
            .and_then(|s| d.steps.iter().find(|x| &x.id == s));
        let (t, n) = sel.map_or((String::new(), String::new()), |s| {
            (s.title.clone(), s.note.clone())
        });
        self.step_title.update(cx, |f, cx| f.set_text(&t, cx));
        self.step_note.update(cx, |f, cx| f.set_text(&n, cx));
    }

    fn load_thumbs(&mut self, d: &GuideData, cx: &mut Context<Self>) {
        let files: Vec<(String, PathBuf)> = d
            .steps
            .iter()
            .filter_map(|s| Some((s.id.clone(), d.dir.join(s.image_file.as_ref()?))))
            .collect();
        cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move {
                    files
                        .into_iter()
                        .filter_map(|(id, p)| {
                            let bytes = std::fs::read(p).ok()?;
                            let rgba = crate::poster::decode_poster(&bytes, 160).ok()?;
                            Some((id, Arc::new(RenderImage::new([Frame::new(rgba)]))))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.thumbs = decoded;
                cx.notify();
            });
        })
        .detach();
    }

    fn load_picture(&mut self, cx: &mut Context<Self>) {
        let Some(image_id) = self.selected_row().and_then(|s| s.image_id.clone()) else {
            self.picture = None;
            return;
        };
        self.call(
            "get_capture",
            json!({ "capture_id": image_id, "include_image": true }),
            cx,
            move |this, r, cx| {
                let decoded = r.ok().and_then(|v| {
                    let b64 = v["image"]["data"].as_str()?.to_owned();
                    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
                    let rgba = crate::poster::decode_poster(&bytes, 1400).ok()?;
                    Some(Arc::new(RenderImage::new([Frame::new(rgba)])))
                });
                if this.selected_row().and_then(|s| s.image_id.as_deref()) == Some(&image_id) {
                    this.picture = decoded.map(|i| (image_id, i));
                    cx.notify();
                }
            },
        );
    }

    pub(crate) fn selected_row(&self) -> Option<&StepRow> {
        let d = self.data()?;
        let sel = self.selected.as_ref()?;
        d.steps.iter().find(|s| &s.id == sel)
    }

    pub(crate) fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(id) {
            return;
        }
        self.commit(cx);
        self.selected = Some(id.to_owned());
        if let Some(d) = self.data().cloned() {
            self.load_selected_fields(&d, cx);
        }
        self.picture = None;
        self.load_picture(cx);
        cx.notify();
    }

    fn select_next(&mut self, cx: &mut Context<Self>) {
        let next = self.data().and_then(|d| {
            let p = d
                .steps
                .iter()
                .position(|s| Some(&s.id) == self.selected.as_ref())?;
            d.steps.get(p + 1).map(|s| s.id.clone())
        });
        if let Some(n) = next {
            self.select(&n, cx);
            let f = self.step_title.clone();
            f.update(cx, |f, cx| {
                let t = f.text().to_owned();
                f.load(&t, CaretAt::End, cx);
            });
        }
    }

    /// Writes the fields that changed as one `set_guide` call each.
    pub(crate) fn commit(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.data().cloned() else {
            return;
        };
        let mut p = serde_json::Map::new();
        let (t, i) = (
            self.title.read(cx).text().trim().to_owned(),
            self.intro.read(cx).text().trim().to_owned(),
        );
        if !t.is_empty() && t != d.title {
            p.insert("title".into(), json!(t));
        }
        if i != d.intro {
            p.insert("intro".into(), json!(i));
        }
        if let Some(s) = self.selected_row() {
            let (st, sn) = (
                self.step_title.read(cx).text().trim().to_owned(),
                self.step_note.read(cx).text().trim().to_owned(),
            );
            if st != s.title || sn != s.note {
                p.insert(
                    "steps".into(),
                    json!([{ "id": s.id, "title": st, "note": sn }]),
                );
            }
        }
        if p.is_empty() {
            return;
        }
        p.insert("guide_id".into(), json!(d.id));
        self.apply("set_guide", Value::Object(p), cx);
    }

    /// One edit: the call, then the new state.
    pub(crate) fn apply(&mut self, method: &'static str, params: Value, cx: &mut Context<Self>) {
        self.error = None;
        self.call(method, params, cx, |this, r, cx| match r {
            Ok(v) => {
                // `undo` and `redo` wrap the guide; the rest answer with it.
                let g = if v.get("guide").is_some() {
                    &v["guide"]
                } else {
                    &v
                };
                match GuideData::parse(g) {
                    Some(d) => this.loaded(d, cx),
                    None => this.reload(cx),
                }
            }
            Err(e) => {
                this.error = Some(e);
                cx.notify();
            }
        });
    }

    fn step_edit(&mut self, entries: Vec<Value>, cx: &mut Context<Self>) {
        self.commit(cx);
        let Some(id) = self.id.clone() else { return };
        self.apply("set_guide", json!({ "guide_id": id, "steps": entries }), cx);
    }

    /// The step ids in the order they are listed.
    fn ids(&self) -> Vec<String> {
        self.data()
            .map(|d| d.steps.iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default()
    }

    fn move_step(&mut self, id: &str, by: isize, cx: &mut Context<Self>) {
        let mut ids = self.ids();
        let Some(p) = ids.iter().position(|s| s == id) else {
            return;
        };
        let to = p.saturating_add_signed(by).min(ids.len().saturating_sub(1));
        let s = ids.remove(p);
        ids.insert(to, s);
        self.step_edit(ids.into_iter().map(|i| json!({ "id": i })).collect(), cx);
    }

    fn duplicate(&mut self, id: &str, cx: &mut Context<Self>) {
        let entries = self
            .ids()
            .into_iter()
            .map(|i| {
                if i == id {
                    json!({ "id": i, "duplicate": true })
                } else {
                    json!({ "id": i })
                }
            })
            .collect();
        self.step_edit(entries, cx);
    }

    fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        self.step_edit(vec![json!({ "id": id, "delete": true })], cx);
    }

    fn add_note_step(&mut self, cx: &mut Context<Self>) {
        let mut entries: Vec<Value> = self.ids().into_iter().map(|i| json!({ "id": i })).collect();
        entries.push(json!({ "title": tr!("New note step") }));
        self.step_edit(entries, cx);
    }

    fn set_numbering(&mut self, style: &'static str, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        self.apply(
            "set_guide",
            json!({ "guide_id": id, "numbering": style }),
            cx,
        );
    }

    fn history(&mut self, undo: bool, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        self.commit(cx);
        self.apply(
            if undo { "undo" } else { "redo" },
            json!({ "capture_id": id }),
            cx,
        );
    }

    fn retake(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(g) = self.id.clone() else { return };
        self.apply("set_guide", json!({ "guide_id": g, "retake": [id] }), cx);
    }

    fn focus_crop(&mut self, id: &str, on: bool, cx: &mut Context<Self>) {
        let Some(g) = self.id.clone() else { return };
        self.apply(
            "set_guide",
            json!({ "guide_id": g, "focus_crop": on, "crop_steps": [id] }),
            cx,
        );
    }

    fn edit_image(&mut self, cx: &mut Context<Self>) {
        self.commit(cx);
        if let Some(image_id) = self.selected_row().and_then(|s| s.image_id.clone()) {
            let app = self.app.clone();
            cx.defer(move |cx| app::set_route(&app, Route::Editor(image_id), cx));
        }
    }

    pub(crate) fn picture_for(&self, step_id: &str) -> Option<Arc<RenderImage>> {
        self.thumbs
            .iter()
            .find(|(i, _)| i == step_id)
            .map(|(_, t)| t.clone())
    }
}

fn step_row_view(
    t: &Theme,
    this: &GuideView,
    s: &StepRow,
    first: bool,
    last: bool,
    cx: &mut Context<GuideView>,
) -> gpui::AnyElement {
    let selected = this.selected.as_deref() == Some(s.id.as_str());
    let id = s.id.clone();
    let mut row = div()
        .id(SharedString::from(format!("guide-step-{}", s.id)))
        .role(Role::ListItem)
        .aria_label(SharedString::from(s.title.clone()))
        .focusable()
        .tab_stop(true)
        .w_full()
        .min_h(px(56.))
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(8.))
        .rounded(px(scale::RADIUS_SM))
        .cursor_pointer()
        .when(selected, |d| d.bg(rgba_of(t.raised)))
        .hover({
            let c = rgba_of(t.raised);
            move |d| d.bg(c)
        })
        .on_click(cx.listener({
            let id = id.clone();
            move |this, _, _, cx| this.select(&id, cx)
        }));
    row = row
        .child(icon("grip-vertical", 14., t.faint))
        .child(
            div()
                .size(px(26.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(13.))
                .border_1()
                .border_color(rgba_of(if selected { t.text } else { t.line }))
                .bg(rgba_of(t.rec))
                .text_color(rgba_of(t.on_rec))
                .text_size(px(type_scale::CAPTION))
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(s.label.clone())),
        )
        .child(match (this.picture_for(&s.id), s.is_note) {
            (Some(im), _) => div()
                .w(px(64.))
                .h(px(40.))
                .flex_none()
                .rounded(px(4.))
                .overflow_hidden()
                .child(img(im).w(px(64.)).h(px(40.)).object_fit(ObjectFit::Cover)),
            (None, note) => div()
                .w(px(64.))
                .h(px(40.))
                .flex_none()
                .rounded(px(4.))
                .border_1()
                .border_color(rgba_of(t.line))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(if note { "file-text" } else { "image" }, 16., t.faint)),
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.text))
                        .when(s.is_note, |d| d.italic())
                        .overflow_hidden()
                        .child(SharedString::from(if s.title.is_empty() {
                            tr!("Untitled step")
                        } else {
                            s.title.clone()
                        })),
                )
                .when(s.has_proposals, |d| {
                    d.child(
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.warning))
                            .child(tr!("Redaction proposals to review")),
                    )
                }),
        );
    if selected {
        let mini = |name: &'static str, aria: String, id2: &'static str| {
            icon_button(
                t,
                SharedString::from(format!("{id2}-{}", s.id)),
                name,
                aria,
                false,
                None,
            )
        };
        row = row
            .child(
                mini("arrow-up", tr!("Move up"), "guide-up")
                    .when(first, |d| d.opacity(0.35))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |this, _, _, cx| this.move_step(&id, -1, cx)
                    })),
            )
            .child(
                mini("arrow-down", tr!("Move down"), "guide-down")
                    .when(last, |d| d.opacity(0.35))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |this, _, _, cx| this.move_step(&id, 1, cx)
                    })),
            );
    }
    row.into_any_element()
}

impl Render for GuideView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let app = self.app.clone();
        let crumb = crumb_link(
            &t,
            "guide-back",
            tr_ctx!("breadcrumb", "Library /"),
            tr!("Back to the library"),
        )
        .on_click(move |_, _, cx| {
            let app = app.clone();
            cx.defer(move |cx| app::set_route(&app, Route::Library, cx));
        });
        let (title, count, can_undo, can_redo) = match self.data() {
            Some(d) => (d.title.clone(), d.steps.len(), d.can_undo, d.can_redo),
            None => (tr!("Click Guide"), 0, false, false),
        };
        let heading = if count > 0 {
            format!(
                "{title}  \u{b7}  {}",
                trn!("{n} step", "{n} steps", count as u64)
            )
        } else {
            title
        };
        let mut actions: Vec<gpui::AnyElement> = Vec::new();
        actions.push(
            icon_button(
                &t,
                "guide-undo",
                "undo-2",
                tr!("Undo"),
                false,
                Some(Vec::new()),
            )
            .when(!can_undo, |d| d.opacity(0.35))
            .on_click(cx.listener(move |this, _, _, cx| {
                if can_undo {
                    this.history(true, cx);
                }
            }))
            .into_any_element(),
        );
        actions.push(
            icon_button(
                &t,
                "guide-redo",
                "redo-2",
                tr!("Redo"),
                false,
                Some(Vec::new()),
            )
            .when(!can_redo, |d| d.opacity(0.35))
            .on_click(cx.listener(move |this, _, _, cx| {
                if can_redo {
                    this.history(false, cx);
                }
            }))
            .into_any_element(),
        );
        actions.push(
            button_with_icon(
                &t,
                "guide-copy",
                tr!("Copy as rich text"),
                ButtonKind::Secondary,
                Some("copy"),
            )
            .on_click(cx.listener(|this, _, _, cx| export::copy_rich_text(this, cx)))
            .into_any_element(),
        );
        actions.push(
            button_with_icon(
                &t,
                "guide-export",
                tr!("Export\u{2026}"),
                ButtonKind::Primary,
                Some("download"),
            )
            .on_click(cx.listener(|this, _, _, cx| export::open(this, cx)))
            .into_any_element(),
        );
        let toolbar = page_toolbar_actions(
            &t,
            "guide-toolbar",
            tr!("Click Guide"),
            crumb,
            SharedString::from(heading),
            actions,
        );

        let body = match &self.load {
            Load::Idle | Load::Loading => div()
                .p(px(24.))
                .text_color(rgba_of(t.text_3))
                .child(tr!("Loading\u{2026}"))
                .into_any_element(),
            Load::Failed(e) => div()
                .p(px(24.))
                .text_color(rgba_of(t.text_2))
                .child(SharedString::from(e.clone()))
                .into_any_element(),
            Load::Ready(d) => {
                let d = d.clone();
                let n = d.steps.len();
                let rows: Vec<gpui::AnyElement> = d
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(i, s)| step_row_view(&t, self, s, i == 0, i + 1 == n, cx))
                    .collect();
                let numbering = |label: &'static str, key: &'static str, id: &'static str| {
                    let on = d.numbering == key;
                    div()
                        .id(id)
                        .role(Role::Button)
                        .aria_label(SharedString::from(label))
                        .px(px(10.))
                        .h(px(30.))
                        .flex()
                        .items_center()
                        .rounded(px(scale::RADIUS_SM))
                        .cursor_pointer()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(if on { t.text } else { t.text_3 }))
                        .when(on, |x| x.bg(rgba_of(t.raised)))
                        .child(SharedString::from(label))
                        .on_click(cx.listener(move |this, _, _, cx| this.set_numbering(key, cx)))
                };
                let left = div()
                    .w(px(380.))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .p(px(24.))
                    .border_r_1()
                    .border_color(rgba_of(t.border))
                    .child(self.title.clone())
                    .child(self.intro.clone())
                    .child(
                        div()
                            .id("guide-steps")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .children(rows),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                button_with_icon(
                                    &t,
                                    "guide-add-note",
                                    tr!("Note step"),
                                    ButtonKind::Secondary,
                                    Some("plus"),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.add_note_step(cx))),
                            )
                            .child(div().flex_1())
                            .child(numbering("1 2 3", "number", "guide-num-1"))
                            .child(numbering("A B C", "upper", "guide-num-a")),
                    );
                let right = self.selected_view(&t, cx);
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(left)
                    .child(right)
                    .into_any_element()
            }
        };
        div()
            .id("guide-view")
            .role(Role::Main)
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgba_of(t.ground))
            .text_color(rgba_of(t.text))
            .child(toolbar)
            .when_some(self.error.clone(), |d, e| {
                d.child(
                    div()
                        .px(px(24.))
                        .py(px(8.))
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.warning))
                        .child(SharedString::from(e)),
                )
            })
            .child(body)
            .when_some(self.export.as_ref(), |d, ex| {
                d.child(export::sheet(&t, ex, self, cx))
            })
    }
}

impl GuideView {
    fn selected_view(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(s) = self.selected_row().cloned() else {
            return div()
                .flex_1()
                .p(px(24.))
                .text_color(rgba_of(t.text_3))
                .child(tr!("This guide has no steps yet"))
                .into_any_element();
        };
        let id = s.id.clone();
        let mut col = div()
            .id("guide-selected")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(24.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(badge(t, SharedString::from(s.label.clone())))
                    .child(div().flex_1().child(self.step_title.clone())),
            )
            .child(self.step_note.clone());
        if let Some((_, im)) = self
            .picture
            .as_ref()
            .filter(|(i, _)| Some(i) == s.image_id.as_ref())
        {
            col = col.child(
                div()
                    .w_full()
                    .rounded(px(scale::RADIUS_MD))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .child(img(im.clone()).w_full().object_fit(ObjectFit::Contain)),
            );
        }
        if !s.is_note {
            let (a, b, c, e) = (id.clone(), id.clone(), id.clone(), id.clone());
            col =
                col.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            button_with_icon(
                                t,
                                "guide-edit-image",
                                tr!("Edit image"),
                                ButtonKind::Primary,
                                Some("pencil"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.edit_image(cx))),
                        )
                        .child(
                            button_with_icon(
                                t,
                                "guide-retake",
                                tr!("Retake"),
                                ButtonKind::Secondary,
                                Some("rotate-ccw"),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.retake(&a, cx))),
                        )
                        .child(
                            button_with_icon(
                                t,
                                "guide-dup",
                                tr!("Duplicate"),
                                ButtonKind::Secondary,
                                Some("copy"),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.duplicate(&b, cx))),
                        )
                        .child(
                            button_with_icon(
                                t,
                                "guide-del",
                                tr!("Delete"),
                                ButtonKind::Ghost,
                                Some("trash-2"),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.delete(&c, cx))),
                        )
                        .child(div().flex_1())
                        .child(
                            button_with_icon(
                                t,
                                "guide-crop",
                                if s.focus_crop {
                                    tr!("Focus crop")
                                } else {
                                    tr!("Whole window")
                                },
                                ButtonKind::Ghost,
                                Some("crop"),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.focus_crop(&e, !s.focus_crop, cx),
                            )),
                        ),
                );
            if let Some((x, y)) = s.click {
                col = col.child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_3))
                        .mono()
                        .child(SharedString::from(format!(
                            "{} \u{b7} {}",
                            tr!(
                                "click at {x}, {y}",
                                x = x.round() as i64,
                                y = y.round() as i64
                            ),
                            tr!("taken on mouse-down")
                        ))),
                );
            }
        } else {
            let c = id.clone();
            col = col.child(
                div().flex().child(
                    button_with_icon(
                        t,
                        "guide-del",
                        tr!("Delete"),
                        ButtonKind::Ghost,
                        Some("trash-2"),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.delete(&c, cx))),
                ),
            );
        }
        col.into_any_element()
    }
}
