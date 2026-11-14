//! The pure parts of the 0.3.4 editor tools that change the picture itself: Retype (ANN-9),
//! Move area (ANN-10) and Blueprint (ANN-11). No GPUI types here.
//!
//! These three ops are **base edits**: the editor inserts them before the first non-base op
//! ([`insert_at`]) and sends the whole list with `replace: true`, because `annotate` refuses a
//! base edit after another op. Their fills are measured **once**, from the picture with the
//! earlier base edits applied ([`measure`]), and stored in the op, so the editor, the CLI and
//! agents render the same (ANN-1.AC6).

use cm_core::annotation::{BlueprintColor, BlueprintStyle, Fill, DEFAULT_BLUEPRINT_RADIUS};
use cm_core::{AnnotationOp, Color, OpKind, Pt, Rc};
use cm_i18n::tr;
use image::RgbaImage;
use serde_json::{json, Value};

/// A text line the recognizer found (one box of `suggest_blueprint`).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub rect: Rc,
    /// The recognized text, cut at 60 characters by the daemon (ends in `...` then).
    pub text: String,
    pub confidence: f64,
    /// Fill and bar colour measured by the daemon (a Blueprint stores them).
    pub fill: Option<Fill>,
    pub color: Option<Color>,
    pub busy: bool,
}

impl Line {
    /// The text to start a retype with: empty when the daemon cut it, because a retype must
    /// not silently keep a truncated copy.
    pub fn editable_text(&self) -> String {
        if self.text.ends_with("...") {
            String::new()
        } else {
            self.text.clone()
        }
    }
}

/// The `suggest_blueprint` request (read-only, Free tier).
pub fn suggest_request(capture_id: &str) -> Value {
    json!({ "capture_id": capture_id })
}

/// Reads the answer of `suggest_blueprint`.
pub fn parse_lines(v: &Value) -> Result<Vec<Line>, String> {
    let boxes = v
        .get("boxes")
        .and_then(Value::as_array)
        .ok_or_else(|| "suggest_blueprint returned no boxes".to_owned())?;
    let mut out = Vec::with_capacity(boxes.len());
    for b in boxes {
        let r = b
            .get("rect")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 4)
            .ok_or_else(|| "a box has no rect".to_owned())?;
        let n: Vec<f64> = r.iter().filter_map(Value::as_f64).collect();
        if n.len() != 4 {
            return Err("a box rect is not four numbers".into());
        }
        out.push(Line {
            rect: Rc::new(n[0], n[1], n[2], n[3]),
            text: b
                .get("text_preview")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            confidence: b.get("confidence").and_then(Value::as_f64).unwrap_or(1.0),
            fill: b
                .get("fill")
                .and_then(|f| serde_json::from_value(f.clone()).ok()),
            color: b
                .get("color")
                .and_then(|c| serde_json::from_value(c.clone()).ok()),
            busy: b.get("busy").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    Ok(out)
}

/// The index of the first op that is not a base edit: where a new base edit goes.
pub fn insert_at(ops: &[AnnotationOp]) -> usize {
    ops.iter()
        .position(|o| !o.kind.is_base_edit())
        .unwrap_or(ops.len())
}

/// The base edits of a list, in order (what a new base edit measures against).
pub fn base_ops(ops: &[AnnotationOp]) -> Vec<AnnotationOp> {
    ops.iter()
        .filter(|o| o.kind.is_base_edit())
        .cloned()
        .collect()
}

/// What measuring a box gives: the fill behind it and the text style inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub fill: Fill,
    /// The ring around the box varies too much for a clean fill.
    pub busy: bool,
    pub color: Color,
    pub size: f64,
}

/// Measures `rect` on `original` with the earlier base edits `base` applied (never the
/// annotations drawn over it): the fill from the ring around the box, the text colour and size
/// inside it. Heavy for a large image: call it off the UI thread.
pub fn measure(
    original: &RgbaImage,
    base: &[AnnotationOp],
    rect: Rc,
    text: Option<&str>,
) -> Result<Measured, String> {
    let picture = if base.is_empty() {
        std::borrow::Cow::Borrowed(original)
    } else {
        std::borrow::Cow::Owned(
            cm_render::render_annotations(original, base).map_err(|e| e.to_string())?,
        )
    };
    let m = cm_render::touchup::measure_fill(&picture, rect);
    let t = cm_render::touchup::measure_text_style(&picture, rect, &m.fill, text);
    Ok(Measured {
        fill: m.fill,
        busy: m.busy,
        color: t.color,
        size: t.size,
    })
}

/// What a measurement is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureFor {
    /// A new retype of this box.
    Retype,
    /// The source of a Move area.
    MoveSource,
}

