//! Drawing and measuring. Text is painted by the project's own renderer (`cm_render::Renderer`,
//! bundled Inter) and measured with cosmic-text configured exactly as `text.rs` lays it out
//! (Inter, line height 1.2 em, no wrapping, advanced shaping), so the word and truth boxes follow
//! the glyph positions of the painted text.

use cm_core::{AnnotationOp, Color, OpKind, Pt};
use cm_render::Renderer;
use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping};
use image::{Rgba, RgbaImage};

use super::data;
use super::rng::Rng;
use crate::schema::{Decoy, DrawnLine, DrawnWord, Truth};

static INTER: &[u8] = include_bytes!("../../../assets/Inter-Regular.ttf");

/// Font system for measuring plus the renderer for painting; built once per run.
pub struct Fonts {
    sys: FontSystem,
    renderer: Renderer,
}

impl Fonts {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_font_data(INTER.to_vec());
        db.set_sans_serif_family("Inter");
        Fonts {
            sys: FontSystem::new_with_locale_and_db("en-US".to_string(), db),
            renderer: Renderer::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Plain,
    Truth(&'static str),
    Decoy(&'static str),
}

#[derive(Clone, Debug)]
pub struct Seg {
    pub text: String,
    pub tag: Tag,
}

pub fn p(s: impl Into<String>) -> Seg {
    Seg {
        text: s.into(),
        tag: Tag::Plain,
    }
}

pub fn t(kind: &'static str, s: impl Into<String>) -> Seg {
    Seg {
        text: s.into(),
        tag: Tag::Truth(kind),
    }
}

pub fn d(what: &'static str, s: impl Into<String>) -> Seg {
    Seg {
        text: s.into(),
        tag: Tag::Decoy(what),
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color,
    pub panel: Color,
    pub panel2: Color,
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub border: Color,
    pub bubble_me: Color,
    pub term_bg: Color,
    pub term_fg: Color,
    pub term_dim: Color,
    pub term_hi: Color,
}

impl Theme {
    pub fn new(dark: bool) -> Self {
        let c = Color::rgb;
        if dark {
            Theme {
                dark,
                bg: c(20, 23, 28),
                panel: c(30, 34, 41),
                panel2: c(38, 43, 52),
                fg: c(230, 233, 238),
                muted: c(152, 162, 179),
                accent: c(106, 163, 255),
                border: c(58, 66, 78),
                bubble_me: c(36, 78, 150),
                term_bg: c(12, 14, 17),
                term_fg: c(206, 212, 220),
                term_dim: c(120, 130, 144),
                term_hi: c(120, 220, 160),
            }
        } else {
            Theme {
                dark,
                bg: c(244, 245, 247),
                panel: c(255, 255, 255),
                panel2: c(236, 239, 243),
                fg: c(27, 31, 36),
                muted: c(95, 107, 122),
                accent: c(11, 95, 255),
                border: c(213, 218, 225),
                bubble_me: c(214, 230, 255),
                term_bg: c(250, 250, 248),
                term_fg: c(40, 44, 52),
                term_dim: c(120, 128, 140),
                term_hi: c(20, 120, 70),
            }
        }
    }
}

/// Everything one picture needs: the canvas, the ground truth recorded so far, the random stream
/// and the decisions made for this picture.
pub struct Ctx<'f> {
    pub fonts: &'f mut Fonts,
    pub rng: Rng,
    pub th: Theme,
    /// Logical size and device pixel ratio.
    pub w: f32,
    pub h: f32,
    pub scale: u32,
    /// Base text size in logical px (the smallest in the picture unless a layout goes lower).
    pub px: f32,
    pub vi: bool,
    /// Picture may carry truths (otherwise decoys only).
    pub positive: bool,
    /// Real card numbers may appear (and then lone expiry or code decoys never do).
    pub card_world: bool,
    base: RgbaImage,
    ops: Vec<AnnotationOp>,
    pub lines: Vec<DrawnLine>,
    pub truths: Vec<Truth>,
    pub decoys: Vec<Decoy>,
    pub min_px: f32,
}

pub struct Placed {
    /// Logical width of the line.
    pub w: f32,
    /// Physical rect of the line box.
    pub rect: [f64; 4],
}

impl<'f> Ctx<'f> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fonts: &'f mut Fonts,
        rng: Rng,
        dark: bool,
        w: u32,
        h: u32,
        scale: u32,
        px: f32,
        vi: bool,
        positive: bool,
        card_world: bool,
    ) -> Self {
        let th = Theme::new(dark);
        let mut base = RgbaImage::new(w * scale, h * scale);
        for pxl in base.pixels_mut() {
            *pxl = rgba(th.bg);
        }
        Ctx {
            fonts,
            rng,
            th,
            w: w as f32,
            h: h as f32,
            scale,
            px,
            vi,
            positive,
            card_world,
            base,
            ops: Vec::new(),
            lines: Vec::new(),
            truths: Vec::new(),
            decoys: Vec::new(),
            min_px: f32::MAX,
        }
    }

    // ----- shapes (logical px) -----

    fn phys(&self, v: f32) -> i64 {
        (v * self.scale as f32).round() as i64
    }

    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        self.fill_a(x, y, w, h, c, 255);
    }

