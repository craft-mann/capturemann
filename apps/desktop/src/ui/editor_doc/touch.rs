//! The document side of the 0.3.4 editor tools: running the measurements and the recognizer
//! off the UI thread, and forwarding the panels' controls to the display-free state. Every
//! change of the capture still goes through the daemon queue (`after_input` -> `submit`).

use super::*;
use crate::editor::touchup;
use cm_core::annotation::{BlueprintColor, BlueprintStyle, MagnifyShape, TextAlign};
use cm_i18n::tr;

impl EditorDoc {
    /// Runs what the state asked for after an input: a measurement for Retype or Move area, the
    /// recognized lines for Retype, the Blueprint suggestion. Nothing here changes the capture.
    pub(super) fn touch_tick(&mut self, cx: &mut Context<Self>) {
        if !self.is_ready() {
            return;
        }
        if let Some(req) = self.state.take_measure_request() {
            self.run_measure(req, cx);
        }
        if let Some(req) = self.state.take_auto_request() {
            self.run_auto_text(req, cx);
        }
        if self.state.lines_wanted() {
            self.fetch_lines(cx);
        }
        if self.bp_after_drain && self.pending() == 0 {
            self.bp_after_drain = false;
            self.fetch_blueprint(cx);
        }
    }

    /// Measures the fill and text style of a box on the picture with the earlier base edits
    /// applied (ANN-9, base-edit rules 5), in a background task.
    fn run_measure(&mut self, req: touchup::MeasureRequest, cx: &mut Context<Self>) {
        let Some(base) = self.base.clone() else {
            self.state
                .touch_measured(req, Err("the picture is not loaded".into()));
            return;
        };
        let ops = self.state.measure_base();
        let (rect, text) = (req.rect, req.text.clone());
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_executor()
                .spawn(async move {
                    let t = (!text.is_empty()).then_some(text.as_str());
                    touchup::measure(&base, &ops, rect, t)
                })
                .await;
            this.update(cx, |d, cx| {
                if d.closing {
                    return;
                }
                d.state.touch_measured(req, res);
                d.sync_preview(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Samples the picture under a new text once (ANN-1.AC8, design request D10). Without base
    /// edits it reads the loaded picture at once, so the colour is in place before the first
    /// frame of the text box; with them it renders them first, in a background task.
    fn run_auto_text(
        &mut self,
        req: crate::editor::state::text_auto::AutoRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(base) = self.base.clone() else {
            return;
        };
        let ops = self.state.measure_base();
        if ops.is_empty() {
            if let Ok(a) = touchup::auto_text(&base, &ops, req.at, req.size) {
                self.state.auto_text_measured(req.seq, a);
                self.sync_preview(cx);
            }
            return;
        }
        cx.spawn(async move |this, cx| {
            let seq = req.seq;
            let res = cx
                .background_executor()
                .spawn(async move { touchup::auto_text(&base, &ops, req.at, req.size) })
                .await;
            this.update(cx, |d, cx| {
                if d.closing {
                    return;
                }
                if let Ok(a) = res {
                    d.state.auto_text_measured(seq, a);
                    d.sync_preview(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The Auto button of the Text panel.
    pub fn choose_text_auto(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        self.state.choose_auto();
        self.after_input(Vec::new(), cx);
    }

    /// Asks the daemon for the recognized text lines (`suggest_blueprint`, read-only). Until the
    /// saves have landed it waits: the daemon reads the stored base edits.
    fn fetch_lines(&mut self, cx: &mut Context<Self>) {
        if self.pending() > 0 || self.state.touch.lines_loading {
            return;
        }
        if !crate::editor::redact::available() {
            // No recognizer: the box can still be dragged by hand.
            self.state
                .set_lines(Err(crate::editor::redact::unavailable_hint().map_or_else(
                    || tr!("Text recognition is not available here."),
                    str::to_owned,
                )));
            return;
        }
        let Some(daemon) = self.backend.daemon.clone() else {
            return;
        };
        self.state.set_lines_loading();
        let rt = self.backend.rt.clone();
        let id = self.capture_id.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    Backend::call(&daemon, "suggest_blueprint", touchup::suggest_request(&id))
                        .await
                        .and_then(|v| touchup::parse_lines(&v))
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
            this.update(cx, |d, cx| {
                if d.closing {
                    return;
                }
                d.state.set_lines(res);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- Blueprint ---------------------------------------------------------------------------

    /// The Blueprint command of the view bar: a second press closes the panel; with a stored
    /// Blueprint the panel opens on it; otherwise the recognizer runs and every line becomes a
    /// bar in a draft.
    pub fn blueprint_start(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        if self.state.blueprint_active() {
            self.state.blueprint_cancel();
            cx.notify();
            return;
        }
        if self.state.touch.bp_loading {
            return;
        }
        self.finish_pending_input(cx);
        if self.state.blueprint_open_existing() {
            cx.notify();
            return;
        }
        if !crate::editor::redact::available() {
            self.state.touch.notice = Some(crate::editor::redact::unavailable_hint().map_or_else(
                || tr!("Text recognition is not available here, so Blueprint cannot find the text."),
                str::to_owned,
            ));
            cx.notify();
            return;
        }
        self.state.set_blueprint_loading(true);
        self.state.touch.notice = None;
        if self.pending() > 0 {
            self.bp_after_drain = true;
        } else {
            self.fetch_blueprint(cx);
        }
        cx.notify();
    }

    fn fetch_blueprint(&mut self, cx: &mut Context<Self>) {
        let Some(daemon) = self.backend.daemon.clone() else {
            self.state.set_blueprint_loading(false);
            return;
        };
        let rt = self.backend.rt.clone();
        let id = self.capture_id.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    Backend::call(&daemon, "suggest_blueprint", touchup::suggest_request(&id))
                        .await
                        .and_then(|v| touchup::parse_lines(&v))
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
            this.update(cx, |d, cx| {
                if d.closing {
                    return;
                }
                match res {
                    Ok(lines) => d.state.blueprint_open(&lines),
                    Err(e) => {
                        d.state.set_blueprint_loading(false);
                        d.state.touch.notice = Some(e);
                    }
                }
                d.sync_preview(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn blueprint_toggle(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.blueprint_toggle(i);
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn blueprint_apply(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.blueprint_apply();
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn blueprint_cancel(&mut self, cx: &mut Context<Self>) {
        self.state.blueprint_cancel();
        self.sync_preview(cx);
        cx.notify();
    }

    pub fn set_blueprint_style(&mut self, style: BlueprintStyle, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_blueprint_style(style);
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn set_blueprint_color(&mut self, color: BlueprintColor, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_blueprint_color(color);
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn set_blueprint_radius(&mut self, radius: f64, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_blueprint_radius(radius);
        self.after_input(e.into_iter().collect(), cx);
    }

    // ---- panels --------------------------------------------------------------------------------

    pub fn set_lens_shape(&mut self, shape: MagnifyShape, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_lens_shape(shape);
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn set_lens_connector(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_lens_connector(on);
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn set_lens_badge(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_lens_badge(on);
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn set_retype_weight(&mut self, weight: u16, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_retype_weight(weight);
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn set_retype_align(&mut self, align: TextAlign, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.set_retype_align(align);
        self.after_input(e.into_iter().collect(), cx);
    }

    /// "Remove contents" of the Move area panel (also Delete on a chosen source).
    pub fn remove_contents(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let e = self.state.remove_contents();
        self.after_input(e.into_iter().collect(), cx);
    }

    pub fn set_snap_on(&mut self, on: bool, cx: &mut Context<Self>) {
        self.state.set_snap_on(on);
        cx.notify();
    }

    /// The busy-background sheet: Cancel stores nothing, Use anyway goes on (ANN-9.AC3).
    pub fn busy_choice(&mut self, use_anyway: bool, cx: &mut Context<Self>) {
        self.state.busy_choice(use_anyway);
        self.sync_preview(cx);
        cx.notify();
    }

    /// Cut inside the text being typed: returns the text for the clipboard.
    pub fn text_cut(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let t = self.state.text_cut();
        self.sync_preview(cx);
        cx.notify();
        t
    }

    /// A press or drag that selects text changed the caret: repaint.
    pub fn touch_notify(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
}
