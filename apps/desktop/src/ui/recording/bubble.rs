//! The webcam bubble (DESIGN 5.4, CAM-1, CAM-4, board R18): a borderless always-on-top window
//! showing the live camera as a circle (or the 4:3 rounded rectangle) with a 4 px `rec` ring that
//! is part of its outer size, no shadow and no backdrop. The window is the bubble plus a
//! transparent band for the toolbar above it (below it near the top edge of the display, see
//! `geometry::bubble_layout`); everything that is not the bubble, or the toolbar while it shows,
//! ignores the mouse. Drag to move (it snaps to the corners and edge midpoints with a 24 px margin
//! within 32 px), hover for the toolbar: S, M, L, circle, rounded, mirror, camera off.
//!
//! The window is excluded from capture (`harden` in `open_bubble`), so `screen.mp4` never
//! contains it; the camera track is recorded separately and the bubble is composited again at
//! export (CAM-1.AC2).

use cm_i18n::tr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    div, img, prelude::*, px, Context, Entity, FontWeight, MouseButton, ObjectFit, Render,
    RenderImage, Role, SharedString, Subscription, Task, Window,
};

use crate::platform_window::NativeWindow;
use crate::recording::drag::DragState;
use crate::recording::geometry::{
    self, bubble_dims, bubble_layout, BubbleLayout, BubbleShape, BubbleSize, Rect, BUBBLE_RING,
    TOOLBAR_BUTTON, TOOLBAR_DIVIDER, TOOLBAR_H, TOOLBAR_ITEM_GAP, TOOLBAR_W,
};
use crate::theme::{type_scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::icon;
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{self as rec, RecState};

/// Pull a new camera frame this often (the preview runs at 15 fps).
const PULL: Duration = Duration::from_millis(50);
const SETTLE: Duration = Duration::from_millis(300);

pub struct BubbleView {
    app: Entity<AppModel>,
    rec: Entity<RecState>,
    image: Option<Arc<RenderImage>>,
    seen: u64,
    hovered: bool,
    drag: DragState,
    settle: Option<Task<()>>,
    /// Where the bubble and the toolbar sit in the window; the window follows it.
    layout: BubbleLayout,
    native: Option<NativeWindow>,
    /// The first hit test puts the window exactly on its layout (AppKit may have moved it).
    placed: bool,
    _subs: Vec<Subscription>,
}

impl BubbleView {
    pub fn new(
        app: Entity<AppModel>,
        rec: Entity<RecState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let layout = {
            let r = rec.read(cx);
            let (w, h) = bubble_dims(r.prefs.size, r.prefs.shape);
            let bubble = crate::ui::recording::bubble_rect(r).unwrap_or(Rect::new(0.0, 0.0, w, h));
            bubble_layout(&bubble, &r.display.map(|d| d.1).unwrap_or(bubble))
        };
        let subs = vec![
            cx.observe(&rec, |_, _, cx| cx.notify()),
            cx.observe_window_bounds(window, |this, window, cx| this.moved(window, cx)),
        ];
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(PULL).await;
            let alive = this.update_in(cx, |this, window, cx| {
                this.pull(cx);
                this.hit_test(window, cx);
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();
        BubbleView {
            app,
            rec,
            image: None,
            seen: 0,
            hovered: false,
            drag: DragState::default(),
            settle: None,
            layout,
            native: NativeWindow::of(window),
            placed: false,
            _subs: subs,
        }
    }

    /// Takes the newest camera frame, if there is one.
    fn pull(&mut self, cx: &mut Context<Self>) {
        let slot = self.rec.read(cx).frames.clone();
        if let Some((seq, frame)) = slot.take_newer(self.seen) {
            self.seen = seq;
            // Already BGRA and owned: moved into the image, never copied.
            let new = Arc::new(RenderImage::new([image::Frame::new(frame)]));
            if let Some(old) = self.image.replace(new) {
                cx.drop_image(old, None);
            }
            cx.notify();
        } else if !slot.has_frame() && self.image.is_some() {
            if let Some(old) = self.image.take() {
                cx.drop_image(old, None);
            }
            cx.notify();
        }
    }

    /// Sets the window's frame from a task, never inside a window update (see `NativeWindow`).
    fn place(&self, to: Rect, cx: &mut Context<Self>) {
        if let Some(n) = self.native.clone() {
            cx.spawn(async move |_, _| n.set_frame(to.x, to.y, to.w, to.h))
                .detach();
        }
    }

    /// The window is bigger than the bubble: its corners (outside the circle or the rounded
    /// shape) and the toolbar band are transparent. Let the mouse through there so they never
    /// swallow a click meant for another app, and show the toolbar only while the pointer is over
    /// the bubble or the toolbar.
    fn hit_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A plain click never moves the window, so the drag flag must expire by itself (R4-8).
        if self
            .drag
            .is_live(Instant::now(), crate::platform_window::left_button_down())
        {
            return;
        }
        let Some((x, y, w, h)) = crate::platform_window::frame(window) else {
            return;
        };
        if !self.placed {
            self.placed = true;
            let to = self.layout.window;
            if (x - to.x).abs() > 0.5
                || (y - to.y).abs() > 0.5
                || (w - to.w).abs() > 0.5
                || (h - to.h).abs() > 0.5
            {
                self.place(to, cx);
                return;
            }
        }
        let Some((mx, my)) = crate::platform_window::mouse_location() else {
            return;
        };
        let shape = self.rec.read(cx).prefs.shape;
        let radius = shape.radius(self.layout.bubble.w, self.layout.bubble.h);
        let over = geometry::layout_hit(&self.layout, (x, y), radius, self.hovered, mx, my);
        crate::platform_window::set_ignores_mouse(window, !over);
        if over != self.hovered {
            self.hovered = over;
            cx.notify();
        }
    }

    fn moved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        if !self
            .drag
            .is_live(now, crate::platform_window::left_button_down())
        {
            return;
        }
        self.drag.moved(now);
        self.settle = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update_in(cx, |this, window, cx| this.settled(window, cx))
                .ok();
        }));
    }

    /// The drag ended: snap the bubble (not the larger window) inside the recorded area
    /// (CAM-1.AC6), let the window follow it (the toolbar may flip) and remember the position as
    /// fractions of the area.
    fn settled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drag.clear();
        let Some((x, y, w, h)) = crate::platform_window::frame(window) else {
            return;
        };
        let (area, display) = {
            let r = self.rec.read(cx);
            (r.area, r.display.map(|d| d.1))
        };
        let Some(area) = area else {
            return;
        };
        let bubble = self.layout.bubble_at(x, y);
        let (snapped, position) = geometry::position_after_drag(&bubble, &area);
        let layout = bubble_layout(&snapped, &display.unwrap_or(area));
        let to = layout.window;
        let moved = (to.x - x).abs() > 0.5
            || (to.y - y).abs() > 0.5
            || (to.w - w).abs() > 0.5
            || (to.h - h).abs() > 0.5;
        self.layout = layout;
        if moved {
            self.place(to, cx);
        }
        cx.notify();
        self.rec.update(cx, |r, cx| {
            r.prefs.position = position;
            cx.notify();
        });
        let app = self.app.clone();
        cx.defer(move |cx| {
            app::set_setting(
                &app,
                crate::recording::start::KEY_BUBBLE_POSITION,
                position.to_json(),
                cx,
            );
        });
    }
}

