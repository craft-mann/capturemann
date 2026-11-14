//! The Long Shot mode bar (CAP-6, design board S1): its state, layout, hit testing and what a
//! click does. Pure and display-free, so it is unit tested; [`crate::long_paint`] draws it and
//! [`crate::app`] feeds it the pointer.
//!
//! The bar is a floating pill centred 34 px (logical) from the bottom of the display. It holds
//! the capture modes (Region, Window, Display, Long Shot, Click Guide), the Auto / Hand segmented
//! control, the direction control and Start. On a system that cannot inject scrolling (Wayland)
//! the segmented control is a single "Hand scroll" label. On macOS without Accessibility, Auto
//! scroll stays shown but dimmed; choosing it (or pressing Start with it chosen) opens the
//! Accessibility card with "Grant access..." and "Use Hand scroll" (CAP-6.AC6).

use cm_i18n::tr;

use crate::canvas::IRect;

/// Whether Auto scroll can be offered (the desktop app asks the capture crate and passes it as
/// `--auto-scroll`; the overlay never links an engine crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSupport {
    Available,
    /// macOS without Accessibility: shown dimmed, with the card.
    NeedsAccessibility,
    /// Wayland or not built: Auto scroll is not shown at all.
    Unavailable,
}

impl AutoSupport {
    pub fn parse(s: &str) -> Option<AutoSupport> {
        match s {
            "available" => Some(AutoSupport::Available),
            "needs_accessibility" => Some(AutoSupport::NeedsAccessibility),
            "unavailable" => Some(AutoSupport::Unavailable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    Auto,
    Hand,
}

impl Scroll {
    pub fn name(self) -> &'static str {
        match self {
            Scroll::Auto => "auto",
            Scroll::Hand => "hand",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Vertical,
    Horizontal,
}

impl Dir {
    pub fn name(self) -> &'static str {
        match self {
            Dir::Vertical => "vertical",
            Dir::Horizontal => "horizontal",
        }
    }
}

/// The modes of the bar. Long Shot is the one this overlay runs; the others end the overlay with
/// `{"switch_to": <name>}` and the desktop app starts that mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarMode {
    Region,
    Window,
    Display,
    LongShot,
    ClickGuide,
}

impl BarMode {
    pub const ALL: [BarMode; 5] = [
        BarMode::Region,
        BarMode::Window,
        BarMode::Display,
        BarMode::LongShot,
        BarMode::ClickGuide,
    ];

    fn switch_name(self) -> Option<&'static str> {
        match self {
            BarMode::Region => Some("region"),
            BarMode::Window => Some("window"),
            BarMode::Display => Some("display"),
            BarMode::ClickGuide => Some("guide"),
            BarMode::LongShot => None,
        }
    }

    fn label(self) -> String {
        match self {
            BarMode::Region => tr!("Region"),
            BarMode::Window => tr!("Window"),
            BarMode::Display => tr!("Display"),
            BarMode::LongShot => tr!("Long Shot"),
            BarMode::ClickGuide => tr!("Click Guide"),
        }
    }
}

/// What a bar entry stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Mode(BarMode),
    Scroll(Scroll),
    Dir(Dir),
    Start,
    GrantAccess,
    UseHand,
}