/// A measurement the editor asks the document to run off the UI thread.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureRequest {
    pub what: MeasureFor,
    pub rect: Rc,
    /// The recognized text (Retype), for the size estimate.
    pub text: String,
}

/// "This area has an image or a control in it, so the fill may show": shown before the op is created, with Cancel
/// (default, stores nothing) and Use anyway.
#[derive(Debug, Clone, PartialEq)]
pub struct BusyPrompt {
    pub request: MeasureRequest,
    pub measured: Measured,
}

/// A Move area whose source is chosen and measured; the op exists once the contents are moved
/// or removed.
#[derive(Debug, Clone, PartialEq)]
pub struct MovePending {
    pub source: Rc,
    pub fill: Fill,
    /// While the contents are being dragged: where they are now.
    pub dest: Option<Pt>,
}

impl MovePending {
    /// The op for the current drag position (a draft for the preview, the op on release).
    pub fn op(&self, to: Option<Pt>) -> OpKind {
        OpKind::MoveArea {
            source: self.source,
            to,
            fill: self.fill.clone(),
        }
    }
}

/// The axis a Move area snapped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// The contents stay on the source row (`dy == 0`).
    Row,
    /// The contents stay on the source column (`dx == 0`).
    Column,
}

/// A pink guide drawn on the canvas while a Move area drag is snapped.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapGuide {
    pub axis: Axis,
    /// The source's centre line: `y` for a row, `x` for a column.
    pub at: f64,
    /// `−150, 0 · row locked`.
    pub tag: String,
}

/// Offsets within this many logical px of the source row or column snap to it.
pub const SNAP_PX: f64 = 8.0;

/// Where a dragged Move area lands. `start` is the destination at the start of the drag, `d`
/// the pointer movement. With `shift` the dominant axis is kept and the other locks to the
/// source; with `snap` an offset within `tol` of the source row or column locks to it.
pub fn snap_move(
    source: Rc,
    start: Pt,
    d: (f64, f64),
    shift: bool,
    snap: bool,
    tol: f64,
) -> (Pt, Option<SnapGuide>) {
    let s = source.normalized();
    let (mut dx, mut dy) = (start.x + d.0 - s.x, start.y + d.1 - s.y);
    let mut row = false;
    let mut col = false;
    if shift {
        if dx.abs() >= dy.abs() {
            dy = 0.0;
            row = true;
        } else {
            dx = 0.0;
            col = true;
        }
    } else if snap {
        if dy.abs() <= tol {
            dy = 0.0;
            row = true;
        }
        if dx.abs() <= tol {
            dx = 0.0;
            col = true;
        }
    }
    let to = Pt::new(s.x + dx, s.y + dy);
    let guide = if row {
        Some(SnapGuide {
            axis: Axis::Row,
            at: s.y + s.h / 2.0,
            tag: offset_tag(dx, dy, true),
        })
    } else if col {
        Some(SnapGuide {
            axis: Axis::Column,
            at: s.x + s.w / 2.0,
            tag: offset_tag(dx, dy, false),
        })
    } else {
        None
    };
    (to, guide)
}

/// `−150, 0 · row locked` (a real minus sign).
pub fn offset_tag(dx: f64, dy: f64, row: bool) -> String {
    let n = |v: f64| {
        let r = v.round();
        if r < 0.0 {
            format!("\u{2212}{}", -r)
        } else {
            format!("{r}")
        }
    };
    let (x, y) = (n(dx), n(dy));
    if row {
        tr!("{x}, {y} \u{B7} row locked", x = x, y = y)
    } else {
        tr!("{x}, {y} \u{B7} column locked", x = x, y = y)
    }
}