    pub fn fill_a(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color, alpha: u8) {
        let (x0, y0) = (self.phys(x), self.phys(y));
        let (x1, y1) = (self.phys(x + w).max(x0 + 1), self.phys(y + h).max(y0 + 1));
        let (bw, bh) = (self.base.width() as i64, self.base.height() as i64);
        for yy in y0.max(0)..y1.min(bh) {
            for xx in x0.max(0)..x1.min(bw) {
                let px = self.base.get_pixel_mut(xx as u32, yy as u32);
                let a = u32::from(alpha);
                for (i, v) in [c.r, c.g, c.b].into_iter().enumerate() {
                    px.0[i] = ((u32::from(v) * a + u32::from(px.0[i]) * (255 - a)) / 255) as u8;
                }
            }
        }
    }

    pub fn hline(&mut self, x: f32, y: f32, w: f32, c: Color) {
        self.fill(x, y, w, 1.0 / self.scale as f32, c);
    }

    pub fn vline(&mut self, x: f32, y: f32, h: f32, c: Color) {
        self.fill(x, y, 1.0 / self.scale as f32, h, c);
    }

    pub fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        self.hline(x, y, w, c);
        self.hline(x, y + h, w, c);
        self.vline(x, y, h, c);
        self.vline(x + w, y, h, c);
    }

    /// A white-ish card with a border.
    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (p, b) = (self.th.panel, self.th.border);
        self.fill(x, y, w, h, p);
        self.frame(x, y, w, h, b);
    }

    /// Background pattern over the whole canvas: 0 none, 1 diagonal stripes, 2 horizontal
    /// stripes, 3 dot grid, 4 checker, 5 vertical gradient.
    pub fn pattern(&mut self, kind: u8, strength: i32) {
        let s = self.scale as i64;
        let dark = self.th.dark;
        let (bw, bh) = (self.base.width() as i64, self.base.height() as i64);
        for y in 0..bh {
            for x in 0..bw {
                let on = match kind {
                    1 => ((x + y) / (5 * s)) % 2 == 0,
                    2 => (y / (3 * s)) % 2 == 0,
                    3 => x % (8 * s) < s && y % (8 * s) < s,
                    4 => ((x / (12 * s)) + (y / (12 * s))) % 2 == 0,
                    _ => false,
                };
                let delta = if kind == 5 {
                    strength * y as i32 / bh.max(1) as i32
                } else if on {
                    strength
                } else {
                    0
                };
                if delta == 0 {
                    continue;
                }
                let px = self.base.get_pixel_mut(x as u32, y as u32);
                for ch in 0..3 {
                    let v = i32::from(px.0[ch]) + if dark { delta } else { -delta };
                    px.0[ch] = v.clamp(0, 255) as u8;
                }
            }
        }
    }

    // ----- text -----

    /// Draws one line made of `segs` with its top-left at (`x`, `y`) in logical px and records
    /// the ground truth. Returns `None` (nothing drawn, nothing recorded) when the line would not
    /// fit inside the canvas.
    pub fn line(&mut self, x: f32, y: f32, px: f32, color: Color, segs: &[Seg]) -> Option<Placed> {
        let text: String = segs.iter().map(|s| s.text.as_str()).collect();
        if text.is_empty() {
            return None;
        }
        let ppx = px * self.scale as f32;
        let at = (self.phys(x), self.phys(y));
        let metrics = Metrics::new(ppx, ppx * 1.2);
        let mut buffer = Buffer::new(&mut self.fonts.sys, metrics);
        buffer.set_size(None, None);
        buffer.set_text(
            &text,
            &Attrs::new().family(Family::Name("Inter")),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.fonts.sys, false);
        let mut glyphs: Vec<(usize, usize, f32, f32)> = Vec::new();
        let (mut line_w, mut baseline) = (0f32, ppx);
        for (i, run) in buffer.layout_runs().enumerate() {
            if i > 0 {
                break;
            }
            line_w = run.line_w;
            baseline = run.line_y;
            glyphs.extend(run.glyphs.iter().map(|g| (g.start, g.end, g.x, g.w)));
        }
        let (bw, bh) = (self.base.width() as f32, self.base.height() as f32);
        if at.0 < 0 || at.1 < 0 || at.0 as f32 + line_w + 2.0 > bw || at.1 as f32 + ppx * 1.2 > bh {
            return None;
        }
        let x_at = |b: usize| -> f32 { glyphs.iter().find(|g| g.0 >= b).map_or(line_w, |g| g.2) };
        let end_at = |b: usize| -> f32 {
            glyphs
                .iter()
                .filter(|g| g.1 <= b)
                .map(|g| g.2 + g.3)
                .fold(0f32, f32::max)
        };
        let top = f64::from(at.1 as f32 + baseline - 0.78 * ppx);
        let height = f64::from(0.98 * ppx);
        let rect_of = |b0: usize, b1: usize| -> [f64; 4] {
            let x0 = x_at(b0);
            let x1 = end_at(b1).max(x0);
            [f64::from(at.0 as f32 + x0), top, f64::from(x1 - x0), height]
        };
        // words
        let mut words = Vec::new();
        let mut start: Option<usize> = None;
        for (i, c) in text.char_indices() {
            if c.is_whitespace() {
                if let Some(s) = start.take() {
                    words.push(DrawnWord {
                        text: text[s..i].to_string(),
                        rect: rect_of(s, i),
                    });
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        if let Some(s) = start {
            words.push(DrawnWord {
                text: text[s..].to_string(),
                rect: rect_of(s, text.len()),
            });
        }
        // truths and decoys
        let mut off = 0;
        for s in segs {
            let (b0, b1) = (off, off + s.text.len());
            off = b1;
            match s.tag {
                Tag::Plain => {}
                Tag::Truth(kind) => self.truths.push(Truth {
                    kind: kind.to_string(),
                    text: s.text.clone(),
                    rect: rect_of(b0, b1),
                    standards: data::standards(kind),
                }),
                Tag::Decoy(what) => self.decoys.push(Decoy {
                    what: what.to_string(),
                    text: s.text.clone(),
                    rect: rect_of(b0, b1),
                }),
            }
        }
        let rect = rect_of(0, text.len());
        self.lines.push(DrawnLine {
            text: text.clone(),
            rect,
            words,
        });
        self.min_px = self.min_px.min(px);
        self.ops.push(AnnotationOp::new(
            "",
            OpKind::Text {
                at: Pt::new(at.0 as f64, at.1 as f64),
                text,
                size: f64::from(ppx),
                color,
                outline: None,
                bg: None,
            },
        ));
        Some(Placed {
            w: line_w / self.scale as f32,
            rect,
        })
    }

    /// Plain text line (no ground truth).
    pub fn plain(&mut self, x: f32, y: f32, px: f32, color: Color, s: &str) -> Option<Placed> {
        self.line(x, y, px, color, &[p(s)])
    }

    /// Width in logical px of `s` at `px` (no drawing, nothing recorded).
    pub fn measure(&mut self, px: f32, s: &str) -> f32 {
        let ppx = px * self.scale as f32;
        let mut buffer = Buffer::new(&mut self.fonts.sys, Metrics::new(ppx, ppx * 1.2));
        buffer.set_size(None, None);
        buffer.set_text(
            s,
            &Attrs::new().family(Family::Name("Inter")),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.fonts.sys, false);
        let w = buffer.layout_runs().map(|r| r.line_w).fold(0f32, f32::max);
        w / self.scale as f32
    }

    /// A multi-line secret (a PEM block): every line is drawn, one truth covers the union.
    pub fn block(
        &mut self,
        x: f32,
        y: f32,
        px: f32,
        color: Color,
        lines: &[String],
        kind: &'static str,
    ) -> f32 {
        let lh = px * 1.45;
        let mut union: Option<[f64; 4]> = None;
        let mut yy = y;
        for l in lines {
            if let Some(pl) = self.line(x, yy, px, color, &[p(l.as_str())]) {
                union = Some(match union {
                    None => pl.rect,
                    Some(u) => {
                        let x0 = u[0].min(pl.rect[0]);
                        let y0 = u[1].min(pl.rect[1]);
                        let x1 = (u[0] + u[2]).max(pl.rect[0] + pl.rect[2]);
                        let y1 = (u[1] + u[3]).max(pl.rect[1] + pl.rect[3]);
                        [x0, y0, x1 - x0, y1 - y0]
                    }
                });
            }
            yy += lh;
        }
        // Only a block that was drawn completely counts.
        let drawn = self.lines.len() >= lines.len()
            && self.lines[self.lines.len() - lines.len()..]
                .iter()
                .zip(lines)
                .all(|(a, b)| &a.text == b);
        if let (Some(r), true) = (union, drawn) {
            self.truths.push(Truth {
                kind: kind.to_string(),
                text: lines.join("\n"),
                rect: r,
                standards: data::standards(kind),
            });
        }
        yy
    }

    // ----- values -----

    pub fn tr(&self, en: &'static str, vi: &'static str) -> &'static str {
        if self.vi {
            vi
        } else {
            en
        }
    }

    pub fn person(&mut self) -> String {
        let v = self.vi;
        data::person(&mut self.rng, v)
    }

    fn truth_text(&mut self, kind: &str) -> String {
        let vi = self.vi;
        let r = &mut self.rng;
        match kind {
            "card" => data::card(r).shown,
            "ssn" => data::ssn(r),
            "itin" => data::itin(r),
            "ein" => data::ein(r),
            "aba" => data::aba(r),
            "iban" => data::iban(r),
            "npi" => data::npi(r),
            "dea" => data::dea(r),
            "mbi" => data::mbi(r),
            "passport" => data::passport(r),
            "vin" => data::vin(r),
            "imei" => data::imei(r),
            "ip_address" => {
                if r.chance(0.25) {
                    data::ipv6(r)
                } else {
                    data::ipv4(r)
                }
            }
            "mac_address" => data::mac(r),
            "phone" => data::phone(r, vi),
            "email" => data::email(r),
            "token" => data::token(r),
            "url_secret" => data::url_secret(r),
            _ => data::ssn(r),
        }
    }

    /// One value for a field that could hold any of `kinds`: a truth in a positive picture
    /// (most of the time), else a decoy that imitates the kind.
    pub fn value(&mut self, kinds: &[&'static str]) -> Vec<Seg> {
        self.value_w(kinds, 36)
    }

    pub fn value_w(&mut self, kinds: &[&'static str], max_chars: usize) -> Vec<Seg> {
        let usable: Vec<&'static str> = kinds
            .iter()
            .copied()
            .filter(|k| *k != "card" || self.card_world)
            .collect();
        let kind: &'static str = if usable.is_empty() || (!self.positive && !kinds.is_empty()) {
            self.rng.pick(kinds)
        } else {
            self.rng.pick(&usable)
        };
        if self.positive && usable.contains(&kind) && self.rng.chance(0.75) {
            for _ in 0..8 {
                let s = self.truth_text(kind);
                if s.chars().count() <= max_chars {
                    return vec![t(kind, s)];
                }
            }
        }
        self.decoy_for(kind)
    }

    pub fn decoy_for(&mut self, kind: &str) -> Vec<Seg> {
        let what = *self.rng.pick(data::related_decoys(kind));
        vec![d(what, data::decoy(what, &mut self.rng))]
    }

    /// Any decoy at all (never a lone expiry or code: those only come with `card_inline`).
    pub fn any_decoy(&mut self) -> Seg {
        loop {
            let what = *self.rng.pick(&data::DECOY_KINDS);
            if what == "lone_expiry" || what == "lone_code" {
                continue;
            }
            return d(what, data::decoy(what, &mut self.rng));
        }
    }

    /// `NAME=value` (or `"name": "value",` for JSON) with the value as the truth in a positive
    /// picture, else a placeholder decoy.
    pub fn secret_line(&mut self, json: bool) -> Vec<Seg> {
        let (name, value) = data::secret_pair(&mut self.rng, json);
        let real = self.positive && self.rng.chance(0.8);
        let (pre, post) = if json {
            (format!("\"{name}\": \""), "\",")
        } else {
            (format!("{name}="), "")
        };
        let mid = if real {
            t("secret_assignment", value)
        } else {
            d(
                "placeholder_secret",
                data::decoy("placeholder_secret", &mut self.rng),
            )
        };
        let mut v = vec![p(pre), mid];
        if !post.is_empty() {
            v.push(p(post));
        }
        v
    }

    /// A card number with its expiry and code on the same line, or the decoy look-alike when no
    /// real cards are allowed (a Luhn-failing number with a lone expiry and a lone code).
    pub fn card_inline(&mut self) -> Vec<Seg> {
        let labels = [
            (" exp ", " cvv "),
            ("  Exp ", "  CVC "),
            ("  ", "  "),
            (" expires ", " code "),
        ];
        let (l1, l2) = *self.rng.pick(&labels);
        if self.card_world {
            let c = data::card(&mut self.rng);
            let e = data::expiry(&mut self.rng);
            let code = data::card_code(&mut self.rng, c.amex);
            vec![
                t("card", c.shown),
                p(l1),
                t("card_extra", e),
                p(l2),
                t("card_extra", code),
            ]
        } else {
            let n = data::decoy("luhn_fail_16_digits", &mut self.rng);
            vec![
                d("luhn_fail_16_digits", n),
                p(l1),
                d("lone_expiry", data::expiry(&mut self.rng)),
                p(l2),
                d("lone_code", self.rng.digits(3)),
            ]
        }
    }

    /// Card number on one line, expiry and code on the next.
    pub fn card_two_lines(&mut self) -> (Vec<Seg>, Vec<Seg>) {
        if self.card_world {
            let c = data::card(&mut self.rng);
            let e = data::expiry(&mut self.rng);
            let code = data::card_code(&mut self.rng, c.amex);
            (
                vec![t("card", c.shown)],
                vec![
                    p("Expires "),
                    t("card_extra", e),
                    p("   Security code "),
                    t("card_extra", code),
                ],
            )
        } else {
            let n = data::decoy("luhn_fail_16_digits", &mut self.rng);
            (
                vec![d("luhn_fail_16_digits", n)],
                vec![
                    p("Expires "),
                    d("lone_expiry", data::expiry(&mut self.rng)),
                    p("   Security code "),
                    d("lone_code", self.rng.digits(3)),
                ],
            )
        }
    }

    // ----- output -----

    /// Paints the text ops over the canvas with the project renderer. Returns the picture as
    /// RGB-in-RGBA (alpha 255).
    pub fn finish(self, debug_boxes: bool) -> Finished {
        let Ctx {
            fonts,
            base,
            ops,
            lines,
            truths,
            decoys,
            min_px,
            ..
        } = self;
        let mut img = fonts
            .renderer
            .render(&base, &ops)
            .unwrap_or_else(|e| panic!("render failed: {e}"));
        if debug_boxes {
            for (rects, col) in [
                (
                    truths.iter().map(|t| t.rect).collect::<Vec<_>>(),
                    [255, 0, 0, 255],
                ),
                (
                    decoys.iter().map(|t| t.rect).collect::<Vec<_>>(),
                    [0, 160, 255, 255],
                ),
            ] {
                for r in rects {
                    outline(&mut img, r, col);
                }
            }
        }
        Finished {
            img,
            lines,
            truths,
            decoys,
            min_px,
        }
    }
}

pub struct Finished {
    pub img: RgbaImage,
    pub lines: Vec<DrawnLine>,
    pub truths: Vec<Truth>,
    pub decoys: Vec<Decoy>,
    pub min_px: f32,
}

fn outline(img: &mut RgbaImage, r: [f64; 4], col: [u8; 4]) {
    let (w, h) = img.dimensions();
    let x0 = r[0].floor().max(0.0) as u32;
    let y0 = r[1].floor().max(0.0) as u32;
    let x1 = ((r[0] + r[2]).ceil() as u32).min(w.saturating_sub(1));
    let y1 = ((r[1] + r[3]).ceil() as u32).min(h.saturating_sub(1));
    for x in x0..=x1 {
        img.put_pixel(x, y0, Rgba(col));
        img.put_pixel(x, y1, Rgba(col));
    }
    for y in y0..=y1 {
        img.put_pixel(x0, y, Rgba(col));
        img.put_pixel(x1, y, Rgba(col));
    }
}

pub fn rgba(c: Color) -> Rgba<u8> {
    Rgba([c.r, c.g, c.b, 255])
}