/// What the app must do after a click on the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarEffect {
    Nothing,
    Redraw,
    /// Confirm the area under the pointer or the dragged region (the picker's Enter).
    Start,
    /// End the overlay; the desktop app starts the named mode.
    SwitchTo(&'static str),
    /// End the overlay; the desktop app asks macOS for Accessibility and reopens Long Shot.
    GrantAccessibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarState {
    pub scroll: Scroll,
    pub dir: Dir,
    pub auto: AutoSupport,
    /// The Accessibility card is open.
    pub card: bool,
}

impl BarState {
    /// Auto scroll when it can run, else Hand scroll; vertical.
    pub fn new(auto: AutoSupport) -> BarState {
        BarState {
            scroll: if auto == AutoSupport::Available {
                Scroll::Auto
            } else {
                Scroll::Hand
            },
            dir: Dir::Vertical,
            auto,
            card: false,
        }
    }

    /// Auto scroll is chosen but cannot run: Start must show the card instead (CAP-6.AC6).
    pub fn start_blocked(&self) -> bool {
        self.scroll == Scroll::Auto && self.auto != AutoSupport::Available
    }

    /// Start (Enter or the button): `true` when the capture may begin, else the card opens.
    pub fn try_start(&mut self) -> bool {
        if self.start_blocked() {
            self.card = self.auto == AutoSupport::NeedsAccessibility;
            return false;
        }
        true
    }

    /// A click on `item`.
    pub fn activate(&mut self, item: Item) -> BarEffect {
        match item {
            Item::Mode(m) => match m.switch_name() {
                Some(name) => BarEffect::SwitchTo(name),
                None => BarEffect::Nothing,
            },
            Item::Scroll(Scroll::Hand) => {
                self.scroll = Scroll::Hand;
                self.card = false;
                BarEffect::Redraw
            }
            Item::Scroll(Scroll::Auto) => match self.auto {
                AutoSupport::Available => {
                    self.scroll = Scroll::Auto;
                    self.card = false;
                    BarEffect::Redraw
                }
                // Shown but disabled: it explains itself instead.
                AutoSupport::NeedsAccessibility => {
                    self.card = true;
                    BarEffect::Redraw
                }
                AutoSupport::Unavailable => BarEffect::Nothing,
            },
            Item::Dir(d) => {
                self.dir = d;
                BarEffect::Redraw
            }
            Item::Start => {
                if self.try_start() {
                    BarEffect::Start
                } else {
                    BarEffect::Redraw
                }
            }
            Item::GrantAccess => BarEffect::GrantAccessibility,
            Item::UseHand => {
                self.scroll = Scroll::Hand;
                self.card = false;
                BarEffect::Redraw
            }
        }
    }
}

/// One drawn entry of the bar or the card.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub item: Item,
    pub rect: IRect,
    pub label: String,
    pub selected: bool,
    /// Drawn dimmed (Auto scroll without Accessibility); a click still explains why.
    pub dimmed: bool,
    pub primary: bool,
    /// A keycap after the label (Start shows Enter).
    pub keycap: Option<&'static str>,
    /// A label only, not a button (Wayland's single "Hand scroll").
    pub inert: bool,
}

/// A divider line between groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divider(pub IRect);

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub rect: IRect,
    pub title: (i32, i32, String),
    pub body: Vec<(i32, i32, String)>,
    pub buttons: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarLayout {
    pub bar: IRect,
    pub entries: Vec<Entry>,
    pub dividers: Vec<Divider>,
    pub card: Option<Card>,
}

/// Text width at a size, in px.
pub type Measure<'a> = &'a dyn Fn(&str, f32) -> f32;

pub const BOTTOM_MARGIN: f64 = 34.0;
pub const PAD: f64 = 6.0;
pub const BUTTON_H: f64 = 40.0;
pub const LABEL_PX: f64 = 13.0;
pub const CARD_W: f64 = 330.0;

fn d(logical: f64, scale: f64) -> i32 {
    (logical * scale).round().max(1.0) as i32
}

