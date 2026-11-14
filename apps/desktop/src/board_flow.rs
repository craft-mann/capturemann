//! Boards (BRD-1, design board S10): the state and the words of the "Make a board" sheet. Pure:
//! no windows here, so it is unit tested. The window is `ui::board_sheet`; **Finish board** calls
//! `create_board` as the user, the same tool an agent uses.
//!
//! Until Finish nothing exists in the library: the layout is this state only, so it stays
//! editable. The canvas is drawn from [`cm_render::compose_board::plan`] with the sources' pixel
//! sizes (a cropped screenshot is a little off until the real picture is made).

use cm_i18n::{tr, trn};
use cm_render::compose_board::{
    self, Arrangement, Background, BoardLayout, BoardOptions, BoardPlan, LayoutKind, MAX_SOURCES,
    MIN_SOURCES,
};
use serde_json::{json, Value};

/// One screenshot of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSource {
    pub id: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

/// The background choices (BRD-1: transparent, white, light grey, dark or a tint colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgChoice {
    Transparent,
    White,
    LightGrey,
    Dark,
    Tint,
}

impl BgChoice {
    pub const ALL: [BgChoice; 5] = [
        BgChoice::Transparent,
        BgChoice::White,
        BgChoice::LightGrey,
        BgChoice::Dark,
        BgChoice::Tint,
    ];

    pub fn label(self) -> String {
        match self {
            BgChoice::Transparent => tr!("Transparent"),
            BgChoice::White => tr!("White"),
            BgChoice::LightGrey => tr!("Light grey"),
            BgChoice::Dark => tr!("Dark"),
            BgChoice::Tint => tr!("Tint"),
        }
    }
}

/// The layouts the sheet offers. Free (a rectangle per picture) is available to agents through
/// `create_board`; the sheet has no drag canvas for it yet.
pub const LAYOUTS: [LayoutKind; 3] = [LayoutKind::Pair, LayoutKind::Grid, LayoutKind::Strip];

pub fn layout_label(k: LayoutKind) -> String {
    match k {
        LayoutKind::Pair => tr!("Pair"),
        LayoutKind::Grid => tr!("Grid"),
        LayoutKind::Strip => tr!("Strip"),
        LayoutKind::Free => tr!("Free"),
    }
}

/// The state of the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSheet {
    pub sources: Vec<BoardSource>,
    pub layout: LayoutKind,
    /// Pair and Strip: stacked in a column.
    pub stacked: bool,
    /// Grid columns; `None` is the default (2 up to four pictures, else 3).
    pub columns: Option<u32>,
    pub gap: u32,
    pub margin: u32,
    pub background: BgChoice,
    /// `#RRGGBB` typed for the tint.
    pub tint: String,
    pub radius: u32,
    pub shadow: bool,
    /// A caption under each picture (its title).
    pub captions: bool,
    pub title: String,
    pub before_after: bool,
    pub busy: bool,
    pub error: Option<String>,
}

impl BoardSheet {
    pub fn new(sources: Vec<BoardSource>) -> Self {
        let defaults = BoardOptions::default();
        let layout = if sources.len() == 2 {
            LayoutKind::Pair
        } else {
            LayoutKind::Grid
        };
        BoardSheet {
            before_after: layout == LayoutKind::Pair,
            sources,
            layout,
            stacked: false,
            columns: None,
            gap: defaults.gap,
            margin: defaults.margin,
            background: BgChoice::White,
            tint: "#DCE6F5".to_owned(),
            radius: 0,
            shadow: false,
            captions: false,
            title: String::new(),
            busy: false,
            error: None,
        }
    }

    /// Whether a layout can be chosen for this many pictures (Pair is exactly two).
    pub fn layout_enabled(&self, k: LayoutKind) -> bool {
        k != LayoutKind::Pair || self.sources.len() == 2
    }

    pub fn set_layout(&mut self, k: LayoutKind) {
        if self.layout_enabled(k) {
            self.layout = k;
        }
    }

    /// Moves picture `ix` by `delta` places (the arrows of the source list).
    pub fn move_source(&mut self, ix: usize, delta: isize) {
        let to = ix as isize + delta;
        if ix < self.sources.len() && to >= 0 && (to as usize) < self.sources.len() {
            self.sources.swap(ix, to as usize);
        }
    }

    /// Takes a picture out; Pair needs two, so the layout falls back to Grid.
    pub fn remove_source(&mut self, ix: usize) {
        if ix < self.sources.len() {
            self.sources.remove(ix);
            if self.layout == LayoutKind::Pair && self.sources.len() != 2 {
                self.layout = LayoutKind::Grid;
            }
        }
    }

    /// Adds a picture (a drop or a pick); at most 12.
    pub fn add_source(&mut self, s: BoardSource) -> bool {
        if self.sources.len() >= MAX_SOURCES || self.sources.iter().any(|x| x.id == s.id) {
            return false;
        }
        self.sources.push(s);
        true
    }

    fn bg(&self) -> Result<Background, String> {
        Ok(match self.background {
            BgChoice::Transparent => Background::Transparent,
            BgChoice::White => Background::White,
            BgChoice::LightGrey => Background::LightGrey,
            BgChoice::Dark => Background::Dark,
            BgChoice::Tint => Background::parse(self.tint.trim())
                .map_err(|_| tr!("The tint must be a colour like #DCE6F5."))?,
        })
    }

    pub fn layout_json(&self) -> BoardLayout {
        let mut l = BoardLayout::new(self.layout);
        if self.stacked && matches!(self.layout, LayoutKind::Pair | LayoutKind::Strip) {
            l.arrangement = Arrangement::Stacked;
        }
        if self.layout == LayoutKind::Grid {
            l.columns = self.columns;
        }
        l
    }

