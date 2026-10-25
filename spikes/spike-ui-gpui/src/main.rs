//! S5 prototype (GPUI): library grid + preview surface + keycap button, docs/DESIGN.md dark tokens.
//! `SPIKE_STATIC=1` stops the preview animation (idle-memory measurement).
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    actions, div, img, prelude::*, px, rgb, rgba, size, App, Bounds, Context, FocusHandle,
    KeyBinding, RenderImage, Role, SharedString, Window, WindowBounds, WindowOptions,
};
use image::{Frame, RgbaImage};

actions!(spike, [Export, Quit]);

const W: u32 = 640;
const H: u32 = 360;

/// CPU plasma, BGRA (gpui's atlas wants BGRA bytes inside an `image::Frame`).
fn plasma(t: f32) -> Arc<RenderImage> {
    let mut buf = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let (fx, fy) = (x as f32 / 40.0, y as f32 / 40.0);
            let v = (fx + t).sin() + (fy * 1.3 - t).sin() + ((fx + fy + t) * 0.7).sin();
            let c = ((v / 3.0 + 1.0) * 0.5 * 255.0) as u8;
            let i = ((y * W + x) * 4) as usize;
            buf[i..i + 4].copy_from_slice(&[255 - c, c / 2 + 60, c, 255]); // B G R A
        }
    }
    let image = RgbaImage::from_raw(W, H, buf).unwrap();
    Arc::new(RenderImage::new([Frame::new(image)]))
}

struct Studio {
    start: Instant,
    first: bool,
    animate: bool,
    frame: Arc<RenderImage>,
    focus: FocusHandle,
    exported: u32,
}

fn keycap(label: &'static str) -> impl IntoElement {
    div()
        .px(px(5.))
        .h(px(18.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(rgb(0x22262e))
        .border_1()
        .border_color(rgb(0x343943))
        .text_size(px(11.))
        .text_color(rgb(0xb3b8c2))
        .child(label)
}

impl Render for Studio {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.first {
            self.first = false;
            println!("first_render_ms={}", self.start.elapsed().as_millis());
        }
        if self.animate {
            let t = self.start.elapsed().as_secs_f32();
            let old = std::mem::replace(&mut self.frame, plasma(t));
            let _ = window.drop_image(old);
            window.request_animation_frame();
        }
        let cards = (0usize..12).map(|i| {
            let title: SharedString = format!("Capture {:02}", i + 1).into();
            div()
                .id(("card", i))
                .focusable()
                .tab_stop(true)
                .role(Role::Button)
                .aria_label(title.clone())
                .w(px(150.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(div().h(px(84.)).rounded(px(8.)).bg(rgb(0x1a1d23)))
                .child(div().text_size(px(13.)).text_color(rgb(0xf4f5f7)).child(title))
                .focus_visible(|s| s.border_1().border_color(rgb(0x9d8fff)))
        });
        div()
            .id("root")
            .font_family("Helvetica Neue")
            .role(Role::Application)
            .aria_label("Capturemann Studio")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Export, _, cx| {
                this.exported += 1;
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .size_full()
            .flex()
            .bg(rgb(0x0b0c10))
            .text_color(rgb(0xf4f5f7))
            .child(
                div().w(px(180.)).h_full().bg(rgb(0x121419)).p(px(12.)).flex().flex_col().gap(px(8.))
                    .child(div().text_size(px(11.)).text_color(rgb(0x8b919c)).child("LIBRARY"))
                    .child(div().px(px(8.)).py(px(4.)).rounded(px(6.)).bg(rgba(0x8b7cff29)).text_color(rgb(0x9d8fff)).child("All captures"))
                    .child(div().px(px(8.)).py(px(4.)).text_color(rgb(0xb3b8c2)).child("Recordings")),
            )
            .child(
                div().flex_1().p(px(16.)).flex().flex_col().gap(px(14.))
                    .child(
                        div().h(px(260.)).bg(rgb(0x000000)).rounded(px(8.)).flex().justify_center()
                            .child(img(self.frame.clone()).h_full())
                    )
                    .child(
                        div().flex().items_center().gap(px(10.))
                            .child(
                                div().id("export").focusable().tab_stop(true).role(Role::Button)
                                    .aria_label("Export")
                                    .h(px(28.)).px(px(12.)).flex().items_center().gap(px(8.))
                                    .rounded(px(6.)).bg(rgb(0x6352e8)).text_color(rgb(0xffffff))
                                    .hover(|s| s.bg(rgb(0x6f5ff0)))
                                    .focus_visible(|s| s.border_2().border_color(rgb(0x9d8fff)))
                                    .on_click(cx.listener(|this, _, _, cx| { this.exported += 1; cx.notify(); }))
                                    .child("Export")
                                    .child(keycap("Cmd+E")),
                            )
                            .child(div().text_size(px(12.)).text_color(rgb(0x8b919c)).child(format!("exported {}", self.exported))),
                    )
                    .child(div().flex().flex_wrap().gap(px(12.)).children(cards)),
            )
    }
}

fn main() {
    let start = Instant::now();
    let animate = std::env::var_os("SPIKE_STATIC").is_none();
    gpui_platform::application().run(move |cx: &mut App| {
        cx.bind_keys([KeyBinding::new("cmd-e", Export, None), KeyBinding::new("cmd-q", Quit, None)]);
        let bounds = Bounds::centered(None, size(px(1000.), px(680.)), cx);
        cx.open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            move |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    Studio { start, first: true, animate, frame: plasma(0.0), focus, exported: 0 }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