/// Breaks `text` into lines of at most `max_w` px (greedy, on spaces).
pub fn wrap(text: &str, max_w: f32, px: f32, measure: Measure<'_>) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let next = if cur.is_empty() {
            word.to_owned()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || measure(&next, px) <= max_w {
            cur = next;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_owned();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Lays the bar out for a display of `w` x `h` physical px at `scale`.
pub fn layout(state: &BarState, w: i32, h: i32, scale: f64, measure: Measure<'_>) -> BarLayout {
    let px = (LABEL_PX * scale) as f32;
    let (pad, btn_h, gap) = (d(PAD, scale), d(BUTTON_H, scale), d(4.0, scale));
    let btn_pad = d(14.0, scale);
    let div_w = d(1.0, scale);
    let div_gap = d(8.0, scale);
    let keycap_w = |k: &str| (measure(k, px * 0.92).ceil() as i32) + d(14.0, scale);

    // Pass 1: widths of the entries in order, with group breaks.
    struct Proto {
        item: Item,
        label: String,
        w: i32,
        selected: bool,
        dimmed: bool,
        primary: bool,
        keycap: Option<&'static str>,
        inert: bool,
        break_before: bool,
    }
    let mut protos: Vec<Proto> = Vec::new();
    let mut push = |item: Item,
                    label: String,
                    selected: bool,
                    dimmed: bool,
                    primary: bool,
                    keycap: Option<&'static str>,
                    inert: bool,
                    break_before: bool| {
        let mut w = measure(&label, px).ceil() as i32 + 2 * btn_pad;
        if let Some(k) = keycap {
            w += d(8.0, scale) + keycap_w(k);
        }
        protos.push(Proto {
            item,
            label,
            w,
            selected,
            dimmed,
            primary,
            keycap,
            inert,
            break_before,
        });
    };
    for m in BarMode::ALL.iter() {
        push(
            Item::Mode(*m),
            m.label(),
            *m == BarMode::LongShot,
            false,
            false,
            None,
            false,
            false,
        );
    }
    match state.auto {
        AutoSupport::Unavailable => push(
            Item::Scroll(Scroll::Hand),
            tr!("Hand scroll"),
            true,
            false,
            false,
            None,
            true,
            true,
        ),
        _ => {
            push(
                Item::Scroll(Scroll::Auto),
                tr!("Auto scroll"),
                state.scroll == Scroll::Auto,
                state.auto == AutoSupport::NeedsAccessibility,
                false,
                None,
                false,
                true,
            );
            push(
                Item::Scroll(Scroll::Hand),
                tr!("Hand scroll"),
                state.scroll == Scroll::Hand,
                false,
                false,
                None,
                false,
                false,
            );
        }
    }
    push(
        Item::Dir(Dir::Vertical),
        tr!("Vertical"),
        state.dir == Dir::Vertical,
        false,
        false,
        None,
        false,
        true,
    );
    push(
        Item::Dir(Dir::Horizontal),
        tr!("Horizontal"),
        state.dir == Dir::Horizontal,
        false,
        false,
        None,
        false,
        false,
    );
    push(
        Item::Start,
        tr!("Start"),
        false,
        false,
        true,
        Some("Enter"),
        false,
        true,
    );

    let spacing = |p: &Proto| {
        if p.break_before {
            2 * div_gap + div_w
        } else {
            gap
        }
    };
    let total = 2 * pad
        + protos.iter().map(|p| p.w).sum::<i32>()
        + protos.iter().skip(1).map(spacing).sum::<i32>();
    let bar_h = btn_h + 2 * pad;
    let x0 = ((w - total) / 2).max(0);
    let y0 = (h - d(BOTTOM_MARGIN, scale) - bar_h).max(0);
    let bar = IRect::new(x0, y0, total.min(w), bar_h);
    let mut shifted = Vec::new();
    let mut dividers = Vec::new();
    let mut x = bar.x + pad;
    let inset = d(8.0, scale);
    for (i, p) in protos.into_iter().enumerate() {
        if i > 0 {
            if p.break_before {
                dividers.push(Divider(IRect::new(
                    x + div_gap,
                    bar.y + pad + inset,
                    div_w,
                    btn_h - 2 * inset,
                )));
            }
            x += spacing(&p);
        }
        shifted.push(Entry {
            item: p.item,
            rect: IRect::new(x, bar.y + pad, p.w, btn_h),
            label: p.label,
            selected: p.selected,
            dimmed: p.dimmed,
            primary: p.primary,
            keycap: p.keycap,
            inert: p.inert,
        });
        x += p.w;
    }

    let card = if state.card && state.auto == AutoSupport::NeedsAccessibility {
        let anchor = shifted
            .iter()
            .find(|e| e.item == Item::Scroll(Scroll::Auto))
            .map_or(bar.x, |e| e.rect.x);
        Some(card_layout(anchor, bar, w, scale, measure))
    } else {
        None
    };
    BarLayout {
        bar,
        entries: shifted,
        dividers,
        card,
    }
}

fn card_layout(anchor_x: i32, bar: IRect, w: i32, scale: f64, measure: Measure<'_>) -> Card {
    let pad = d(16.0, scale);
    let cw = d(CARD_W, scale);
    let title_px = (15.0 * scale) as f32;
    let body_px = (13.0 * scale) as f32;
    let title = tr!("Auto scroll needs Accessibility");
    let body = tr!("Allow Accessibility to scroll for you. Hand scroll still works without it.");
    let lines = wrap(&body, (cw - 2 * pad) as f32, body_px, measure);
    let title_h = (title_px * 1.3).ceil() as i32;
    let line_h = (body_px * 1.45).ceil() as i32;
    let btn_h = d(36.0, scale);
    let gap = d(10.0, scale);
    let h = pad + title_h + d(6.0, scale) + line_h * lines.len() as i32 + gap + btn_h + pad;
    let x = anchor_x.clamp(0, (w - cw).max(0));
    let y = (bar.y - d(10.0, scale) - h).max(0);
    let rect = IRect::new(x, y, cw, h);
    let mut ty = y + pad;
    let title_pos = (x + pad, ty, title);
    ty += title_h + d(6.0, scale);
    let mut body_lines = Vec::new();
    for l in lines {
        body_lines.push((x + pad, ty, l));
        ty += line_h;
    }
    ty += gap;
    let grant_label = tr!("Grant access\u{2026}");
    let hand_label = tr!("Use Hand scroll");
    let btn_px = (LABEL_PX * scale) as f32;
    let gw = measure(&grant_label, btn_px).ceil() as i32 + d(28.0, scale);
    let hw = measure(&hand_label, btn_px).ceil() as i32 + d(28.0, scale);
    let buttons = vec![
        Entry {
            item: Item::GrantAccess,
            rect: IRect::new(x + pad, ty, gw, btn_h),
            label: grant_label,
            selected: false,
            dimmed: false,
            primary: true,
            keycap: None,
            inert: false,
        },
        Entry {
            item: Item::UseHand,
            rect: IRect::new(x + pad + gw + d(8.0, scale), ty, hw, btn_h),
            label: hand_label,
            selected: false,
            dimmed: false,
            primary: false,
            keycap: None,
            inert: false,
        },
    ];
    Card {
        rect,
        title: title_pos,
        body: body_lines,
        buttons,
    }
}

fn inside(r: IRect, x: f64, y: f64) -> bool {
    x >= f64::from(r.x)
        && x < f64::from(r.x + r.w)
        && y >= f64::from(r.y)
        && y < f64::from(r.y + r.h)
}

impl BarLayout {
    /// The entry under (x, y) physical px, if any.
    pub fn hit(&self, x: f64, y: f64) -> Option<Item> {
        if let Some(c) = &self.card {
            if let Some(b) = c.buttons.iter().find(|b| inside(b.rect, x, y)) {
                return Some(b.item);
            }
        }
        self.entries
            .iter()
            .find(|e| !e.inert && inside(e.rect, x, y))
            .map(|e| e.item)
    }

    /// Whether (x, y) is over the bar or the card (the picker must not see the click).
    pub fn covers(&self, x: f64, y: f64) -> bool {
        inside(self.bar, x, y) || self.card.as_ref().is_some_and(|c| inside(c.rect, x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 7 px per character at any size 13: the layout is checked, not the font.
    fn m(s: &str, px: f32) -> f32 {
        s.chars().count() as f32 * px * 0.55
    }

    fn lay(state: &BarState) -> BarLayout {
        layout(state, 1440, 900, 1.0, &m)
    }

    #[test]
    fn the_bar_is_centred_with_a_34_px_bottom_margin() {
        let l = lay(&BarState::new(AutoSupport::Available));
        let centre = l.bar.x + l.bar.w / 2;
        assert!((centre - 720).abs() <= 1, "{centre}");
        assert_eq!(l.bar.y + l.bar.h, 900 - 34);
        assert_eq!(l.bar.h, 40 + 12);
    }

    #[test]
    fn entries_do_not_overlap_and_stay_inside_the_bar() {
        let l = lay(&BarState::new(AutoSupport::Available));
        for (i, a) in l.entries.iter().enumerate() {
            assert!(
                a.rect.x >= l.bar.x && a.rect.x + a.rect.w <= l.bar.x + l.bar.w,
                "{a:?}"
            );
            for b in &l.entries[i + 1..] {
                assert!(a.rect.x + a.rect.w <= b.rect.x, "{a:?} overlaps {b:?}");
            }
        }
        assert_eq!(l.dividers.len(), 3);
    }

    #[test]
    fn long_shot_is_the_selected_mode_and_start_is_primary() {
        let l = lay(&BarState::new(AutoSupport::Available));
        let sel: Vec<_> = l
            .entries
            .iter()
            .filter(|e| matches!(e.item, Item::Mode(_)) && e.selected)
            .collect();
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0].item, Item::Mode(BarMode::LongShot));
        let start = l.entries.iter().find(|e| e.item == Item::Start).unwrap();
        assert!(start.primary);
        assert_eq!(start.keycap, Some("Enter"));
    }

    #[test]
    fn auto_is_the_default_only_when_it_can_run() {
        assert_eq!(BarState::new(AutoSupport::Available).scroll, Scroll::Auto);
        assert_eq!(
            BarState::new(AutoSupport::NeedsAccessibility).scroll,
            Scroll::Hand
        );
        assert_eq!(BarState::new(AutoSupport::Unavailable).scroll, Scroll::Hand);
        assert_eq!(BarState::new(AutoSupport::Available).dir, Dir::Vertical);
    }

    /// Wayland (S1): the segmented control is a single "Hand scroll" label; Auto is not shown.
    #[test]
    fn wayland_shows_one_inert_hand_scroll_label_and_no_auto() {
        let l = lay(&BarState::new(AutoSupport::Unavailable));
        assert!(l
            .entries
            .iter()
            .all(|e| e.item != Item::Scroll(Scroll::Auto)));
        let hand: Vec<_> = l
            .entries
            .iter()
            .filter(|e| e.item == Item::Scroll(Scroll::Hand))
            .collect();
        assert_eq!(hand.len(), 1);
        assert!(hand[0].inert);
        // Not clickable.
        let r = hand[0].rect;
        assert_eq!(l.hit(f64::from(r.x + 2), f64::from(r.y + 2)), None);
    }

    /// CAP-6.AC6: Auto scroll without Accessibility is shown dimmed; choosing it opens the card
    /// with "Grant access..." and "Use Hand scroll"; Hand scroll still works.
    #[test]
    fn cap_6_ac6_auto_without_accessibility_opens_the_card() {
        let mut s = BarState::new(AutoSupport::NeedsAccessibility);
        let l = lay(&s);
        let auto = l
            .entries
            .iter()
            .find(|e| e.item == Item::Scroll(Scroll::Auto))
            .unwrap();
        assert!(auto.dimmed, "shown, disabled");
        assert!(l.card.is_none());
        assert_eq!(s.activate(Item::Scroll(Scroll::Auto)), BarEffect::Redraw);
        assert!(s.card);
        assert_eq!(s.scroll, Scroll::Hand, "Auto is not selected");
        let l = lay(&s);
        let card = l.card.as_ref().unwrap();
        assert_eq!(card.rect.w, 330);
        assert!(card.title.2.contains("Accessibility"));
        assert!(card
            .body
            .iter()
            .any(|(_, _, t)| t.contains("Allow Accessibility to scroll for you")));
        let labels: Vec<_> = card.buttons.iter().map(|b| b.item).collect();
        assert_eq!(labels, vec![Item::GrantAccess, Item::UseHand]);
        assert!(
            card.rect.y + card.rect.h <= l.bar.y,
            "the card sits above the bar"
        );
        // Grant leaves for the desktop app; Use Hand scroll closes the card and keeps Hand.
        assert_eq!(s.activate(Item::GrantAccess), BarEffect::GrantAccessibility);
        assert_eq!(s.activate(Item::UseHand), BarEffect::Redraw);
        assert!(!s.card);
        assert_eq!(s.scroll, Scroll::Hand);
        // Hand scroll then starts.
        assert_eq!(s.activate(Item::Start), BarEffect::Start);
    }

    #[test]
    fn start_with_auto_that_cannot_run_shows_the_card_instead() {
        let mut s = BarState::new(AutoSupport::NeedsAccessibility);
        s.scroll = Scroll::Auto;
        assert!(s.start_blocked());
        assert_eq!(s.activate(Item::Start), BarEffect::Redraw);
        assert!(s.card);
        let mut ok = BarState::new(AutoSupport::Available);
        assert_eq!(ok.activate(Item::Start), BarEffect::Start);
    }

    #[test]
    fn direction_and_scroll_controls_change_the_options() {
        let mut s = BarState::new(AutoSupport::Available);
        s.activate(Item::Dir(Dir::Horizontal));
        s.activate(Item::Scroll(Scroll::Hand));
        assert_eq!((s.dir, s.scroll), (Dir::Horizontal, Scroll::Hand));
        s.activate(Item::Scroll(Scroll::Auto));
        assert_eq!(s.scroll, Scroll::Auto);
        assert_eq!(Dir::Horizontal.name(), "horizontal");
        assert_eq!(Scroll::Hand.name(), "hand");
    }

    #[test]
    fn other_modes_end_the_overlay_with_their_name() {
        let mut s = BarState::new(AutoSupport::Available);
        assert_eq!(
            s.activate(Item::Mode(BarMode::Region)),
            BarEffect::SwitchTo("region")
        );
        assert_eq!(
            s.activate(Item::Mode(BarMode::ClickGuide)),
            BarEffect::SwitchTo("guide")
        );
        assert_eq!(
            s.activate(Item::Mode(BarMode::LongShot)),
            BarEffect::Nothing
        );
    }

    #[test]
    fn hit_testing_finds_entries_and_the_bar_covers_its_pixels() {
        let l = lay(&BarState::new(AutoSupport::Available));
        let start = l.entries.iter().find(|e| e.item == Item::Start).unwrap();
        let (cx, cy) = (
            f64::from(start.rect.x + start.rect.w / 2),
            f64::from(start.rect.y + start.rect.h / 2),
        );
        assert_eq!(l.hit(cx, cy), Some(Item::Start));
        assert!(l.covers(cx, cy));
        assert!(!l.covers(10.0, 10.0));
        assert_eq!(l.hit(10.0, 10.0), None);
    }

    #[test]
    fn layout_scales_with_the_display() {
        let a = layout(&BarState::new(AutoSupport::Available), 2880, 1800, 2.0, &m);
        assert_eq!(a.bar.h, 104);
        assert_eq!(a.bar.y + a.bar.h, 1800 - 68);
    }

    #[test]
    fn wrap_breaks_on_spaces_and_never_exceeds_the_width_unless_a_word_does() {
        let lines = wrap("aaaa bbbb cccc dddd", 60.0, 10.0, &|s, px| {
            s.len() as f32 * px * 0.5
        });
        assert_eq!(lines, vec!["aaaa bbbb", "cccc dddd"]);
        let lines = wrap("supercalifragilistic x", 20.0, 10.0, &|s, px| {
            s.len() as f32 * px * 0.5
        });
        assert_eq!(lines, vec!["supercalifragilistic", "x"]);
        assert!(wrap("", 10.0, 10.0, &|_, _| 0.0).is_empty());
    }
}