/// A toolbar button (30 px).
fn tool(t: &Theme, id: &'static str, label: &str, on: bool) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.line), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.to_owned()))
        .focusable()
        .tab_stop(true)
        .size(px(TOOLBAR_BUTTON as f32))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .border_color(gpui::transparent_black())
        .when(on, |d| d.bg(rgba_of(t.raised)))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(if on { t.text } else { t.text_3 }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

impl BubbleView {
    /// The R18 toolbar: S, M, L | circle, rounded, mirror, camera off. Every button maps to a
    /// shipped setting (`bubble.size`, `bubble.shape`, `bubble.mirror`) or the HUD's camera
    /// button (`toggle_bubble`).
    fn toolbar(&self, t: &Theme, prefs: rec::BubblePrefs) -> impl IntoElement {
        let app = self.app.clone();
        let mut sizes = Vec::new();
        for size in BubbleSize::ALL {
            let name = match size {
                BubbleSize::S => "S",
                BubbleSize::M => "M",
                BubbleSize::L => "L",
            };
            let id = match size {
                BubbleSize::S => "bubble-size-s",
                BubbleSize::M => "bubble-size-m",
                BubbleSize::L => "bubble-size-l",
            };
            let app = app.clone();
            sizes.push(
                tool(
                    t,
                    id,
                    &tr!("Size {size}", size = size.label()),
                    prefs.size == size,
                )
                .font_weight(FontWeight::MEDIUM)
                .mono()
                .on_click(move |_, _, cx| {
                    let app = app.clone();
                    cx.defer(move |cx| rec::set_bubble(&app, |p| p.size = size, cx));
                })
                .child(name),
            );
        }
        let shape_button = |id: &'static str, shape: BubbleShape, glyph: &'static str| {
            let app = app.clone();
            tool(
                t,
                id,
                &tr!("{shape} shape", shape = shape.label()),
                prefs.shape == shape,
            )
            .on_click(move |_, _, cx| {
                let app = app.clone();
                cx.defer(move |cx| rec::set_bubble(&app, |p| p.shape = shape, cx));
            })
            .child(icon(
                glyph,
                14.,
                if prefs.shape == shape {
                    t.text
                } else {
                    t.text_3
                },
            ))
        };
        let (mirror_app, hide_app) = (app.clone(), app.clone());
        div()
            .id("bubble-toolbar")
            .role(Role::Toolbar)
            .aria_label(tr!("Webcam bubble"))
            .absolute()
            .left(px(self.layout.toolbar.x as f32))
            .top(px(self.layout.toolbar.y as f32))
            .w(px(TOOLBAR_W as f32))
            .h(px(TOOLBAR_H as f32))
            .p(px(4.))
            .flex()
            .items_center()
            .gap(px(TOOLBAR_ITEM_GAP as f32))
            .rounded(px(12.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(crate::theme::derived::HUD_FILL))
            .children(sizes)
            .child(
                div()
                    .w(px(TOOLBAR_DIVIDER as f32))
                    .h(px(18.))
                    .flex_none()
                    .bg(rgba_of(t.line)),
            )
            .child(shape_button(
                "bubble-shape-circle",
                BubbleShape::Circle,
                "circle",
            ))
            .child(shape_button(
                "bubble-shape-rounded",
                BubbleShape::Rounded,
                "rectangle-horizontal",
            ))
            .child(
                tool(
                    t,
                    "bubble-mirror",
                    &if prefs.mirror {
                        tr!("Mirror on")
                    } else {
                        tr!("Mirror off")
                    },
                    prefs.mirror,
                )
                .on_click(move |_, _, cx| {
                    let app = mirror_app.clone();
                    cx.defer(move |cx| rec::set_bubble(&app, |p| p.mirror = !p.mirror, cx));
                })
                .child(icon(
                    "flip-horizontal-2",
                    14.,
                    if prefs.mirror { t.text } else { t.text_3 },
                )),
            )
            .child(
                tool(t, "bubble-hide", &tr!("Hide the bubble"), false)
                    .on_click(move |_, _, cx| {
                        let app = hide_app.clone();
                        cx.defer(move |cx| rec::toggle_bubble(&app, cx));
                    })
                    .child(icon("video-off", 14., t.text_3)),
            )
    }
}

impl Render for BubbleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let (prefs, error) = {
            let r = self.rec.read(cx);
            (r.prefs, r.frames.error())
        };
        let bubble_box = self.layout.bubble;
        let radius = prefs.shape.radius(bubble_box.w, bubble_box.h) as f32;
        let ring = BUBBLE_RING as f32;

        let content = match &self.image {
            Some(image) => img(image.clone())
                .size_full()
                .rounded(px((radius - ring).max(0.)))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .rounded(px((radius - ring).max(0.)))
                .bg(rgba_of(crate::theme::derived::BUBBLE_PLACEHOLDER))
                .child(icon(
                    "rec-user",
                    64.,
                    crate::theme::derived::BUBBLE_PLACEHOLDER_ICON,
                ))
                .when_some(error, |d, e| {
                    d.child(
                        div()
                            .px(px(16.))
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.text_3))
                            .child(SharedString::from(e)),
                    )
                })
                .into_any_element(),
        };

        let bubble = div()
            .id("webcam-bubble")
            .role(Role::Group)
            .aria_label(SharedString::from(if self.image.is_some() {
                tr!("Webcam bubble, live preview")
            } else {
                tr!("Webcam bubble, camera not available")
            }))
            .absolute()
            .left(px(bubble_box.x as f32))
            .top(px(bubble_box.y as f32))
            .w(px(bubble_box.w as f32))
            .h(px(bubble_box.h as f32))
            .relative()
            .overflow_hidden()
            .rounded(px(radius))
            .border(px(ring))
            .border_color(rgba_of(t.rec))
            .bg(rgba_of(crate::theme::derived::BUBBLE_PLACEHOLDER))
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| {
                    this.drag.press(Instant::now());
                    window.start_window_move();
                }),
            )
            // The native move loop may swallow the mouse-up; when it does arrive, the click is over.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag.clear()),
            )
            .child(content);

        div()
            .size_full()
            .relative()
            .font_family(UI_FONT)
            .font_features(ui_features())
            .child(bubble)
            .when(self.hovered, |d| d.child(self.toolbar(&t, prefs)))
    }
}
