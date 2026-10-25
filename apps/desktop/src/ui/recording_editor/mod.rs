//! The recording editor (EDT-1 to EDT-3, DESIGN §5.5): a preview, a transport row, an inspector
//! for speed and background, and a timeline with trim handles, hatched cuts, split markers and
//! speed badges. This file holds the state and the events; `timeline` and `panels` draw.
//!
//! Every edit is a daemon action (`trim`, `cut_segments`, `restore_segments`, `split`,
//! `set_speed`, `set_background`, `undo`, `redo`), sent one at a time; the daemon's answer is the
//! new edit list, so the GUI shows exactly what the CLI and agents would. The logic is in
//! [`crate::recedit`] (unit tested); this is the thin GPUI shell and is **not** covered by tests
//! that look at the screen.

mod ai_cards;
mod caption_panel;
mod panels;
pub(crate) mod tabs;
mod timeline;
mod view_cards;

use std::cell::Cell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use cm_core::MediaTime;
use cm_i18n::{tr, tr_ctx};
use cm_media::decode::AudioBank;
use gpui::{
    div, prelude::*, px, Bounds, Context, Entity, FocusHandle, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, Role, ScrollDelta,
    ScrollWheelEvent, SharedString, Window,
};

use crate::backend::Backend;
use crate::flows;
use crate::preview::{default_path, Presenter};
use crate::recedit::audio::AudioEngine;
use crate::recedit::captions::{build_track, PreviewCaptions};
use crate::recedit::geom::{self, Geom, Hit};
use crate::recedit::keys::{map_key, Cmd};
use crate::recedit::model::{Basis, EditRequest, EditorModel, Selection};
use crate::recedit::pacer::{Pacer, Tick};
use crate::recedit::render::{PreviewRenderer, RenderRequest, ViewContext, ViewSpec};
use crate::recedit::snap::{self, FrameGrid};
use crate::recedit::source;
use crate::recedit::BgSpec;
use crate::recording::files;
use crate::ui::app::{self, AppModel};
use crate::ui::paint::rgba_of;
use crate::ui::studio::ThemeGlobal;
use cm_media::layout_plan::{FrameLayout, LayoutPlanner, Placement};

/// A key no earlier call used. The daemon replays the answer of a repeated `request_id`, so the
/// counter must not restart when the editor is reopened (or the app restarted): the process start
/// time is part of the key.
fn next_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    static BOOT: OnceLock<u128> = OnceLock::new();
    let boot = *BOOT.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    });
    format!("ui-edit-{boot}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// What a track row on the timeline shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowKind {
    Screen,
    Camera,
    Mic,
}