/// The `to` of a dragged Move area op after the pointer moved by `d` (the same rule for a
/// pending area and for an existing op).
pub fn moved_to(
    source: Rc,
    start: Pt,
    d: (f64, f64),
    shift: bool,
    snap: bool,
    tol: f64,
    image: (u32, u32),
) -> (Pt, Option<SnapGuide>) {
    let (mut to, guide) = snap_move(source, start, d, shift, snap, tol);
    // Keep at least a sliver on the image so the area is never lost.
    let s = source.normalized();
    to.x = to.x.clamp(-s.w + 4.0, f64::from(image.0) - 4.0);
    to.y = to.y.clamp(-s.h + 4.0, f64::from(image.1) - 4.0);
    (to, guide)
}

/// One text line in the Blueprint panel.
#[derive(Debug, Clone, PartialEq)]
pub struct BpBox {
    pub rect: Rc,
    pub text: String,
    pub color: Color,
    pub fill: Fill,
    /// The line becomes a bar; false when the user keeps its real text.
    pub replaced: bool,
}

/// The Blueprint panel (ANN-11): every recognized line as a bar, a keep-list, style, colour and
/// radius. A new Blueprint is a draft until Apply (one op, one undo step); an existing one
/// (`existing`) is edited op by op, each keep-list change being one op update and so one undo
/// step (AC3).
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintPanel {
    pub boxes: Vec<BpBox>,
    pub style: BlueprintStyle,
    pub color: BlueprintColor,
    pub radius: f64,
    /// The id of the stored op when editing one.
    pub existing: Option<String>,
    /// Earlier keep-lists of a draft, for Undo inside the panel.
    history: Vec<Vec<bool>>,
    /// The line under the pointer.
    pub hover: Option<usize>,
}

impl BlueprintPanel {
    /// A draft from `suggest_blueprint`: every line becomes a bar.
    pub fn from_lines(lines: &[Line]) -> Self {
        let boxes = lines
            .iter()
            .map(|l| BpBox {
                rect: l.rect,
                text: l.text.clone(),
                color: l.color.unwrap_or(Color::rgb(0x99, 0x99, 0x99)),
                fill: l.fill.clone().unwrap_or_else(|| Fill::solid(Color::WHITE)),
                replaced: true,
            })
            .collect();
        BlueprintPanel {
            boxes,
            style: BlueprintStyle::Bar,
            color: BlueprintColor::Auto,
            radius: DEFAULT_BLUEPRINT_RADIUS,
            existing: None,
            history: Vec::new(),
            hover: None,
        }
    }

