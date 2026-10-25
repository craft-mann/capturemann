//! Fixed-width controls against long translations, without a window.
//!
//! Every case names a control, the width its label has to fit in (worked out from the sizes in the
//! code that draws it, noted per case) and where the labels come from (the label accessors of the
//! app). `text_metrics_tests` measures each label in the bundled Geist fonts in English, in
//! Vietnamese and in the pseudo-locale (about 30 percent longer than English). English and
//! Vietnamese must fit; the pseudo-locale is reported only. A case that wraps says how many lines
//! the control has room for, a control that cuts with an ellipsis is listed with the lines it
//! would need, so a label that no longer fits shows up here before it is cut on screen.
//!
//! The same table writes the report in `design/i18n-STATUS-overflow.md`:
//! `LAYOUT_WIDTH_REPORT=<file> cargo test -p cm-desktop --lib layout_width_tests`. When a
//! translation edit makes a label too wide, the test names the control, the label and both widths.
//! The fix is a shorter wording, wrapping or `ellipsis_text`, or, for a pinned size, a new
//! constant changed together with the test that pins it (see `locales/README.md`, "Layout: long
//! translations").

use cm_i18n::{with_language, Lang};

use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};
use crate::text_metrics_tests::{lines, width, Face};
use crate::theme::type_scale;

/// How a control treats a label that is too wide.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Overflow {
    /// It wraps onto at most this many lines (1: it must fit on one line).
    Lines(usize),
}

/// Where the width comes from.
enum Source {
    /// One label measured in the case's font.
    Text(Box<dyn Fn() -> String>),
    /// A row of several things, measured by the closure in px (a button pair, a chip with keys).
    Needed(Box<dyn Fn() -> f32>),
}

struct Item {
    what: String,
    source: Source,
}

struct Case {
    control: &'static str,
    /// Where the number comes from, for the report.
    geometry: &'static str,
    /// The room for the label, px.
    avail: f32,
    size: f32,
    weight: u16,
    mono: bool,
    overflow: Overflow,
    items: Vec<Item>,
}

impl Case {
    fn new(
        control: &'static str,
        geometry: &'static str,
        avail: f32,
        size: f32,
        weight: u16,
        overflow: Overflow,
    ) -> Case {
        Case {
            control,
            geometry,
            avail,
            size,
            weight,
            mono: false,
            overflow,
            items: Vec::new(),
        }
    }

    fn text(mut self, what: impl Into<String>, f: impl Fn() -> String + 'static) -> Case {
        self.items.push(Item {
            what: what.into(),
            source: Source::Text(Box::new(f)),
        });
        self
    }

    fn texts(mut self, list: impl IntoIterator<Item = (String, Box<dyn Fn() -> String>)>) -> Case {
        for (what, f) in list {
            self.items.push(Item {
                what,
                source: Source::Text(f),
            });
        }
        self
    }

    fn needed(mut self, what: impl Into<String>, f: impl Fn() -> f32 + 'static) -> Case {
        self.items.push(Item {
            what: what.into(),
            source: Source::Needed(Box::new(f)),
        });
        self
    }

    fn face(&self) -> Face {
        Face::of(self.mono, self.weight)
    }
}

/// A label accessor, evaluated under each language.
type Labeler = Box<dyn Fn() -> String>;

/// One measurement of one label in one language.
#[derive(Clone, Debug)]
struct Measure {
    text: String,
    /// Width on one line, px.
    px: f32,
    /// Lines needed in the room (None: a single word is wider than the room).
    lines: Option<usize>,
}