impl RowKind {
    /// The name shown at the left of the track.
    pub fn label(self) -> String {
        match self {
            RowKind::Screen => tr_ctx!("track", "Screen"),
            RowKind::Camera => tr_ctx!("track", "Webcam"),
            RowKind::Mic => tr_ctx!("track", "Mic"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TrackRow {
    pub kind: RowKind,
    pub from: MediaTime,
    pub to: MediaTime,
}

/// A pointer drag on the timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Drag {
    Playhead,
    /// Moving the in (`true`) or out (`false`) handle; `t` is where it would land.
    Handle {
        is_in: bool,
        t: MediaTime,
    },
    /// Pressed on the ruler: a click seeks, a drag selects a range.
    Ruler {
        anchor: MediaTime,
        moved: bool,
    },
    Pan {
        last_x: f32,
    },
}

pub(crate) struct Session {
    pub model: EditorModel,
    pub geom: Geom,
    pub rows: Vec<TrackRow>,
    pub renderer: PreviewRenderer,
    pub title: String,
    /// `30 fps · hevc · 3024 × 1964`.
    pub stats: String,
    pub fps: f64,
    /// Why the preview is not the real picture, or what went wrong decoding it.
    pub source_note: Option<String>,
    pub preview_error: Option<String>,
    /// The picture on screen has no background or bubble (no GPU adapter).
    pub plain: bool,
    seq: u64,
    shown_seq: u64,
    wanted: Option<(MediaTime, BgSpec, FrameLayout, u64, u64)>,
    /// The recording's folder and id, to build the caption track from.
    bundle: PathBuf,
    capture_id: String,
    /// The captions the preview draws (switched on and a transcript exists).
    captions: PreviewCaptions,
    /// The zooms, cursor style, reframe and previewed aspect the renderer draws; replaced (new
    /// id) when an edit changes them.
    view: Arc<ViewSpec>,
    /// Whether the recording's cursor is drawn from the telemetry (it can be styled).
    pub cursor_separate: bool,
    pending: VecDeque<EditRequest>,
    running: bool,
    /// The playback clock (audio is the master while it runs).
    pacer: Pacer,
    /// The edited sound, when the recording has audio and an output device exists.
    audio: Option<AudioEngine>,
    /// Why there is no sound, for the status strip.
    pub audio_note: Option<String>,
    /// Output time (seconds) of the picture last shown while playing.
    out_now: f64,
    pub drag: Option<Drag>,
}

enum Load {
    Idle,
    Loading,
    Ready(Box<Session>),
    Failed(files::FileError),
}

struct Loaded {
    bundle: PathBuf,
    title: String,
    stats: String,
    fps: f64,
    basis: Basis,
    list: cm_core::EditList,
    grid: FrameGrid,
    flags: (bool, bool),
    rows: Vec<TrackRow>,
    source: Box<dyn source::FrameSource>,
    view_ctx: ViewContext,
    note: Option<String>,
    /// The decoded audio tracks (read in the background so opening never blocks the window).
    audio: Option<AudioBank>,
    audio_note: Option<String>,
}

pub struct RecordingEditor {
    app: Entity<AppModel>,
    id: Option<String>,
    load: Load,
    focus: FocusHandle,
    /// The timeline's track area in window px, as painted last frame.
    area: Rc<Cell<Bounds<Pixels>>>,
    presenter: Presenter,
    /// The captions and suggestions (M5).
    ai: ai_cards::AiEditor,
    /// Scroll state of the captions list, which scrolls inside the inspector.
    captions_scroll: gpui::ScrollHandle,
    /// The open inspector tab.
    tab: tabs::Inspector,
    /// What the tabs last followed on the timeline (a changed pick opens its tab).
    last_selection: Option<Selection>,
    last_zoom: Option<MediaTime>,
}

fn load_blocking(root: Option<PathBuf>, id: &str) -> Result<Loaded, files::FileError> {
    let root = root.ok_or(files::FileError::NoLibrary)?;
    let dir = files::find_bundle(&root, id)
        .ok_or_else(|| files::FileError::RecordingNotFound(id.to_owned()))?;
    let m = cm_library::read_manifest(&dir).map_err(|e| files::FileError::Detail(e.to_string()))?;
    let basis = Basis {
        end: cm_library::rec_edit::session_end(&m),
        live_at: cm_library::rec_edit::live_at(&m),
        gaps: m.gaps.clone(),
    };
    if basis.end <= MediaTime::ZERO {
        return Err(files::FileError::NoLength);
    }
    let list = cm_library::rec_edit::load_edits(&m);
    let tick = |t: &serde_json::Value, key: &str| {
        t.get(key)
            .and_then(serde_json::Value::as_i64)
            .map(MediaTime::from_ticks)
    };
    let present = |name: &str| {
        m.tracks
            .get(name)
            .filter(|t| t.get("missing").and_then(serde_json::Value::as_bool) != Some(true))
    };
    let screen = present("screen");
    let grid = FrameGrid::new(
        screen
            .and_then(|s| tick(s, "start"))
            .unwrap_or(MediaTime::ZERO),
        screen
            .and_then(|s| s.get("fps"))
            .and_then(serde_json::Value::as_f64),
    );
    let mut rows = vec![TrackRow {
        kind: RowKind::Screen,
        from: screen
            .and_then(|s| tick(s, "start"))
            .unwrap_or(MediaTime::ZERO),
        to: basis.end,
    }];
    for (name, kind) in [("camera", RowKind::Camera), ("mic", RowKind::Mic)] {
        if let Some(t) = present(name) {
            rows.push(TrackRow {
                kind,
                from: tick(t, "start").unwrap_or(MediaTime::ZERO),
                to: tick(t, "end").unwrap_or(basis.end).min(basis.end),
            });
        }
    }
    let details = files::read_details(&dir)?;
    let (source, note) = source::open(&dir, &m);
    // Where the cursor track is and how the recorded area maps to the picture (EDT-4, EDT-5).
    let screen_info = serde_json::to_value(&m)
        .ok()
        .and_then(|v| cm_media::spec::QuickSpec::from_manifest(&v).ok())
        .map(|q| q.screen);
    let view_ctx = ViewContext {
        bundle: dir.clone(),
        source_size: screen_info
            .as_ref()
            .map_or_else(|| source.size(), |s| s.source_size),
        display_scale: screen_info.as_ref().map_or(1.0, |s| s.scale),
        cursor_separate: screen_info.as_ref().is_some_and(|s| s.cursor_separate),
    };
    let files = source::audio_files(&dir, &m);
    let (audio, audio_note) = if files.is_empty() {
        (None, None)
    } else {
        match cm_media::tools::locate() {
            Ok(tools) => match AudioBank::load(&tools.ffmpeg, &files) {
                Ok(bank) => (Some(bank), None),
                Err(e) => (None, Some(tr!("No sound in the preview: {e}", e = e))),
            },
            Err(_) => (
                None,
                Some(tr!("No sound in the preview: FFmpeg is not installed.")),
            ),
        }
    };
    Ok(Loaded {
        bundle: dir.clone(),
        audio,
        audio_note,
        title: details.title,
        stats: details.video.unwrap_or_default(),
        fps: grid.fps,
        flags: cm_library::rec_edit::history_flags(&m),
        basis,
        list,
        grid,
        rows,
        source,
        view_ctx,
        note,
    })
}

impl RecordingEditor {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        RecordingEditor {
            app,
            id: None,
            load: Load::Idle,
            focus: cx.focus_handle(),
            area: Rc::new(Cell::new(Bounds::default())),
            presenter: Presenter::new(default_path()),
            ai: ai_cards::AiEditor::new(cx),
            captions_scroll: gpui::ScrollHandle::new(),
            tab: tabs::Inspector::Speed,
            last_selection: None,
            last_zoom: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.id.is_some()
    }

    /// Opens recording `id` (a no-op while it is open).
    pub fn set_capture(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.id.as_deref() == Some(id) {
            return;
        }
        self.close(cx);
        self.id = Some(id.to_owned());
        self.tab = tabs::Inspector::Speed;
        self.last_selection = None;
        self.last_zoom = None;
        self.load = Load::Loading;
        let root = self.app.read(cx).backend.library_root.clone();
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let key = id.clone();
            let loaded = cx
                .background_executor()
                .spawn(async move { load_blocking(root, &id) })
                .await;
            this.update(cx, |this, cx| {
                if this.id.as_deref() != Some(key.as_str()) {
                    return;
                }
                this.load = match loaded {
                    Ok(l) => Load::Ready(Box::new(Session::new(&key, l))),
                    Err(e) => {
                        app::note_open_failed(&e.to_string(), cx);
                        Load::Failed(e)
                    }
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Leaves the recording: stops the renderer thread and releases the picture.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.id = None;
        self.load = Load::Idle;
        self.presenter = Presenter::new(default_path());
        cx.notify();
    }

    fn session(&mut self) -> Option<&mut Session> {
        match &mut self.load {
            Load::Ready(s) => Some(s),
            _ => None,
        }
    }

    // ---- edits ---------------------------------------------------------------------------

    /// Queues daemon calls; they run one at a time, in order.
    fn send(&mut self, reqs: Vec<EditRequest>, cx: &mut Context<Self>) {
        if reqs.is_empty() {
            cx.notify();
            return;
        }
        if let Some(s) = self.session() {
            s.pending.extend(reqs);
        }
        self.pump(cx);
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let Some(s) = self.session() else { return };
        if s.running {
            return;
        }
        let Some(mut req) = s.pending.pop_front() else {
            cx.notify();
            return;
        };
        s.running = true;
        req.params["request_id"] = serde_json::json!(next_request_id());
        let method = req.method;
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { Backend::call(&daemon, req.method, req.params).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            this.update(cx, |this, cx| this.edit_done(method, res, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn edit_done(
        &mut self,
        method: &'static str,
        res: Result<serde_json::Value, String>,
        cx: &mut Context<Self>,
    ) {
        let app = self.app.clone();
        // An undo or redo may have changed the caption lines or switches: read them again.
        if matches!(method, "undo" | "redo") {
            self.ai.reload = true;
        }
        if let Some(s) = self.session() {
            s.running = false;
            match res {
                Ok(v) => {
                    // An undo with nothing to undo answers `changed: false` and no edit list.
                    s.model.adopt(&v);
                    // The timeline under the playhead may have changed: restart the clock.
                    s.pacer.stop();
                }
                Err(e) => {
                    // Later queued edits were meant for a state that did not happen.
                    s.pending.clear();
                    app::push_toast(
                        &app,
                        flows::error_toast_live(|| tr!("Couldn\u{2019}t apply the edit"), e),
                        cx,
                    );
                }
            }
        }
        self.pump(cx);
        cx.notify();
    }

    fn run(&mut self, cmd: Cmd, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        if cmd == Cmd::PlayPause && !s.model.playing && s.model.playhead >= s.model.list.end {
            s.model.seek(s.model.list.start);
        }
        let reqs = s.model.command(cmd);
        // Any command may move the playhead: the clock restarts from where it is now.
        s.pacer.stop();
        if matches!(
            cmd,
            Cmd::Step { .. } | Cmd::StepSecs { .. } | Cmd::Home | Cmd::End
        ) {
            s.geom.reveal(s.model.playhead);
        }
        self.send(reqs, cx);
    }

    // ---- pointer -------------------------------------------------------------------------

    fn local(&self, p: gpui::Point<Pixels>) -> (f32, f32) {
        let b = self.area.get();
        (f32::from(p.x - b.origin.x), f32::from(p.y - b.origin.y))
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let (x, y) = self.local(ev.position);
        let Some(s) = self.session() else { return };
        if ev.button == MouseButton::Middle {
            s.drag = Some(Drag::Pan { last_x: x });
            return;
        }
        if ev.button != MouseButton::Left {
            return;
        }
        let segs = s.model.segments();
        let hit = geom::hit_test(
            &s.geom,
            &s.model.list,
            &segs,
            s.model.playhead,
            s.rows.len(),
            x,
            y,
        );
        let t = s.geom.t_of(x);
        s.model.playing = false;
        match hit {
            Hit::Ruler(t) => {
                if ev.modifiers.shift {
                    s.model.shift_click(t);
                } else {
                    s.drag = Some(Drag::Ruler {
                        anchor: t,
                        moved: false,
                    });
                }
            }
            Hit::Playhead => s.drag = Some(Drag::Playhead),
            Hit::InHandle => {
                s.drag = Some(Drag::Handle {
                    is_in: true,
                    t: s.model.list.start,
                })
            }
            Hit::OutHandle => {
                s.drag = Some(Drag::Handle {
                    is_in: false,
                    t: s.model.list.end,
                })
            }
            Hit::Split(i) => {
                if let Some(at) = s.model.list.splits.get(i).copied() {
                    s.model.click_time(at);
                }
            }
            Hit::Cut(i) => {
                s.model.selection = Selection::Cut(i);
                s.model.click_time(t);
            }
            Hit::Segment(i) => {
                if ev.modifiers.shift {
                    s.model.shift_click(t);
                } else {
                    s.model.selection = Selection::Segment(i);
                    s.model.click_time(s.model.grid.round(t));
                    s.drag = Some(Drag::Playhead);
                }
            }
            Hit::Trimmed | Hit::Nothing => {
                s.model.selection = Selection::None;
                s.model.click_time(s.model.grid.round(t));
            }
        }
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let (x, _) = self.local(ev.position);
        let Some(s) = self.session() else { return };
        let Some(drag) = s.drag else { return };
        let points = snap::candidates(&s.model.list, s.model.playhead, s.model.end());
        let th = snap::threshold(s.geom.ticks_per_px());
        let at = |s: &Session, x: f32| {
            snap::snap(s.geom.t_of(x), &points, th, s.model.snapping, &s.model.grid)
        };
        match drag {
            Drag::Playhead => {
                let t = at(s, x);
                s.model.seek(t);
            }
            Drag::Handle { is_in, .. } => {
                let t = at(s, x);
                s.drag = Some(Drag::Handle { is_in, t });
            }
            Drag::Ruler { anchor, moved } => {
                let t = at(s, x);
                let far = (s.geom.x_of(anchor) - x).abs() > 3.0;
                if moved || far {
                    s.model.select_range(anchor, t);
                    s.drag = Some(Drag::Ruler {
                        anchor,
                        moved: true,
                    });
                }
            }
            Drag::Pan { last_x } => {
                s.geom.pan(last_x - x);
                s.drag = Some(Drag::Pan { last_x: x });
            }
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        let Some(drag) = s.drag.take() else { return };
        let mut reqs = Vec::new();
        match drag {
            Drag::Handle { is_in: true, t } => {
                let end = s.model.list.end;
                if t < end {
                    reqs = s.model.trim_to(t, end);
                }
            }
            Drag::Handle { is_in: false, t } => {
                let start = s.model.list.start;
                if t > start {
                    reqs = s.model.trim_to(start, t);
                }
            }
            Drag::Ruler {
                anchor,
                moved: false,
            } => {
                s.model.click_time(s.model.grid.round(anchor));
            }
            Drag::Ruler { .. } | Drag::Playhead | Drag::Pan { .. } => {}
        }
        self.send(reqs, cx);
    }

    fn on_scroll(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let (x, _) = self.local(ev.position);
        let Some(s) = self.session() else { return };
        let (dx, dy) = match ev.delta {
            ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(l) => (l.x * 20.0, l.y * 20.0),
        };
        let secondary = if cfg!(target_os = "macos") {
            ev.modifiers.platform
        } else {
            ev.modifiers.control
        };
        if secondary || ev.modifiers.control {
            s.geom.zoom_at((f64::from(dy) * 0.01).exp(), x);
        } else {
            s.geom.pan(-(dx + if dx == 0.0 { dy } else { 0.0 }));
        }
        cx.notify();
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Typed text is no longer stopped by the caption field (it goes to the input method), so
        // the shortcuts below must leave the keys of a focused field alone.
        let k = &ev.keystroke;
        let secondary = if cfg!(target_os = "macos") {
            k.modifiers.platform
        } else {
            k.modifiers.control
        };
        // Cmd/Ctrl+F opens find and replace on the Captions tab, from inside a field as well.
        if self.tab == tabs::Inspector::Captions && secondary && k.key == "f" {
            self.ai_open_find(window, cx);
            cx.stop_propagation();
            return;
        }
        if self.ai.field_focused(window, cx) {
            return;
        }
        // Esc stops "play this line" (Space already pauses, and that forgets the line).
        if k.key == "escape" && self.ai.line_play.is_some() {
            self.ai_stop_line_play(cx);
            cx.stop_propagation();
            return;
        }
        if let Some(cmd) = map_key(&k.key, k.modifiers.shift, secondary, k.modifiers.alt) {
            cx.stop_propagation();
            self.run(cmd, cx);
        }
    }
}

impl Session {
    fn new(id: &str, l: Loaded) -> Session {
        let end = l.basis.end;
        let cursor_separate = l.view_ctx.cursor_separate;
        let view = Arc::new(ViewSpec {
            id: 1,
            list: l.list.clone(),
            aspect: crate::recedit::PreviewAspect::default(),
        });
        let (audio, audio_note) = match l.audio {
            Some(bank) => match AudioEngine::open(bank, l.basis.gaps.clone()) {
                Ok(a) => (Some(a), None),
                Err(e) => (None, Some(tr!("No sound in the preview: {e}", e = e))),
            },
            None => (None, l.audio_note),
        };
        Session {
            model: EditorModel::new(id, l.basis, l.list, l.grid, l.flags),
            audio,
            audio_note,
            pacer: Pacer::default(),
            out_now: 0.0,
            geom: Geom::new(800.0, end),
            rows: l.rows,
            renderer: PreviewRenderer::spawn_with(l.source, Some(l.view_ctx)),
            view,
            cursor_separate,
            title: l.title,
            stats: l.stats,
            fps: l.fps,
            source_note: l.note,
            preview_error: None,
            plain: false,
            seq: 0,
            shown_seq: 0,
            wanted: None,
            bundle: l.bundle,
            capture_id: id.to_owned(),
            captions: PreviewCaptions::default(),
            pending: VecDeque::new(),
            running: false,
            drag: None,
        }
    }

    /// Advances playback to the output time the clock says: the audio device while it plays, the
    /// wall clock otherwise (no audio, no device, or the mix is still being rendered).
    fn tick(&mut self) {
        let now = Instant::now();
        if let Some(a) = &mut self.audio {
            // What is heard follows the edit list; a finished mix replaces the old one.
            a.sync(&self.model.list);
            a.poll();
        }
        if !self.model.playing {
            if self.pacer.is_running() {
                self.pacer.stop();
            }
            if let Some(a) = &self.audio {
                if a.is_playing() {
                    a.pause();
                }
            }
            return;
        }
        if !self.pacer.is_running() {
            // Playback starts (or restarts after a seek or an edit) at the playhead.
            self.out_now = self.model.output_position().as_secs_f64();
            self.pacer.start(self.out_now, now);
            if let Some(a) = &self.audio {
                a.pause();
            }
        }
        if let Some(a) = &self.audio {
            if a.is_current() && !a.is_playing() {
                a.play_from(self.out_now);
            }
        }
        let duration = self.model.output_duration().as_secs_f64();
        let audio_pos = self.audio.as_ref().and_then(AudioEngine::position);
        match self
            .pacer
            .tick(now, audio_pos, duration, self.model.looping)
        {
            Tick::Play { out, resync } => {
                if resync {
                    if let Some(a) = &self.audio {
                        a.play_from(out);
                    }
                }
                self.out_now = out;
                self.model.set_output_time(out);
                self.geom.reveal(self.model.playhead);
            }
            Tick::Looped => {
                self.out_now = 0.0;
                self.model.set_output_time(0.0);
                if let Some(a) = &self.audio {
                    a.play_from(0.0);
                }
                self.geom.reveal(self.model.playhead);
            }
            Tick::Ended => {
                self.model.playing = false;
                if let Some(a) = &self.audio {
                    a.pause();
                }
                self.model.seek(self.model.list.end);
            }
        }
    }

    /// Asks the renderer for the picture at the playhead when it changed.
    fn request_picture(&mut self) {
        // The camera layout at the playhead (CAM-3), on the preview's own frame size.
        let planner = LayoutPlanner::new(
            &self.model.list,
            self.model.camera_base(),
            self.renderer.size(),
            Placement::Whole,
        );
        // A changed edit list or previewed aspect is a new view for the renderer.
        if self.view.list != self.model.list || self.view.aspect != self.model.preview_aspect {
            self.view = Arc::new(ViewSpec {
                id: self.view.id + 1,
                list: self.model.list.clone(),
                aspect: self.model.preview_aspect,
            });
        }
        let want = (
            self.model.playhead,
            self.model.background(),
            planner.at(self.model.playhead),
            self.view.id,
            self.captions.id(),
        );
        if self.wanted.as_ref() == Some(&want) {
            return;
        }
        self.seq += 1;
        self.renderer.request(RenderRequest {
            seq: self.seq,
            session: want.0,
            background: want.1.clone(),
            layout: want.2,
            view: Some(self.view.clone()),
            // The captions of the recording when they are switched on and a transcript exists;
            // `None` draws none. The renderer and the export share the drawing (AI-1).
            captions: self.captions.track(),
        });
        self.wanted = Some(want);
    }

    /// Brings the preview's caption track up to date (`on`: captions are switched on and a
    /// transcript exists; `rev`: the editor's counter of caption line and switch changes). A
    /// changed track makes the next picture be drawn again.
    fn sync_captions(&mut self, on: bool, rev: u64) {
        let played = self.model.played();
        let (bundle, id) = (&self.bundle, &self.capture_id);
        if self.captions.sync(on, rev, &self.model.list, || {
            build_track(bundle, id, &played)
        }) {
            self.wanted = None;
        }
    }

    /// Whether a picture is on its way (the view keeps redrawing until it arrives).
    fn waiting(&self) -> bool {
        self.shown_seq < self.seq
    }
}

impl Render for RecordingEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        // Captions: read the transcript when the recording changed or a new one arrived.
        self.ai_sync(cx);
        // The open tab follows the timeline selection and "Open captions".
        self.sync_tab(cx);
        // The caption list: what has the keyboard, how wide the window is, and any move that
        // waited for a window (Enter to the next line, back from the find bar).
        self.ai.view.typing.set(self.ai.field_focused(window, cx));
        self.ai
            .view
            .window_w
            .set(f32::from(window.viewport_size().width));
        if self.tab != tabs::Inspector::Captions {
            self.ai_leave_captions(window, cx);
        }
        self.ai_apply_pending(window, cx);
        // The preview's captions follow the switch, the lines and the edit list.
        let (captions_on, captions_rev) = (self.ai.shows_captions(), self.ai.rev);
        // Frame work first: playback, the request for a new picture, the picture that arrived.
        let mut redraw = false;
        if let Load::Ready(s) = &mut self.load {
            s.sync_captions(captions_on, captions_rev);
            s.tick();
            // "Play this line" pauses at the end of the line, or is forgotten when playback was
            // stopped some other way.
            if let Some(lp) = self.ai.line_play {
                if lp.finished(s.model.playhead, s.model.playing) {
                    s.model.playing = false;
                    self.ai.line_play = None;
                }
            }
            s.request_picture();
            if let Some(done) = s.renderer.take() {
                s.shown_seq = done.seq;
                s.plain = done.plain;
                s.preview_error = done.error;
                self.presenter.set(&done.image, window, cx);
            }
            redraw = s.model.playing || s.waiting();
        }
        if redraw {
            window.request_animation_frame();
        }

        let body = match &self.load {
            Load::Idle | Load::Loading => div()
                .p(px(24.))
                .text_color(rgba_of(t.text_3))
                .child(tr!("Loading\u{2026}"))
                .into_any_element(),
            Load::Failed(e) => div()
                .p(px(24.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(e.to_string())),
                )
                .child(self.back_button(&t, cx))
                .into_any_element(),
            Load::Ready(_) => self.ready(&t, cx),
        };
        div()
            .id("recording-editor")
            .role(Role::Group)
            .aria_label(tr!("Recording editor"))
            .key_context("RecordingEditor")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba_of(t.ground))
            .text_color(rgba_of(t.text))
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::next_request_id;

    /// A reopened editor must not reuse a key: the daemon would replay the old answer.
    #[test]
    fn request_ids_never_repeat() {
        let a: Vec<String> = (0..50).map(|_| next_request_id()).collect();
        let mut sorted = a.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), a.len());
        assert!(a[0].starts_with("ui-edit-"));
    }
}