    /// The panel of a stored op: its boxes are the lines that are bars. (Lines kept earlier are
    /// no longer text the recognizer can find on the edited picture, so they cannot come back
    /// from here; undo restores them.)
    pub fn from_op(id: &str, kind: &OpKind) -> Option<Self> {
        let OpKind::Blueprint {
            boxes,
            style,
            color,
            radius,
            colors,
            fills,
        } = kind
        else {
            return None;
        };
        let rows = boxes
            .iter()
            .enumerate()
            .map(|(i, r)| BpBox {
                rect: *r,
                text: String::new(),
                color: colors
                    .get(i)
                    .copied()
                    .unwrap_or(Color::rgb(0x99, 0x99, 0x99)),
                fill: fills
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| Fill::solid(Color::WHITE)),
                replaced: true,
            })
            .collect();
        Some(BlueprintPanel {
            boxes: rows,
            style: *style,
            color: *color,
            radius: *radius,
            existing: Some(id.to_owned()),
            history: Vec::new(),
            hover: None,
        })
    }

    /// `(lines found, become bars, kept)`.
    pub fn counts(&self) -> (usize, usize, usize) {
        let bars = self.boxes.iter().filter(|b| b.replaced).count();
        (self.boxes.len(), bars, self.boxes.len() - bars)
    }

    /// The line under `p` (the smallest when boxes overlap).
    pub fn line_at(&self, p: Pt) -> Option<usize> {
        self.boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                let r = b.rect.normalized();
                p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
            })
            .min_by(|a, b| (a.1.rect.w * a.1.rect.h).total_cmp(&(b.1.rect.w * b.1.rect.h)))
            .map(|(i, _)| i)
    }

    /// Keeps the real text of line `i`, or turns it back into a bar. One keep-list change.
    pub fn toggle(&mut self, i: usize) -> bool {
        if i >= self.boxes.len() {
            return false;
        }
        self.history
            .push(self.boxes.iter().map(|b| b.replaced).collect());
        self.boxes[i].replaced = !self.boxes[i].replaced;
        true
    }

    /// Takes back the last keep-list change of a draft.
    pub fn undo_keep(&mut self) -> bool {
        let Some(prev) = self.history.pop() else {
            return false;
        };
        for (b, r) in self.boxes.iter_mut().zip(prev) {
            b.replaced = r;
        }
        true
    }

    /// Whether a keep-list change can be taken back inside the panel.
    pub fn can_undo_keep(&self) -> bool {
        !self.history.is_empty()
    }

    /// The op for the current panel: the lines that become bars, with the colour and fill
    /// measured for each stored (the render never measures again). `None` when every line is
    /// kept.
    pub fn op_kind(&self) -> Option<OpKind> {
        let bars: Vec<&BpBox> = self.boxes.iter().filter(|b| b.replaced).collect();
        if bars.is_empty() {
            return None;
        }
        Some(OpKind::Blueprint {
            boxes: bars.iter().map(|b| b.rect).collect(),
            style: self.style,
            color: self.color,
            radius: self.radius,
            colors: if self.color == BlueprintColor::Auto {
                bars.iter().map(|b| b.color).collect()
            } else {
                Vec::new()
            },
            fills: bars.iter().map(|b| b.fill.clone()).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::annotation::validate_list;

    fn line(x: f64, y: f64, text: &str) -> Line {
        Line {
            rect: Rc::new(x, y, 80.0, 14.0),
            text: text.into(),
            confidence: 0.9,
            fill: Some(Fill::solid(Color::WHITE)),
            color: Some(Color::rgb(0x20, 0x20, 0x20)),
            busy: false,
        }
    }

    fn op(id: &str, k: OpKind) -> AnnotationOp {
        AnnotationOp::new(id, k)
    }

    fn arrow() -> OpKind {
        OpKind::Arrow {
            from: Pt::new(0.0, 0.0),
            to: Pt::new(5.0, 5.0),
            color: Color::BLACK,
            width: 4.0,
        }
    }

    fn retype() -> OpKind {
        OpKind::Retype {
            rect: Rc::new(10.0, 10.0, 60.0, 16.0),
            fill: Fill::solid(Color::WHITE),
            text: "x".into(),
            size: 14.0,
            color: Color::BLACK,
            weight: 400,
            align: cm_core::annotation::TextAlign::Left,
        }
    }

    /// ANN-9.AC4 (editor side): a new base edit goes before the first non-base op, after the
    /// earlier base edits, and the list is then valid for `annotate`.
    #[test]
    fn a_base_edit_is_inserted_before_the_first_non_base_op() {
        let mut ops = vec![op("a", retype()), op("b", arrow()), op("c", arrow())];
        assert_eq!(insert_at(&ops), 1);
        ops.insert(insert_at(&ops), op("n", retype()));
        assert_eq!(ops[1].id, "n");
        assert!(validate_list(&ops).is_ok());
        assert_eq!(insert_at(&[]), 0);
        assert_eq!(insert_at(&[op("a", retype())]), 1);
        assert_eq!(base_ops(&ops).len(), 2);
    }

    /// ANN-10: the axis snaps to the source row or column, and Shift locks the axis.
    #[test]
    fn move_area_snaps_to_the_source_row_and_shift_locks_the_axis() {
        let src = Rc::new(100.0, 50.0, 40.0, 20.0);
        // Dragged 120 right and 3 down: within tolerance of the row.
        let (to, g) = snap_move(src, Pt::new(100.0, 50.0), (120.0, 3.0), false, true, 8.0);
        assert_eq!((to.x, to.y), (220.0, 50.0));
        let g = g.unwrap();
        assert_eq!(g.axis, Axis::Row);
        assert_eq!(g.at, 60.0);
        assert_eq!(g.tag, "120, 0 \u{B7} row locked");
        // Far from both axes: free, no guide.
        let (to, g) = snap_move(src, Pt::new(100.0, 50.0), (60.0, 40.0), false, true, 8.0);
        assert_eq!((to.x, to.y), (160.0, 90.0));
        assert!(g.is_none());
        // Snapping switched off.
        let (to, g) = snap_move(src, Pt::new(100.0, 50.0), (120.0, 3.0), false, false, 8.0);
        assert_eq!((to.x, to.y), (220.0, 53.0));
        assert!(g.is_none());
        // Shift: the dominant axis is kept even when far from the row.
        let (to, g) = snap_move(src, Pt::new(100.0, 50.0), (20.0, -90.0), true, false, 8.0);
        assert_eq!((to.x, to.y), (100.0, -40.0));
        let g = g.unwrap();
        assert_eq!(g.axis, Axis::Column);
        assert_eq!(g.at, 120.0);
        assert_eq!(g.tag, "0, \u{2212}90 \u{B7} column locked");
        // The tag of the design board: -150, 0 with a real minus sign.
        let (_, g) = snap_move(src, Pt::new(100.0, 50.0), (-150.0, 2.0), false, true, 8.0);
        assert_eq!(g.unwrap().tag, "\u{2212}150, 0 \u{B7} row locked");
    }

    #[test]
    fn a_moved_area_never_leaves_the_image_entirely() {
        let src = Rc::new(10.0, 10.0, 40.0, 20.0);
        let (to, _) = moved_to(
            src,
            Pt::new(10.0, 10.0),
            (5000.0, 5000.0),
            false,
            false,
            8.0,
            (200, 100),
        );
        assert!(to.x < 200.0 && to.y < 100.0);
        let (to, _) = moved_to(
            src,
            Pt::new(10.0, 10.0),
            (-5000.0, -5000.0),
            false,
            false,
            8.0,
            (200, 100),
        );
        assert!(to.x + 40.0 > 0.0 && to.y + 20.0 > 0.0);
    }

    /// ANN-11.AC3: a keep-list change is one change; undo inside the panel takes it back; the
    /// op has only the lines that become bars, with the measured colours stored.
    #[test]
    fn blueprint_keep_list_and_op() {
        let lines: Vec<Line> = (0..20)
            .map(|i| line(10.0, 10.0 + 20.0 * f64::from(i), "t"))
            .collect();
        let mut p = BlueprintPanel::from_lines(&lines);
        assert_eq!(p.counts(), (20, 20, 0));
        assert!(p.toggle(3));
        assert!(p.toggle(7));
        assert_eq!(p.counts(), (20, 18, 2));
        let OpKind::Blueprint {
            boxes,
            colors,
            fills,
            color,
            ..
        } = p.op_kind().unwrap()
        else {
            panic!()
        };
        assert_eq!(boxes.len(), 18);
        assert_eq!(colors.len(), 18, "auto colours are stored per box");
        assert_eq!(fills.len(), 18);
        assert_eq!(color, BlueprintColor::Auto);
        assert!(!boxes.contains(&lines[3].rect));
        assert!(p.undo_keep());
        assert_eq!(p.counts(), (20, 19, 1));
        assert!(p.undo_keep());
        assert!(!p.undo_keep());
        assert_eq!(p.counts(), (20, 20, 0));
        // Keeping every line leaves nothing to apply.
        for i in 0..20 {
            p.toggle(i);
        }
        assert!(p.op_kind().is_none());
    }

    #[test]
    fn a_fixed_colour_blueprint_stores_no_per_box_colours() {
        let mut p = BlueprintPanel::from_lines(&[line(0.0, 0.0, "a"), line(0.0, 30.0, "b")]);
        p.color = BlueprintColor::Fixed(Color::rgb(0xCC, 0xCC, 0xCC));
        let OpKind::Blueprint { colors, boxes, .. } = p.op_kind().unwrap() else {
            panic!()
        };
        assert!(colors.is_empty());
        assert_eq!(boxes.len(), 2);
        // The stored op round-trips through the panel.
        let k = p.op_kind().unwrap();
        let q = BlueprintPanel::from_op("bp_1", &k).unwrap();
        assert_eq!(q.existing.as_deref(), Some("bp_1"));
        assert_eq!(q.counts(), (2, 2, 0));
    }

    #[test]
    fn the_line_under_the_pointer_is_the_smallest_box() {
        let mut big = line(0.0, 0.0, "para");
        big.rect = Rc::new(0.0, 0.0, 200.0, 100.0);
        let small = line(10.0, 10.0, "word");
        let p = BlueprintPanel::from_lines(&[big, small]);
        assert_eq!(p.line_at(Pt::new(20.0, 15.0)), Some(1));
        assert_eq!(p.line_at(Pt::new(150.0, 80.0)), Some(0));
        assert_eq!(p.line_at(Pt::new(300.0, 80.0)), None);
    }

    #[test]
    fn suggest_blueprint_answers_are_parsed() {
        let v = json!({"boxes": [
            {"rect": [10, 20, 100, 14], "text_preview": "Save", "confidence": 0.97,
             "fill": {"kind": "solid", "colors": ["#FFFFFF"]}, "color": "#202020", "busy": false},
            {"rect": [10, 40, 100, 14], "text_preview": "A very long line...", "confidence": 0.5}
        ]});
        let lines = parse_lines(&v).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].rect, Rc::new(10.0, 20.0, 100.0, 14.0));
        assert_eq!(lines[0].editable_text(), "Save");
        assert!(lines[0].fill.is_some());
        assert_eq!(lines[0].color, Some(Color::rgb(0x20, 0x20, 0x20)));
        assert_eq!(
            lines[1].editable_text(),
            "",
            "a cut preview is not kept as the text"
        );
        assert!(parse_lines(&json!({})).is_err());
        assert!(parse_lines(&json!({"boxes": [{"rect": [1, 2]}]})).is_err());
    }

    /// ANN-9.AC2 / AC3 on synthetic pixels: a flat dialog is a clean solid fill with the text
    /// colour measured; a noisy photo is busy (the tool then warns before it makes the op).
    #[test]
    fn measuring_a_flat_box_is_clean_and_a_noisy_one_is_busy() {
        let mut flat = RgbaImage::from_pixel(120, 60, image::Rgba([240, 240, 240, 255]));
        for y in 24..34 {
            for x in 30..70 {
                flat.put_pixel(x, y, image::Rgba([20, 20, 20, 255]));
            }
        }
        let m = measure(&flat, &[], Rc::new(28.0, 20.0, 46.0, 18.0), Some("Save")).unwrap();
        assert!(!m.busy);
        assert_eq!(m.fill.colors.len(), 1);
        assert!(m.color.r < 80, "dark text measured: {:?}", m.color);
        let mut x = 12345u32;
        let noisy = RgbaImage::from_fn(120, 60, |_, _| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = (x >> 24) as u8;
            image::Rgba([v, v.wrapping_add(80), v.wrapping_mul(3), 255])
        });
        let m = measure(&noisy, &[], Rc::new(28.0, 20.0, 46.0, 18.0), None).unwrap();
        assert!(m.busy);
    }

    /// Measuring uses the picture with the earlier base edits applied: a Move area that painted
    /// a red hole earlier is what a later retype's ring sees.
    #[test]
    fn measuring_sees_the_earlier_base_edits() {
        let img = RgbaImage::from_pixel(120, 60, image::Rgba([255, 255, 255, 255]));
        let paint_red = op(
            "m",
            OpKind::MoveArea {
                source: Rc::new(0.0, 0.0, 120.0, 60.0),
                to: None,
                fill: Fill::solid(Color::rgb(200, 0, 0)),
            },
        );
        let m = measure(&img, &[paint_red], Rc::new(40.0, 20.0, 30.0, 12.0), None).unwrap();
        assert_eq!(m.fill.colors[0], Color::rgb(200, 0, 0));
    }
}