fn measure(case: &Case, item: &Item, lang: Lang) -> Measure {
    with_language(lang, || match &item.source {
        Source::Text(f) => {
            let text = f();
            let px = width(&text, case.face(), case.size);
            let l = lines(&text, case.face(), case.size, case.avail);
            Measure { text, px, lines: l }
        }
        Source::Needed(f) => {
            let px = f();
            Measure {
                text: String::new(),
                px,
                lines: (px <= case.avail + 0.01).then_some(1),
            }
        }
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verdict {
    Fits,
    Wraps(usize),
    Overflows,
}

impl Verdict {
    fn of(case: &Case, m: &Measure) -> Verdict {
        let fits = m.px <= case.avail + 0.01;
        match (case.overflow, m.lines) {
            (_, _) if fits => Verdict::Fits,
            (Overflow::Lines(max), Some(n)) if n <= max => Verdict::Wraps(n),
            _ => Verdict::Overflows,
        }
    }

    fn word(self) -> String {
        match self {
            Verdict::Fits => "fits".into(),
            Verdict::Wraps(n) => format!("wraps ({n} lines)"),
            Verdict::Overflows => "OVERFLOWS".into(),
        }
    }
}

struct Row {
    control: &'static str,
    geometry: &'static str,
    avail: f32,
    what: String,
    en: Measure,
    vi: Measure,
    xa: Measure,
    ve: Verdict,
    vv: Verdict,
    vx: Verdict,
}

fn rows(cases: &[Case]) -> Vec<Row> {
    let mut out = Vec::new();
    for c in cases {
        for it in &c.items {
            let (en, vi, xa) = (
                measure(c, it, Lang::En),
                measure(c, it, Lang::Vi),
                measure(c, it, Lang::EnXa),
            );
            out.push(Row {
                control: c.control,
                geometry: c.geometry,
                avail: c.avail,
                what: it.what.clone(),
                ve: Verdict::of(c, &en),
                vv: Verdict::of(c, &vi),
                vx: Verdict::of(c, &xa),
                en,
                vi,
                xa,
            });
        }
    }
    out
}

fn shown(m: &Measure) -> String {
    format!("{:.0}", m.px)
}

// ---- the cases -----------------------------------------------------------------------------

/// A boxed accessor with its name, for [`Case::texts`].
fn t(
    what: impl Into<String>,
    f: impl Fn() -> String + 'static,
) -> (String, Box<dyn Fn() -> String>) {
    (what.into(), Box::new(f))
}

fn key_w(label: &str, size: f32, pad: f32, min: f32) -> f32 {
    (width(label, Face::MonoRegular, size) + 2. * pad).max(min)
}

fn keys_w(keys: &[String], size: f32, pad: f32, min: f32, gap: f32) -> f32 {
    keys.iter().map(|k| key_w(k, size, pad, min)).sum::<f32>()
        + gap * keys.len().saturating_sub(1) as f32
}

fn mcp_titles() -> Vec<(String, Box<dyn Fn() -> String>)> {
    use crate::agent_status::{self, McpState, McpStatus, OfflineReason};
    let mut v = vec![t("McpStatus::title(ready)", || {
        agent_status::derive(true, true, None, &[]).title()
    })];
    for r in [
        OfflineReason::LibraryUnavailable,
        OfflineReason::NoEndpoint,
        OfflineReason::SocketOwned,
    ] {
        v.push(t(format!("McpStatus::title({r:?})"), move || {
            McpStatus {
                state: McpState::Offline(r),
                last_call: None,
            }
            .title()
        }));
    }
    v
}

fn row_label_keys() -> Vec<String> {
    // Every key `daemon_text::row_label` words, read from its source so a new row is measured too.
    include_str!("daemon_text.rs")
        .split("pub fn row_label")
        .nth(1)
        .and_then(|s| s.split("\n}\n").next())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            l.strip_prefix('"')
                .and_then(|r| r.split_once("\" =>"))
                .map(|(k, _)| k.to_owned())
        })
        .collect()
}

fn toast_specs() -> Vec<(&'static str, crate::toast::ToastSpec)> {
    use crate::recording::model::Ended;
    use crate::recording::{exports, toasts, uploads};
    vec![
        (
            "ended: stopped",
            toasts::ended_toast(&Ended::Stopped {
                capture_id: Some("rec_1".into()),
                duration_s: Some(42.0),
                reason: "user".into(),
                error: None,
                permission: None,
            })
            .expect("a stopped take has a toast"),
        ),
        (
            "ended: permission revoked",
            toasts::ended_toast(&Ended::Stopped {
                capture_id: Some("rec_1".into()),
                duration_s: None,
                reason: "permission_revoked".into(),
                error: None,
                permission: Some(cm_core::Permission::Screen),
            })
            .expect("a revoked permission has a toast"),
        ),
        ("exports: skipped", exports::export_skipped_toast("rec_1")),
        (
            "exports: failed (ffmpeg)",
            exports::export_failed_toast("rec_1", "x", true),
        ),
        (
            "exports: exported",
            exports::exported_toast("rec_1", "My take", Some("burned")),
        ),
        ("uploads: reconnect", uploads::reconnect_toast("My Drive")),
        (
            "uploads: failed",
            uploads::upload_failed_toast("job_1", Some("My take"), "My Drive", "quota"),
        ),
    ]
}

fn cases() -> Vec<Case> {
    use crate::nav::{DockAction, NavItem};
    use crate::ui::settings::model::SettingsSection;
    let ui = type_scale::UI;
    let cap = type_scale::CAPTION;
    let one = Overflow::Lines(1);

    let mut v = Vec::new();

    // ---- HUD: the window is exactly the 506 px bar --------------------------------------------
    // Stop is `hud_button` (44 px) widened to STOP_W 88, no padding, 1 px border: room beside the
    // 16 px icon and its 8 px gap.
    v.push(
        Case::new(
            "HUD Stop label",
            "STOP_W 88 - 2 border - 16 icon - 8 gap",
            crate::recording::hud_layout::STOP_W as f32 - 2. - 16. - 8.,
            ui,
            600,
            one,
        )
        .text("stop_label", crate::recording::hud_layout::stop_label),
    );
    // The hint chip over the bar: 10 px padding, 1 px border, then the keycaps (about 70 px).
    let bar = crate::recording::hud_layout::bar_width(None) as f32;
    v.push(
        Case::new(
            "HUD hover hint (chip over the bar)",
            "bar 506 - 2*10 padding - 2 border - 70 keycaps",
            bar - 22. - 70.,
            cap,
            400,
            one,
        )
        .texts(
            crate::ui::recording::hud::all_hints()
                .into_iter()
                .enumerate()
                .map(|(i, _)| {
                    t(format!("HudHint #{i}"), move || {
                        crate::ui::recording::hud::all_hints()[i].clone()
                    })
                }),
        ),
    );
    v.push(
        Case::new(
            "HUD health chip",
            "bar 506 - 2*10 padding - 2 border - 12 icon - 6 gap",
            bar - 22. - 18.,
            cap,
            400,
            one,
        )
        .text("Dropping frames", || cm_i18n::tr!("Dropping frames")),
    );
    // The agent chip's width is estimated from the number of characters (CHIP_CHAR); the real
    // text must fit what the estimate gives.
    for client in ["Claude", "Claude Desktop", "Claude Code CLI"] {
        v.push(
            Case::new(
                "HUD agent chip (width estimated from characters)",
                "agent_chip_width(client) - CHIP_FIXED 40",
                crate::recording::hud_layout::agent_chip_width(client) as f32 - 40.,
                cap,
                400,
                one,
            )
            .text(format!("Started by {client}"), move || {
                crate::recording::hud_layout::agent_chip_text(client)
            }),
        );
    }

    // ---- sidebar 248 px: 12 px side padding ----------------------------------------------------
    // nav_row: border 2, padding 20, 18 px icon, 10 px gap, then the label; a count takes about
    // 32 px more.
    let nav_room = crate::theme::scale::SIDEBAR_W - 24. - 2. - 20. - 18. - 10.;
    v.push(
        Case::new(
            "Sidebar nav row (with a count)",
            "248 - 24 - 2 - 20 - 18 - 10 - 32 count",
            nav_room - 32.,
            ui,
            500,
            one,
        )
        .texts(
            NavItem::ALL
                .into_iter()
                .map(|n| t(format!("NavItem::label({n:?})"), move || n.label())),
        ),
    );
    v.push(
        Case::new(
            "Settings sidebar nav row",
            "248 - 24 - 2 - 20 - 18 - 10",
            nav_room,
            ui,
            500,
            one,
        )
        .texts(
            SettingsSection::ALL
                .into_iter()
                .map(|s| t(format!("SettingsSection::label({s:?})"), move || s.label())),
        ),
    );
    v.push(
        Case::new(
            "Sidebar MCP card title",
            "248 - 24 - 2 border - 28 padding - 14 bot - 7 dot - 2*8 gaps",
            crate::theme::scale::SIDEBAR_W - 24. - 2. - 28. - 14. - 7. - 16.,
            13.,
            500,
            one,
        )
        .texts(mcp_titles()),
    );
    // Capture dock: 224 - 2 border - 16 padding = 206, three tiles 6 px apart, 1 px border.
    let tile = (crate::theme::scale::SIDEBAR_W - 24. - 2. - 16. - 12.) / 3. - 2.;
    v.push(
        Case::new(
            "Capture dock tile",
            "(248 - 24 - 2 - 16 - 2*6) / 3 - 2 border",
            tile,
            12.,
            400,
            one,
        )
        .texts(
            DockAction::TILES
                .into_iter()
                .map(|a| t(format!("DockAction::label({a:?})"), move || a.label())),
        ),
    );
    // The Record row: border 2, padding 36, a 16 px icon and 8 px gaps (icon, label, spacer,
    // keys), then the keycaps (min 20, padding 5, 2 px apart).
    for (platform, name) in [(Platform::Mac, "macOS"), (Platform::Windows, "Windows")] {
        let keys = ShortcutMap::defaults(platform)
            .get(ShortcutAction::ToggleRecording)
            .keycaps(platform);
        // The Record row shows the same keys as the tiles: the whole chord up to three keys,
        // else the last one.
        let keys = crate::nav::tile_keys(&keys);
        let keys_px = keys_w(&keys, 12., 5., 20., 2.);
        v.push(
            Case::new(
                if platform == Platform::Mac {
                    "Dock Record row (macOS chord)"
                } else {
                    "Dock Record row (Windows chord)"
                },
                "206 - 2 - 36 - 16 icon - 3*8 gaps - keycaps",
                206. - 2. - 36. - 16. - 24. - keys_px,
                ui,
                600,
                one,
            )
            .text(format!("Record row on {name}"), || {
                DockAction::Record.label()
            }),
        );
    }

    // ---- recording editor ----------------------------------------------------------------------
    // Timeline label column: LABEL_W 100, a 14 px icon and an 8 px gap.
    v.push(
        Case::new(
            "Timeline track label",
            "LABEL_W 100 - 14 icon - 8 gap",
            crate::recedit::geom::LABEL_W - 22.,
            13.,
            400,
            one,
        )
        .texts(
            [
                crate::ui::recording_editor::RowKind::Screen,
                crate::ui::recording_editor::RowKind::Camera,
                crate::ui::recording_editor::RowKind::Mic,
            ]
            .into_iter()
            .map(|k| t(format!("RowKind::label({k:?})"), move || k.label())),
        ),
    );
    // The lanes under the tracks (Zoom, Captions) use the same label column at 12 px.
    v.push(
        Case::new(
            "Timeline lane label",
            "LABEL_W 100 - 14 icon - 8 gap",
            crate::recedit::geom::LABEL_W - 22.,
            cap,
            400,
            one,
        )
        .text("lane Zoom", || cm_i18n::tr_ctx!("timeline lane", "Zoom"))
        .text("lane Captions", || {
            cm_i18n::tr_ctx!("timeline lane", "Captions")
        }),
    );
    // Camera layout tiles: the panel is 400 - 54 rail - 10 gap - 2 border - 40 padding = 294; two
    // columns 8 px apart; a tile has 14 px padding, a 20 px glyph, a 10 px gap, 2 px border. The
    // label wraps in two lines (52 px tile, 21 px lines).
    v.push(
        Case::new(
            "Camera layout tile",
            "(294 - 8) / 2 - 2 - 28 - 20 - 10",
            (294. - 8.) / 2. - 2. - 28. - 20. - 10.,
            ui,
            400,
            Overflow::Lines(2),
        )
        .texts(crate::recedit::camera::MODES.into_iter().map(|m| {
            t(format!("mode_label({m:?})"), move || {
                crate::recedit::camera::mode_label(m)
            })
        })),
    );
    // Inspector field row: icon 14 + 12 gap + control (a switch 44 or a stepper about 130).
    v.push(
        Case::new(
            "Recording inspector field label",
            "294 - 14 icon - 2*12 gaps - 130 stepper",
            294. - 14. - 24. - 130.,
            ui,
            400,
            Overflow::Lines(2),
        )
        .text("Padding", || cm_i18n::tr!("Padding"))
        .text("Corner radius", || cm_i18n::tr!("Corner radius"))
        .text("Keep audio", || cm_i18n::tr!("Keep audio")),
    );

    // ---- toast 440 -----------------------------------------------------------------------------
    // 440 - 2*6 inset - 2 border - 17 - 8 padding = 403; the text column is that minus the 30 px
    // tile, the 28 px dismiss button and two 12 px gaps.
    let col = 440. - 12. - 2. - 17. - 8. - 30. - 28. - 24.;
    let specs = toast_specs();
    let mut buttons = Vec::new();
    let mut titles = Vec::new();
    for (i, (name, spec)) in specs.iter().enumerate() {
        for b in 0..spec.buttons.len() {
            buttons.push(t(format!("{name}: button {b}"), move || {
                toast_specs()[i].1.buttons[b].label.clone()
            }));
        }
        titles.push(t(format!("{name}: title"), move || {
            toast_specs()[i].1.title.clone()
        }));
    }
    v.push(
        Case::new(
            "Toast title (window height: 38 characters per line)",
            "440 - 12 - 2 - 25 - 30 - 28 - 24 = 319",
            col,
            ui,
            600,
            Overflow::Lines(2),
        )
        .texts(titles),
    );
    v.push(
        Case::new(
            "Toast action button",
            "text column 319 - 24 padding - 2 border - 14 icon - 6 gap",
            col - 24. - 2. - 14. - 6.,
            cap,
            400,
            one,
        )
        .texts(buttons),
    );

    // ---- confirmation dialog 520 -----------------------------------------------------------------
    // Detail rows: a 84 px key column (words wrap, a word must fit), 13 px.
    let mut keys = Vec::new();
    for k in row_label_keys() {
        let key = k.clone();
        keys.push(t(format!("row_label({k})"), move || {
            crate::daemon_text::row_label(&key)
        }));
    }
    v.push(
        Case::new(
            "Confirmation dialog detail key",
            "w(84), 13 px, wraps",
            84.,
            13.,
            400,
            Overflow::Lines(3),
        )
        .texts(keys),
    );
    // The footer: 520 - 2 border - 48 padding; Cancel (16 padding, 2 border) and the primary
    // button (18 padding, 2 border, an icon with its gap) 8 px apart.
    let primaries: Vec<(&str, Labeler)> = vec![
        (
            "Start recording",
            Box::new(|| cm_i18n::tr!("Start recording")),
        ),
        ("Publish video", Box::new(|| cm_i18n::tr!("Publish video"))),
        (
            "Publish to YouTube",
            Box::new(|| cm_i18n::tr!("Publish to {platform}", platform = "YouTube")),
        ),
        (
            "Delete capture",
            Box::new(|| cm_i18n::tr!("Delete capture")),
        ),
        ("Discard take", Box::new(|| cm_i18n::tr!("Discard take"))),
        (
            "Discard and restart",
            Box::new(|| cm_i18n::tr!("Discard and restart")),
        ),
    ];
    let mut footer = Case::new(
        "Confirmation dialog buttons (Cancel + primary)",
        "520 - 2 - 48 = 470",
        470.,
        ui,
        600,
        one,
    );
    for (name, f) in primaries {
        footer = footer.needed(format!("Cancel + {name}"), move || {
            let cancel = width(&cm_i18n::tr!("Cancel"), Face::Regular, ui) + 32. + 2.;
            let primary = width(&f(), Face::SemiBold, ui) + 36. + 2. + 16. + 8.;
            cancel + 8. + primary
        });
    }
    v.push(footer);

    // ---- Record sheet 600 ------------------------------------------------------------------------
    // Footer: 600 - 2 border - 44 padding = 554 for the left column (countdown line, caption and
    // keycaps, 12 px) and the buttons, 12 px apart. The left column gives way (it wraps), the
    // buttons do not: Cancel (16 padding, 2 border) and Start (18 padding, 2 border, a 16 px icon,
    // two 8 px gaps and the 20 px Enter chip) 10 px apart.
    v.push(
        Case::new(
            "Record sheet footer buttons (Cancel + Start)",
            "554 - 12 gap - 120 for the left column",
            554. - 12. - 120.,
            ui,
            600,
            one,
        )
        .needed("Cancel + Start recording", move || {
            let cancel = width(&cm_i18n::tr!("Cancel"), Face::Regular, ui) + 34.;
            let start = width(&cm_i18n::tr!("Start recording"), Face::SemiBold, ui)
                + 38.
                + 16.
                + 8.
                + 8.
                + 20.;
            cancel + 10. + start
        }),
    );
    v.push(
        Case::new(
            "Record sheet footer left column",
            "554 - 12 gap - 270 for the buttons, wraps",
            554. - 12. - 270.,
            cap,
            400,
            Overflow::Lines(3),
        )
        .text("countdown line", || {
            cm_i18n::tr!(
                "{secs} s countdown \u{00B7} Space skips, Esc cancels",
                secs = 3
            )
        })
        .text("Start or stop from anywhere", || {
            cm_i18n::tr!("Start or stop from anywhere")
        }),
    );
    // Kind tiles: three, 8 px apart, in 600 - 2 - 44 = 554.
    v.push(
        Case::new(
            "Record sheet kind tile",
            "(554 - 16) / 3 - 2 border",
            (554. - 16.) / 3. - 2.,
            13.5,
            500,
            one,
        )
        .texts(
            crate::recording::start::TargetKind::ALL
                .into_iter()
                .map(|k| t(format!("TargetKind::label({k:?})"), move || k.label())),
        ),
    );

    // ---- annotation editor inspector 300 ---------------------------------------------------------
    // 300 - 2 border - 32 padding = 266; the status pill adds 20 padding, 2 border, 12 icon, 6 gap.
    v.push(
        Case::new(
            "Annotation inspector status pill",
            "266 - 20 - 2 - 12 - 6",
            266. - 40.,
            cap,
            400,
            one,
        )
        .text("All changes saved", || cm_i18n::tr!("All changes saved"))
        .text("Saving", || cm_i18n::tr!("Saving\u{2026}")),
    );
    // Accept all + Dismiss wrap, so each button must fit alone: 266, 18/16 padding, 2 border.
    v.push(
        Case::new(
            "Annotation inspector button",
            "266 - 2 border - 36 padding",
            266. - 38.,
            ui,
            600,
            one,
        )
        .text("Accept all", || cm_i18n::tr!("Accept all"))
        .text("Accept", || cm_i18n::tr_ctx!("button", "Accept"))
        .text("Dismiss", || cm_i18n::tr_ctx!("button", "Dismiss")),
    );

    v
}

#[test]
fn english_and_vietnamese_labels_fit_their_controls() {
    let rows = rows(&cases());
    assert!(rows.len() > 60, "only {} labels measured", rows.len());
    if std::env::var("LAYOUT_WIDTH_ALL").is_ok() {
        for r in &rows {
            println!(
                "{:<48} {:<40} avail {:>5.0} en {:>4} vi {:>4} xa {:>4}  {} / {} / {}",
                r.control,
                r.what,
                r.avail,
                shown(&r.en),
                shown(&r.vi),
                shown(&r.xa),
                r.ve.word(),
                r.vv.word(),
                r.vx.word()
            );
        }
    }
    let bad: Vec<String> = rows
        .iter()
        .filter(|r| r.ve == Verdict::Overflows || r.vv == Verdict::Overflows)
        .map(|r| {
            format!(
                "{} / {}: room {:.0} px, English {:.0} px {:?}, Vietnamese {:.0} px {:?}",
                r.control, r.what, r.avail, r.en.px, r.en.text, r.vi.px, r.vi.text
            )
        })
        .collect();
    assert!(
        bad.is_empty(),
        "a label does not fit its control (shorten the wording, let it wrap, or change the pinned \
         size and its test together; locales/README.md, \"Layout: long translations\"):\n  {}",
        bad.join("\n  ")
    );
}

/// The accessor of a case is a real label, not an empty string (a typo would measure nothing).
#[test]
fn every_measured_label_has_text() {
    for r in rows(&cases()) {
        if r.en.text.is_empty() && r.en.px == 0. {
            panic!("{} / {}: nothing measured", r.control, r.what);
        }
        if !r.en.text.is_empty() {
            assert!(r.en.px > 0., "{} / {}", r.control, r.what);
        }
    }
}

/// The pseudo-locale is about 30 percent longer: the measure sees it.
#[test]
fn the_pseudo_locale_is_wider_than_english() {
    let rows = rows(&cases());
    let texts: Vec<&Row> = rows.iter().filter(|r| !r.en.text.is_empty()).collect();
    let wider = texts.iter().filter(|r| r.xa.px > r.en.px * 1.2).count();
    assert!(wider * 10 >= texts.len() * 8, "{wider} of {}", texts.len());
}

// ---- the sizes the code estimates from character counts --------------------------------------------

/// The toast window is sized from `estimate_height`: one line of title, 48 characters per line of
/// body, two buttons per row. The real text must not need more.
#[test]
fn toast_height_estimates_hold_in_every_language() {
    let col = 440. - 12. - 2. - 17. - 8. - 30. - 28. - 24.;
    for lang in [Lang::En, Lang::Vi, Lang::EnXa] {
        for (name, _) in toast_specs() {
            let spec = with_language(lang, || {
                toast_specs()
                    .into_iter()
                    .find(|(n, _)| *n == name)
                    .expect("spec")
                    .1
            });
            let title_lines = lines(&spec.title, Face::SemiBold, 14., col);
            let body_lines = spec
                .body
                .as_deref()
                .and_then(|b| lines(b, Face::Regular, 13., col));
            // The estimate: the title wraps at 38 characters, the body at 48.
            if lang != Lang::EnXa {
                let est = spec.title.chars().count().div_ceil(38).max(1);
                assert!(
                    title_lines.is_some_and(|n| n <= est),
                    "{lang:?} {name}: the title needs {title_lines:?} lines, the estimate allows {est}"
                );
            }
            if let (Some(b), Some(n)) = (&spec.body, body_lines) {
                let est = b
                    .split('\n')
                    .map(|l| l.chars().count().div_ceil(48).max(1))
                    .sum::<usize>();
                if lang != Lang::EnXa {
                    assert!(
                        n <= est,
                        "{lang:?} {name}: body needs {n} lines, the estimate allows {est}"
                    );
                }
            }
            // The estimate allows two buttons per row.
            if lang != Lang::EnXa && !spec.buttons.is_empty() {
                let w: Vec<f32> = spec
                    .buttons
                    .iter()
                    .map(|b| width(&b.label, Face::Regular, 12.) + 24. + 2. + 20.)
                    .collect();
                let mut rows_needed = 1;
                let mut used = 0.;
                for x in &w {
                    if used > 0. && used + 8. + x > col {
                        rows_needed += 1;
                        used = *x;
                    } else {
                        used += if used > 0. { 8. + x } else { *x };
                    }
                }
                assert!(
                    rows_needed <= spec.buttons.len().div_ceil(2),
                    "{lang:?} {name}: the buttons need {rows_needed} rows"
                );
            }
        }
    }
}

/// The Record sheet and the confirmation dialog are sized from character counts too: the hint of a
/// source row (74 characters per line, `ROW_HINT_CHARS`) and the dialog summary (60 per line).
#[test]
fn sheet_and_dialog_height_estimates_hold_in_every_language() {
    let hints: Vec<(&str, Labeler)> = vec![
        (
            "camera hint (preview on)",
            Box::new(|| {
                cm_i18n::tr!(
                    "Live preview on screen. Drag it anywhere; it stays out of the recording."
                )
            }),
        ),
        (
            "camera hint (preview off)",
            Box::new(|| cm_i18n::tr!("A circle with your face on the recording.")),
        ),
        (
            "mic hint",
            Box::new(|| cm_i18n::tr!("System default input")),
        ),
        (
            "system audio hint",
            Box::new(|| cm_i18n::tr!("Sounds from apps, without your own")),
        ),
    ];
    // 554 - 32 row padding - 34 tile - 2*12 gaps - 44 switch = 420 px, 12.5 px text.
    for lang in [Lang::En, Lang::Vi] {
        for (name, f) in &hints {
            let text = with_language(lang, f);
            let est = text.chars().count().div_ceil(74).max(1);
            let need = lines(&text, Face::Regular, 12.5, 420.);
            assert!(
                need.is_some_and(|n| n <= est),
                "{lang:?} {name}: {text:?} needs {need:?} lines, the sheet reserves {est}"
            );
        }
        // The dialog summary: 520 - 48 padding = 472 px at 14 px, 60 characters per line.
        let summaries = with_language(lang, || {
            vec![
                crate::confirm::delete_dialog("x").summary,
                crate::confirm::discard_dialog(true, "0:10", None).summary,
                crate::confirm::discard_dialog(false, "0:10", None).summary,
            ]
        });
        for text in summaries {
            let est = text.chars().count().div_ceil(60).max(1);
            let need = lines(&text, Face::Regular, 14., 472.);
            assert!(
                need.is_some_and(|n| n <= est),
                "{lang:?}: {text:?} needs {need:?} lines, the dialog reserves {est}"
            );
        }
    }
}

/// `LAYOUT_WIDTH_REPORT=<file>` writes the table of `design/i18n-STATUS-overflow.md`: per control
/// the label with the largest Vietnamese width. Without the variable it only checks that the table
/// can be built.
#[test]
fn the_report_table_can_be_written() {
    let rows = rows(&cases());
    let mut controls: Vec<&'static str> = Vec::new();
    for r in &rows {
        if !controls.contains(&r.control) {
            controls.push(r.control);
        }
    }
    let mut md = String::from(
        "| Control | Room (px) | Worst label | English px | Vietnamese px | Pseudo px | Vi / room | Labels | Verdict (Vietnamese; pseudo) | Room worked out from |\n\
         |---|---:|---|---:|---:|---:|---:|---:|---|---|\n",
    );
    for c in controls {
        let group: Vec<&Row> = rows.iter().filter(|r| r.control == c).collect();
        let worst = group
            .iter()
            .max_by(|a, b| a.vi.px.total_cmp(&b.vi.px))
            .expect("a group has a row");
        let name = if worst.vi.text.is_empty() {
            worst.what.clone()
        } else {
            worst.vi.text.chars().take(36).collect::<String>()
        };
        md.push_str(&format!(
            "| {} | {:.0} | {} | {:.0} | {:.0} | {:.0} | {:.2} | {} | {}; {} | {} |\n",
            c,
            worst.avail,
            name.replace('|', "/"),
            worst.en.px,
            worst.vi.px,
            worst.xa.px,
            worst.vi.px / worst.avail.max(1.),
            group.len(),
            worst.vv.word(),
            worst.vx.word(),
            worst.geometry,
        ));
    }
    assert!(md.lines().count() > 10);
    if let Ok(path) = std::env::var("LAYOUT_WIDTH_REPORT") {
        std::fs::write(path, md).expect("write the report");
    }
}