    pub fn options(&self) -> Result<BoardOptions, String> {
        Ok(BoardOptions {
            gap: self.gap,
            margin: self.margin,
            background: self.bg()?,
            radius: self.radius,
            shadow: self.shadow,
            captions: if self.captions {
                self.sources.iter().map(|s| s.title.clone()).collect()
            } else {
                Vec::new()
            },
            title: Some(self.title.trim().to_owned()).filter(|t| !t.is_empty()),
            before_after: self.layout == LayoutKind::Pair && self.before_after,
            label_words: Some([
                tr!("BEFORE").to_uppercase(),
                tr!("AFTER").to_uppercase(),
            ]),
        })
    }

    /// The geometry the canvas draws.
    pub fn plan(&self) -> Result<BoardPlan, String> {
        let sizes: Vec<(u32, u32)> = self
            .sources
            .iter()
            .map(|s| (s.width.max(1), s.height.max(1)))
            .collect();
        compose_board::plan(&sizes, &self.layout_json(), &self.options()?)
            .map_err(|e| e.to_string())
    }

    /// The size chip: "2400 x 1200 px".
    pub fn size_chip(&self) -> String {
        match self.plan() {
            Ok(p) => format!("{} \u{00D7} {} px", p.width, p.height),
            Err(_) => String::new(),
        }
    }

    pub fn title_text(&self) -> String {
        trn!(
            "Board of {n} screenshot",
            "Board of {n} screenshots",
            self.sources.len()
        )
    }

    pub fn can_finish(&self) -> bool {
        let n = self.sources.len();
        !self.busy
            && (MIN_SOURCES..=MAX_SOURCES).contains(&n)
            && (self.layout != LayoutKind::Pair || n == 2)
            && self.plan().is_ok()
    }

    /// The `create_board` input.
    pub fn request(&self) -> Result<Value, String> {
        let n = self.sources.len();
        if !(MIN_SOURCES..=MAX_SOURCES).contains(&n) {
            return Err(tr!("A board needs 2 to 12 screenshots."));
        }
        let options = self.options()?;
        let mut p = json!({
            "capture_ids": self.sources.iter().map(|s| &s.id).collect::<Vec<_>>(),
            "layout": self.layout_json(),
            "options": options,
        });
        if !self.title.trim().is_empty() {
            p["title"] = json!(self.title.trim());
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(id: &str, w: u32, h: u32) -> BoardSource {
        BoardSource {
            id: id.into(),
            title: id.to_uppercase(),
            width: w,
            height: h,
        }
    }

    #[test]
    fn two_pictures_start_as_a_pair_with_labels() {
        let s = BoardSheet::new(vec![src("a", 400, 200), src("b", 300, 300)]);
        assert_eq!(s.layout, LayoutKind::Pair);
        assert!(s.before_after);
        let p = s.plan().unwrap();
        assert_eq!(p.cells[0].h, p.cells[1].h);
        assert_eq!(s.size_chip(), format!("{} \u{00D7} {} px", p.width, p.height));
        let r = s.request().unwrap();
        assert_eq!(r["layout"]["kind"], "pair");
        assert_eq!(r["options"]["before_after"], true);
        assert_eq!(r["options"]["background"], "white");
        assert_eq!(r["capture_ids"], json!(["a", "b"]));
    }

    #[test]
    fn pair_needs_two_and_falls_back_to_a_grid() {
        let mut s = BoardSheet::new(vec![src("a", 10, 10), src("b", 10, 10), src("c", 10, 10)]);
        assert_eq!(s.layout, LayoutKind::Grid);
        assert!(!s.layout_enabled(LayoutKind::Pair));
        s.set_layout(LayoutKind::Pair);
        assert_eq!(s.layout, LayoutKind::Grid);
        s.set_layout(LayoutKind::Strip);
        s.stacked = true;
        assert_eq!(s.request().unwrap()["layout"]["arrangement"], "stacked");
        s.remove_source(0);
        s.set_layout(LayoutKind::Pair);
        assert_eq!(s.layout, LayoutKind::Pair);
        s.remove_source(0);
        assert!(!s.can_finish());
        assert!(s.request().is_err());
    }

    #[test]
    fn sources_move_and_the_count_is_capped() {
        let mut s = BoardSheet::new(vec![src("a", 10, 10), src("b", 10, 10)]);
        s.move_source(0, 1);
        assert_eq!(s.sources[0].id, "b");
        s.move_source(0, -1);
        assert_eq!(s.sources[0].id, "b");
        for i in 0..20 {
            s.add_source(src(&format!("x{i}"), 10, 10));
        }
        assert_eq!(s.sources.len(), 12);
        assert!(!s.add_source(src("b", 10, 10)));
    }

    #[test]
    fn captions_and_a_tint_go_into_the_options() {
        let mut s = BoardSheet::new(vec![src("a", 10, 10), src("b", 10, 10)]);
        s.captions = true;
        s.background = BgChoice::Tint;
        s.tint = "#112233".into();
        s.title = " Release ".into();
        let r = s.request().unwrap();
        assert_eq!(r["options"]["captions"], json!(["A", "B"]));
        assert_eq!(r["options"]["background"], "#112233");
        assert_eq!(r["options"]["title"], "Release");
        assert_eq!(r["title"], "Release");
        s.tint = "blue".into();
        assert!(s.request().is_err());
        assert!(!s.can_finish());
    }
}
